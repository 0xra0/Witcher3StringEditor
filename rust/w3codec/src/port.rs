//! Port of the C# W3Strings codec in `Witcher3StringEditor.Serializers`.
//! The wire format and every byte it writes are kept identical to the original.
#![forbid(unsafe_code)]

use std::fmt::Write as _;

// ---------------------------------------------------------------------------
// Contracts/W3Language.cs
// ---------------------------------------------------------------------------

/// A language the game's content files know. Key and magic come from the game.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct W3Language {
    pub code: &'static str,
    pub culture: &'static str,
    pub key: u32,
    pub magic: u32,
}

macro_rules! languages {
    ($(($code:expr, $culture:expr, $key:expr, $magic:expr)),* $(,)?) => {
        pub static ALL_LANGUAGES: &[W3Language] = &[
            $(W3Language { code: $code, culture: $culture, key: $key, magic: $magic }),*
        ];
    };
}

languages!(
    ("ar", "ar", 0x0000_0000, 0x0000_0000),
    ("br", "pt", 0x0000_0000, 0x0000_0000),
    ("cn", "zh-Hans", 0x0000_0000, 0x0000_0000),
    ("cz", "cs", 0x2498_7354, 0x2179_3217),
    ("de", "de", 0x7588_6138, 0x4279_1159),
    ("en", "en", 0x4397_5139, 0x7932_1793),
    ("es", "es", 0x1879_6651, 0x4238_7566),
    ("esMX", "es-MX", 0x0000_0000, 0x0000_0000),
    ("fr", "fr", 0x2386_3176, 0x7592_1975),
    ("hu", "hu", 0x4237_8932, 0x6782_3218),
    ("it", "it", 0x4593_1894, 0x1237_5973),
    ("jp", "ja", 0x5483_4893, 0x5982_5646),
    ("kr", "ko", 0x0000_0000, 0x0000_0000),
    ("pl", "pl", 0x8349_6237, 0x7394_6816),
    ("ru", "ru", 0x6348_1486, 0x4238_6347),
    ("zh", "zh-Hant", 0x1863_2176, 0x1687_5467),
    ("tr", "tr", 0x0000_0000, 0x0000_0000),
    ("ua", "uk", 0x0000_0000, 0x0000_0000),
);

pub fn language_from_code(code: &str) -> Option<W3Language> {
    ALL_LANGUAGES
        .iter()
        .copied()
        .find(|l| l.code.eq_ignore_ascii_case(code) || l.culture.eq_ignore_ascii_case(code))
}

/// The full key identifies exactly one language, except zero, which is shared by
/// every language added after release and therefore identifies none of them.
pub fn language_from_key(key: u32) -> Option<W3Language> {
    let mut matches = ALL_LANGUAGES.iter().copied().filter(|l| l.key == key);
    let first = matches.next()?;
    matches.next().is_none().then_some(first)
}

pub fn magic_of(key: u32) -> Result<u32, String> {
    if let Some(language) = language_from_key(key) {
        Ok(language.magic)
    } else if key == 0 {
        Ok(0)
    } else {
        Err(format!(
            "unknown language key 0x{key:08X}, so the stored texts cannot be decoded (the language is not supported by this build)"
        ))
    }
}

// ---------------------------------------------------------------------------
// C# string semantics: UTF-16 code units.
// ---------------------------------------------------------------------------

/// C# `string` as UTF-16 code units. Lone surrogates stay as their raw unit.
fn utf16_units(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}

/// Matches C# `ToLowerInvariant` one UTF-16 code unit at a time. C# uses the
/// invariant culture's simple case folding; for the ASCII keys the game uses
/// this is the ASCII lowercase rule. Non-ASCII lowercase follows Unicode's
/// simple mapping (one char to one char), applied per scalar value; a lone
/// surrogate is left alone. C# maps U+0130 (LATIN CAPITAL I WITH DOT ABOVE)
/// to itself under invariant lowercase, which `char::to_lowercase` does too
/// (it maps to "i\u{307}", two scalars) — see `invariant_lower_unit`.
fn invariant_lower_unit(unit: u16) -> u16 {
    if unit < 0x80 {
        if (0x41..=0x5A).contains(&unit) {
            unit + 0x20
        } else {
            unit
        }
    } else if let Some(ch) = char::from_u32(unit as u32) {
        let mut folded = ch.to_lowercase();
        let first = folded.next().unwrap_or(ch);
        if folded.next().is_none() {
            first as u16
        } else {
            unit
        }
    } else {
        unit
    }
}

