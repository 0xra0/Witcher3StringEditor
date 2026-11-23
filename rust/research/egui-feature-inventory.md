# egui Feature Inventory — Wave 3 (`app` crate)

Scope: what the C# WPF app exposes and what wave 3 must rebuild in `rust/app` (egui/eframe).
Sources: `README.md`, `Witcher3StringEditor/Views/MainWindow.xaml`, `Witcher3StringEditor/ViewModels/*.cs`,
`Witcher3StringEditor.Dialogs/Views/*.xaml`, `Witcher3StringEditor.Dialogs/ViewModels/*.cs`,
`Witcher3StringEditor.Shared/Behaviors/*.cs`, `rust/DECISIONS.md` (ADR-005/006/007), `rust/MAP.md`.
Crate names below are the units in `rust/MAP.md`; `app` owns construction directly (ADR-007, no DI/MVVM).

## 1. Windows / panels

- **Main window** (`Witcher3StringEditor/Views/MainWindow.xaml`): rows = 48px title bar (app title, search box, theme
  toggle), toolbar (`CommandBar`), content area (grid or "drop here" empty-state text), pager row, update `InfoBar`.
  → `app` (one eframe window; top panel / central panel / bottom panel). Theme toggle → `app` with settings persisted
  via `settings`.
- **Main grid** (`SfDataGrid`, MainWindow.xaml:272-367): editable, cell navigation, extended row selection, resizable
  columns, row headers with row numbers, wrapped text column, sortable, paginated. → `app` (egui table with
  `egui_extras::Table`/`egui_table` or custom).
- **Data pager** (`SfDataPager`, MainWindow.xaml:369-376): `NumericButtonCount=10`, page size from settings.
  → `app` + `settings` (page size persisted).
- **Update notification** (`InfoBar`, MainWindow.xaml:378-389): bound to `IsUpdateAvailable`, button opens Nexus page.
  → `app` + `nexus` (check) + `settings` (Nexus URL).
- Note: no separate windows beyond main + modal dialogs; all dialogs are `CenterOwner` modals
  (`*.xaml` `WindowStartupLocation="CenterOwner"`) → `app` egui modal windows.

## 2. Menu / toolbar actions (all `AppBarButton` in MainWindow.xaml:124-251; commands in
`Witcher3StringEditor/ViewModels/MainWindowViewModel*.cs`)

| Action (XAML label key / command) | Behavior / handler | Rust crate |
| --- | --- | --- |
| Open (`OpenFileCommand`) | open dialog → `LoadFileAsync` | `app`, `serializer-core`/`csv`/`xlsx`/`codec`, `services` (recent) |
| Recent (`ShowRecentDialogCommand`) | modal recent-files dialog | `app`, `services` |
| Merge (`MergeDataCommand`) | open 2nd file, confirm, join by StrId, overwrite text | `app`, serializer crates |
| Add (`EditCommand`, no selection) | new item pre-filled with `CreateNextStrId()` | `app`, `contracts` |
| Edit (`EditCommand`, selected item) | edit dialog; falls back to Add when no selection | `app` |
| Delete (`DeleteCommand`, selected items) | confirmation dialog, remove items | `app` |
| Translate (`ShowTranslateDialogCommand`) | translation dialog; uses filtered `PagedSource` if present | `app`, `translate`, `dictionary` |
| Dictionary (`ShowDictionaryDialogCommand`) | only visible when translator = Microsoft | `app`, `dictionary` |
| Save (`ShowSaveDialogCommand`) | save dialog (format/language/encoding) | `app`, `serializer-core` |
| Backup (`ShowBackupDialogCommand`) | backup management dialog | `app`, `services` |
| Log (`ShowLogDialogCommand`) | log viewer dialog | `app`, `services` |
| Settings (`ShowSettingsDialogCommand`) | settings dialog | `app`, `settings` |
| Open working folder (`OpenWorkingFolderCommand`) | shell-open `OutputFolder`; enabled when dir exists | `app`, `services` |
| Play game (`PlayGameCommand`) | **out of scope** (ADR-003/MAP) — omit | — |
| Nexus Mods (`OpenNexusModsCommand`) | open URL in browser → `app`, `settings` (URL); update via `nexus` | `app`, `nexus` |
| About (`ShowAboutCommand`) | about dialog | `app` |
| Theme toggle (code-behind `ThemeSwitchBtn_OnClick`) | light/dark switch | `app`, `settings` |

Command enablement flags (`MainWindowViewModel.cs:78-121`): `HasW3StringItems` gates Edit/Delete/Merge/Save/Translate;
`CanShowTranslateDialog` (Dialogs.cs:98-103) requires non-empty items/paged source; `CanPlayGame`/`CanOpenWorkingFolder`
(Shell.cs:17-23). Replicate as per-frame `ui.add_enabled()` conditions in `app`.

## 3. Grid columns and editing operations

