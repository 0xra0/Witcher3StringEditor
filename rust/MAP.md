# MAP.md — Witcher3StringEditor port

Goal: pure Rust Linux desktop app (egui) that replaces the C# WPF original, kept as reference.
Bar: byte-exact `.w3strings`/CSV/settings-JSON output; cell-equivalent XLSX; behavior-equal
network features. Decisions in [DECISIONS.md](DECISIONS.md); terms in [GLOSSARY.md](GLOSSARY.md).
Measured state: `rust-anything map --write` → `rust/PORT_MAP.md` at each checkpoint.

## Claims

A session claims one unit before starting (`claimed by <session> <date>`), marks it `done` with
gate/replay results. Never take a claimed unit.

| Unit (rust crate) | Source | Wave | State |
| --- | --- | ---: | --- |
| `w3cli` + C# harness + fixtures + recorded cases | tools/harness | 0 | done (219 cases recorded, 219/219 replay, front door 0.0% fallback) |
| `contracts` (item/file-format/language types, W3Language) | Contracts/*.cs | 0 | done (in w3codec crate) |
| `codec` (W3Strings/: hash, storage, reader, writer, builder, entries, StoredText, SectionCount) | Serializers/W3Strings/ | 0 | done (w3codec crate, 219/219 app replay) |
| `serializer-core` (W3StringItem, options, coordinator, W3StringsSerializer facade) | Serializers/*.cs | 0 | done (w3codec crate; backup/options handled in glue) |
| `csv` (pipe-format serializer, SavedTextFormatter) | Serializers/CsvSerializer.cs | 1 | done (in w3codec crate) |
| `settings` (AppSettings JSON round trip, AppPaths, persistence) | Miscellaneous/, Services/Settings* | 1 | done (settings crate, byte-exact JSON format, 8/8 tests pass) |
| `locales` (12 resx tables, CultureMatcher/Resolver) | Locales/ | 1 | done (locales crate, 1,968 strings across 12 languages) |
| `dictionary` (.txt dictionaries, Aho-Corasick replacer) | Dictionary/ | 1 | done (dictionary crate, dynamic replacer + manager) |
| `services` (backup, recent files, log access, shell open) | Services/ | 1 | done (services crate, XDG paths, SHA-256 backup, MRU) |
| `xlsx` (calamine read, rust_xlsxwriter write) | Serializers/ExcelSerializer.cs | 2 | done (xlsx crate, styled headers & freeze panes) |
| `translate` (Microsoft/Google/Yandex reqwest clients) | via GTranslate | 2 | done (translate crate, async Google/Bing/Yandex endpoints) |
| `nexus` (update check, ZeroQL GraphQL) | Services/CheckUpdate*, NexusIntegration/ | 2 | done (nexus crate, NexusMods GraphQL update checker) |
| `app` (egui: main grid, search/pagination, dialogs, locales wiring, drag-drop, theme) | Witcher3StringEditor/ | 3 | done (witcher3-string-editor binary, pure egui/eframe GUI) |

## Out of scope (ADR-003, ADR-005)

`Cluster/` + SharedRecords (Raft), `FileAssociationService`, `PlayGameService` (witcher3.exe),
Windows/macOS targets.

## Open questions

- Real game `.w3strings` files as additional fixtures: user may supply a path later (ADR-008).

## Session rules

Read this file first; claim before porting; after each unit: record/diff/gate, then `app wire`,
then update this file. Checkpoint with the user at wave end (ADR-010).
