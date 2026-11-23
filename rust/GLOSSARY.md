# GLOSSARY.md — domain terms

| Term | Meaning | Rust name |
| --- | --- | --- |
| Container | A `.w3strings` file: RTSW-magic binary string table the game loads | `W3StringsFile` |
| Generation | Format era: v162 = UTF-16LE storage, v164 = UTF-8 storage | `Generation` (`Utf16Le`, `Utf8`) |
| Entry | One string row inside a container (id, key hash, text) | `W3StringEntry` |
| Key | Localization key name; the game resolves it by hash | `KeyName` / key in `W3KeyEntry` |
| Key hash | `hash*31 + utf16unit` over the lowercased key's UTF-16 units, `u32` | `LocalizationKeyHash::of` |
| Language key | 8-byte language identifier split across head/tail of the container | `language_key` |
| Item | The editor's row model: StrId, KeyHex, KeyName, Text | `W3StringItem` |
| CSV | Actually a pipe-delimited text format (`;` comments, `|` separator, 4+ fields) | `csv` module |
| Backup | Auto-made copy of a file before save, in the backup dir | `BackupService` |
| Recent file | MRU entry persisted across runs | `RecentFilesService` |
| Dictionary | User `.txt` word list applied during Microsoft translation | `DictionaryService` |
| Locale | UI translation table, 13 languages from .resx | `locales` module |
| SavedText | Stored form of one text inside the string buffer | `StoredText` |
| SectionCount | One of the three count headers after the magic | `SectionCount` |