Columns (`MainWindow.xaml:328-366`):
- **ID** → `StrId`, editable, centered, width 100.
- **Key(Hex)** → `KeyHex`, **read-only**, centered, width 100.
- **Key(String)** → `KeyName`, editable, centered, width 300.
- **Text** → `Text`, editable, wrapped (`WrapWithOverflow`), min width 300.
- **Reset** template column → per-row undo button bound to `ResetTextCommand`; enabled when `IsModified`.
  `IsModified` = `OldText` non-empty; `ResetText()` restores `Text = OldText` and clears `OldText`
  (`Witcher3StringEditor/Models/W3StringItem.cs:38,99-121`). → `app` (row model holds `old_text`; `contracts` for item type).

Editing operations:
- In-cell edit of StrId/KeyName/Text; `CurrentCellEndEdit` invalidates row height
  (`Witcher3StringEditor.Shared/Behaviors/SfDataGridCurrentCellEndEditBehavior.cs:37-41`) → in `app`, recompute row height
  on commit. First Text edit snapshots original into `OldText` (`W3StringItem.cs:99-102`).
- Add/Edit via modal form; Delete via confirmation dialog; Merge by StrId (editing operations in
  `MainWindowViewModel.Editing.cs:27-181`).
- Search: `SearchHelper.AllowFiltering=true`, case-insensitive, no highlight (`MainWindow.xaml.cs:35-40`);
  query submitted → `Search(query)`, empty text → `ClearSearch()`; results feed `PagedSource`
  (`MainWindow.xaml.cs:90-123`) → filtering + pagination pure functions in `app`.
- Sort: `SortColumnsChanged` → notify source changed (`MainWindow.xaml.cs:158-168`) → `app`.
- Row height/size behaviors (`SfDataGridQueryRowHeightBehavior`, `SfDataGridSizeChangedBehavior`) are layout-only → `app`.

## 4. Dialogs (each = `Views/*.xaml` + `ViewModels/*.cs` in `Witcher3StringEditor.Dialogs/`)

- **Save** (`SaveDialog.xaml`, `SaveDialogViewModel.cs`): target file format combo (`Csv`/`W3Strings`), target language
  combo, payload-encoding combo (UTF-16/UTF-8, visible only for `.w3strings`), Save/Cancel. Defaults from settings;
  Save calls serializer with `W3StringSerializationOptions` and remembers encoding → `app` UI + `serializer-core` +
  `contracts`/`codec` + `settings`.
- **Settings** (`SettingsDialog.xaml`, `SettingDialogViewModel.cs`): Preferred file type, preferred game version
  (encoding), preferred W3 language, translator, game-exe path (file picker `witcher3.exe`), file-association toggle
  (**out of scope**, ADR-003), UI language, page size (20/30/50/100). Live-save to settings; translator/language/page-size
  changes broadcast messages → `app` UI + `settings` + `locales`; game path only for out-of-scope Play.
- **Backup** (`BackupDialog.xaml`, `BackupDialogViewModel.cs`): searchable, paginated grid (page size 15, sorted by
  backup time desc) with columns FileName, Hash, OriginPath, BackupTime + Restore/Delete buttons. Restore/Delete confirm;
  missing backup offers delete. → `app` + `services` (backup) + `settings` (`BackupItems`).
- **Recent** (`RecentDialog.xaml`, `RecentDialogViewModel.cs`): searchable, paginated grid (page size 30, sorted by
  opened time desc) columns FilePath, OpenedTime, Mark (pin checkbox), Open, Delete. Open posts `RecentFileOpened` message;
  missing file offers removal. → `app` + `services` (recent files) + `settings` (`RecentItems`).
- **Dictionary manager** (`DictionaryManagerDialog.xaml`, `DictionaryManagerDialogViewModel.cs`): language-grouped tree
  of dictionaries + term grid (Key/Value, page size 30) + search, Import `.txt` (duplicate confirm) and Remove (confirm)
  buttons; selecting a dictionary loads its entries. → `app` + `dictionary` + `services`.
- **Log viewer** (`LogDialog.xaml`, `LogDialogViewModel.cs`): searchable, paginated grid (page size 30) columns
  Timestamp, Level, Message; toolbar Delete-old-logs, Open-log-folder, Report-bug (Nexus bugs tab). Live-subscribes to new
  log events. → `app` + `services` (log access, shell open) + `settings` (Nexus URL) + `tracing` (ADR-007).
- **About** (`AboutDialog.xaml`, `AboutDialogViewModel.cs`): version, build time, OS, runtime + dependency grid
  (name/version, version-sorted). → `app` (build metadata via `env!`/`clap` crate version).
- **Translation** (`TranslationDialog.xaml`, `TranslationDialogViewModel.cs`): mode-switch button between single and
  batch; detects source language on load; confirms on mode switch/close if busy, auto-saves unsaved single translation.
  → `app` + `translate` + `dictionary`.
  - **Single mode** (`TranslateDataTemplate.xaml`, `SingleItemTranslationViewModel.cs`): source/target language combos,
    read-only source text, editable translated text (placeholder "Translating..." when busy), buttons Previous /
    Translate / Save / Next (Prev/Next auto-offer save of unsaved translation). → `app` + `translate`.
  - **Batch mode** (`BatchTranslateDataTemplate.xaml`, `BatchItemsTranslationViewModel.cs`): source/target language,
    optional dictionary combo (Microsoft + English source only), StartIndex/EndIndex number boxes, success/failure/pending
    counters, Start/Cancel buttons; optional dictionary replacement before translate. → `app` + `translate` +
    `dictionary`.
