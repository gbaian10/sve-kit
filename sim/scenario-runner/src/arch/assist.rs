//! A1–A4 and B1–B3: divergence record, halted automation and realignment (design section 4).

use serde_json::Value;

use super::support::{Ids, Probe, check_fixed, has_match, mark_decisions, normalize};
use super::{
    ArchFixtures, ArchOptions, AssistEngine, AssistFactory, CheckReport, Layer, Layered, Realign,
    ReplayFactory, positions,
};
use crate::compare;
use crate::engine::{EngineError, Step, View};

type Checked = Result<Vec<String>, EngineError>;

const LAYERS: [Layer; 2] = [Layer::Sandbox, Layer::Shadow];

/// Runs A1–A4 and B1–B3 against fresh instances from `factory`.
///
/// `replay` gives the design's replay engine: A3 compares each step after Rewind with
/// the same step of path P played by it.
pub fn check_assist(
    factory: &mut AssistFactory<'_>,
    replay: &mut ReplayFactory<'_>,
    fixtures: &ArchFixtures,
    options: &ArchOptions,
) -> Vec<CheckReport> {
    let mut reports = Vec::new();
    match sequence_a(factory, replay, fixtures, options) {
        Ok([a1, a2, a3]) => reports.extend([
            CheckReport::from_result("A1", Ok(a1)),
            CheckReport::from_result("A2", Ok(a2)),
            CheckReport::from_result("A3", Ok(a3)),
        ]),
        Err(e) => reports
            .extend(["A1", "A2", "A3"].map(|id| CheckReport::from_result(id, Err(e.clone())))),
    }
    reports.push(CheckReport::from_result("A4", a4(factory, fixtures)));
    match sequence_b(factory, fixtures) {
        Ok([b1, b2]) => reports.extend([
            CheckReport::from_result("B1", Ok(b1)),
            CheckReport::from_result("B2", Ok(b2)),
        ]),
        Err(e) => {
            reports.extend(["B1", "B2"].map(|id| CheckReport::from_result(id, Err(e.clone()))));
        }
    }
    reports.push(CheckReport::from_result("B3", b3(factory, fixtures)));
    reports
}

const fn layer_name(layer: Layer) -> &'static str {
    match layer {
        Layer::Sandbox => "sandbox",
        Layer::Shadow => "shadow",
    }
}

fn start(
    factory: &mut AssistFactory<'_>,
    fixtures: &ArchFixtures,
) -> Result<Box<dyn AssistEngine>, EngineError> {
    let mut engine = factory();
    engine.start(
        fixtures.position(positions::M)?,
        &fixtures.spec.seed,
        &fixtures.spec.game,
    )?;
    Ok(engine)
}

/// Plays the first `n` decisions of path P as ordinary decisions.
fn prefix(
    engine: &mut dyn AssistEngine,
    fixtures: &ArchFixtures,
    n: usize,
) -> Result<Vec<Layered<Step>>, EngineError> {
    fixtures
        .spec
        .replay
        .path
        .iter()
        .take(n)
        .map(|d| engine.act(d))
        .collect()
}

fn query(
    engine: &dyn AssistEngine,
    layer: Layer,
    path: &str,
) -> Result<Option<Value>, EngineError> {
    engine.query(layer, View::Omniscient, path)
}

/// `path` in `layer` equals `want` under the runner's rules.
fn expect(
    engine: &dyn AssistEngine,
    layer: Layer,
    path: &str,
    want: &Value,
    label: &str,
) -> Checked {
    let got = query(engine, layer, path)?;
    Ok(
        if got
            .as_ref()
            .is_some_and(|g| compare::assert_value(path, want, g))
        {
            Vec::new()
        } else {
            vec![format!(
                "{label}: {} {path} is {got:?}, expected {want}",
                layer_name(layer)
            )]
        },
    )
}

fn acted(value: bool) -> Value {
    serde_json::json!({ "acted": value })
}

fn digests(engine: &dyn AssistEngine) -> Result<(String, String), EngineError> {
    Ok((
        engine.digest(Layer::Sandbox)?,
        engine.digest(Layer::Shadow)?,
    ))
}

