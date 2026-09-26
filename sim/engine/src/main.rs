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
use std::env::args;
use std::fs::{create_dir_all, write};
use std::path::{Path, PathBuf};

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
    Fixture, RunOptions, View, load_dir, load_selection, run, score_g1, summary,
};

type Result<T> = result::Result<T, Box<dyn Error>>;

fn main() -> Result<()> {
    let mut arguments = args().skip(1);
    let snapshot = PathBuf::from(arguments.next().ok_or(
        "usage: sve-prototype SNAPSHOT [ROOT] [all|g1|ai|replay|assist|validate|rules] [OUTPUT] [SELECTION]",
    )?);
    let root = PathBuf::from(arguments.next().unwrap_or_else(|| ".".into()));
    let mode = arguments.next().unwrap_or_else(|| "all".into());
    let output = PathBuf::from(
        arguments
            .next()
            .unwrap_or_else(|| "target/prototype".into()),
    );
    create_dir_all(&output)?;
    let catalog = Arc::new(Catalog::load(&snapshot, &root.join("authored"))?);
    if mode == "rules" {
        return rules(
            &catalog,
            &root,
            &output,
            arguments.next().map(PathBuf::from).as_deref(),
        );
    }
    if mode == "validate" {
        println!(
            "Schema-validated authored programs: {}",
            catalog.authored_count()
        );
        return Ok(());
    }
    if mode == "all" || mode == "g1" {
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
    if matches!(mode.as_str(), "all" | "replay" | "assist") {
        let fixtures = load_fixtures(&root.join("tests/architecture-fixtures"))?;
        architecture(&catalog, &fixtures, &mode, &output)?;
    }
    if mode == "all" || mode == "ai" {
        intelligence(&catalog, &root, &output)?;
    }
    if !matches!(mode.as_str(), "all" | "g1" | "replay" | "assist" | "ai") {
        return Err("unknown report mode".into());
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
    mode: &str,
    output: &Path,
) -> Result<()> {
    let mut replay =
        || -> Box<dyn ReplayEngine> { Box::new(ReplayAdapter::new(Arc::clone(catalog))) };
    let mut assist =
        || -> Box<dyn AssistEngine> { Box::new(AssistAdapter::new(Arc::clone(catalog))) };
    let options = ArchOptions::default();
    if mode == "all" || mode == "replay" {
        retain(
            &output.join("replay.txt"),
            &check_replay(&mut replay, fixtures, &options),
        )?;
    }
    if mode == "all" || mode == "assist" {
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
