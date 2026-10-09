//! Reproducible evaluation; raw runner reports are retained without reinterpretation.

#![expect(
    clippy::use_debug,
    reason = "The shared runner exposes original reports through Debug, not Serialize."
)]
#![expect(
    clippy::print_stdout,
    reason = "This evaluation command is a console report producer."
)]
#![expect(
    clippy::indexing_slicing,
    reason = "Writes target constructed JSON objects."
)]

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use core::error::Error;
use core::fmt::Debug;
use core::result;
use std::fs::{create_dir_all, write};
use std::path::Path;

use clap::Parser as _;
use serde_json::{Value, to_vec_pretty};
use sve_engine::adapter::{Adapter, AiAdapter, AssistAdapter, ReplayAdapter};
use sve_engine::ai::Profile;
use sve_engine::catalog::Catalog;
use sve_scenario_runner::ai::{AiEngine, LOAD_SEED, check_ai, check_search_log, load_positions};
use sve_scenario_runner::arch::{
    ArchFixtures, ArchOptions, AssistEngine, ReplayEngine, check_assist, check_replay,
    load_fixtures,
};
use sve_scenario_runner::{
    Fixture, RunOptions, View, gate, load_dir, load_known_failures, load_selection, run, score_g1,
    summary,
};

type Result<T> = result::Result<T, Box<dyn Error>>;

mod cli;
use cli::{Cli, Mode};

fn main() -> Result<()> {
    let Cli {
        snapshot,
        root,
        mode,
        output,
        selection_or_known,
    } = Cli::parse();
    create_dir_all(&output)?;
    let catalog = Arc::new(Catalog::load(&snapshot, &root.join("authored"))?);
    if mode == Mode::Rules {
        return rules(&catalog, &root, &output, selection_or_known.as_deref());
    }
    if mode == Mode::Gate {
        let known =
            selection_or_known.unwrap_or_else(|| root.join("tests/engine/known-failures.yaml"));
        return strict_gate(&catalog, &root, &output, &known);
    }
    if mode == Mode::Validate {
        println!("Loaded authored programs: {}", catalog.authored_count());
        println!("Rejected at load: {}", catalog.rejections().len());
        for (card, findings) in catalog.rejections() {
            for finding in findings {
                println!("REJECTED {card} {finding}");
            }
        }
        return Ok(());
    }
    if matches!(mode, Mode::All | Mode::G1) {
        let questions = load_dir(&root.join("tests/rules-scenarios/questions"))?;
        let selection = load_selection(&root.join("tests/rules-scenarios/g1-selection.yaml"))?;
        let reports = score_g1(
            &mut Adapter::new(Arc::clone(&catalog)),
            &questions,
            &selection,
            41,
        )?;
        retain(&output.join("g1.txt"), &reports)?;
        println!("G1 {:?}", summary(&reports));
    }
    if matches!(mode, Mode::All | Mode::Replay | Mode::Assist) {
        let fixtures = load_fixtures(&root.join("tests/architecture-fixtures"))?;
        architecture(&catalog, &fixtures, mode, &output)?;
    }
    if matches!(mode, Mode::All | Mode::Ai) {
        intelligence(&catalog, &root, &output)?;
    }
    Ok(())
}

fn rules(
    catalog: &Arc<Catalog>,
    root: &Path,
    output: &Path,
    selection: Option<&Path>,
) -> Result<()> {
    let questions = load_dir(&root.join("tests/rules-scenarios/questions"))?;
    let selection = selection.map(load_selection).transpose()?;
    let reports = run(
        &mut Adapter::new(Arc::clone(catalog)),
        &questions,
        selection.as_ref(),
        RunOptions {
            require_verified: true,
        },
    )?;
    retain(&output.join("rules.txt"), &reports)?;
    let mut bytes = to_vec_pretty(&reports)?;
    bytes.push(b'\n');
    write(output.join("rules.json"), bytes)?;
    println!("Rules {:?}", summary(&reports));
    Ok(())
}

