//! Runs the checks of every AI position (design sections 1 and 2).

use alloc::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde_json::{Map, Value};

use super::{
    AiCheckReport, AiEngine, AiFactory, AiOutcome, AiPosition, AiPositions, AiReport, Before,
    Branch, Check, LOAD_SEED, ProfileEntry, Think,
};
use crate::compare;
use crate::engine::{EngineError, Step, View};
use crate::inherit::Fixture;
use crate::model::from_yaml;
use crate::runner::card_numbers;

type Checked = Result<Vec<String>, EngineError>;

const PROFILE_IDS: [&str; 3] = ["general", "aggro", "control"];
/// Zones every player can see; a hidden card whose number is also here cannot be told apart.
const PUBLIC_ZONES: [&str; 4] = ["field", "cemetery", "banish", "ex"];

/// Runs every check of every position against fresh instances from `factory`.
///
/// `profiles` is the design's `profiles.yaml` (check P0).
pub fn check_ai(
    factory: &mut AiFactory<'_>,
    positions: &AiPositions,
    profiles: &Path,
) -> Vec<AiCheckReport> {
    let mut out = vec![report("profiles", "P0", Ok(check_profiles(profiles)))];
    let mut cache: BTreeMap<(String, String), AiReport> = BTreeMap::new();
    for (stem, position) in &positions.0 {
        let ctx = Ctx {
            positions,
            stem,
            position,
        };
        for check in &position.checks {
            out.extend(ctx.run_check(factory, check, &mut cache));
        }
        out.extend(ctx.diagnostics(&cache));
    }
    out
}

fn report(position: &str, id: &str, result: Checked) -> AiCheckReport {
    let outcome = match result {
        Ok(reasons) if reasons.is_empty() => AiOutcome::Pass,
        Ok(reasons) => AiOutcome::Fail { reasons },
        Err(EngineError::Unsupported(reason)) => AiOutcome::Unsupported { reason },
        Err(EngineError::Adapter(reason)) => AiOutcome::AdapterError { reason },
    };
    AiCheckReport {
        position: position.to_owned(),
        id: id.to_owned(),
        outcome,
    }
}

fn diagnostic(position: &str, id: &str, notes: Vec<String>) -> AiCheckReport {
    AiCheckReport {
        position: position.to_owned(),
        id: id.to_owned(),
        outcome: AiOutcome::Diagnostic { notes },
    }
}

fn check_profiles(path: &Path) -> Vec<String> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) => return vec![format!("{}: {e}", path.display())],
    };
    let entries: Vec<ProfileEntry> = match from_yaml(&text) {
        Ok(entries) => entries,
        Err(e) => return vec![format!("{}: {e}", path.display())],
    };
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let mut reasons = Vec::new();
    for want in PROFILE_IDS {
        let Some(entry) = entries.iter().find(|e| e.id.as_deref() == Some(want)) else {
            reasons.push(format!("profile {want} is missing"));
            continue;
        };
        if entry.subtype.as_deref().is_none_or(str::is_empty) {
            reasons.push(format!("profile {want} has no subtype"));
        }
        if entry.period.as_deref().is_none_or(str::is_empty) {
            reasons.push(format!("profile {want} has no period"));
        }
        match entry.file.as_deref() {
            Some(file) if dir.join(file).is_file() => {}
            _ => reasons.push(format!("profile {want}: file missing")),
        }
    }
    reasons
}

fn view(name: &str) -> Result<View, EngineError> {
    View::parse(name).ok_or_else(|| EngineError::Adapter(format!("unknown view {name:?}")))
}

/// Concatenates the lists of two `random` blocks, key by key (parent first).
fn merge_random(base: &Value, more: Option<&Value>) -> Value {
    let mut out = base.as_object().cloned().unwrap_or_default();
    for (key, list) in more.and_then(Value::as_object).into_iter().flatten() {
        let mut items = out
            .get(key)
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        items.extend(list.as_array().cloned().unwrap_or_default());
        out.insert(key.clone(), Value::Array(items));
    }
    Value::Object(out)
}

fn fixture(position: &AiPosition, random: Value) -> Fixture {
    let mut setup = position.setup.clone();
    if let (Some(room), Value::Object(map)) = (&position.room, &mut setup) {
        map.insert("room".to_owned(), room.clone());
    }
    Fixture {
        question: position.id.clone(),
        scenario: position.id.clone(),
        setup,
        card_facts: Value::Object(Map::new()),
        random,
    }
}

