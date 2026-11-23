mod cli;
mod original;
mod units;

use std::ffi::OsString;
use std::process::ExitCode;

fn main() -> ExitCode {
    original::report_fallbacks();
    let argv: Vec<OsString> = std::env::args_os().collect();
    if argv.len() <= 1 {
        eprintln!(
            "usage: harness hash <key> | decode <in.w3strings> <out.json> | encode <in.json> <outdir> <langCode> <utf8|utf16> | csv-decode <in> <out.json> | csv-encode <in.json> <outdir> <langCode> | gen-fixtures <dir> | settings-default <out.json> | settings-roundtrip <in.json> <out.json>"
        );
        return ExitCode::from(2);
    }
    if let Ok(matches) = cli::command().try_get_matches_from(&argv) {
        let (command, sub) = matches.subcommand().unwrap_or((cli::ROOT, &matches));
        if let Some(code) = units::dispatch(command, sub) {
            return code;
        }
    }
    original::run(&argv[1..])
}
