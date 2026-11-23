//! Glue from the front door to `settings-rs`: parse harness arguments,
//! call `settings_rs::port`, print what the original prints, return its exit code.

use std::ffi::OsString;
use std::fs;
use std::path::Path;
use std::process::ExitCode;

use clap::ArgMatches;

use settings_rs::port as settings;

fn error(message: &str) -> ExitCode {
    println!("error: {message}");
    ExitCode::from(3)
}

fn write(path: &str, content: &[u8]) -> Result<(), String> {
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
        "settings-default" => {
            let out = settings::serialize(&settings::default_settings());
            match write(&args[0], out.as_bytes()) {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => error(&e),
            }
        }
        "settings-roundtrip" => {
            let text = match fs::read_to_string(&args[0]) {
                Ok(t) => t,
                Err(e) => return error(&e.to_string()),
            };
            match settings::parse(&text) {
                Ok(parsed) => {
                    let out = settings::serialize(&parsed);
                    match write(&args[1], out.as_bytes()) {
                        Ok(()) => ExitCode::SUCCESS,
                        Err(e) => error(&e),
                    }
                }
                Err(e) => error(&e),
            }
        }
        _ => crate::original::run(&[]),
    }
}