/// Runs every shared scenario and fails unless the result matches the reviewed list exactly.
fn strict_gate(catalog: &Arc<Catalog>, root: &Path, output: &Path, known: &Path) -> Result<()> {
    let known = load_known_failures(known)?;
    let questions = load_dir(&root.join("tests/rules-scenarios/questions"))?;
    let reports = run(
        &mut Adapter::new(Arc::clone(catalog)),
        &questions,
        None,
        RunOptions {
            require_verified: true,
        },
    )?;
    retain(&output.join("rules.txt"), &reports)?;
    let mut bytes = to_vec_pretty(&reports)?;
    bytes.push(b'\n');
    write(output.join("rules.json"), bytes)?;
    let result = gate(&reports, &known);
    println!("Rules {:?}", summary(&reports));
    println!(
        "Gate: {} pass, {} known failures, {} problems",
        result.passed,
        result.known,
        result.problems.len()
    );
    for problem in &result.problems {
        println!("GATE {problem:?}");
    }
    if result.ok() {
        Ok(())
    } else {
        Err(format!("gate failed with {} problems", result.problems.len()).into())
    }
}

fn retain<T>(path: &Path, reports: &[T]) -> Result<()>
where
    T: Debug,
{
    let mut raw = String::new();
    for report in reports {
        let line = format!("{report:?}\n");
        print!("{line}");
        raw.push_str(&line);
    }
    write(path, raw)?;
    Ok(())
}

fn architecture(
    catalog: &Arc<Catalog>,
    fixtures: &ArchFixtures,
    mode: Mode,
    output: &Path,
) -> Result<()> {
    let mut replay =
        || -> Box<dyn ReplayEngine> { Box::new(ReplayAdapter::new(Arc::clone(catalog))) };
    let mut assist =
        || -> Box<dyn AssistEngine> { Box::new(AssistAdapter::new(Arc::clone(catalog))) };
    let options = ArchOptions::default();
    if matches!(mode, Mode::All | Mode::Replay) {
        retain(
            &output.join("replay.txt"),
            &check_replay(&mut replay, fixtures, &options),
        )?;
    }
    if matches!(mode, Mode::All | Mode::Assist) {
        retain(
            &output.join("assist.txt"),
            &check_assist(&mut assist, &mut replay, fixtures, &options),
        )?;
    }
    Ok(())
}

fn intelligence(catalog: &Arc<Catalog>, root: &Path, output: &Path) -> Result<()> {
    let mut profiles = BTreeMap::new();
    for id in ["general", "aggro", "control"] {
        let profile = Profile::load(&root.join(format!("sim/engine/profiles/{id}.yaml")))?;
        profiles.insert(id.to_owned(), profile);
    }
    let positions = load_positions(&root.join("tests/ai-positions"))?;
    let mut factory =
        || -> Box<dyn AiEngine> { Box::new(AiAdapter::new(Arc::clone(catalog), profiles.clone())) };
    retain(
        &output.join("ai.txt"),
        &check_ai(&mut factory, &positions, &root.join("profiles.yaml")),
    )?;
    let mut audits = Vec::new();
    for (stem, position) in &positions.0 {
        let Some(check) = position.checks.iter().find(|c| c.id == "Q2") else {
            continue;
        };
        let Some(think) = &check.think else {
            continue;
        };
        let mut adapter = AiAdapter::new(Arc::clone(catalog), profiles.clone());
        let mut setup = position.setup.clone();
        setup["room"] = position.room.clone().unwrap_or_default();
        adapter.load(
            &Fixture {
                question: position.id.clone(),
                scenario: stem.clone(),
                setup,
                card_facts: Value::Null,
                random: position.random.clone(),
            },
            LOAD_SEED,
        )?;
        let report = adapter.think(View::P1, &think.profile, think.budget, &think.seed)?;
        let core = adapter.report().ok_or("missing core search report")?;
        let audit = check_search_log(&core.log.entries, &report, think.budget, position);
        audits.push((
            stem,
            audit,
            report.edges,
            report.engine_steps,
            report.millis,
        ));
        write(
            output.join(format!("{stem}-search.json")),
            to_vec_pretty(core)?,
        )?;
    }
    retain(&output.join("ai-search-audit.txt"), &audits)?;
    Ok(())
}