/// The divergences added since `before` entries.
fn new_divergences(engine: &dyn AssistEngine, before: usize) -> Result<Vec<Value>, EngineError> {
    Ok(engine.divergences()?.into_iter().skip(before).collect())
}

fn divergence_id(record: &Value) -> Result<String, EngineError> {
    record
        .get("id")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| EngineError::Adapter(format!("divergence without id: {record}")))
}

/// A record kept with its original fields and only `resolved` set to `mode`.
fn kept(engine: &dyn AssistEngine, original: &Value, mode: &str, label: &str) -> Checked {
    let id = divergence_id(original)?;
    let all = engine.divergences()?;
    let matching: Vec<&Value> = all
        .iter()
        .filter(|d| d.get("id").and_then(Value::as_str) == Some(id.as_str()))
        .collect();
    let [record] = matching.as_slice() else {
        return Ok(vec![format!(
            "{label}: divergence {id} appears {} times",
            matching.len()
        )]);
    };
    let mut fails = Vec::new();
    for field in ["kind", "expected", "actual", "refs"] {
        if record.get(field) != original.get(field) {
            fails.push(format!(
                "{label}: divergence {id} field {field} was rewritten"
            ));
        }
    }
    if record.get("resolved").and_then(Value::as_str) != Some(mode) {
        fails.push(format!(
            "{label}: divergence {id} resolved is {:?}, expected {mode}",
            record.get("resolved")
        ));
    }
    Ok(fails)
}

/// Sequence A up to the manual attack; returns the digests after the prefix and the new divergence.
struct Diverged {
    engine: Box<dyn AssistEngine>,
    /// Both layers' steps of the ordinary decisions before the manual one.
    before: Vec<Layered<Step>>,
    aligned: (String, String),
    record: Option<Value>,
    a1: Vec<String>,
}

fn diverge(
    factory: &mut AssistFactory<'_>,
    fixtures: &ArchFixtures,
) -> Result<Diverged, EngineError> {
    let spec = &fixtures.spec.assist;
    let mut engine = start(factory, fixtures)?;
    let before_steps = prefix(&mut *engine, fixtures, spec.prefix)?;
    let mut a1 = Vec::new();
    let aligned = digests(&*engine)?;
    if aligned.0 != aligned.1 {
        a1.push("sandbox and shadow differ before any manual operation".to_owned());
    }
    let before = engine.divergences()?.len();
    engine.act(&spec.manual_attack)?;
    a1.extend(expect(
        &*engine,
        Layer::Sandbox,
        &spec.a2_path,
        &acted(true),
        "A1",
    )?);
    a1.extend(expect(
        &*engine,
        Layer::Shadow,
        &spec.a2_path,
        &acted(false),
        "A1",
    )?);
    let shadow_awaiting = engine.observe(Layer::Shadow, View::Omniscient)?.awaiting;
    if !shadow_awaiting
        .as_ref()
        .is_some_and(|a| has_match(a, &spec.pending))
    {
        a1.push(format!(
            "A1: shadow awaiting {shadow_awaiting:?}, expected the pending {}",
            spec.pending
        ));
    }
    if engine.digest(Layer::Shadow)? != aligned.1 {
        a1.push("A1: the shadow changed with a manual operation".to_owned());
    }
    let added = new_divergences(&*engine, before)?;
    let record = added.first().cloned();
    match (added.as_slice(), &record) {
        ([_], Some(r)) => a1.extend(missed_record(r, spec)),
        _ => a1.push(format!("A1: expected one new divergence, got {added:?}")),
    }
    Ok(Diverged {
        engine,
        before: before_steps,
        aligned,
        record,
        a1,
    })
}