- **Edit/Add** (`EditDataDialog.xaml`, `EditDataDialogViewModel.cs`): ID, Key(String), Text fields + Submit/Cancel;
  works on a clone so cancel leaves item untouched. → `app` + `contracts`.
- **Delete** (`DeleteDataDialog.xaml`, `DeleteDataDialogViewModel.cs`): read-only grid of items to delete (ID, Key(Hex),
  Key(String), Text) + Delete/Cancel. → `app`.

## 5. Settings inventory (`Witcher3StringEditor/Models/AppSettings.cs`; JSON round-trip → `settings`)

`GameExePath`, `Language` (UI locale, system language on first run), `PageSize` (default 30), `PreferredW3Language`
(JSON key `PreferredLanguage`), `PreferredW3StringsEncoding` (Unicode=UTF-16 default / UTF8), `PreferredW3StringFileType`
(JSON key `PreferredFileType`), `Translator` (default `MicrosoftTranslator`), `NexusModUrl` (constant, not serialized),
`RecentItems`, `BackupItems`. All persisted under `%AppData%\Witcher3StringEditor` (Rust: XDG-ish path in `settings`/
`services`). Locales: README lists 10 UI languages; ADR-006/MAP say 13; the repo actually ships **12** `Strings*.resx`
(`Witcher3StringEditor.Locales/`) — reconcile with user before finalizing `locales` table (additional `id-ID`, `uk-ua`).

## 6. Keyboard shortcuts

None. repo-wide grep for `KeyBinding`/`InputBinding`/`KeyGesture`/`KeyDown` in XAML and C# found **no explicit shortcuts**
(all interaction is pointer/menu-driven; grid uses default Syncfusion cell navigation). Wave 3 may add egui-native
shortcuts as an enhancement, but there is no legacy behavior to preserve.

## 7. Data flow (message bus in C# → plain callbacks/channels in `app` per ADR-007)

- **Open**: file dialog / drag-drop (`MainWindowViewModel.DragDrop.cs:26-42`) / Recent dialog / startup arg
  (`StartupFileService.cs:24-39`) → `LoadFileAsync` (`FileOperations.cs:34-54`) → serializer → wrap in items, sort by
  StrId, set `OutputFolder`, add to recent, clear search, refresh paged source.
- **Refresh paged source**: `RequestDataGridPagedSource` message (`Paging.cs:34-39`, 100ms delay) answered by code-behind
  from the grid's `PagedCollectionView` (`MainWindow.xaml.cs:68-82`); search/sort changes post
  `DataGridPagedSourceChanged` (`MainWindow.xaml.cs:173-181`) → `PagedSource` updated (`Messages.cs:51-57`).
  In `app` this becomes direct recomputation of `filtered/sorted/page` state from the item vector (no messenger).
- **Dialog → main**: Recent open posts `RecentFileOpened` (`RecentDialogViewModel.cs:69-75`); translator busy posts
  `TranslatorIsBusy`; settings changes post `PageSizeChanged`, `GameExePathChanged`, `TranslatorChanged`
  (`Messages.cs:27-68`; `MessageTokens.cs`). Translation save mutates shared items by `TrackingId`
  (`SingleItemTranslationViewModel.cs:229-236`, `TranslationDialogViewModel.cs:187-200`). In `app`, use shared
  `Arc<Mutex<Vec<...>>>`/callbacks instead of the WeakReferenceMessenger.
- **Save**: items + options → serializer (writes `.w3strings`/`.csv`/`.xlsx`); on `.w3strings` a backup is auto-created
  in the backup dir before overwrite (`README.md:9-10`) → `serializer-core`, `services`.
- **Startup**: `WindowLoaded` → `CheckSettings` + update check (`Window.cs:19-26`); `WindowClosing` confirms unsaved
  (`IsModified`) changes (`Window.cs:33-40`) → `app` + `settings` + `nexus`.

## 8. Behavior-equivalence notes / out of scope

- Out of scope for wave 3 (ADR-003/005, `rust/MAP.md`): Play game, file-association toggle, Raft/SharedRecords
  replication, Windows/macOS targets. Their UI affordances (Play button in toolbar, association toggle in settings) should
  be dropped rather than ported.
- Multi-script fonts must be bundled (Latin/Greek/Cyrillic/CJK/Thai — ADR-006) since game text is mixed-script.
- CSV format is pipe-delimited with `;` comments and 4+ fields (README:69-72, `rust/GLOSSARY.md`); CSV/XLSX/container
  byte-equivalence is owned by `csv`/`xlsx`/`codec`, wave 3 `app` only calls them.