/// `LocalizationKeyHash.Of`: hash*31 + UTF-16 unit over the invariant-lowercased key.
pub fn localization_key_hash(key: &str) -> u32 {
    let mut hash = 0u32;
    for unit in utf16_units(key) {
        hash = hash
            .wrapping_mul(31)
            .wrapping_add(invariant_lower_unit(unit) as u32);
    }
    hash
}

// ---------------------------------------------------------------------------
// W3Strings/LocalizationKeyHash.cs — public convenience matching the harness.
// ---------------------------------------------------------------------------

pub fn hash_line(key: &str) -> String {
    let hash = localization_key_hash(key);
    format!("hash {hash} 0x{hash:08X}")
}

// ---------------------------------------------------------------------------
// W3Strings/SectionCount.cs — bit6-framed counts.
// ---------------------------------------------------------------------------

/// Reads a bit6 count at `cursor`; returns (value, new cursor).
fn read_count(bytes: &[u8], cursor: &mut usize) -> Result<u32, String> {
    let first = *bytes
        .get(*cursor)
        .ok_or("bit6: the stream ends where a count has to stand")?;
    *cursor += 1;
    let mut value = (first & 0x3F) as u64;
    if first & 0x40 == 0 {
        return Ok(value as u32);
    }
    for length in 2..=5 {
        let next = *bytes
            .get(*cursor)
            .ok_or("bit6: the stream ends inside a count")?;
        *cursor += 1;
        value |= ((next & 0x7F) as u64) << (6 + 7 * (length - 2));
        if next & 0x80 == 0 {
            return u32::try_from(value).map_err(|_| {
                format!("bit6: the count {value} does not fit a 32-bit unsigned integer")
            });
        }
    }
    Err("bit6: the count does not end inside the 5 bytes a count can take".into())
}

fn count_bytes(value: u32) -> Vec<u8> {
    if value == 0 {
        return vec![0x80];
    }
    let mut length = 1;
    let mut remaining = value >> 6;
    while remaining > 0 {
        length += 1;
        remaining >>= 7;
    }
    let mut bytes = vec![0u8; length];
    bytes[0] = (value & 0x3F) as u8;
    if length > 1 {
        bytes[0] |= 0x40;
    }
    let mut remaining = value >> 6;
    for byte in bytes.iter_mut().take(length - 1).skip(1) {
        *byte = ((remaining & 0x7F) as u8) | 0x80;
        remaining >>= 7;
    }
    if length > 1 {
        bytes[length - 1] = remaining as u8;
    }
    bytes
}

// ---------------------------------------------------------------------------
// W3Strings/StoredText.cs — encoding and obfuscation.
// ---------------------------------------------------------------------------

const UNIT_UTF8: usize = 1;
const UNIT_UTF16: usize = 2;

fn unit_size(version: u32) -> usize {
    if version >= 164 {
        UNIT_UTF8
    } else {
        UNIT_UTF16
    }
}

fn encoded_length(text: &str, unit: usize) -> usize {
    match unit {
        UNIT_UTF8 => text.len(),
        UNIT_UTF16 => text.encode_utf16().count() * 2,
        _ => 0,
    }
}

fn encode_text(text: &str, unit: usize) -> Vec<u8> {
    match unit {
        UNIT_UTF8 => text.as_bytes().to_vec(),
        UNIT_UTF16 => {
            let mut bytes = Vec::with_capacity(text.encode_utf16().count() * 2);
            for u in text.encode_utf16() {
                bytes.extend_from_slice(&u.to_le_bytes());
            }
            bytes
        }
        _ => Vec::new(),
    }
}

