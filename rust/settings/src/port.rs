//! Port of the C# settings JSON round trip in `Witcher3StringEditor.Models.AppSettings`
//! (recorded through the harness's `settings-default` / `settings-roundtrip` commands).
//! The written bytes must match the original's System.Text.Json output exactly.
#![forbid(unsafe_code)]

use std::fmt::Write as _;

// ---------------------------------------------------------------------------
// The model
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FileType {
    Csv,
    W3Strings,
    Excel,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PayloadEncoding {
    Utf8,
    Utf16Le,
}

/// A timestamp as it survives the round trip: the date-time digits, the fraction
/// in 100ns ticks (a .NET DateTime stores exactly that), and whether the input
/// carried a `Z`. Written back trimmed of trailing fraction zeros.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Timestamp {
    pub date_time: String,
    pub ticks: u32,
    pub utc: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecentItem {
    pub file_path: String,
    pub is_marked: bool,
    pub opened_time: Timestamp,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackupItem {
    pub file_name: String,
    pub hash: String,
    pub orgin_path: String,
    pub backup_path: String,
    pub backup_time: Timestamp,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Settings {
    /// The model's default is `""`, but the JSON constructor leaves it `null`
    /// when the settings file does not carry it; both write differently.
    pub game_exe_path: Option<String>,
    pub language: String,
    pub page_size: i64,
    pub preferred_language: String,
    pub preferred_encoding: PayloadEncoding,
    pub preferred_file_type: FileType,
    pub translator: String,
    pub recent_items: Vec<RecentItem>,
    pub backup_items: Vec<BackupItem>,
}

/// `new AppSettings()`: the model's own defaults.
pub fn default_settings() -> Settings {
    Settings {
        game_exe_path: Some(String::new()),
        language: String::new(),
        page_size: 30,
        preferred_language: "en".into(),
        preferred_encoding: PayloadEncoding::Utf16Le,
        preferred_file_type: FileType::Csv,
        translator: "MicrosoftTranslator".into(),
        recent_items: Vec::new(),
        backup_items: Vec::new(),
    }
}

// ---------------------------------------------------------------------------
// Reading (System.Text.Json with the model's converters)
// ---------------------------------------------------------------------------

struct Json<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Json<'a> {
    fn new(text: &'a str) -> Self {
        Self {
            bytes: text.as_bytes(),
            pos: 0,
        }
    }

    fn ws(&mut self) {
        loop {
            while self.pos < self.bytes.len() && self.bytes[self.pos].is_ascii_whitespace() {
                self.pos += 1;
            }
            if self.rest().starts_with("//") {
                while self.pos < self.bytes.len() && self.bytes[self.pos] != b'\n' {
                    self.pos += 1;
                }
            } else if self.rest().starts_with("/*") {
                self.pos += 2;
                while self.pos + 1 < self.bytes.len() && !self.rest().starts_with("*/") {
                    self.pos += 1;
                }
                self.pos = (self.pos + 2).min(self.bytes.len());
            } else {
                break;
            }
        }
    }

    fn rest(&self) -> &'a str {
        std::str::from_utf8(&self.bytes[self.pos..]).unwrap_or("")
    }

    fn peek(&mut self) -> Option<u8> {
        self.ws();
        self.bytes.get(self.pos).copied()
    }

    fn take(&mut self, ch: u8) -> bool {
        if self.peek() == Some(ch) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    /// A JSON string as an owned value.
    fn string(&mut self) -> Option<String> {
        if self.peek() != Some(b'"') {
            return None;
        }
        self.pos += 1;
        let mut out = String::new();
        while self.pos < self.bytes.len() {
            let b = self.bytes[self.pos];
            self.pos += 1;
            match b {
                b'"' => return Some(out),
                b'\\' => {
                    let esc = *self.bytes.get(self.pos)?;
                    self.pos += 1;
                    match esc {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'b' => out.push('\u{8}'),
                        b'f' => out.push('\u{c}'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'u' => {
                            let unit = self.hex4()?;
                            if (0xD800..=0xDBFF).contains(&unit)
                                && self.bytes.get(self.pos..self.pos + 2) == Some(b"\\u")
                            {
                                let save = self.pos;
                                self.pos += 2;
                                if let Some(low) = self.hex4()
                                    && (0xDC00..=0xDFFF).contains(&low)
                                {
                                    let scalar = 0x10000
                                        + (((unit as u32 - 0xD800) << 10)
                                            | (low as u32 - 0xDC00));
                                    if let Some(ch) = char::from_u32(scalar) {
                                        out.push(ch);
                                        continue;
                                    }
                                }
                                self.pos = save;
                            }
                            // A lone surrogate cannot live in a Rust string; the
                            // original would carry it, so this reader refuses.
                            out.push(char::from_u32(unit as u32)?);
                        }
                        _ => return None,
                    }
                }
                _ if b < 0x80 => out.push(b as char),
                _ => {
                    self.pos -= 1;
                    let ch = self.rest().chars().next()?;
                    self.pos += ch.len_utf8();
                    out.push(ch);
                }
            }
        }
        None
    }

    fn hex4(&mut self) -> Option<u16> {
        let hex = std::str::from_utf8(self.bytes.get(self.pos..self.pos + 4)?).ok()?;
        let unit = u16::from_str_radix(hex, 16).ok()?;
        self.pos += 4;
        Some(unit)
    }

    /// A number kept as its digits (the model only stores an `Int32`).
    fn integer(&mut self) -> Option<i64> {
        let start = self.pos;
        let neg = self.take(b'-');
        let digits = self.digits()?;
        if self.peek() == Some(b'.') || matches!(self.peek(), Some(b'e') | Some(b'E')) {
            self.pos = start;
            return None; // System.Text.Json refuses a fractional value for Int32.
        }
        let val = digits.parse::<i64>().ok()?;
        Some(if neg { -val } else { val })
    }

    fn digits(&mut self) -> Option<String> {
        let start = self.pos;
        while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
            self.pos += 1;
        }
        if self.pos == start {
            return None;
        }
        Some(std::str::from_utf8(&self.bytes[start..self.pos]).ok()?.into())
    }

    /// Skips any JSON value (used for unknown members).
    fn skip_value(&mut self) -> Option<()> {
        match self.peek()? {
            b'"' => self.string().map(|_| ()),
            b't' | b'f' | b'n' => {
                for word in ["true", "false", "null"] {
                    if self.rest().starts_with(word) {
                        self.pos += word.len();
                        return Some(());
                    }
                }
                None?
            }
            b'-' | b'0'..=b'9' => {
                let _ = self.digits();
                if self.take(b'.') {
                    self.digits()?;
                }
                if matches!(self.peek(), Some(b'e') | Some(b'E')) {
                    self.pos += 1;
                    if matches!(self.peek(), Some(b'+') | Some(b'-')) {
                        self.pos += 1;
                    }
                    self.digits()?;
                }
                Some(())
            }
            b'[' => {
                self.pos += 1;
                if self.take(b']') {
                    return Some(());
                }
                loop {
                    self.skip_value()?;
                    if self.take(b',') {
                        continue;
                    }
                    return if self.take(b']') { Some(()) } else { None };
                }
            }
            b'{' => {
                self.pos += 1;
                if self.take(b'}') {
                    return Some(());
                }
                loop {
                    self.string()?;
                    if !self.take(b':') {
                        return None;
                    }
                    self.skip_value()?;
                    if self.take(b',') {
                        continue;
                    }
                    return if self.take(b'}') { Some(()) } else { None };
                }
            }
            _ => None,
        }
    }
}

fn parse_timestamp(s: &str) -> Option<Timestamp> {
    let bytes = s.as_bytes();
    // yyyy-MM-ddTHH:mm:ss
    if bytes.len() < 19
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || (bytes[10] != b'T' && bytes[10] != b' ')
        || bytes[13] != b':'
        || bytes[16] != b':'
    {
        return None;
    }
    for (i, b) in bytes[..19].iter().enumerate() {
        match i {
            4 | 7 | 10 | 13 | 16 => {}
            _ if !b.is_ascii_digit() => return None,
            _ => {}
        }
    }
    let date_time = s[..19].to_string();
    let mut ticks = 0u32;
    let mut utc = false;
    let mut rest = &s[19..];
    if let Some(frac) = rest.strip_prefix('.') {
        let digits: String = frac.chars().take_while(|c| c.is_ascii_digit()).collect();
        if digits.is_empty() {
            return None;
        }
        // .NET keeps 100ns ticks: pad the first digits to seven; a fraction of
        // more than seven digits rounds in the original, and truncates here —
        // an input no settings file has carried yet.
        let mut padded: String = digits.chars().take(7).collect();
        while padded.len() < 7 {
            padded.push('0');
        }
        ticks = padded.parse().ok()?;
        rest = &frac[digits.len()..];
    }
    if rest == "Z" {
        utc = true;
        rest = "";
    }
    if !rest.is_empty() {
        return None; // offsets are not part of any settings file recorded so far
    }
    Some(Timestamp {
        date_time,
        ticks,
        utc,
    })
}

fn language_from_code(code: &str) -> String {
    // Mirror of Contracts.W3Language.FromCode; anything unknown falls back to English.
    static LANGS: &[(&str, &str)] = &[
        ("ar", "ar"),
        ("br", "pt"),
        ("cn", "zh-Hans"),
        ("cz", "cs"),
        ("de", "de"),
        ("en", "en"),
        ("es", "es"),
        ("esMX", "es-MX"),
        ("fr", "fr"),
        ("hu", "hu"),
        ("it", "it"),
        ("jp", "ja"),
        ("kr", "ko"),
        ("pl", "pl"),
        ("ru", "ru"),
        ("zh", "zh-Hant"),
        ("tr", "tr"),
        ("ua", "uk"),
    ];
    for (code_, culture) in LANGS {
        if code.eq_ignore_ascii_case(code_) || code.eq_ignore_ascii_case(culture) {
            return (*code_).into();
        }
    }
    "en".into()
}

/// The deserialization path: the JSON constructor's defaults, then the members
/// the object carries. Property names match case-sensitively.
pub fn parse(text: &str) -> Result<Settings, String> {
    let mut json = Json::new(text);
    let mut settings = Settings {
        game_exe_path: None, // the constructor param, default null
        language: String::new(),
        page_size: 30,
        preferred_language: "en".into(),
        preferred_encoding: PayloadEncoding::Utf16Le,
        preferred_file_type: FileType::Csv,
        translator: "MicrosoftTranslator".into(),
        recent_items: Vec::new(),
        backup_items: Vec::new(),
    };
    if !json.take(b'{') {
        return Err("The JSON value could not be converted to the settings model".into());
    }
    if json.take(b'}') {
        return Ok(settings);
    }
    loop {
        let key = json.string().ok_or_else(|| "bad JSON member".to_string())?;
        if !json.take(b':') {
            return Err("bad JSON member".into());
        }
        match key.as_str() {
            "GameExePath" => {
                if json.peek() == Some(b'n') && json.rest().starts_with("null") {
                    json.pos += 4;
                } else {
                    settings.game_exe_path =
                        Some(json.string().ok_or_else(|| "bad GameExePath".to_string())?);
                }
            }
            "Language" => {
                settings.language =
                    json.string().ok_or_else(|| "bad Language".to_string())?;
            }
            "PageSize" => {
                settings.page_size =
                    json.integer().ok_or_else(|| "bad PageSize".to_string())?;
            }
            "PreferredLanguage" => {
                let code = json.string().ok_or_else(|| "bad PreferredLanguage".to_string())?;
                settings.preferred_language = language_from_code(&code);
            }
            "PreferredW3StringsEncoding" => {
                let web = json.string().ok_or_else(|| "bad encoding".to_string())?;
                settings.preferred_encoding = if web.eq_ignore_ascii_case("utf-8") {
                    PayloadEncoding::Utf8
                } else {
                    PayloadEncoding::Utf16Le
                };
            }
            "PreferredFileType" => {
                let name = json.string().ok_or_else(|| "bad file type".to_string())?;
                settings.preferred_file_type = if name.eq_ignore_ascii_case("Csv") {
                    FileType::Csv
                } else if name.eq_ignore_ascii_case("W3Strings") {
                    FileType::W3Strings
                } else if name.eq_ignore_ascii_case("Excel") {
                    FileType::Excel
                } else {
                    return Err(
                        "The JSON value could not be converted to W3StringFileFormat".into(),
                    );
                };
            }
            "Translator" => {
                settings.translator = json
                    .string()
                    .ok_or_else(|| "bad Translator".to_string())?;
            }
            "RecentItems" => {
                settings.recent_items = read_recent_items(&mut json)?;
            }
            "BackupItems" => {
                settings.backup_items = read_backup_items(&mut json)?;
            }
            _ => json.skip_value().ok_or_else(|| "bad JSON value".to_string())?,
        }
        if json.take(b',') {
            continue;
        }
        return if json.take(b'}') {
            Ok(settings)
        } else {
            Err("bad JSON object".into())
        };
    }
}

fn read_recent_items(json: &mut Json<'_>) -> Result<Vec<RecentItem>, String> {
    if json.peek() == Some(b'n') && json.rest().starts_with("null") {
        json.pos += 4;
        return Ok(Vec::new());
    }
    if !json.take(b'[') {
        return Err("bad RecentItems".into());
    }
    let mut items = Vec::new();
    if json.take(b']') {
        return Ok(items);
    }
    loop {
        let mut file_path = None;
        let mut is_marked = false;
        let mut opened_time = None;
        if !json.take(b'{') {
            return Err("bad RecentItem".into());
        }
        if !json.take(b'}') {
            loop {
                let key = json.string().ok_or_else(|| "bad RecentItem".to_string())?;
                if !json.take(b':') {
                    return Err("bad RecentItem".into());
                }
                match key.as_str() {
                    "FilePath" => {
                        file_path = Some(json.string().ok_or_else(|| "bad FilePath".to_string())?)
                    }
                    "IsMarked" => {
                        is_marked = match json.peek() {
                            Some(b't') => {
                                json.pos += 4;
                                true
                            }
                            Some(b'f') => {
                                json.pos += 5;
                                false
                            }
                            _ => return Err("bad IsMarked".into()),
                        };
                    }
                    "OpenedTime" => {
                        let raw = json.string().ok_or_else(|| "bad OpenedTime".to_string())?;
                        opened_time =
                            Some(parse_timestamp(&raw).ok_or_else(|| "bad OpenedTime".to_string())?)
                    }
                    _ => json.skip_value().ok_or_else(|| "bad JSON value".to_string())?,
                }
                if json.take(b',') {
                    continue;
                }
                if json.take(b'}') {
                    break;
                }
                return Err("bad RecentItem".into());
            }
        }
        items.push(RecentItem {
            file_path: file_path.ok_or_else(|| "missing FilePath".to_string())?,
            is_marked,
            opened_time: opened_time.ok_or_else(|| "missing OpenedTime".to_string())?,
        });
        if json.take(b',') {
            continue;
        }
        return if json.take(b']') {
            Ok(items)
        } else {
            Err("bad RecentItems".into())
        };
    }
}

fn read_backup_items(json: &mut Json<'_>) -> Result<Vec<BackupItem>, String> {
    if json.peek() == Some(b'n') && json.rest().starts_with("null") {
        json.pos += 4;
        return Ok(Vec::new());
    }
    if !json.take(b'[') {
        return Err("bad BackupItems".into());
    }
    let mut items = Vec::new();
    if json.take(b']') {
        return Ok(items);
    }
    loop {
        let mut item = BackupItem {
            file_name: String::new(),
            hash: String::new(),
            orgin_path: String::new(),
            backup_path: String::new(),
            backup_time: Timestamp {
                date_time: String::new(),
                ticks: 0,
                utc: false,
            },
        };
        if !json.take(b'{') {
            return Err("bad BackupItem".into());
        }
        if !json.take(b'}') {
            loop {
                let key = json.string().ok_or_else(|| "bad BackupItem".to_string())?;
                if !json.take(b':') {
                    return Err("bad BackupItem".into());
                }
                match key.as_str() {
                    "FileName" => {
                        item.file_name =
                            json.string().ok_or_else(|| "bad FileName".to_string())?
                    }
                    "Hash" => item.hash = json.string().ok_or_else(|| "bad Hash".to_string())?,
                    "OrginPath" => {
                        item.orgin_path =
                            json.string().ok_or_else(|| "bad OrginPath".to_string())?
                    }
                    "BackupPath" => {
                        item.backup_path =
                            json.string().ok_or_else(|| "bad BackupPath".to_string())?
                    }
                    "BackupTime" => {
                        let raw = json.string().ok_or_else(|| "bad BackupTime".to_string())?;
                        item.backup_time =
                            parse_timestamp(&raw).ok_or_else(|| "bad BackupTime".to_string())?
                    }
                    _ => json.skip_value().ok_or_else(|| "bad JSON value".to_string())?,
                }
                if json.take(b',') {
                    continue;
                }
                if json.take(b'}') {
                    break;
                }
                return Err("bad BackupItem".into());
            }
        }
        items.push(item);
        if json.take(b',') {
            continue;
        }
        return if json.take(b']') {
            Ok(items)
        } else {
            Err("bad BackupItems".into())
        };
    }
}

// ---------------------------------------------------------------------------
// Writing (System.Text.Json indented, JavaScriptEncoder.Default)
// ---------------------------------------------------------------------------

/// The escape rule System.Text.Json's default encoder produces, pinned by the
/// recorded `settings-roundtrip-settings-tricky` case: uppercase hex, HTML
/// separators and delimiters escaped, non-ASCII written as UTF-16 units.
fn escape(s: &str, out: &mut String) {
    for unit in s.encode_utf16() {
        match unit {
            0x08 => out.push_str("\\b"),
            0x0C => out.push_str("\\f"),
            0x0A => out.push_str("\\n"),
            0x0D => out.push_str("\\r"),
            0x09 => out.push_str("\\t"),
            0x22 => out.push_str("\\\""),
            0x5C => out.push_str("\\\\"),
            0x3C => out.push_str("\\u003C"), // <
            0x3E => out.push_str("\\u003E"), // >
            0x26 => out.push_str("\\u0026"), // &
            0x27 => out.push_str("\\u0027"), // '
            0x2B => out.push_str("\\u002B"), // +
            0x60 => out.push_str("\\u0060"), // `
            c if !(0x20..0x7F).contains(&c) => {
                let _ = write!(out, "\\u{c:04X}");
            }
            c => {
                out.push(char::from_u32(c as u32).unwrap_or('\u{FFFD}'));
            }
        }
    }
}

fn timestamp_string(t: &Timestamp) -> String {
    let mut out = t.date_time.clone();
    if t.ticks > 0 {
        let mut fraction = format!("{:07}", t.ticks);
        while fraction.ends_with('0') {
            fraction.pop();
        }
        let _ = write!(&mut out, ".{fraction}");
    }
    if t.utc {
        out.push('Z');
    }
    out
}

fn file_type_name(t: &FileType) -> &'static str {
    match t {
        FileType::Csv => "Csv",
        FileType::W3Strings => "W3Strings",
        FileType::Excel => "Excel",
    }
}

fn encoding_web_name(e: &PayloadEncoding) -> &'static str {
    match e {
        PayloadEncoding::Utf8 => "utf-8",
        PayloadEncoding::Utf16Le => "utf-16",
    }
}

