//! R0–R6: replay, branches, undo and causality (design sections 3 and 5).

use alloc::collections::BTreeMap;

use serde_json::Value;

use super::support::{
    Ids, Probe, awaits, check_fixed, has_match, hidden_at, known, leaks, normalize,
    observation_value, view_name,
};
use super::{
    ArchFixtures, ArchOptions, BranchKind, CheckReport, NodeId, ReplayEngine, ReplayFactory,
    positions,
};
use crate::compare;
use crate::engine::{EngineError, Step, View};
use crate::runner::card_numbers;

type Checked = Result<Vec<String>, EngineError>;

const VIEWS: [View; 3] = [View::Omniscient, View::P1, View::P2];
const PLAYERS: [View; 2] = [View::P1, View::P2];
/// Longest cause chain followed before calling it a cycle.
const CHAIN_LIMIT: usize = 64;

/// Runs R0–R6 against fresh instances from `factory`.
pub fn check_replay(
    factory: &mut ReplayFactory<'_>,
    fixtures: &ArchFixtures,
    options: &ArchOptions,
) -> Vec<CheckReport> {
    vec![
        CheckReport::from_result("R0", r0(factory, fixtures)),
        CheckReport::from_result("R1", r1(factory, fixtures, options)),
        CheckReport::from_result("R2", r2(factory, fixtures)),
        CheckReport::from_result("R2b", r2b(factory, fixtures)),
        CheckReport::from_result("R3", r3(factory, fixtures)),
        CheckReport::from_result("R4", r4(factory, fixtures)),
        CheckReport::from_result("R5", r5(factory, fixtures, options)),
        CheckReport::from_result("R6", r6(factory, fixtures)),
    ]
}

/// One engine instance and the ids it has issued to us, in order.
struct Run {
    engine: Box<dyn ReplayEngine>,
    ids: Ids,
}

/// Node number on path P → node id.
type Line = Vec<(usize, NodeId)>;

impl Run {
    fn start(
        factory: &mut ReplayFactory<'_>,
        fixtures: &ArchFixtures,
        name: &str,
        seed: &str,
    ) -> Result<(Self, NodeId), EngineError> {
        let mut engine = factory();
        let root = engine.start(fixtures.position(name)?, seed, &fixtures.spec.game)?;
        let mut ids = Ids::default();
        ids.node(&root);
        Ok((Self { engine, ids }, root))
    }

    /// Plays `decisions` from node `at` (number `first`); returns every node, `at` included.
    fn walk(
        &mut self,
        at: &NodeId,
        first: usize,
        decisions: &[Value],
    ) -> Result<(Line, Vec<Step>), EngineError> {
        let mut line = vec![(first, at.clone())];
        let mut steps = Vec::new();
        let mut current = at.clone();
        for (i, decision) in decisions.iter().enumerate() {
            let (next, step) = self.engine.decide(&current, decision)?;
            self.ids.node(&next);
            self.ids.events(&step.events);
            line.push((first.saturating_add(i).saturating_add(1), next.clone()));
            steps.push(step);
            current = next;
        }
        Ok((line, steps))
    }

    fn branch(&mut self, at: &NodeId, kind: BranchKind) -> Result<NodeId, EngineError> {
        let root = self.engine.branch(at, kind)?;
        self.ids.node(&root);
        Ok(root)
    }

    fn fixed(
        &self,
        fixtures: &ArchFixtures,
        line: &Line,
        label: &str,
    ) -> Result<(Vec<String>, Option<Vec<String>>), EngineError> {
        let mut fails = Vec::new();
        let mut deck = None;
        for fixed in &fixtures.spec.replay.fixed {
            let Some((_, node)) = line.iter().find(|(k, _)| *k == fixed.node) else {
                continue;
            };
            let query = |path: &str| self.engine.query(node, View::Omniscient, path);
            let awaiting = || Ok(self.engine.observe(node, View::Omniscient)?.awaiting);
            let probe = Probe {
                query: &query,
                awaiting: &awaiting,
            };
            fails.extend(check_fixed(&probe, fixed, &mut deck, label)?);
        }
        Ok((fails, deck))
    }
}

fn node_at(line: &Line, k: usize) -> Result<&NodeId, EngineError> {
    line.iter()
        .find(|(n, _)| *n == k)
        .map(|(_, id)| id)
        .ok_or_else(|| EngineError::Adapter(format!("no node N{k} on the line")))
}

