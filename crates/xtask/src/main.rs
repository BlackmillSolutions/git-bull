//! Tasks for developing git-bull, run with `cargo xtask <task>`.
//!
//! - `notices` writes `THIRD-PARTY-NOTICES.md` (task 9.1, spec
//!   `distribution`).
//! - `notices --check` fails when that file is not what `notices` would
//!   write, as after a change of the dependencies.

mod notices;

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let result = match args[..] {
        ["notices"] => notices::write(),
        ["notices", "--check"] => notices::check(),
        _ => Err("usage: cargo xtask notices [--check]".to_owned()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}