/// Decisions compare as contract objects, except that lists of chosen targets or
/// selected objects are sets.
fn same_decision(a: &Value, b: &Value) -> bool {
    compare::exact(&canonical(a), &canonical(b))
}

fn canonical(decision: &Value) -> Value {
    let mut out = decision.clone();
    if let Some(Value::Object(targets)) = out.get_mut("targets") {
        targets.values_mut().for_each(sort_list);
    }
    if let Some(select) = out.get_mut("select") {
        sort_list(select);
    }
    out
}

fn sort_list(list: &mut Value) {
    if let Value::Array(items) = list {
        items.sort_by_key(ToString::to_string);
    }
}

/// Whether two lists hold the same decisions, as multisets.
fn same_set(expected: &[Value], actual: &[Value]) -> Vec<String> {
    let mut left: Vec<&Value> = actual.iter().collect();
    let mut reasons = Vec::new();
    for want in expected {
        match left.iter().position(|got| same_decision(want, got)) {
            Some(i) => {
                left.remove(i);
            }
            None => reasons.push(format!("legal set lacks {want}")),
        }
    }
    reasons.extend(
        left.iter()
            .map(|extra| format!("legal set has extra {extra}")),
    );
    reasons
}

fn engine_fail(step: &Step, decision: &Value) -> Option<String> {
    step.outcome
        .starts_with("cannot")
        .then(|| format!("decision {decision} was rejected: {}", step.outcome))
}

/// Loads a fresh instance and submits `decisions` with `random`.
fn at(
    factory: &mut AiFactory<'_>,
    position: &AiPosition,
    decisions: &[Value],
    random: &Value,
) -> Result<(Box<dyn AiEngine>, Vec<String>), EngineError> {
    let mut engine = factory();
    engine.load(&fixture(position, random.clone()), LOAD_SEED)?;
    let mut reasons = Vec::new();
    for decision in decisions {
        let step = engine.decide(decision)?;
        reasons.extend(engine_fail(&step, decision));
    }
    Ok((engine, reasons))
}

fn before_of(position: &AiPosition, check: &Check) -> Vec<Value> {
    match check.before() {
        None => Vec::new(),
        Some(Before::Decisions(list)) => list,
        Some(Before::Ref(id)) => position
            .checks
            .iter()
            .find(|c| c.id == id)
            .map(|c| before_of(position, c))
            .unwrap_or_default(),
    }
}

/// Calls `think`; the decision must be legal here and within the budget.
fn think(
    engine: &mut dyn AiEngine,
    call: &Think,
    reasons: &mut Vec<String>,
) -> Result<AiReport, EngineError> {
    let got = engine.think(view(&call.seat)?, &call.profile, call.budget, &call.seed)?;
    if got.edges > call.budget {
        reasons.push(format!(
            "{} edges exceed the budget of {}",
            got.edges, call.budget
        ));
    }
    if !engine
        .legal()?
        .iter()
        .any(|d| same_decision(d, &got.decision))
    {
        reasons.push(format!("decision {} is not legal here", got.decision));
    }
    Ok(got)
}

struct Ctx<'ctx> {
    positions: &'ctx AiPositions,
    stem: &'ctx str,
    position: &'ctx AiPosition,
}