/// The serialized form, byte for byte as the original writes it: two-space
/// indentation, LF newlines, no trailing newline.
pub fn serialize(s: &Settings) -> String {
    let mut out = String::with_capacity(256);
    out.push_str("{\n");
    let _ = writeln!(
        out,
        "  \"GameExePath\": {},",
        match &s.game_exe_path {
            Some(path) => {
                let mut v = String::new();
                v.push('"');
                escape(path, &mut v);
                v.push('"');
                v
            }
            None => "null".into(),
        }
    );
    let _ = write!(out, "  \"Language\": ");
    out.push('"');
    escape(&s.language, &mut out);
    out.push_str("\",\n");
    let _ = writeln!(out, "  \"PageSize\": {},", s.page_size);
    let _ = write!(out, "  \"PreferredLanguage\": ");
    out.push('"');
    escape(&s.preferred_language, &mut out);
    out.push_str("\",\n");
    let _ = writeln!(
        out,
        "  \"PreferredW3StringsEncoding\": \"{}\",",
        encoding_web_name(&s.preferred_encoding)
    );
    let _ = writeln!(
        out,
        "  \"PreferredFileType\": \"{}\",",
        file_type_name(&s.preferred_file_type)
    );
    let _ = write!(out, "  \"Translator\": ");
    out.push('"');
    escape(&s.translator, &mut out);
    out.push_str("\",\n");
    if s.recent_items.is_empty() {
        out.push_str("  \"RecentItems\": [],\n");
    } else {
        out.push_str("  \"RecentItems\": [\n");
        for (i, item) in s.recent_items.iter().enumerate() {
            out.push_str("    {\n");
            let _ = write!(out, "      \"FilePath\": ");
            out.push('"');
            escape(&item.file_path, &mut out);
            out.push_str("\",\n");
            let _ = writeln!(out, "      \"IsMarked\": {},", item.is_marked);
            let _ = write!(out, "      \"OpenedTime\": ");
            out.push('"');
            escape(&timestamp_string(&item.opened_time), &mut out);
            out.push_str("\"\n");
            out.push_str(if i + 1 == s.recent_items.len() {
                "    }\n"
            } else {
                "    },\n"
            });
        }
        out.push_str("  ],\n");
    }
    if s.backup_items.is_empty() {
        out.push_str("  \"BackupItems\": []\n");
    } else {
        out.push_str("  \"BackupItems\": [\n");
        for (i, item) in s.backup_items.iter().enumerate() {
            out.push_str("    {\n");
            let _ = write!(out, "      \"FileName\": ");
            out.push('"');
            escape(&item.file_name, &mut out);
            out.push_str("\",\n");
            let _ = write!(out, "      \"Hash\": ");
            out.push('"');
            escape(&item.hash, &mut out);
            out.push_str("\",\n");
            let _ = write!(out, "      \"OrginPath\": ");
            out.push('"');
            escape(&item.orgin_path, &mut out);
            out.push_str("\",\n");
            let _ = write!(out, "      \"BackupPath\": ");
            out.push('"');
            escape(&item.backup_path, &mut out);
            out.push_str("\",\n");
            let _ = write!(out, "      \"BackupTime\": ");
            out.push('"');
            escape(&timestamp_string(&item.backup_time), &mut out);
            out.push_str("\"\n");
            out.push_str(if i + 1 == s.backup_items.len() {
                "    }\n"
            } else {
                "    },\n"
            });
        }
        out.push_str("  ]\n");
    }
    out.push('}');
    out
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_write_like_the_recorded_case() {
        let out = serialize(&default_settings());
        assert_eq!(
            out,
            "{\n  \"GameExePath\": \"\",\n  \"Language\": \"\",\n  \"PageSize\": 30,\n  \
             \"PreferredLanguage\": \"en\",\n  \"PreferredW3StringsEncoding\": \"utf-16\",\n  \
             \"PreferredFileType\": \"Csv\",\n  \"Translator\": \"MicrosoftTranslator\",\n  \
             \"RecentItems\": [],\n  \"BackupItems\": []\n}"
        );
    }

    #[test]
    fn null_game_exe_path_writes_null() {
        let settings = Settings {
            game_exe_path: None,
            ..default_settings()
        };
        assert!(serialize(&settings).starts_with("{\n  \"GameExePath\": null,"));
    }

    #[test]
    fn timestamps_trim_and_keep_seven_digits() {
        let t = parse_timestamp("2025-12-31T23:59:59.1234567").unwrap();
        assert_eq!(timestamp_string(&t), "2025-12-31T23:59:59.1234567");
        let t = parse_timestamp("2024-06-30T23:00:00.500").unwrap();
        assert_eq!(timestamp_string(&t), "2024-06-30T23:00:00.5");
        let t = parse_timestamp("2026-01-02T03:04:05").unwrap();
        assert_eq!(timestamp_string(&t), "2026-01-02T03:04:05");
        let t = parse_timestamp("2026-03-04T05:06:07.12345670").unwrap();
        assert_eq!(timestamp_string(&t), "2026-03-04T05:06:07.1234567");
    }

    #[test]
    fn unknown_language_falls_back_to_english() {
        let settings = parse(
            r#"{"PreferredLanguage": "xx-unknown", "PreferredFileType": "Csv"}"#,
        )
        .unwrap();
        assert_eq!(settings.preferred_language, "en");
    }

    #[test]
    fn unknown_encoding_falls_back_to_utf16() {
        let settings = parse(r#"{"PreferredW3StringsEncoding": "utf-32"}"#).unwrap();
        assert_eq!(settings.preferred_encoding, PayloadEncoding::Utf16Le);
        let settings = parse(r#"{"PreferredW3StringsEncoding": "UTF-8"}"#).unwrap();
        assert_eq!(settings.preferred_encoding, PayloadEncoding::Utf8);
    }

    #[test]
    fn tricky_strings_escape_like_the_encoder() {
        let settings = parse(r#"{"GameExePath": "a<b>c&d'e+f`g\"h\\i\u00E9\ud83d\udc3e"}"#)
            .unwrap();
        let out = serialize(&settings);
        assert!(out.contains(
            "\"GameExePath\": \"a\\u003Cb\\u003Ec\\u0026d\\u0027e\\u002Bf\\u0060g\\\"h\\\\i\\u00E9\\uD83D\\uDC3E\","
        ));
    }

    #[test]
    fn empty_object_takes_constructor_defaults() {
        let settings = parse("{}").unwrap();
        assert_eq!(
            settings,
            Settings {
                game_exe_path: None,
                ..default_settings()
            }
        );
    }

    #[test]
    fn null_collections_become_empty() {
        let settings = parse(r#"{"RecentItems": null, "BackupItems": null}"#).unwrap();
        assert!(settings.recent_items.is_empty());
        assert!(settings.backup_items.is_empty());
    }
}