fn head(path: &[Value], n: usize) -> &[Value] {
    path.get(..n).unwrap_or(path)
}

fn tail(path: &[Value], n: usize) -> &[Value] {
    path.get(n..).unwrap_or_default()
}

fn r0(factory: &mut ReplayFactory<'_>, fixtures: &ArchFixtures) -> Checked {
    let spec = &fixtures.spec;
    let mut fails = Vec::new();
    let (mut main, root) = Run::start(factory, fixtures, positions::M, &spec.seed)?;
    let (line, _) = main.walk(&root, 0, &spec.replay.path)?;
    let digests = line
        .iter()
        .map(|(_, n)| main.engine.digest(n))
        .collect::<Result<Vec<_>, _>>()?;
    for (k, pair) in digests.windows(2).enumerate() {
        if pair.first() == pair.get(1) {
            fails.push(format!(
                "N{k} and N{} have the same digest",
                k.saturating_add(1)
            ));
        }
    }
    let start_digest = main.engine.digest(&root)?;
    for name in [positions::M_CARD, positions::M_ORDER] {
        let (other, other_root) = Run::start(factory, fixtures, name, &spec.seed)?;
        if other.engine.digest(&other_root)? == start_digest {
            fails.push(format!(
                "{name} N0 has the digest of M N0 (hidden deck ignored)"
            ));
        }
    }
    let (mut alt, alt_root) = Run::start(factory, fixtures, positions::M, &spec.alt_seed)?;
    let (alt_line, _) = alt.walk(&alt_root, 0, head(&spec.replay.path, 5))?;
    if alt.engine.digest(node_at(&alt_line, 5)?)? == main.engine.digest(node_at(&line, 5)?)? {
        fails.push("another seed gives the same N5 digest (random state ignored)".to_owned());
    }
    Ok(fails)
}

fn r1(factory: &mut ReplayFactory<'_>, fixtures: &ArchFixtures, options: &ArchOptions) -> Checked {
    let spec = &fixtures.spec;
    let path = &spec.replay.path;
    let (mut a, root) = Run::start(factory, fixtures, positions::M, &spec.seed)?;
    let (line, _) = a.walk(&root, 0, head(path, 4))?;
    let n4 = node_at(&line, 4)?.clone();
    let blob = a.engine.save(&n4)?;
    let mut b = Run {
        engine: factory(),
        ids: Ids::default(),
    };
    let r4 = b.engine.restore(&blob)?;
    // Labels count from the save point in both runs, so earlier raw ids do not matter.
    a.ids = Ids::default();
    a.ids.node(&n4);
    b.ids.node(&r4);
    let mut fails = compare_nodes(&a, &n4, &b, &r4, options, "N4")?;
    let (a_line, a_steps) = a.walk(&n4, 4, tail(path, 4))?;
    let (b_line, b_steps) = b.walk(&r4, 4, tail(path, 4))?;
    for ((k, sa), sb) in a_line
        .iter()
        .skip(1)
        .map(|(k, _)| k)
        .zip(&a_steps)
        .zip(&b_steps)
    {
        if sa.outcome != sb.outcome {
            fails.push(format!(
                "N{k}: outcome {} after restore, {} without",
                sb.outcome, sa.outcome
            ));
        }
        let ea = normalize(&Value::from(sa.events.clone()), &a.ids, options);
        let eb = normalize(&Value::from(sb.events.clone()), &b.ids, options);
        if ea != eb {
            fails.push(format!("N{k}: events differ after restore: {eb} vs {ea}"));
        }
    }
    for ((k, na), (_, nb)) in a_line.iter().zip(&b_line).skip(1) {
        fails.extend(compare_nodes(&a, na, &b, nb, options, &format!("N{k}"))?);
    }
    let (fa, da) = a.fixed(fixtures, &a_line, "saved run")?;
    let (fb, db) = b.fixed(fixtures, &b_line, "restored run")?;
    let (mut c, c_root) = Run::start(factory, fixtures, positions::M, &spec.seed)?;
    let (c_line, _) = c.walk(&c_root, 0, path)?;
    let (fc, dc) = c.fixed(fixtures, &c_line, "reference run")?;
    fails.extend(fa.into_iter().chain(fb).chain(fc));
    if db != dc || da != dc {
        fails.push(format!(
            "deck after the shuffle: restored {db:?}, saved {da:?}, never saved {dc:?}"
        ));
    }
    Ok(fails)
}

