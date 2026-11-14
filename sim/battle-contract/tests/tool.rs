//! The tool prints the schema and proves that fixtures survive a trip through another runtime.

#![expect(
    clippy::unwrap_used,
    reason = "A tool that cannot be started must fail the test."
)]

use std::io::Write as _;
use std::process::{Command, Output, Stdio};
use sve_battle_contract::schema;

fn tool(command: &str, input: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_sve-battle-contract"))
        .arg(command)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

fn emitted() -> String {
    let output = tool("emit", "");
    assert!(output.status.success(), "emit failed");
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn schema_prints_the_generated_schema() {
    let output = tool("schema", "");
    assert!(output.status.success());
    let printed: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(printed, serde_json::to_value(schema()).unwrap());
}

#[test]
fn emitted_fixtures_verify_unchanged() {
    let lines = emitted();
    assert!(lines.contains("\"root\":\"HostMessage\""));
    assert!(lines.contains("\"root\":\"Question\""));
    assert!(tool("verify", &lines).status.success());
}

#[test]
fn a_changed_missing_or_extra_message_fails_verification() {
    let lines = emitted();
    let changed = lines.replacen("\"viewer\":\"p1\"", "\"viewer\":\"p2\"", 1);
    assert_ne!(changed, lines);
    let (first, rest) = lines.split_once('\n').unwrap();
    let renamed = lines.replacen("host/ready.json", "host/other.json", 1);
    assert_ne!(renamed, lines);
    for damaged in [
        changed,
        rest.to_owned(),
        format!("{lines}{first}\n"),
        renamed,
        lines.replacen("\"format\":1", "\"format\":2", 1),
        "not json\n".to_owned(),
    ] {
        let output = tool("verify", &damaged);
        assert!(!output.status.success());
        assert_ne!(output.stderr, Vec::<u8>::new());
    }
}

#[test]
fn an_unknown_command_prints_the_usage() {
    for command in ["", "help"] {
        let output = tool(command, "");
        assert!(!output.status.success());
        assert!(String::from_utf8(output.stderr).unwrap().contains("usage"));
    }
}