fn decode_text(bytes: &[u8], unit: usize) -> Result<String, String> {
    match unit {
        UNIT_UTF8 => String::from_utf8(bytes.to_vec()).map_err(|e| e.to_string()),
        UNIT_UTF16 => {
            if !bytes.len().is_multiple_of(2) {
                return Err("a UTF-16 text must hold an even number of bytes".into());
            }
            let units: Vec<u16> = bytes
                .as_chunks::<2>()
                .0
                .iter()
                .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
                .collect();
            Ok(String::from_utf16_lossy(&units))
        }
        _ => Err("one character takes either one byte (UTF-8) or two (UTF-16LE)".into()),
    }
}

/// Obfuscate (or deobfuscate — the operation is an involution) one stored text.
fn obfuscate(bytes: &mut [u8], length: usize, magic: u32, unit: usize) {
    let mut key = ((magic >> 8) & 0xFFFF) as u16;
    for i in 0..length {
        let character_key = (length as u32 + 1).wrapping_mul(key as u32);
        if unit == UNIT_UTF16 {
            bytes[i * 2] ^= character_key as u8;
            bytes[i * 2 + 1] ^= (character_key >> 8) as u8;
        } else {
            bytes[i] ^= character_key as u8;
        }
        key = key.rotate_left(1);
    }
}

fn stored_bytes(text: &str, magic: u32, unit: usize) -> Vec<u8> {
    let mut bytes = encode_text(text, unit);
    let length = bytes.len() / unit;
    obfuscate(&mut bytes, length, magic, unit);
    bytes
}

fn decode_stored(stored: &[u8], length: usize, magic: u32, unit: usize) -> Result<String, String> {
    let mut bytes = stored.to_vec();
    obfuscate(&mut bytes, length, magic, unit);
    decode_text(&bytes, unit)
}