/// Digest and the three observations of two nodes, ids normalized per run.
fn compare_nodes(
    a: &Run,
    na: &NodeId,
    b: &Run,
    nb: &NodeId,
    options: &ArchOptions,
    label: &str,
) -> Checked {
    let mut fails = Vec::new();
    if a.engine.digest(na)? != b.engine.digest(nb)? {
        fails.push(format!("{label}: digests differ"));
    }
    for view in VIEWS {
        let oa = normalize(
            &observation_value(&a.engine.observe(na, view)?),
            &a.ids,
            options,
        );
        let ob = normalize(
            &observation_value(&b.engine.observe(nb, view)?),
            &b.ids,
            options,
        );
        if oa != ob {
            fails.push(format!(
                "{label} {}: observation differs: {ob} vs {oa}",
                view_name(view)
            ));
        }
    }
    Ok(fails)
}

fn r2(factory: &mut ReplayFactory<'_>, fixtures: &ArchFixtures) -> Checked {
    let spec = &fixtures.spec;
    let at = spec.replay.branch_from;
    let (mut a, root) = Run::start(factory, fixtures, positions::M, &spec.seed)?;
    let (line, _) = a.walk(&root, 0, head(&spec.replay.path, at))?;
    let b3 = a.branch(node_at(&line, at)?, BranchKind::Replay)?;
    let (mut f, f_root) = Run::start(factory, fixtures, positions::M, &spec.seed)?;
    let (f_line, _) = f.walk(&f_root, 0, head(&spec.replay.path, at))?;
    Ok(
        if a.engine.digest(&b3)? == f.engine.digest(node_at(&f_line, at)?)? {
            Vec::new()
        } else {
            vec![format!(
                "branch at N{at} and a fresh replay to N{at} have different digests"
            )]
        },
    )
}

fn r2b(factory: &mut ReplayFactory<'_>, fixtures: &ArchFixtures) -> Checked {
    let spec = &fixtures.spec;
    let replay = &spec.replay;
    let at = replay.branch_from;
    let (mut a, root) = Run::start(factory, fixtures, positions::M, &spec.seed)?;
    let (line, _) = a.walk(&root, 0, &replay.path)?;
    let branch = a.branch(node_at(&line, at)?, BranchKind::Replay)?;
    let (mut f, f_root) = Run::start(factory, fixtures, positions::M, &spec.seed)?;
    let (f_line, _) = f.walk(&f_root, 0, head(&replay.path, at))?;
    let f_node = node_at(&f_line, at)?;
    let mut fails = carried_checks(
        &*a.engine,
        &branch,
        &*f.engine,
        f_node,
        &replay.branch_known,
        &replay.branch_unknown,
        "replay branch",
    )?;
    if a.engine.digest(&branch)? == f.engine.digest(f_node)? {
        fails.push("the branch carrying knowledge has the digest of a fresh replay".to_owned());
    }
    let (b_line, _) = a.walk(&branch, at, tail(&replay.path, at))?;
    fails.extend(a.fixed(fixtures, &b_line, "replay branch")?.0);
    Ok(fails)
}

/// Identities each view must know on `branch` (carried unless already identifiable at
/// the same point without the branch) and must not know.
fn carried_checks(
    engine: &dyn ReplayEngine,
    branch: &NodeId,
    reference: &dyn ReplayEngine,
    at: &NodeId,
    must_know: &BTreeMap<String, Vec<String>>,
    must_not: &BTreeMap<String, Vec<String>>,
    label: &str,
) -> Checked {
    let mut fails = Vec::new();
    for view in PLAYERS {
        let name = view_name(view);
        let (identifiable, carried) = known(&engine.observe(branch, view)?.knowledge);
        let (before, _) = known(&reference.observe(at, view)?.knowledge);
        for object in must_know.get(name).into_iter().flatten() {
            let ok = if before.contains(object) {
                identifiable.contains(object) || carried.contains(object)
            } else {
                carried.contains(object)
            };
            if !ok {
                fails.push(format!("{label}: {name} does not carry {object}"));
            }
        }
        for object in must_not.get(name).into_iter().flatten() {
            if identifiable.contains(object) || carried.contains(object) {
                fails.push(format!("{label}: {name} knows {object}"));
            }
        }
    }
    Ok(fails)
}