impl Ctx<'_> {
    fn pair(&self) -> Result<&AiPosition, EngineError> {
        let stem = self
            .position
            .pair
            .as_deref()
            .ok_or_else(|| EngineError::Adapter(format!("{} has no pair", self.stem)))?;
        self.positions
            .0
            .get(stem)
            .ok_or_else(|| EngineError::Adapter(format!("pair {stem:?} is not loaded")))
    }

    fn legal_of(&self, id: &str) -> Option<&Vec<Value>> {
        self.position
            .checks
            .iter()
            .find(|c| c.id == id)
            .and_then(|c| c.legal_exact.as_ref())
    }

    fn run_check(
        &self,
        factory: &mut AiFactory<'_>,
        check: &Check,
        cache: &mut BTreeMap<(String, String), AiReport>,
    ) -> Vec<AiCheckReport> {
        let id = self.position.id.as_str();
        if !check.same_instance.is_empty() {
            let (decisions, unchanged) = self.same_instance(factory, check);
            let mut out = vec![report(id, &check.id, decisions)];
            if check.unchanged.is_some() {
                out.push(report(id, "P2", unchanged));
            }
            return out;
        }
        vec![report(id, &check.id, self.single(factory, check, cache))]
    }

    fn single(
        &self,
        factory: &mut AiFactory<'_>,
        check: &Check,
        cache: &mut BTreeMap<(String, String), AiReport>,
    ) -> Checked {
        let before = before_of(self.position, check);
        let (mut engine, mut reasons) = at(factory, self.position, &before, &self.position.random)?;
        if let Some(want) = &check.legal_exact {
            reasons.extend(same_set(want, &engine.legal()?));
        }
        for branch in &check.branches {
            reasons.extend(self.branch(factory, &before, &self.position.random, branch)?);
        }
        if let Some(seat) = &check.projection_equal_to_pair {
            let pair = self.pair()?;
            let (other, other_reasons) = at(factory, pair, &before_of(pair, check), &pair.random)?;
            reasons.extend(other_reasons);
            if engine.projection(view(seat)?)? != other.projection(view(seat)?)? {
                reasons.push(format!("{seat} projection differs from the pair's"));
            }
        }
        if let (Some(query), Some(part)) = (&check.query, &check.equals_setup) {
            reasons.extend(self.query_setup(engine.as_ref(), &query.view, &query.path, part)?);
        }
        if let Some(seat) = &check.projection_hides_from {
            reasons.extend(self.hides(engine.as_ref(), seat, &check.objects)?);
        }
        if let Some(call) = &check.think {
            let got = think(engine.as_mut(), call, &mut reasons)?;
            if let Some(from) = &check.decision_in {
                let allowed = self.legal_of(from).cloned().unwrap_or_default();
                if !allowed.iter().any(|d| same_decision(d, &got.decision)) {
                    reasons.push(format!("decision {} is not in {from}", got.decision));
                }
            }
            if let Some(fields) = &check.equal_to_pair {
                let pair = self.pair()?;
                let (mut other, other_reasons) =
                    at(factory, pair, &before_of(pair, check), &pair.random)?;
                reasons.extend(other_reasons);
                let theirs = think(other.as_mut(), call, &mut reasons)?;
                reasons.extend(compare_reports(fields, &got, &theirs));
            }
            cache.insert((self.stem.to_owned(), check.id.clone()), got);
        }
        Ok(reasons)
    }

    fn branch(
        &self,
        factory: &mut AiFactory<'_>,
        parent: &[Value],
        parent_random: &Value,
        branch: &Branch,
    ) -> Checked {
        let mut decisions = parent.to_vec();
        decisions.extend(branch.before.iter().cloned());
        let random = merge_random(parent_random, branch.random.as_ref());
        let (engine, found) = at(factory, self.position, &decisions, &random)?;
        let mut reasons: Vec<String> = found
            .into_iter()
            .map(|r| format!("{}: {r}", branch.name))
            .collect();
        if let Some(want) = &branch.legal_exact {
            reasons.extend(
                same_set(want, &engine.legal()?)
                    .into_iter()
                    .map(|r| format!("{}: {r}", branch.name)),
            );
        }
        for child in &branch.branches {
            reasons.extend(self.branch(factory, &decisions, &random, child)?);
        }
        Ok(reasons)
    }

    fn query_setup(&self, engine: &dyn AiEngine, seat: &str, path: &str, part: &str) -> Checked {
        let want = part
            .split('.')
            .try_fold(self.position.setup.get("players"), |node, key| {
                node.map(|n| n.get(key))
            })
            .flatten()
            .cloned()
            .unwrap_or(Value::Null);
        let got = engine.query(view(seat)?, path)?.unwrap_or(Value::Null);
        Ok(if compare::subset(&want, &got) {
            Vec::new()
        } else {
            vec![format!(
                "{seat} query {path} is {got}, the setup has {want}"
            )]
        })
    }

    fn hides(&self, engine: &dyn AiEngine, seat: &str, objects: &[String]) -> Checked {
        let seen = engine.projection(view(seat)?)?;
        let numbers = card_numbers(&self.position.setup);
        let public = public_numbers(&self.position.setup);
        let mut reasons = Vec::new();
        for object in objects {
            if compare::mentions(&seen, &Value::from(object.as_str())) {
                reasons.push(format!("{seat} projection mentions hidden {object}"));
            }
            if let Some(card) = numbers.get(object)
                && !public.iter().any(|p| p == card)
                && compare::contains_text(&seen, card)
            {
                reasons.push(format!(
                    "{seat} projection mentions {card} of hidden {object}"
                ));
            }
        }
        Ok(reasons)
    }

    fn same_instance(&self, factory: &mut AiFactory<'_>, check: &Check) -> (Checked, Checked) {
        let before = before_of(self.position, check);
        let mut decided = Vec::new();
        let mut unchanged = Vec::new();
        let run = at(factory, self.position, &before, &self.position.random).and_then(
            |(mut engine, found)| {
                decided.extend(found);
                for expect in &check.same_instance {
                    let before_think = engine.projection(View::Omniscient)?;
                    let got = think(engine.as_mut(), &expect.think, &mut decided)?;
                    if !same_decision(&expect.decision, &got.decision) {
                        decided.push(format!(
                            "profile {} chose {}, expected {}",
                            expect.think.profile, got.decision, expect.decision
                        ));
                    }
                    if engine.projection(View::Omniscient)? != before_think {
                        unchanged.push(format!(
                            "think with profile {} changed the position",
                            expect.think.profile
                        ));
                    }
                }
                Ok(())
            },
        );
        match run {
            Ok(()) => (Ok(decided), Ok(unchanged)),
            Err(e) => (Err(e.clone()), Err(e)),
        }
    }

    fn diagnostics(&self, cache: &BTreeMap<(String, String), AiReport>) -> Vec<AiCheckReport> {
        let id = self.position.id.as_str();
        let mut out = Vec::new();
        if let Some(q2) = cache.get(&(self.stem.to_owned(), "Q2".to_owned())) {
            let found = q2.trace.iter().any(|node| {
                node.get("by").and_then(Value::as_str) == Some("P2")
                    && node.get("at").and_then(Value::as_str) == Some("quick")
                    && node
                        .get("options")
                        .and_then(Value::as_array)
                        .is_some_and(|o| {
                            o.iter()
                                .any(|d| d.get("do").and_then(Value::as_str) == Some("play"))
                        })
            });
            out.push(diagnostic(
                id,
                "Q3",
                vec![format!("self-reported trace has a P2 Quick play: {found}")],
            ));
        }
        if let (Some(pair), Some(mine)) = (
            self.position.pair.as_deref(),
            cache.get(&(self.stem.to_owned(), "R2".to_owned())),
        ) && let Some(theirs) = cache.get(&(pair.to_owned(), "R2".to_owned()))
        {
            out.push(diagnostic(
                id,
                "R4",
                vec![format!(
                    "choice after the reveal: {} here, {} in {pair}; candidates differ: {}",
                    mine.decision,
                    theirs.decision,
                    mine.candidates != theirs.candidates
                )],
            ));
        }
        out
    }
}

