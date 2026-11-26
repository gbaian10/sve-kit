//! Command line of the self-play tool; see the crate README.

#![expect(
    clippy::multiple_crate_versions,
    reason = "The engine's dependencies already pull two syn and getrandom generations."
)]

use std::env::args;
use std::io::{Write as _, stderr};
use std::process::ExitCode;
use sve_selfplay::cli::command;

fn main() -> ExitCode {
    let arguments: Vec<String> = args().skip(1).collect();
    match command(&arguments, &mut stderr()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            // Nothing else can be done if stderr itself is gone.
            drop(writeln!(stderr(), "{error}"));
            ExitCode::FAILURE
        }
    }
}