fn r3(factory: &mut ReplayFactory<'_>, fixtures: &ArchFixtures) -> Checked {
    let spec = &fixtures.spec;
    let path = &spec.replay.path;
    let (mut a, root) = Run::start(factory, fixtures, positions::M, &spec.seed)?;
    let (line, _) = a.walk(&root, 0, head(path, 4))?;
    let n4 = node_at(&line, 4)?.clone();
    let before = VIEWS
        .iter()
        .map(|v| a.engine.observe(&n4, *v))
        .collect::<Result<Vec<_>, _>>()?;
    let n4_digest = a.engine.digest(&n4)?;
    let c3 = a.branch(node_at(&line, 3)?, BranchKind::Replay)?;
    let step = |i: usize| path.get(i).cloned().unwrap_or(Value::Null);
    // Interleaved so that a branch sharing mutable state with its source shows it.
    let (a5, _) = a.engine.decide(&n4, &step(4))?;
    let (c4, _) = a.engine.decide(&c3, &step(3))?;
    let (a6, _) = a.engine.decide(&a5, &step(5))?;
    let (c5, _) = a.engine.decide(&c4, &spec.replay.alt_step5)?;
    let (a_rest, _) = a.walk(&a6, 6, tail(path, 6))?;
    let (c_rest, _) = a.walk(&c5, 5, tail(path, 5))?;
    let mut a_line: Line = line;
    a_line.extend([(5, a5), (6, a6)]);
    a_line.extend(a_rest.into_iter().skip(1));
    let mut c_line: Line = vec![(3, c3), (4, c4), (5, c5)];
    c_line.extend(c_rest.into_iter().skip(1));
    let mut fails = Vec::new();
    let after = VIEWS
        .iter()
        .map(|v| a.engine.observe(&n4, *v))
        .collect::<Result<Vec<_>, _>>()?;
    if after != before || a.engine.digest(&n4)? != n4_digest {
        fails.push("N4 of the source branch changed after branching".to_owned());
    }
    let (mut reference, r_root) = Run::start(factory, fixtures, positions::M, &spec.seed)?;
    let (r_line, _) = reference.walk(&r_root, 0, path)?;
    for ((k, na), (_, nr)) in a_line.iter().zip(&r_line) {
        if a.engine.digest(na)? != reference.engine.digest(nr)? {
            fails.push(format!(
                "source branch N{k} differs from the unbranched reference"
            ));
        }
    }
    let (Some((_, a_tail)), Some((_, c_tail))) = (a_line.last(), c_line.last()) else {
        return Ok(fails);
    };
    if a.engine.decisions(a_tail)? == a.engine.decisions(c_tail)? {
        fails.push("both branches report the same decisions".to_owned());
    }
    for (k, nc) in &c_line {
        if a_line.iter().any(|(ka, na)| ka == k && na == nc) {
            fails.push(format!("N{k} has the same node id on both branches"));
        }
    }
    Ok(fails)
}

fn r4(factory: &mut ReplayFactory<'_>, fixtures: &ArchFixtures) -> Checked {
    let spec = &fixtures.spec;
    let undo = &spec.replay.undo;
    let (mut a, root) = Run::start(factory, fixtures, positions::L, &spec.seed)?;
    let (line, steps) = a.walk(&root, 0, &undo.path)?;
    let logged = a.engine.admin_log()?.len();
    let u0 = a.branch(&root, BranchKind::Undo)?;
    let mut fails = carried_checks(
        &*a.engine,
        &u0,
        &*a.engine,
        &root,
        &undo.known,
        &undo.unknown,
        "undo",
    )?;
    for path in &undo.board {
        let now = a.engine.query(&u0, View::Omniscient, path)?;
        let then = a.engine.query(&root, View::Omniscient, path)?;
        let same = match (&now, &then) {
            (Some(x), Some(y)) => compare::exact(x, y),
            (None, None) => true,
            (Some(_), None) | (None, Some(_)) => false,
        };
        if !same {
            fails.push(format!(
                "after undo {path} is {now:?}, at the start it was {then:?}"
            ));
        }
    }
    if !a.engine.decisions(&u0)?.is_empty() {
        fails.push("the undo shows up in the decision sequence".to_owned());
    }
    let log = a.engine.admin_log()?;
    let undo_logged = log
        .iter()
        .skip(logged)
        .any(|e| e.get("kind").and_then(Value::as_str) == Some("undo"));
    if log.len() != logged.saturating_add(1) || !undo_logged {
        fails.push(format!("admin log after undo: {log:?}"));
    }
    if a.engine.digest(&u0)? == a.engine.digest(&root)? {
        fails.push(
            "the undo root has the digest of the start (carried knowledge ignored)".to_owned(),
        );
    }
    let (_, tail_id) = line.last().cloned().unwrap_or_else(|| (0, root.clone()));
    fails.extend(original_line_intact(&mut a, &tail_id, undo)?);
    let (_, redo) = a.walk(&u0, 0, &undo.path)?;
    let old: Vec<&str> = steps.iter().map(|s| s.outcome.as_str()).collect();
    let new: Vec<&str> = redo.iter().map(|s| s.outcome.as_str()).collect();
    if old != new {
        fails.push(format!(
            "replaying after undo gives {new:?}, originally {old:?}"
        ));
    }
    Ok(fails)
}