fn public_numbers(setup: &Value) -> Vec<String> {
    let players = setup.get("players").and_then(Value::as_object);
    players
        .into_iter()
        .flat_map(|p| p.values())
        .filter_map(|p| p.get("zones"))
        .flat_map(|zones| PUBLIC_ZONES.iter().filter_map(move |z| zones.get(*z)))
        .filter_map(Value::as_array)
        .flatten()
        .filter_map(|card| card.get("card").and_then(Value::as_str))
        .map(ToOwned::to_owned)
        .collect()
}

fn compare_reports(fields: &[String], mine: &AiReport, theirs: &AiReport) -> Vec<String> {
    let mut reasons = Vec::new();
    for field in fields {
        let same = match field.as_str() {
            "decision" => same_decision(&mine.decision, &theirs.decision),
            "candidates" => {
                mine.candidates.len() == theirs.candidates.len()
                    && mine
                        .candidates
                        .iter()
                        .zip(&theirs.candidates)
                        .all(|((a, x), (b, y))| same_decision(a, b) && x.to_bits() == y.to_bits())
            }
            other => {
                reasons.push(format!("unknown report field {other:?}"));
                continue;
            }
        };
        if !same {
            reasons.push(format!("{field} differs from the pair's"));
        }
    }
    reasons
}
