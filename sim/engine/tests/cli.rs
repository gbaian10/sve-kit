//! Process tests ensure the executable parses before opening inputs or making reports.

#![cfg(feature = "runner")]

use std::env::temp_dir;
use std::process::{Command, id};

fn command() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_sve-prototype"));
    command.env("FORCE_COLOR", "1").env("CLICOLOR_FORCE", "1");
    command
}

#[test]
fn help_and_version_succeed_without_input_files() {
    for flag in ["--help", "-h", "--version", "-V"] {
        let output = command().arg(flag).output().unwrap();
        assert!(output.status.success());
        assert!(!output.stdout.is_empty());
        assert!(output.stderr.is_empty());
    }
}

#[test]
fn missing_snapshot_is_a_usage_error() {
    let output = command().output().unwrap();
    assert_eq!(output.status.code(), Some(2_i32));
    assert!(output.stdout.is_empty());
    assert!(!output.stderr.is_empty());
}

#[test]
fn invalid_arguments_are_rejected_before_creating_output_or_loading_snapshot() {
    for (label, mode, extra) in [
        ("mode", "invalid", [].as_slice()),
        ("option", "rules", ["--unknown"].as_slice()),
        ("extra", "rules", ["selection.yaml", "extra"].as_slice()),
    ] {
        let root = temp_dir().join(format!("sve cli refusal {}-{label}", id()));
        let snapshot = root.join("missing.jsonl");
        let output_dir = root.join("reports");
        assert!(!root.exists());
        let output = command()
            .arg(&snapshot)
            .arg(&root)
            .arg(mode)
            .arg(output_dir)
            .args(extra)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2_i32));
        assert!(output.stdout.is_empty());
        assert!(!output.stderr.is_empty());
        assert!(!root.exists());
    }
}