fn original_line_intact(a: &mut Run, tail_id: &NodeId, undo: &super::UndoSpec) -> Checked {
    let mut fails = Vec::new();
    let decisions = a.engine.decisions(tail_id)?;
    if decisions.len() != undo.path.len()
        || !decisions
            .iter()
            .zip(&undo.path)
            .all(|(x, y)| compare::exact(y, x))
    {
        fails.push(format!(
            "the original line lost its decisions: {decisions:?}"
        ));
    }
    if a.engine.events(tail_id).is_err() {
        fails.push("the original line's events cannot be read".to_owned());
    }
    match a.engine.decide(tail_id, &undo.continue_with) {
        Ok((_, step)) if !step.outcome.starts_with("cannot") => {}
        Ok((_, step)) => fails.push(format!(
            "the original line cannot continue: {}",
            step.outcome
        )),
        Err(e) => fails.push(format!("the original line cannot continue: {e}")),
    }
    Ok(fails)
}

fn r5(factory: &mut ReplayFactory<'_>, fixtures: &ArchFixtures, options: &ArchOptions) -> Checked {
    let mut fails = scan_path_and_branch(factory, fixtures)?;
    fails.extend(scan_undo(factory, fixtures)?);
    fails.extend(paired(factory, fixtures, options)?);
    Ok(fails)
}

/// A played path P plus the replay branch from `branch_from`, continued to N13.
struct Played {
    run: Run,
    line: Line,
    branch: Line,
}

fn play_with_branch(
    factory: &mut ReplayFactory<'_>,
    fixtures: &ArchFixtures,
    name: &str,
) -> Result<Played, EngineError> {
    let spec = &fixtures.spec;
    let at = spec.replay.branch_from;
    let (mut run, root) = Run::start(factory, fixtures, name, &spec.seed)?;
    let (line, _) = run.walk(&root, 0, &spec.replay.path)?;
    let b = run.branch(node_at(&line, at)?, BranchKind::Replay)?;
    let (branch, _) = run.walk(&b, at, tail(&spec.replay.path, at))?;
    Ok(Played { run, line, branch })
}

fn scan_node(
    engine: &dyn ReplayEngine,
    node: &NodeId,
    view: View,
    hidden: &[String],
    cards: &BTreeMap<String, String>,
    seed: &str,
    label: &str,
) -> Checked {
    let observed = engine.observe(node, view)?;
    let outputs = Value::Array(vec![
        observed.projection,
        observed.awaiting.unwrap_or(Value::Null),
        engine.export(node, view)?,
    ]);
    Ok(leaks(&outputs, hidden, cards, seed)
        .into_iter()
        .map(|l| format!("{label} {}: {l}", view_name(view)))
        .collect())
}

fn scan_path_and_branch(factory: &mut ReplayFactory<'_>, fixtures: &ArchFixtures) -> Checked {
    let spec = &fixtures.spec;
    let cards = card_numbers(&fixtures.position(positions::M)?.setup);
    let played = play_with_branch(factory, fixtures, positions::M)?;
    let mut fails = Vec::new();
    for view in PLAYERS {
        let carried = spec
            .replay
            .branch_known
            .get(view_name(view))
            .cloned()
            .unwrap_or_default();
        for (k, node) in &played.line {
            let hidden = hidden_at(&spec.replay.hidden, view, *k, &[]);
            fails.extend(scan_node(
                &*played.run.engine,
                node,
                view,
                &hidden,
                &cards,
                &spec.seed,
                &format!("N{k}"),
            )?);
        }
        for (k, node) in &played.branch {
            let hidden = hidden_at(&spec.replay.hidden, view, *k, &carried);
            fails.extend(scan_node(
                &*played.run.engine,
                node,
                view,
                &hidden,
                &cards,
                &spec.seed,
                &format!("branch N{k}"),
            )?);
        }
    }
    Ok(fails)
}