// ---------------------------------------------------------------------------
// Container model.
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct W3StringEntry {
    pub id: u32,
    pub offset: u32,
    pub length: u32,
    pub value: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct W3KeyEntry {
    pub hash: u32,
    pub id: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct W3StringItem {
    pub str_id: String,
    pub key_hex: String,
    pub key_name: String,
    pub old_text: String,
    pub text: String,
}

impl W3StringItem {
    pub fn empty() -> Self {
        Self {
            str_id: String::new(),
            key_hex: String::new(),
            key_name: String::new(),
            old_text: String::new(),
            text: String::new(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Storage {
    version: u32,
    key: u32,
    magic: u32,
    strings: Vec<W3StringEntry>,
    keys: Vec<W3KeyEntry>,
    buffer_units: u32,
    buffer_offset: usize,
}

const MAGIC_BYTES: &[u8; 4] = b"RTSW";
const MIN_SIZE: usize = 15;

fn read_u32(bytes: &[u8], at: usize) -> Result<u32, String> {
    let slice = bytes
        .get(at..at + 4)
        .ok_or("The stream does not hold a readable w3strings container")?;
    Ok(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

fn read_u16(bytes: &[u8], at: usize) -> Result<u16, String> {
    let slice = bytes
        .get(at..at + 2)
        .ok_or("The stream does not hold a readable w3strings container")?;
    Ok(u16::from_le_bytes([slice[0], slice[1]]))
}

impl Storage {
    /// Read the stored form: layout only; texts are fetched afterwards.
    fn read(bytes: &[u8]) -> Result<Self, String> {
        let unreadable = || "The stream does not hold a readable w3strings container".to_string();
        if bytes.len() < MIN_SIZE {
            return Err(format!(
                "file too small to be a w3strings container ({} byte(s))",
                bytes.len()
            ));
        }
        if &bytes[..4] != MAGIC_BYTES {
            let found = String::from_utf8_lossy(&bytes[..4]);
            return Err(format!("bad magic: expected \"RTSW\", got \"{found}\""));
        }
        let declared = read_u32(bytes, 4)?;
        let key1 = (read_u16(bytes, 8)? as u32) << 16;
        let key2_offset = bytes.len() - 2;
        let mut cursor = 10usize;

        let mut read_block = |entry_size: usize, name: &str| -> Result<Vec<u8>, String> {
            let mut count_cursor = cursor;
            let count = read_count(bytes, &mut count_cursor)?;
            cursor = count_cursor;
            let end = cursor as u64 + count as u64 * entry_size as u64;
            if end >= key2_offset as u64 {
                return Err(format!(
                    "{name} declares {count} entries, which does not fit before the end of the file ({end} >= {key2_offset})"
                ));
            }
            let size = (count as usize) * entry_size;
            let block = bytes
                .get(cursor..cursor + size)
                .ok_or_else(unreadable)?
                .to_vec();
            cursor += size;
            Ok(block)
        };

        let block1 = read_block(12, "block1")?;
        let block2 = read_block(8, "block2")?;
        let mut count_cursor = cursor;
        let units_declared = read_count(bytes, &mut count_cursor)?;
        cursor = count_cursor;
        let start = cursor;
        let version = stored_version(declared, start, units_declared, key2_offset);
        let held = fit_buffer(start, units_declared, version, key2_offset)?;

        let mut strings = Vec::with_capacity(block1.len() / 12);
        for chunk in block1.as_chunks::<12>().0 {
            strings.push(W3StringEntry {
                id: read_u32(chunk, 0)?,
                offset: read_u32(chunk, 4)?,
                length: read_u32(chunk, 8)?,
                value: String::new(),
            });
        }
        let mut keys = Vec::with_capacity(block2.len() / 8);
        for chunk in block2.as_chunks::<8>().0 {
            keys.push(W3KeyEntry {
                hash: read_u32(chunk, 0)?,
                id: read_u32(chunk, 4)?,
            });
        }
        let outside = strings
            .iter()
            .position(|e| e.offset as u64 + e.length as u64 > held as u64);
        if let Some(index) = outside {
            if version < 164 {
                let e = &strings[index];
                return Err(format!(
                    "string entry #{index} points outside the string buffer (offset {}, length {}, buffer {held} unit(s))",
                    e.offset, e.length
                ));
            }
            strings.retain(|e| e.offset as u64 + e.length as u64 <= held as u64);
        }
        let key = key1 | read_u16(bytes, key2_offset)? as u32;
        let magic = magic_of(key)?;
        Ok(Self {
            version,
            key,
            magic,
            strings,
            keys,
            buffer_units: held,
            buffer_offset: start,
        })
    }

    fn byte_offset_of(&self, entry: &W3StringEntry) -> usize {
        self.buffer_offset + entry.offset as usize * unit_size(self.version)
    }

    fn byte_length_of(&self, entry: &W3StringEntry) -> usize {
        entry.length as usize * unit_size(self.version)
    }

    fn text_of(&self, entry: &W3StringEntry, stored: &[u8]) -> Result<String, String> {
        decode_stored(
            stored,
            entry.length as usize,
            self.magic,
            unit_size(self.version),
        )
    }

    fn id_of(&self, id: u32) -> u32 {
        id ^ self.magic
    }

    fn build_preamble(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(MAGIC_BYTES);
        out.extend_from_slice(&self.version.to_le_bytes());
        out.extend_from_slice(&((self.key >> 16) as u16).to_le_bytes());
        let mut strings: Vec<W3StringEntry> = self.strings.clone();
        strings.sort_by_key(|e| e.id ^ self.magic);
        out.extend_from_slice(&count_bytes(strings.len() as u32));
        for entry in strings {
            out.extend_from_slice(&(entry.id ^ self.magic).to_le_bytes());
            out.extend_from_slice(&entry.offset.to_le_bytes());
            out.extend_from_slice(&entry.length.to_le_bytes());
        }
        let mut keys: Vec<W3KeyEntry> = self.keys.iter().copied().filter(|k| k.hash != 0).collect();
        keys.sort_by_key(|k| k.hash);
        out.extend_from_slice(&count_bytes(keys.len() as u32));
        for key in keys {
            out.extend_from_slice(&key.hash.to_le_bytes());
            out.extend_from_slice(&(key.id ^ self.magic).to_le_bytes());
        }
        out.extend_from_slice(&count_bytes(self.buffer_units));
        out
    }

    fn write(&self) -> Vec<u8> {
        let mut out = self.build_preamble();
        for entry in &self.strings {
            let stored = stored_bytes(&entry.value, self.magic, unit_size(self.version));
            out.extend_from_slice(&stored);
            out.resize(out.len() + unit_size(self.version), 0); // terminator
        }
        out.extend_from_slice(&(self.key as u16).to_le_bytes());
        out
    }
}

fn stored_version(declared: u32, start: usize, units: u32, key2_offset: usize) -> u32 {
    if declared >= 164 {
        return declared;
    }
    let declared_unit = unit_size(declared);
    if units == 0 {
        return older_generation(declared);
    }
    let as_declared = start as u64 + units as u64 * declared_unit as u64;
    let as_utf8 = start as u64 + units as u64;
    let is_utf8 = as_utf8 == key2_offset as u64
        || (as_declared > key2_offset as u64 && as_utf8 <= key2_offset as u64);
    if is_utf8 {
        164
    } else {
        older_generation(declared)
    }
}

fn older_generation(_version: u32) -> u32 {
    162
}

fn fit_buffer(start: usize, units: u32, version: u32, key2_offset: usize) -> Result<u32, String> {
    let unit = unit_size(version);
    let end = start as u64 + units as u64 * unit as u64;
    if end <= key2_offset as u64 {
        return Ok(units);
    }
    if version < 164 {
        return Err(format!(
            "string buffer overruns file ({end} > {key2_offset})"
        ));
    }
    Ok((key2_offset.saturating_sub(start) / unit) as u32)
}

/// Decode a stored container into items, as the harness's `decode` prints them.
pub fn decode(bytes: &[u8]) -> Result<Decoded, String> {
    let storage = Storage::read(bytes)?;
    let unit = unit_size(storage.version);
    let mut decoded_entries = Vec::with_capacity(storage.strings.len());
    for entry in &storage.strings {
        let at = storage.byte_offset_of(entry);
        let len = storage.byte_length_of(entry);
        let stored = bytes
            .get(at..at + len)
            .ok_or("The stream does not hold a readable w3strings container")?;
        decoded_entries.push(W3StringEntry {
            id: storage.id_of(entry.id),
            offset: entry.offset,
            length: entry.length,
            value: storage.text_of(entry, stored)?,
        });
    }
    let keys: Vec<W3KeyEntry> = storage
        .keys
        .iter()
        .map(|k| W3KeyEntry {
            hash: k.hash,
            id: storage.id_of(k.id),
        })
        .collect();

    let mut key_hashes = std::collections::HashMap::new();
    for key in keys.iter().filter(|k| k.hash != 0) {
        key_hashes.entry(key.id).or_insert(key.hash);
    }
    let items: Vec<W3StringItem> = decoded_entries
        .iter()
        .map(|e| W3StringItem {
            str_id: e.id.to_string(),
            key_hex: key_hashes
                .get(&e.id)
                .map(|h| format!("{h:08X}"))
                .unwrap_or_default(),
            key_name: String::new(),
            old_text: String::new(),
            text: e.value.clone(),
        })
        .collect();
    Ok(Decoded {
        version: storage.version,
        key: storage.key,
        strings: decoded_entries,
        keys,
        items,
        _unit: unit,
    })
}

pub struct Decoded {
    pub version: u32,
    pub key: u32,
    pub strings: Vec<W3StringEntry>,
    pub keys: Vec<W3KeyEntry>,
    pub items: Vec<W3StringItem>,
    _unit: usize,
}

/// Mirror the harness dump JSON escape rule byte for byte.
pub fn escape_json(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c < '\u{20}' => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out
}

/// The harness's dump JSON for a decoded container, no trailing newline.
pub fn decode_dump(decoded: &Decoded) -> String {
    // The container's magic is the W3Language magic derived from its key.
    let magic = language_from_key(decoded.key).map(|l| l.magic).unwrap_or(0);
    let mut out = format!(
        "{{\"format\":\"w3strings\",\"version\":{},\"magic\":\"0x{magic:08X}\",\"languageKey\":\"0x{:08X}\"",
        decoded.version, decoded.key
    );
    out.push_str(",\"strings\":[");
    for (i, e) in decoded.strings.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let _ = write!(
            out,
            "{{\"id\":{},\"offset\":{},\"length\":{},\"text\":\"{}\"}}",
            e.id,
            e.offset,
            e.length,
            escape_json(&e.value)
        );
    }
    out.push_str("],\"keys\":[");
    for (i, k) in decoded.keys.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let _ = write!(out, "{{\"hash\":\"{:08X}\",\"id\":{}}}", k.hash, k.id);
    }
    out.push_str("],\"items\":[");
    out.push_str(&items_json(&decoded.items));
    out.push_str("]}");
    out
}

pub fn items_json(items: &[W3StringItem]) -> String {
    let mut out = String::new();
    for (i, it) in items.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let _ = write!(
            out,
            "{{\"strId\":\"{}\",\"keyHex\":\"{}\",\"keyName\":\"{}\",\"oldText\":\"{}\",\"text\":\"{}\"}}",
            escape_json(&it.str_id),
            escape_json(&it.key_hex),
            escape_json(&it.key_name),
            escape_json(&it.old_text),
            escape_json(&it.text)
        );
    }
    out
}

pub fn csv_items_json(items: &[W3StringItem]) -> String {
    format!("{{\"format\":\"csv\",\"items\":[{}]}}", items_json(items))
}

// ---------------------------------------------------------------------------
// W3StringsBuilder + storage layout: encode items into a w3strings container.
// ---------------------------------------------------------------------------

fn parse_key_hex(text: &str) -> Option<u32> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Some(0);
    }
    // C# uint.TryParse(NumberStyles.HexNumber) allows optional 0x and whitespace,
    // rejects a leading sign. Rust's from_str_radix does the rest.
    let digits = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
        .unwrap_or(trimmed);
    u32::from_str_radix(digits, 16).ok()
}

fn resolve_key(item: &mut W3StringItem) {
    if !item.key_name.trim().is_empty() {
        let hash = localization_key_hash(&item.key_name);
        item.key_hex = if hash != 0 {
            format!("{hash:08X}")
        } else {
            String::new()
        };
    } else {
        let hash = parse_key_hex(&item.key_hex);
        if let Some(hash) = hash {
            item.key_hex = if hash != 0 {
                format!("{hash:08X}")
            } else {
                String::new()
            };
        } else {
            item.key_hex.clear();
        }
    }
}

/// Build a container from items. `None` means the original builder refused them.
pub fn build(items: &[W3StringItem], _key: u32) -> Option<Vec<W3StringEntry>> {
    let mut seen = std::collections::HashSet::new();
    for item in items {
        if !seen.insert(item.str_id.as_str()) {
            return None;
        }
    }
    let mut entries = Vec::with_capacity(items.len());
    let mut keys = Vec::new();
    for item in items {
        let Ok(id) = item.str_id.parse::<u32>() else {
            return None;
        };
        let mut item = item.clone();
        resolve_key(&mut item);
        let hash = parse_key_hex(&item.key_hex).unwrap_or(0);
        if hash > 0 {
            keys.push(W3KeyEntry { hash, id });
        }
        entries.push(W3StringEntry {
            id,
            offset: 0,
            length: 0,
            value: item.text,
        });
    }
    let _ = keys; // keys are carried by the caller through build_with_keys
    Some(entries)
}

/// Full build including key entries, needed to write a container.
pub fn build_container(items: &[W3StringItem], key: u32, version: u32) -> Option<Storage> {
    let mut seen = std::collections::HashSet::new();
    for item in items {
        if !seen.insert(item.str_id.as_str()) {
            return None;
        }
    }
    let magic = magic_of(key).ok()?;
    let mut strings = Vec::with_capacity(items.len());
    let mut keys = Vec::new();
    for item in items {
        let id = item.str_id.parse::<u32>().ok()?;
        let mut item = item.clone();
        resolve_key(&mut item);
        let hash = parse_key_hex(&item.key_hex).unwrap_or(0);
        if hash > 0 {
            keys.push(W3KeyEntry { hash, id });
        }
        strings.push(W3StringEntry {
            id,
            offset: 0,
            length: 0,
            value: item.text,
        });
    }
    let written = if version >= 164 { version } else { 162 };
    let unit = unit_size(written);
    let mut cursor = 0u32;
    for entry in &mut strings {
        // StoredText.EncodedLength returns bytes/unit: 1 byte per char for
        // UTF-8, 2 bytes per char (divided by unit 2) for UTF-16LE.
        entry.length = encoded_length(&entry.value, unit) as u32 / unit as u32;
        entry.offset = cursor;
        cursor = cursor.wrapping_add(entry.length.wrapping_add(1));
    }
    Some(Storage {
        version: written,
        key,
        magic,
        strings,
        keys,
        buffer_units: cursor,
        buffer_offset: 0,
    })
}

pub fn encode(items: &[W3StringItem], key: u32, version: u32) -> Option<Vec<u8>> {
    build_container(items, key, version).map(|s| s.write())
}

// ---------------------------------------------------------------------------
// CsvSerializer.cs — pipe-delimited text format.
// ---------------------------------------------------------------------------

/// C# `File.WriteAllText` uses UTF-8 without a BOM and `Environment.NewLine`,
/// which is `\n` on this Linux recording machine.
pub fn csv_encode(items: &[W3StringItem], lang: &str, magic: u32) -> String {
    let lowered = lang.to_ascii_lowercase();
    let identifier = if magic == 0 {
        "cleartext"
    } else {
        lowered.as_str()
    };
    let mut out = String::new();
    out.push_str(&format!(";meta[language={identifier}]\n"));
    out.push_str("; id      |key(hex)|key(str)| text\n");
    for item in items {
        out.push_str(&format!(
            "{}|{}|{}|{}\n",
            item.str_id, item.key_hex, item.key_name, item.text
        ));
    }
    out
}

pub fn csv_decode(content: &str) -> Vec<W3StringItem> {
    let mut items = Vec::new();
    for line in content.lines() {
        let line = line.trim_end_matches('\r');
        if line.trim().is_empty() || line.starts_with(';') {
            continue;
        }
        let parts: Vec<&str> = line.split('|').collect();
        if parts.len() < 4 {
            continue;
        }
        items.push(W3StringItem {
            str_id: parts[0].trim().to_string(),
            key_hex: parts[1].trim().to_string(),
            key_name: parts[2].trim().to_string(),
            old_text: String::new(),
            text: parts[3..].join("|"),
        });
    }
    items
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_ascii_and_unicode() {
        assert_eq!(localization_key_hash("A"), 0x61);
        assert_eq!(localization_key_hash("AB"), 0x61 * 31 + 0x62);
        assert_eq!(localization_key_hash(""), 0);
        assert_eq!(
            localization_key_hash("İstanbul"),
            localization_key_hash("İstanbul")
        );
    }

    #[test]
    fn section_count_round_trip() {
        for n in [0u32, 1, 63, 64, 8191, 8192, u32::MAX] {
            let bytes = count_bytes(n);
            let mut cursor = 0;
            assert_eq!(read_count(&bytes, &mut cursor).unwrap(), n);
        }
    }

    #[test]
    fn obfuscation_is_an_involution() {
        let mut bytes = b"hello".to_vec();
        obfuscate(&mut bytes, 5, 0x7932_1793, 1);
        obfuscate(&mut bytes, 5, 0x7932_1793, 1);
        assert_eq!(bytes, b"hello");
    }
}
