//! Port of Witcher3StringEditor.Dictionary.
//! Manages custom format dictionary files and dynamic term replacement.
#![forbid(unsafe_code)]

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use aho_corasick::{AhoCorasick, MatchKind};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DictionaryInfo {
    pub path: PathBuf,
    pub name: String,
    pub note: String,
    pub source_language: String,
    pub target_language: String,
    pub term_count: usize,
}

pub fn map_w3_lang(code: &str) -> String {
    match code.to_ascii_uppercase().as_str() {
        "JP" => "ja".into(),
        "KR" => "ko".into(),
        "CN" => "zh-Hans".into(),
        "ZH" => "zh-Hant".into(),
        "BR" => "pt-BR".into(),
        "ESMX" => "es-MX".into(),
        other => other.to_ascii_lowercase(),
    }
}

pub fn parse_dictionary_info(path: &Path, content: &str) -> Result<DictionaryInfo, String> {
    let mut lines = content.lines().map(str::trim);
    let header = lines
        .next()
        .ok_or_else(|| format!("Dictionary file is empty: {}", path.display()))?;

    if !header.starts_with(';') {
        return Err(format!("Missing header (must start with ';'): {}", path.display()));
    }

    let trimmed = header.trim_start_matches(';');
    let parts: Vec<&str> = trimmed.splitn(3, '|').collect();
    if parts.len() < 3 {
        return Err(format!("Invalid header format (expected ';Name|Source|Target'): {}", path.display()));
    }

    let mut name = parts[0].trim().to_string();
    if name.is_empty() {
        name = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Dictionary".into());
    }
    let source_language = map_w3_lang(parts[1].trim());
    let target_language = map_w3_lang(parts[2].trim());

    let mut note = String::new();
    let mut term_count = 0;

    for line in lines {
        if line.is_empty() {
            continue;
        }
        if line.starts_with(';') {
            let note_part = line.trim_start_matches(';').trim();
            if let Some(rest) = note_part.strip_prefix("Note|") {
                note = rest.trim().to_string();
            } else if note.is_empty() {
                note = note_part.to_string();
            }
        } else if line.contains('|') {
            term_count += 1;
        }
    }

    Ok(DictionaryInfo {
        path: path.to_path_buf(),
        name,
        note,
        source_language,
        target_language,
        term_count,
    })
}

pub fn parse_entries(content: &str) -> Vec<(String, String)> {
    let mut entries = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with(';') {
            continue;
        }
        if let Some((k, v)) = trimmed.split_once('|') {
            let key = k.trim().to_string();
            let value = v.trim().to_string();
            if !key.is_empty() && !value.is_empty() {
                entries.push((key, value));
            }
        }
    }
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    entries
}

fn html_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

pub struct AcDynamicDictionaryReplacer {
    current_dict: Option<DictionaryInfo>,
    entries: HashMap<String, String>,
    pattern_keys: Vec<String>,
    matcher: Option<AhoCorasick>,
}

impl Default for AcDynamicDictionaryReplacer {
    fn default() -> Self {
        Self::new()
    }
}

impl AcDynamicDictionaryReplacer {
    pub fn new() -> Self {
        Self {
            current_dict: None,
            entries: HashMap::new(),
            pattern_keys: Vec::new(),
            matcher: None,
        }
    }

    pub fn is_ready(&self) -> bool {
        self.matcher.is_some() && !self.entries.is_empty()
    }

    pub fn current_dictionary(&self) -> Option<&DictionaryInfo> {
        self.current_dict.as_ref()
    }

    pub fn bind(&mut self, dict: DictionaryInfo, entries: Vec<(String, String)>) -> Result<(), String> {
        let mut map = HashMap::new();
        let mut patterns = Vec::new();

        for (k, v) in entries {
            let lower = k.to_ascii_lowercase();
            map.insert(lower.clone(), v);
            patterns.push(k);
        }

        if patterns.is_empty() {
            self.current_dict = None;
            self.entries.clear();
            self.pattern_keys.clear();
            self.matcher = None;
            return Ok(());
        }

        let ac = AhoCorasick::builder()
            .ascii_case_insensitive(true)
            .match_kind(MatchKind::Standard)
            .build(&patterns)
            .map_err(|e| e.to_string())?;

        self.current_dict = Some(dict);
        self.entries = map;
        self.pattern_keys = patterns;
        self.matcher = Some(ac);
        Ok(())
    }