fn scan_undo(factory: &mut ReplayFactory<'_>, fixtures: &ArchFixtures) -> Checked {
    let spec = &fixtures.spec;
    let undo = &spec.replay.undo;
    let cards = card_numbers(&fixtures.position(positions::L)?.setup);
    let (mut run, root) = Run::start(factory, fixtures, positions::L, &spec.seed)?;
    let (line, _) = run.walk(&root, 0, &undo.path)?;
    let u0 = run.branch(&root, BranchKind::Undo)?;
    let (redo, _) = run.walk(&u0, 0, &undo.path)?;
    let mut fails = Vec::new();
    for view in PLAYERS {
        let carried = undo.known.get(view_name(view)).cloned().unwrap_or_default();
        for (k, node) in &line {
            let hidden = hidden_at(&undo.hidden, view, *k, &[]);
            fails.extend(scan_node(
                &*run.engine,
                node,
                view,
                &hidden,
                &cards,
                &spec.seed,
                &format!("L N{k}"),
            )?);
        }
        for (k, node) in &redo {
            let hidden = hidden_at(&undo.hidden, view, *k, &carried);
            fails.extend(scan_node(
                &*run.engine,
                node,
                view,
                &hidden,
                &cards,
                &spec.seed,
                &format!("undo N{k}"),
            )?);
        }
    }
    Ok(fails)
}

/// Exports of `view` on two runs, compared node by node where `keep` says so.
fn same_exports(
    (a, a_line): (&Run, &Line),
    (b, b_line): (&Run, &Line),
    view: View,
    keep: &dyn Fn(usize) -> bool,
    options: &ArchOptions,
    label: &str,
) -> Checked {
    let mut fails = Vec::new();
    for ((k, na), (_, nb)) in a_line.iter().zip(b_line).filter(|((k, _), _)| keep(*k)) {
        let ea = normalize(&a.engine.export(na, view)?, &a.ids, options);
        let eb = normalize(&b.engine.export(nb, view)?, &b.ids, options);
        if ea != eb {
            fails.push(format!("{label} N{k} {}: exports differ", view_name(view)));
        }
    }
    Ok(fails)
}

fn paired(
    factory: &mut ReplayFactory<'_>,
    fixtures: &ArchFixtures,
    options: &ArchOptions,
) -> Checked {
    let spec = &fixtures.spec;
    let all = |_: usize| true;
    let m = play_with_branch(factory, fixtures, positions::M)?;
    let card = play_with_branch(factory, fixtures, positions::M_CARD)?;
    let order = play_with_branch(factory, fixtures, positions::M_ORDER)?;
    let mut fails = same_exports(
        (&m.run, &m.line),
        (&card.run, &card.line),
        View::P2,
        &all,
        options,
        "M/M'",
    )?;
    fails.extend(same_exports(
        (&m.run, &m.line),
        (&order.run, &order.line),
        View::P2,
        &all,
        options,
        "M/M''",
    )?);
    fails.extend(same_exports(
        (&m.run, &m.branch),
        (&order.run, &order.branch),
        View::P2,
        &all,
        options,
        "M/M'' branch",
    )?);
    let search = spec.replay.search_node;
    let before_search = |k: usize| k < search;
    fails.extend(same_exports(
        (&m.run, &m.line),
        (&order.run, &order.line),
        View::P1,
        &before_search,
        options,
        "M/M''",
    )?);
    fails.extend(search_awaiting(&m, fixtures)?);
    let s45 = play_with_branch(factory, fixtures, positions::M_CARD_45)?;
    let s54 = play_with_branch(factory, fixtures, positions::M_CARD_54)?;
    let shuffled = |k: usize| (5..=12).contains(&k);
    fails.extend(same_exports(
        (&s45.run, &s45.line),
        (&s54.run, &s54.line),
        View::P1,
        &shuffled,
        options,
        "M' shuffles",
    )?);
    Ok(fails)
}

