//! The tool's commands: compose inputs, run seeds, replay records.

use crate::play::{End, Limits, Record, StepCost, replay};
use crate::published::{Published, compose};
use crate::report::{Report, run_seeds};
use core::fmt::Display;
use serde_json::{Value, json};
use std::fs::{File, create_dir_all, read_to_string, write};
use std::io::{BufWriter, Write};
use std::path::Path;
use std::time::Instant;

const USAGE: &str = "usage:
  sve-selfplay prepare <reader-dir> <export-root> <out-dir>
  sve-selfplay run <inputs.json> <programs.json> <out-dir> <first-seed> <games> [jobs] [records]
  sve-selfplay replay <inputs.json> <programs.json> <records.jsonl> <timings.jsonl>";

fn failed<E>(error: E) -> String
where
    E: Display,
{
    error.to_string()
}

fn load(inputs: &str, programs: &str, log: &mut dyn Write) -> Result<Published, String> {
    let start = Instant::now();
    let published = Published::new(
        &read_to_string(inputs).map_err(failed)?,
        &read_to_string(programs).map_err(failed)?,
    )?;
    writeln!(
        log,
        "prepared in {:.1}s: {} eligible printings, {} excluded, {} without a program",
        start.elapsed().as_secs_f64(),
        published.arena.pools.eligible().len(),
        published.excluded.len(),
        published.unprogrammed,
    )
    .map_err(failed)?;
    Ok(published)
}

fn write_line<W>(out: &mut W, value: &Value) -> Result<(), String>
where
    W: Write,
{
    serde_json::to_writer(&mut *out, value).map_err(failed)?;
    out.write_all(b"\n").map_err(failed)
}

fn number(text: Option<&String>, default: u64) -> Result<u64, String> {
    text.map_or(Ok(default), |value| value.parse().map_err(failed))
}

fn run(arguments: &[String], log: &mut dyn Write) -> Result<(), String> {
    let [inputs, programs, out, first, games, rest @ ..] = arguments else {
        return Err(USAGE.into());
    };
    let (first, games): (u64, u64) = (
        first.parse().map_err(failed)?,
        games.parse().map_err(failed)?,
    );
    let jobs = usize::try_from(number(rest.first(), 4)?).map_err(failed)?;
    let records = number(rest.get(1), 0)?;
    let published = load(inputs, programs, log)?;
    let out = Path::new(out);
    create_dir_all(out).map_err(failed)?;
    let excluded: Vec<Value> = published
        .excluded
        .iter()
        .map(|card| json!({"number": card.number, "reason": card.reason}))
        .collect();
    write(out.join("excluded.json"), json!(excluded).to_string()).map_err(failed)?;
    let mut failures = BufWriter::new(File::create(out.join("failures.jsonl")).map_err(failed)?);
    let mut kept = BufWriter::new(File::create(out.join("records.jsonl")).map_err(failed)?);
    let mut report = Report::default();
    let mut done = 0_u64;
    let mut problem = None;
    let start = Instant::now();
    let limits = Limits::default();
    run_seeds(&published.arena, first, games, jobs, limits, |played| {
        report.add(&published.arena, &played);
        done = done.saturating_add(1);
        let mut write_out = || -> Result<(), String> {
            if let End::Failed(failure) = &played.end {
                let entry = json!({"seed": played.record.seed, "failure": failure, "record": played.record});
                write_line(&mut failures, &entry)?;
                // A long run can be interrupted; what was found so far must already be on disk.
                failures.flush().map_err(failed)?;
            }
            if played.record.seed.saturating_sub(first) < records {
                write_line(&mut kept, &json!(played.record))?;
            }
            if done.is_multiple_of(100) || done == games {
                writeln!(
                    log,
                    "{done}/{games} matches, {}s",
                    start.elapsed().as_secs()
                )
                .map_err(failed)?;
            }
            Ok(())
        };
        if let Err(error) = write_out() {
            problem.get_or_insert(error);
        }
    });
    kept.flush().map_err(failed)?;
    let mut summary = report.summary(&published.arena);
    if let Some(fields) = summary.as_object_mut() {
        fields.insert("first_seed".into(), json!(first));
        fields.insert("jobs".into(), json!(jobs));
        fields.insert("wall_seconds".into(), json!(start.elapsed().as_secs()));
    }
    write(
        out.join("summary.json"),
        serde_json::to_string_pretty(&summary).map_err(failed)?,
    )
    .map_err(failed)?;
    problem.map_or(Ok(()), Err)
}

fn replay_records(arguments: &[String], log: &mut dyn Write) -> Result<(), String> {
    let [inputs, programs, records, timings] = arguments else {
        return Err(USAGE.into());
    };
    let published = load(inputs, programs, log)?;
    let mut out = BufWriter::new(File::create(timings).map_err(failed)?);
    for line in read_to_string(records).map_err(failed)?.lines() {
        let record: Record = serde_json::from_str(line).map_err(failed)?;
        let played = replay(&published.arena, &record, Limits::default());
        let end = match &played.end {
            End::Finished { .. } => "finished".to_owned(),
            End::Failed(failure) => format!("{}: {}", failure.kind, failure.detail),
        };
        let costs = |pick: fn(&StepCost) -> u64| played.costs.iter().map(pick).collect::<Vec<_>>();
        write_line(
            &mut out,
            &json!({
                "seed": record.seed,
                "end": end,
                "engine_micros": costs(|cost| cost.engine_micros),
                "encode_micros": costs(|cost| cost.encode_micros),
            }),
        )?;
    }
    out.flush().map_err(failed)
}

fn prepare(arguments: &[String]) -> Result<(), String> {
    let [reader, export, out] = arguments else {
        return Err(USAGE.into());
    };
    let (inputs, programs) = compose(Path::new(reader), Path::new(export))?;
    let out = Path::new(out);
    create_dir_all(out).map_err(failed)?;
    write(out.join("inputs.json"), inputs).map_err(failed)?;
    write(out.join("programs.json"), programs).map_err(failed)
}

/// Run one command line; progress goes to `log`.
///
/// # Errors
/// Wrong usage, unreadable inputs or an unwritable output directory.
pub fn command(arguments: &[String], log: &mut dyn Write) -> Result<(), String> {
    match arguments.split_first() {
        Some((name, rest)) if name == "prepare" => prepare(rest),
        Some((name, rest)) if name == "run" => run(rest, log),
        Some((name, rest)) if name == "replay" => replay_records(rest, log),
        Some(_) | None => Err(USAGE.into()),
    }
}