    pub fn replace(&self, text: &str) -> String {
        if text.is_empty() || !self.is_ready() {
            return text.to_string();
        }

        let Some(matcher) = &self.matcher else {
            return text.to_string();
        };

        // Collect matches
        let mut hits: Vec<(usize, usize, usize)> = matcher
            .find_iter(text)
            .map(|m| (m.start(), m.end() - m.start(), m.pattern().as_usize()))
            .collect();

        // Sort by begin asc, length desc
        hits.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| b.1.cmp(&a.1)));

        // Filter non-overlapping valid hits
        let mut occupied = vec![false; text.len()];
        let mut valid_hits = Vec::new();
        for hit in hits {
            let start = hit.0;
            let len = hit.1;
            let end = (start + len).min(text.len());
            if occupied[start..end].iter().any(|&b| b) {
                continue;
            }
            for b in &mut occupied[start..end] {
                *b = true;
            }
            valid_hits.push(hit);
        }
        valid_hits.sort_by_key(|h| h.0);

        let mut out = String::new();
        let mut pos = 0;
        for (start, len, _) in valid_hits {
            if start > pos {
                out.push_str(&text[pos..start]);
            }
            let phrase = &text[start..start + len];
            let lower = phrase.to_ascii_lowercase();
            if let Some(translation) = self.entries.get(&lower) {
                out.push_str(&format!(
                    "<mstrans:dictionary translation='{}'>{}</mstrans:dictionary>",
                    html_encode(translation),
                    html_encode(phrase)
                ));
            } else {
                out.push_str(phrase);
            }
            pos = start + len;
        }
        if pos < text.len() {
            out.push_str(&text[pos..]);
        }
        out
    }
}

pub struct DictionaryManager {
    directory: PathBuf,
    dictionaries: Vec<DictionaryInfo>,
}

impl DictionaryManager {
    pub fn new(directory: PathBuf) -> Self {
        let _ = fs::create_dir_all(&directory);
        let mut mgr = Self {
            directory,
            dictionaries: Vec::new(),
        };
        mgr.reload();
        mgr
    }

    pub fn reload(&mut self) {
        self.dictionaries.clear();
        let Ok(read_dir) = fs::read_dir(&self.directory) else {
            return;
        };
        for entry in read_dir.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "txt")
                && let Ok(content) = fs::read_to_string(&path)
                && let Ok(info) = parse_dictionary_info(&path, &content)
            {
                self.dictionaries.push(info);
            }
        }
    }

    pub fn dictionaries(&self) -> &[DictionaryInfo] {
        &self.dictionaries
    }

    pub fn contains_duplicate(&self, file_path: &Path) -> bool {
        let file_name = file_path.file_name();
        self.dictionaries
            .iter()
            .any(|d| d.path.file_name() == file_name)
    }

    pub fn import(&mut self, file_path: &Path) -> Result<DictionaryInfo, String> {
        let content = fs::read_to_string(file_path).map_err(|e| e.to_string())?;
        let file_name = file_path
            .file_name()
            .ok_or_else(|| "Invalid file name".to_string())?;
        let dest = self.directory.join(file_name);

        self.remove_by_path(&dest);

        fs::write(&dest, &content).map_err(|e| e.to_string())?;
        let info = parse_dictionary_info(&dest, &content)?;
        self.dictionaries.push(info.clone());
        Ok(info)
    }

    pub fn remove_by_path(&mut self, path: &Path) {
        if let Some(pos) = self.dictionaries.iter().position(|d| d.path == path) {
            let _ = fs::remove_file(path);
            self.dictionaries.remove(pos);
        }
    }

    pub fn find(&self, language: &str) -> Vec<DictionaryInfo> {
        let lower = language.to_ascii_lowercase();
        self.dictionaries
            .iter()
            .filter(|d| {
                d.source_language.to_ascii_lowercase() == lower
                    || d.target_language.to_ascii_lowercase() == lower
            })
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_dictionary_info_and_entries() {
        let content = ";Geralt Dict|en|pl\n;Note|Game terminology\nGeralt|Geralt z Rivii\nCiri|Jaskier\n";
        let info = parse_dictionary_info(Path::new("test.txt"), content).unwrap();
        assert_eq!(info.name, "Geralt Dict");
        assert_eq!(info.source_language, "en");
        assert_eq!(info.target_language, "pl");
        assert_eq!(info.note, "Game terminology");
        assert_eq!(info.term_count, 2);

        let entries = parse_entries(content);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0], ("Ciri".into(), "Jaskier".into()));
        assert_eq!(entries[1], ("Geralt".into(), "Geralt z Rivii".into()));
    }

    #[test]
    fn test_dynamic_replacer() {
        let content = ";Terms|en|pl\nGeralt|Geralt z Rivii\nWhite Wolf|Biały Wilk\n";
        let info = parse_dictionary_info(Path::new("test.txt"), content).unwrap();
        let entries = parse_entries(content);

        let mut replacer = AcDynamicDictionaryReplacer::new();
        replacer.bind(info, entries).unwrap();

        let input = "Hello Geralt, also known as the white wolf!";
        let replaced = replacer.replace(input);
        assert!(replaced.contains("<mstrans:dictionary translation='Geralt z Rivii'>Geralt</mstrans:dictionary>"));
        assert!(replaced.contains("<mstrans:dictionary translation='Biały Wilk'>white wolf</mstrans:dictionary>"));
    }
}