fn search_awaiting(m: &Played, fixtures: &ArchFixtures) -> Checked {
    let spec = &fixtures.spec.replay;
    let node = node_at(&m.line, spec.search_node)?;
    let mut fails = Vec::new();
    let own = m.run.engine.observe(node, View::P1)?.awaiting;
    if !awaits(own.as_ref(), "P1", &spec.search_do) {
        fails.push(format!(
            "N{} P1: awaiting {own:?}, expected a {}",
            spec.search_node, spec.search_do
        ));
    }
    let other = m.run.engine.observe(node, View::P2)?.awaiting;
    let only_by = other
        .as_ref()
        .and_then(Value::as_object)
        .is_some_and(|o| o.len() == 1 && o.get("by").and_then(Value::as_str) == Some("P1"));
    if !only_by {
        fails.push(format!(
            "N{} P2: awaiting {other:?}, expected only by: P1",
            spec.search_node
        ));
    }
    Ok(fails)
}

fn r6(factory: &mut ReplayFactory<'_>, fixtures: &ArchFixtures) -> Checked {
    let spec = &fixtures.spec;
    let cause = &spec.replay.cause;
    let (mut a, root) = Run::start(factory, fixtures, positions::M, &spec.seed)?;
    let (line, _) = a.walk(&root, 0, &spec.replay.path)?;
    let mut events: Vec<Value> = Vec::new();
    for (_, node) in line.iter().skip(1) {
        events.extend(a.engine.events(node)?);
    }
    let by_id: BTreeMap<String, Value> = events
        .iter()
        .filter_map(|e| {
            e.get("id")
                .and_then(Value::as_str)
                .map(|id| (id.to_owned(), e.clone()))
        })
        .collect();
    let decision_node = node_at(&line, cause.ability_step)?;
    let mut fails = Vec::new();
    match events.iter().find(|e| compare::subset(&cause.placed, e)) {
        None => fails.push(format!("no event {}", cause.placed)),
        Some(placed) => {
            let (chain, end) = follow(placed, &by_id);
            if !chain.iter().any(|e| has_match(e, &cause.ability)) {
                fails.push(format!(
                    "{} does not trace to ability {}",
                    cause.placed, cause.ability
                ));
            }
            if end != End::Decision(decision_node.0.clone()) {
                fails.push(format!(
                    "{} ends at {end:?}, expected the node of step {}",
                    cause.placed, cause.ability_step
                ));
            }
        }
    }
    match events.iter().find(|e| compare::subset(&cause.trigger, e)) {
        None => fails.push(format!("no event {}", cause.trigger)),
        Some(trigger) => {
            let (chain, _) = follow(trigger, &by_id);
            if !chain
                .iter()
                .skip(1)
                .any(|e| compare::subset(&cause.placed, e))
            {
                fails.push(format!(
                    "{} does not trace to {}",
                    cause.trigger, cause.placed
                ));
            }
        }
    }
    for event in &events {
        match follow(event, &by_id).1 {
            End::Decision(node) if a.ids.knows_node(&node) => {}
            End::Rule => {}
            end @ (End::Decision(_) | End::Dangling(_) | End::NoCause | End::TooLong) => {
                fails.push(format!("chain of {event} ends at {end:?}"));
            }
        }
    }
    Ok(fails)
}

#[derive(Debug, PartialEq, Eq)]
enum End {
    Decision(String),
    Rule,
    Dangling(String),
    NoCause,
    TooLong,
}

/// Follows `cause` links from `start`; returns the events passed and where it ended.
fn follow(start: &Value, by_id: &BTreeMap<String, Value>) -> (Vec<Value>, End) {
    let mut chain = vec![start.clone()];
    let mut current = start.clone();
    for _ in 0..CHAIN_LIMIT {
        let cause = current.get("cause");
        if let Some(node) = cause
            .and_then(|c| c.get("decision"))
            .and_then(Value::as_str)
        {
            return (chain, End::Decision(node.to_owned()));
        }
        if cause.and_then(|c| c.get("rule")).is_some() {
            return (chain, End::Rule);
        }
        let Some(id) = cause.and_then(|c| c.get("event")).and_then(Value::as_str) else {
            return (chain, End::NoCause);
        };
        let Some(next) = by_id.get(id) else {
            return (chain, End::Dangling(id.to_owned()));
        };
        chain.push(next.clone());
        current = next.clone();
    }
    (chain, End::TooLong)
}
