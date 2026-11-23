# DECISIONS.md — Witcher3StringEditor C# → Rust port

ADRs in decision order; a changed decision gets a new ADR that supersedes the old one.

## ADR-001: Shape — Linux desktop app plus a verification CLI
Status: accepted (2026-11-23)
Context: The original is a WPF GUI (net10.0-windows) that cannot build or run on this Linux
machine; rust-anything's `bridge` does not take C# units, so per-unit recording must go through a
runnable harness instead.
Decision: The port is a pure Rust desktop app (`rust/app`, eframe/egui) plus `rust/w3cli`, a small
CLI that mirrors a C# harness command-for-command (`decode-w3strings`, `encode-w3strings`,
`hash`, CSV/JSON/XLSX round trips). `w3cli` is the replay surface for the recorded cases and stays
after the lift; the GUI uses the same library crates.
Consequences: Recording and replay work on Linux; GUI correctness is checked by running the real
app on fixtures, not by recorded cases.

## ADR-002: Byte-exact bar
Status: accepted (2026-11-23)
Context: User requirement "byte-byte". XLSX is written by Syncfusion.XlsIO whose zip stream
internals cannot be reproduced by any Rust writer.
Decision: `.w3strings` output, CSV (pipe format) output and settings JSON must be byte-identical
to the C# original. XLSX must be cell-equivalent (opens with identical cells and formatting
semantics). Decoded values (items, texts) must match exactly. "The C# original" means its
serializer code as run on the recording machine (Linux): CSV line endings are `Environment.NewLine`
(`\n` here, `\r\n` in the shipped Windows build) — if Windows-parity line endings are wanted, that
is a case decision for the user at the wave-1 checkpoint. The harness's dump JSON (its own
interchange format, not app output) is byte-compared too, with the escape rule fixed in the
harness source.
Consequences: Every codec change is validated by byte comparison of harness outputs; XLSX is
compared cell-by-cell only.

## ADR-003: Platforms — Linux only
Status: accepted (2026-11-23)
Context: User chose Linux-only targets. The original relies on Windows for file association and
`witcher3.exe` launch.
Decision: Build and ship Linux x86-64. `FileAssociationService` and `PlayGameService`
(witcher3.exe launch) are dropped. App data lives under the XDG data dir
(`~/.local/share/Witcher3StringEditor`) instead of `%AppData%`.
Consequences: CsWin32 has no counterpart; backup/recent/log paths resolve per-platform from one
`AppPaths` module.

## ADR-004: Network features ported, behavior-equivalent
Status: accepted (2026-11-23)
Context: Translation helper (Microsoft/Google/Yandex via GTranslate), Nexus update check and
Nexus integration (ZeroQL GraphQL) hit remote servers; their responses cannot be byte-compared.
Decision: Port all three. Requests must match what the services expect (same endpoints, headers,
payload shapes); responses are compared on parsed behavior, not bytes. Research in
`rust/research/gtranslate-endpoints.md` before porting translate.
Consequences: GTranslate is replaced by hand-rolled reqwest clients; ZeroQL by a small GraphQL
JSON client.

## ADR-005: Cluster behavior without Raft
Status: accepted (2026-11-23)
Context: The original replicates recent-file/backup records through an internal Raft cluster
(DotNext.Net.Cluster, `Witcher3StringEditor/Cluster/` + `SharedRecordsService`, ~2500 lines).
No user-visible effect on a single machine; no Rust equivalent crate.
Decision: Do not port the Raft machinery. `RecentFilesService`, `BackupService` and settings
persistence behave identically, writing straight to their stores. `Cluster/`,
`SharedRecordsService`, `SharedRecordCodec`, `SharedRecordsStateMachine` stay unported reference.
Consequences: ~2.5k lines intentionally out of scope; shared-record events become direct writes.

## ADR-006: UI — egui replaces WPF/Syncfusion
Status: accepted (2026-11-23)
Context: WPF XAML, MVVM toolkit, Syncfusion SfGrid, converters, behaviors, template selectors
have no direct Rust counterparts; egui is immediate-mode.
Decision: UI is rebuilt in egui (eframe): main grid view with search/pagination/sort, add/delete/
edit, drag-and-drop open, dialogs (settings, backup, recent, dictionaries, about), 13 UI locales
ported from the .resx tables, theme following the original's look. Fonts for Latin, Greek,
Cyrillic, CJK and Thai are bundled (game text is multi-script; system-font coverage on Linux is
not guaranteed). Research in `rust/research/egui-platform.md` before wave 3.
Consequences: ViewModels/Views/Behaviors/Converters/TemplateSelectors do not port; their logic
moves into egui panels and plain functions.

## ADR-007: Dependency mapping
Status: accepted (2026-11-23)
Decision:
- Serilog → `tracing` (+ file sink), same log file location.
- Syncfusion.XlsIO → `calamine` (read) + `rust_xlsxwriter` (write).
- NReco.Text.AhoCorasickDoubleArrayTrie → `aho-corasick` crate.
- GTranslate → reqwest clients per ADR-004.
- ZeroQL → reqwest + serde_json GraphQL query (schema kept as reference).
- CommunityToolkit.Mvvm / HanumanInstitute.MvvmDialogs → egui immediate mode + `rfd` dialogs.
- System.Reactive messaging → plain channels/callbacks in egui.
- ZString → not needed (Rust strings are already UTF-8).
- CommunityToolkit.Diagnostics guards → plain `Result`/`debug_assert`.
Consequences: no MVVM layer, no DI container; `rust/app` owns construction directly.

## ADR-008: Verification — harness recording, byte comparison, gate
Status: accepted (2026-11-23)
Context: `bridge` takes no C# units; the equivalence proof needs a runnable original.
Decision: A C# console harness (`tools/harness`, net10.0, references Serializers/Contracts/
Dictionary) performs the recordable operations on fixture files. Cases are recorded with
`rust-anything app record` (two runs, noise wildcards). The Rust `w3cli` answers the same
commands; `app replay` must pass 100% before any wave closes, and every crate passes
`rust-anything gate`. Fixtures: generated v162/v164 containers, corrupt/edge inputs
(empty, bad magic, declared-size mismatch), CSVs with separators-in-text, settings JSON, and
non-ASCII text (Polish, CJK, Thai, emoji). Real game files may be added as fixtures later.
Consequences: The original's serializer behavior is frozen as cases before porting; a mismatch
is always a port bug, never a case edit (ADR-002).

## ADR-009: UTF-16 fidelity
Status: accepted (2026-11-23)
Context: C# strings are UTF-16; the key hash and the v162 codec count/iterate UTF-16 code units,
including lone surrogates the game may hold. Rust strings are UTF-8.
Decision: Any algorithm that indexes or counts code units (hash, v162 offsets/lengths, error
positions) operates on a UTF-16 view of the text (`u16` slice conversion at the boundary);
v164 paths and all file I/O are byte-level UTF-8. Ordering and length semantics follow C# per
case, verified by the recorded traps (Turkish i lowercase rules, surrogate pairs, U+FFFF).
Consequences: `rust/utf16.rs` helpers shared by codec and any consumer; no `char`-based
approximations where the original counts units.

## ADR-010: Waves and checkpoints
Status: accepted (2026-11-23)
Decision: Waves per `MAP.md`. Checkpoint with the user after each wave: units done, gate and
replay results, fallback rate. Wave 0 (contracts types, w3strings codec, hash) must reach 100%
replay before wave 1 starts.
Consequences: none.
