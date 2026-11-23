use std::ffi::OsString;

use clap::{Arg, Command, value_parser};

/// The command `units::dispatch` gets when argv names no subcommand.
pub const ROOT: &str = "harness";

fn rest() -> Arg {
    Arg::new("args")
        .num_args(0..)
        .trailing_var_arg(true)
        .allow_hyphen_values(true)
        .value_parser(value_parser!(OsString))
}

/// Routes argv to a command. Anything this parser rejects runs the original, so it never
/// has to match every way the original accepts its arguments.
pub fn command() -> Command {
    Command::new(ROOT)
        .disable_help_flag(true)
        .disable_version_flag(true)
        .disable_help_subcommand(true)
        .args_conflicts_with_subcommands(true)
        .arg(rest())
        .subcommand(
            Command::new("csv-decode")
                .disable_help_flag(true)
                .arg(rest()),
        )
        .subcommand(
            Command::new("csv-encode")
                .disable_help_flag(true)
                .arg(rest()),
        )
        .subcommand(Command::new("decode").disable_help_flag(true).arg(rest()))
        .subcommand(Command::new("encode").disable_help_flag(true).arg(rest()))
        .subcommand(Command::new("hash").disable_help_flag(true).arg(rest()))
        .subcommand(Command::new("settings-default").disable_help_flag(true).arg(rest()))
        .subcommand(Command::new("settings-roundtrip").disable_help_flag(true).arg(rest()))
}
