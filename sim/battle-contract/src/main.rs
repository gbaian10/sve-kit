//! Generation and round-trip tool for the battle contract.
//!
//! `schema` prints the JSON Schema that the TypeScript types and the validator are
//! generated from. `emit` prints every fixture as this crate encodes it, one JSON line
//! each; `verify` reads such lines back after another runtime has validated and
//! re-encoded them, and fails unless every fixture returns unchanged.

use core::error::Error;
use core::fmt;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use std::env;
use std::fs;
use std::io::{self, Write as _};
use std::path::Path;
use sve_battle_contract::observation::BattleObservation;
use sve_battle_contract::question::Question;
use sve_battle_contract::{ClientMessage, HostMessage, decode, schema};

/// Why the tool stopped.
#[derive(Debug)]
struct Failure(String);

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Error for Failure {}

fn failure<E>(context: &str, error: &E) -> Failure
where
    E: fmt::Display,
{
    Failure(format!("{context}: {error}"))
}

/// Every fixture of one group, decoded as its root type, in a stable order.
fn fixtures<T>(group: &str) -> Result<Vec<(String, T)>, Failure>
where
    T: DeserializeOwned + Serialize,
{
    let folder = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join(group);
    let mut names = Vec::new();
    for entry in fs::read_dir(&folder).map_err(|error| failure(group, &error))? {
        let name = entry.map_err(|error| failure(group, &error))?.file_name();
        names.push(name.to_string_lossy().into_owned());
    }
    names.sort();
    let mut found = Vec::new();
    for name in names {
        let text =
            fs::read_to_string(folder.join(&name)).map_err(|error| failure(&name, &error))?;
        let message = decode(&text).map_err(|error| failure(&name, &error))?;
        found.push((format!("{group}/{name}"), message));
    }
    Ok(found)
}

/// One line per fixture: its root type, its name and the message as this crate encodes it.
fn emit<T>(root: &str, group: &str, lines: &mut String) -> Result<(), Failure>
where
    T: DeserializeOwned + Serialize,
{
    for (name, message) in fixtures::<T>(group)? {
        let line = json!({ "root": root, "name": name, "message": message });
        lines.push_str(&line.to_string());
        lines.push('\n');
    }
    Ok(())
}

/// The lines for one root type must be exactly this crate's fixtures, each unchanged.
fn verify<T>(root: &str, group: &str, lines: &[Value]) -> Result<(), Failure>
where
    T: DeserializeOwned + Serialize + PartialEq,
{
    let mut returned = lines
        .iter()
        .filter(|line| line.get("root").and_then(Value::as_str) == Some(root));
    for (name, expected) in fixtures::<T>(group)? {
        let line = returned
            .next()
            .ok_or_else(|| Failure(format!("{name}: missing")))?;
        let message = line.get("message").unwrap_or(&Value::Null).to_string();
        let decoded: T = decode(&message).map_err(|error| failure(&name, &error))?;
        if line.get("name").and_then(Value::as_str) != Some(name.as_str()) || decoded != expected {
            return Err(Failure(format!("{name}: changed on the way back")));
        }
    }
    if returned.next().is_some() {
        return Err(Failure(format!("{root}: unexpected extra message")));
    }
    Ok(())
}

/// What one command prints.
fn run(command: &str) -> Result<String, Failure> {
    match command {
        "schema" => {
            let text = serde_json::to_string_pretty(&schema())
                .map_err(|error| failure("schema", &error))?;
            Ok(format!("{text}\n"))
        }
        "emit" => {
            let mut lines = String::new();
            emit::<HostMessage>("HostMessage", "host", &mut lines)?;
            emit::<ClientMessage>("ClientMessage", "client", &mut lines)?;
            emit::<BattleObservation>("BattleObservation", "observation", &mut lines)?;
            emit::<Question>("Question", "question", &mut lines)?;
            Ok(lines)
        }
        "verify" => {
            let input =
                io::read_to_string(io::stdin()).map_err(|error| failure("input", &error))?;
            let mut lines = Vec::new();
            for line in input.lines() {
                lines.push(serde_json::from_str(line).map_err(|error| failure("input", &error))?);
            }
            verify::<HostMessage>("HostMessage", "host", &lines)?;
            verify::<ClientMessage>("ClientMessage", "client", &lines)?;
            verify::<BattleObservation>("BattleObservation", "observation", &lines)?;
            verify::<Question>("Question", "question", &lines)?;
            Ok(String::new())
        }
        _ => Err(Failure(
            "usage: sve-battle-contract schema | emit | verify".to_owned(),
        )),
    }
}

fn main() -> Result<(), Failure> {
    let command = env::args().nth(1).unwrap_or_default();
    let output = run(&command)?;
    io::stdout()
        .write_all(output.as_bytes())
        .map_err(|error| failure("output", &error))
}
