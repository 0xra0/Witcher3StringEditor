//! Glue from the front door to `w3codec-rs`: parse harness arguments,
//! call `w3codec_rs::port`, print what the original prints, return its exit code.

use std::ffi::OsString;
use std::fs;
use std::path::Path;
use std::process::ExitCode;

use clap::ArgMatches;

use w3codec_rs::port as codec;

// --- minimal JSON reader for the harness's item-input interchange ------------

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
            // System.Text.Json with JsonCommentHandling.Skip accepts // and /* */ comments.
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

    fn take(&mut self, ch: u8) -> bool {
        self.ws();
        if self.bytes.get(self.pos) == Some(&ch) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn string(&mut self) -> Option<String> {
        self.ws();
        if self.bytes.get(self.pos) != Some(&b'"') {
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
                            let hex = std::str::from_utf8(self.bytes.get(self.pos..self.pos + 4)?)
                                .ok()?;
                            let unit = u16::from_str_radix(hex, 16).ok()?;
                            self.pos += 4;
                            if (0xD800..=0xDBFF).contains(&unit)
                                && self.bytes.get(self.pos..self.pos + 2) == Some(b"\\u")
                            {
                                let hex2 = std::str::from_utf8(
                                    self.bytes.get(self.pos + 2..self.pos + 6)?,
                                )
                                .ok()?;
                                let low = u16::from_str_radix(hex2, 16).ok()?;
                                if (0xDC00..=0xDFFF).contains(&low) {
                                    self.pos += 6;
                                    let scalar = 0x10000
                                        + (((unit as u32 - 0xD800) << 10) | (low as u32 - 0xDC00));
                                    if let Some(ch) = char::from_u32(scalar) {
                                        out.push(ch);
                                    }
                                    continue;
                                }
                            }
                            out.push(char::from_u32(unit as u32).unwrap_or('\u{FFFD}'));
                        }
                        _ => return None,
                    }
                }
                _ if b < 0x80 => out.push(b as char),
                _ => {
                    // multi-byte UTF-8: take the full sequence
                    self.pos -= 1;
                    let ch = self.rest().chars().next()?;
                    self.pos += ch.len_utf8();
                    out.push(ch);
                }
            }
        }
        None
    }

    fn items(&mut self) -> Option<Vec<codec::W3StringItem>> {
        if !self.take(b'[') {
            return None;
        }
        let mut items = Vec::new();
        if self.take(b']') {
            return Some(items);
        }
        loop {
            if !self.take(b'{') {
                return None;
            }
            let mut item = codec::W3StringItem::empty();
            loop {
                let key = self.string()?;
                if !self.take(b':') {
                    return None;
                }
                let value = self.string().unwrap_or_default();
                if key.eq_ignore_ascii_case("strId") {
                    item.str_id = value;
                } else if key.eq_ignore_ascii_case("keyHex") {
                    item.key_hex = value;
                } else if key.eq_ignore_ascii_case("keyName") {
                    item.key_name = value;
                } else if key.eq_ignore_ascii_case("oldText") {
                    item.old_text = value;
                } else if key.eq_ignore_ascii_case("text") {
                    item.text = value;
                }
                if self.take(b',') {
                    // Allow a trailing comma before the closing brace.
                    self.ws();
                    if self.bytes.get(self.pos) == Some(&b'}') {
                        self.pos += 1;
                        break;
                    }
                    continue;
                }
                if self.take(b'}') {
                    break;
                }
                return None;
            }
            items.push(item);
            if self.take(b',') {
                self.ws();
                if self.bytes.get(self.pos) == Some(&b']') {
                    self.pos += 1;
                    break;
                }
                continue;
            }
            if self.take(b']') {
                break;
            }
            return None;
        }
        Some(items)
    }
}

fn read_file(path: &str) -> Result<Vec<u8>, String> {
    if path.is_empty() {
        return Err("The value cannot be an empty string. (Parameter 'path')".to_string());
    }
    match fs::read(path) {
        Ok(b) => Ok(b),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let full = std::env::current_dir()
                .map(|cwd| cwd.join(path))
                .unwrap_or_else(|_| Path::new(path).to_path_buf());
            Err(format!("Could not find file '{}'.", full.display()))
        }
        Err(e) => Err(e.to_string()),
    }
}