fn missed_record(record: &Value, spec: &super::AssistSpec) -> Vec<String> {
    let mut fails = Vec::new();
    let kind = record.get("kind").and_then(Value::as_str);
    if !matches!(kind, Some("missed" | "illegal")) {
        fails.push(format!("A1: divergence kind {kind:?}"));
    }
    if !record
        .get("expected")
        .is_some_and(|e| has_match(e, &spec.pending))
    {
        fails.push(format!(
            "A1: divergence expected {:?} does not point to {}",
            record.get("expected"),
            spec.pending
        ));
    }
    let refs: Vec<&str> = record
        .get("refs")
        .and_then(Value::as_array)
        .map(|r| r.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    if !spec.refs_any.iter().any(|r| refs.contains(&r.as_str())) {
        fails.push(format!(
            "A1: divergence refs {refs:?}, expected one of {:?}",
            spec.refs_any
        ));
    }
    if record.get("resolved").is_some_and(|r| !r.is_null()) {
        fails.push("A1: a new divergence is already resolved".to_owned());
    }
    fails
}

/// A2: automation refuses while diverged and nothing moves.
fn halted(engine: &mut dyn AssistEngine, label: &str) -> Checked {
    let mut fails = Vec::new();
    if engine.auto_advance()? {
        fails.push(format!("{label}: auto_advance is on while diverged"));
    }
    let before = digests(engine)?;
    if engine.advance()?.is_some() {
        fails.push(format!("{label}: advance was not refused while diverged"));
    }
    if digests(engine)? != before {
        fails.push(format!("{label}: a refused advance changed the state"));
    }
    Ok(fails)
}

fn sequence_a(
    factory: &mut AssistFactory<'_>,
    replay: &mut ReplayFactory<'_>,
    fixtures: &ArchFixtures,
    options: &ArchOptions,
) -> Result<[Vec<String>; 3], EngineError> {
    let Diverged {
        mut engine,
        before,
        aligned,
        record,
        a1,
    } = diverge(factory, fixtures)?;
    let a2 = halted(&mut *engine, "A2")?;
    let Some(record) = record else {
        return Ok([a1, a2, vec!["A3: no divergence to realign".to_owned()]]);
    };
    let spec = &fixtures.spec.assist;
    engine.realign(&divergence_id(&record)?, Realign::Rewind)?;
    let mut a3 = Vec::new();
    for layer in LAYERS {
        a3.extend(expect(&*engine, layer, &spec.a2_path, &acted(false), "A3")?);
    }
    let now = digests(&*engine)?;
    if now.0 != now.1 || now != aligned {
        a3.push("A3: after Rewind the layers do not match the aligned state".to_owned());
    }
    if !engine.auto_advance()? {
        a3.push("A3: auto_advance is still off after Rewind".to_owned());
    }
    a3.extend(kept(&*engine, &record, "rewind", "A3")?);
    a3.extend(resume(&mut *engine, &before, replay, fixtures, options)?);
    Ok([a1, a2, a3])
}

/// A3: path P continued after Rewind. Each step, in each layer, has the outcome and the
/// events (causes included, ids relabelled per source) of the same step played by the
/// design's replay engine; the third-party fixed assertions run at the nodes reached.
fn resume(
    engine: &mut dyn AssistEngine,
    before: &[Layered<Step>],
    replay: &mut ReplayFactory<'_>,
    fixtures: &ArchFixtures,
    options: &ArchOptions,
) -> Checked {
    let spec = &fixtures.spec;
    let first = spec.assist.prefix;
    let path: Vec<&Value> = spec
        .replay
        .path
        .iter()
        .take(spec.assist.resume_to)
        .collect();
    let baseline = replay_steps(replay, fixtures, &path)?;
    let mut sandbox: Vec<Step> = before.iter().map(|s| s.sandbox.clone()).collect();
    let mut shadow: Vec<Step> = before.iter().map(|s| s.shadow.clone()).collect();
    let mut fails = Vec::new();
    let mut decks: [Option<Vec<String>>; 2] = [None, None];
    for (i, decision) in path.iter().enumerate().skip(first) {
        let got = engine.act(decision)?;
        sandbox.push(got.sandbox);
        shadow.push(got.shadow);
        for (layer, deck) in LAYERS.into_iter().zip(decks.iter_mut()) {
            fails.extend(fixed_at(
                &*engine,
                fixtures,
                layer,
                i.saturating_add(1),
                deck,
            )?);
        }
    }
    let want = relabelled(&baseline, options);
    for (layer, steps) in [(Layer::Sandbox, &sandbox), (Layer::Shadow, &shadow)] {
        let got = relabelled(steps, options);
        for (i, ((out_got, ev_got), (out_want, ev_want))) in
            got.iter().zip(&want).enumerate().skip(first)
        {
            let node = i.saturating_add(1);
            if out_got != out_want {
                fails.push(format!(
                    "A3 N{node}: {} outcome {out_got}, replay {out_want}",
                    layer_name(layer)
                ));
            }
            if ev_got != ev_want {
                fails.push(format!(
                    "A3 N{node}: {} events {ev_got}, replay {ev_want}",
                    layer_name(layer)
                ));
            }
        }
    }
    Ok(fails)
}

/// Path P played from the start by a fresh replay engine: the baseline of A3.
fn replay_steps(
    replay: &mut ReplayFactory<'_>,
    fixtures: &ArchFixtures,
    path: &[&Value],
) -> Result<Vec<Step>, EngineError> {
    let mut engine = replay();
    let mut at = engine.start(
        fixtures.position(positions::M)?,
        &fixtures.spec.seed,
        &fixtures.spec.game,
    )?;
    let mut steps = Vec::new();
    for decision in path {
        let (next, step) = engine.decide(&at, decision)?;
        steps.push(step);
        at = next;
    }
    Ok(steps)
}

/// Outcome and events of each step, event ids labelled by first appearance over the whole
/// history of that source and decision causes reduced to a marker.
fn relabelled(steps: &[Step], options: &ArchOptions) -> Vec<(String, Value)> {
    let mut ids = Ids::default();
    for step in steps {
        ids.events(&step.events);
    }
    steps
        .iter()
        .map(|step| {
            let mut events = Value::from(step.events.clone());
            mark_decisions(&mut events);
            (step.outcome.clone(), normalize(&events, &ids, options))
        })
        .collect()
}

/// The replay fixed assertions for node `node`, asked of one assist layer.
fn fixed_at(
    engine: &dyn AssistEngine,
    fixtures: &ArchFixtures,
    layer: Layer,
    node: usize,
    deck: &mut Option<Vec<String>>,
) -> Checked {
    let mut fails = Vec::new();
    let label = format!("A3 {}", layer_name(layer));
    for fixed in fixtures.spec.replay.fixed.iter().filter(|f| f.node == node) {
        let query = |path: &str| engine.query(layer, View::Omniscient, path);
        let awaiting = || Ok(engine.observe(layer, View::Omniscient)?.awaiting);
        let probe = Probe {
            query: &query,
            awaiting: &awaiting,
        };
        fails.extend(check_fixed(&probe, fixed, deck, &label)?);
    }
    Ok(fails)
}

fn a4(factory: &mut AssistFactory<'_>, fixtures: &ArchFixtures) -> Checked {
    let spec = &fixtures.spec.assist;
    let Diverged {
        mut engine, record, ..
    } = diverge(factory, fixtures)?;
    let Some(record) = record else {
        return Ok(vec!["A4: no divergence to adopt".to_owned()]);
    };
    engine.realign(&divergence_id(&record)?, Realign::Adopt)?;
    let mut fails = expect(&*engine, Layer::Shadow, &spec.a2_path, &acted(true), "A4")?;
    let (sandbox, shadow) = digests(&*engine)?;
    if sandbox != shadow {
        fails.push("A4: layers differ after Adopt".to_owned());
    }
    fails.extend(kept(&*engine, &record, "adopt", "A4")?);
    let attack = engine.act(&spec.other_attack)?;
    for (layer, step) in [
        (Layer::Sandbox, &attack.sandbox),
        (Layer::Shadow, &attack.shadow),
    ] {
        if step.outcome != "cannot-attack" {
            fails.push(format!(
                "A4: {} outcome {} for an attack with a trigger pending",
                layer_name(layer),
                step.outcome
            ));
        }
    }
    let awaiting = engine.observe(Layer::Shadow, View::Omniscient)?.awaiting;
    if !awaiting
        .as_ref()
        .is_some_and(|a| has_match(a, &spec.pending))
    {
        fails.push(format!(
            "A4: shadow awaiting {awaiting:?}, expected the pending {}",
            spec.pending
        ));
    }
    let choose = fixtures
        .spec
        .replay
        .path
        .get(spec.prefix)
        .cloned()
        .unwrap_or(Value::Null);
    engine.act(&choose)?;
    let next = engine.observe(Layer::Shadow, View::Omniscient)?.awaiting;
    if !super::support::awaits(next.as_ref(), "P1", &spec.search_do) {
        fails.push(format!(
            "A4: after resolving the trigger the shadow awaits {next:?}"
        ));
    }
    Ok(fails)
}

/// Sequence B up to the manual override; returns the engine and the new divergence.
/// The engine after the override, the new divergence and B1's failures.
type Overridden = (Box<dyn AssistEngine>, Option<Value>, Vec<String>);

fn overridden(
    factory: &mut AssistFactory<'_>,
    fixtures: &ArchFixtures,
) -> Result<Overridden, EngineError> {
    let spec = &fixtures.spec.assist;
    let mut engine = start(factory, fixtures)?;
    prefix(&mut *engine, fixtures, spec.override_after)?;
    let before = engine.divergences()?.len();
    engine.act(&spec.manual_set)?;
    let mut fails = expect(
        &*engine,
        Layer::Sandbox,
        &spec.life_path,
        &Value::from(spec.life_manual),
        "B1",
    )?;
    fails.extend(expect(
        &*engine,
        Layer::Shadow,
        &spec.life_path,
        &Value::from(spec.life_shadow),
        "B1",
    )?);
    let added = new_divergences(&*engine, before)?;
    let record = added.first().cloned();
    let fits = record.as_ref().is_some_and(|r| {
        r.get("kind").and_then(Value::as_str) == Some("mismatch")
            && r.get("expected")
                .is_some_and(|e| has_match(e, &Value::from(spec.life_shadow)))
            && r.get("actual")
                .is_some_and(|a| has_match(a, &Value::from(spec.life_manual)))
            && compare::contains_text(r, &spec.life_path)
    });
    if added.len() != 1 || !fits {
        fails.push(format!(
            "B1: expected one mismatch divergence on {}, got {added:?}",
            spec.life_path
        ));
    }
    if engine.auto_advance()? {
        fails.push("B1: auto_advance is on while diverged".to_owned());
    }
    Ok((engine, record, fails))
}

/// Realigns, checks both layers, then attacks once more and checks the result.
fn realign_and_attack(
    engine: &mut dyn AssistEngine,
    fixtures: &ArchFixtures,
    record: &Value,
    mode: Realign,
    label: &str,
) -> Checked {
    let spec = &fixtures.spec.assist;
    engine.realign(&divergence_id(record)?, mode)?;
    let (now, after, name) = match mode {
        Realign::Rewind => (spec.life_shadow, spec.life_after_rewind, "rewind"),
        Realign::Adopt => (spec.life_manual, spec.life_after_adopt, "adopt"),
    };
    let mut fails = Vec::new();
    for layer in LAYERS {
        fails.extend(expect(
            engine,
            layer,
            &spec.life_path,
            &Value::from(now),
            label,
        )?);
    }
    fails.extend(kept(engine, record, name, label)?);
    engine.act(&spec.other_attack)?;
    engine.act(&spec.opponent_pass)?;
    for layer in LAYERS {
        fails.extend(expect(
            engine,
            layer,
            &spec.life_path,
            &Value::from(after),
            label,
        )?);
    }
    Ok(fails)
}

fn sequence_b(
    factory: &mut AssistFactory<'_>,
    fixtures: &ArchFixtures,
) -> Result<[Vec<String>; 2], EngineError> {
    let (mut engine, record, b1) = overridden(factory, fixtures)?;
    let b2 = match record {
        Some(r) => realign_and_attack(&mut *engine, fixtures, &r, Realign::Rewind, "B2")?,
        None => vec!["B2: no divergence to realign".to_owned()],
    };
    Ok([b1, b2])
}

fn b3(factory: &mut AssistFactory<'_>, fixtures: &ArchFixtures) -> Checked {
    let (mut engine, record, _) = overridden(factory, fixtures)?;
    record.map_or_else(
        || Ok(vec!["B3: no divergence to adopt".to_owned()]),
        |r| realign_and_attack(&mut *engine, fixtures, &r, Realign::Adopt, "B3"),
    )
}