fn read_to_string(path: &str) -> Result<String, String> {
    if path.is_empty() {
        return Err("The value cannot be an empty string. (Parameter 'path')".to_string());
    }
    match fs::read_to_string(path) {
        Ok(s) => Ok(s),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let full = std::env::current_dir()
                .map(|cwd| cwd.join(path))
                .unwrap_or_else(|_| Path::new(path).to_path_buf());
            Err(format!("Could not find file '{}'.", full.display()))
        }
        Err(e) => Err(e.to_string()),
    }
}

fn check_dir(path: &str) -> Result<(), String> {
    if path.is_empty() {
        return Err("The value cannot be an empty string. (Parameter 'path')".to_string());
    }
    fs::create_dir_all(path).map_err(|e| e.to_string())
}

fn parse_items(path: &str) -> Result<Vec<codec::W3StringItem>, String> {
    let text = read_to_string(path)?;
    Json::new(&text)
        .items()
        .ok_or_else(|| "no items in input".to_string())
}

fn error(message: &str) -> ExitCode {
    println!("error: {message}");
    ExitCode::from(3)
}

fn write(path: &str, content: &[u8]) -> Result<(), String> {
    if path.is_empty() {
        return Err("The value cannot be an empty string. (Parameter 'path')".to_string());
    }
    if let Some(parent) = Path::new(path).parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(path, content).map_err(|e| e.to_string())
}

pub fn run(command: &str, matches: &ArgMatches) -> ExitCode {
    let args: Vec<String> = matches
        .get_many::<OsString>("args")
        .into_iter()
        .flatten()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    match command {
        "hash" => {
            println!(
                "{}",
                codec::hash_line(args.first().map(String::as_str).unwrap_or(""))
            );
            ExitCode::SUCCESS
        }
        "decode" => {
            let bytes = match read_file(&args[0]) {
                Ok(b) => b,
                Err(e) => return error(&e),
            };
            match codec::decode(&bytes) {
                Ok(decoded) => {
                    let dump = codec::decode_dump(&decoded);
                    match write(&args[1], dump.as_bytes()) {
                        Ok(()) => {
                            println!("ok {}", decoded.items.len());
                            ExitCode::SUCCESS
                        }
                        Err(e) => error(&e),
                    }
                }
                Err(e) => error(&e),
            }
        }
        "encode" => {
            let items = match parse_items(&args[0]) {
                Ok(i) => i,
                Err(e) => return error(&e),
            };
            let Some(lang) = codec::language_from_code(&args[2]) else {
                return error(&format!("unknown language code {}", args[2]));
            };
            let version = match args[3].as_str() {
                "utf8" => 164,
                "utf16" => 162,
                other => return error(&format!("unknown encoding {other}")),
            };
            if let Err(e) = check_dir(&args[1]) {
                return error(&e);
            }
            match codec::encode(&items, lang.key, version) {
                Some(bytes) => {
                    let count = items.len();
                    if let Err(e) = write(
                        &format!("{}/{}.w3strings", args[1], lang.code.to_ascii_lowercase()),
                        &bytes,
                    ) {
                        return error(&e);
                    }
                    println!("ok {count}");
                    ExitCode::SUCCESS
                }
                None => {
                    println!("fail");
                    ExitCode::from(1)
                }
            }
        }
        "csv-decode" => {
            let content = match read_to_string(&args[0]) {
                Ok(c) => c,
                Err(e) => return error(&e),
            };
            let items = codec::csv_decode(&content);
            let dump = codec::csv_items_json(&items);
            match write(&args[1], dump.as_bytes()) {
                Ok(()) => {
                    println!("ok {}", items.len());
                    ExitCode::SUCCESS
                }
                Err(e) => error(&e),
            }
        }
        "csv-encode" => {
            let items = match parse_items(&args[0]) {
                Ok(i) => i,
                Err(e) => return error(&e),
            };
            let Some(lang) = codec::language_from_code(&args[2]) else {
                return error(&format!("unknown language code {}", args[2]));
            };
            if let Err(e) = check_dir(&args[1]) {
                return error(&e);
            }
            let content = codec::csv_encode(&items, lang.code, lang.magic);
            if let Err(e) = write(
                &format!("{}/{}.csv", args[1], lang.code.to_ascii_lowercase()),
                content.as_bytes(),
            ) {
                return error(&e);
            }
            println!("ok {}", items.len());
            ExitCode::SUCCESS
        }
        _ => crate::original::run(&[]),
    }
}
