//! Fake engines that validate the runner itself.
//!
//! `Oracle` answers every query with what the question expects, so a correct runner
//! must pass every scenario with it. `Mutant` wraps the oracle and breaks exactly one
//! thing, so a correct runner must fail every scenario the mutation applies to.

#![allow(dead_code, reason = "each test binary uses a different subset")]

use std::collections::HashMap;

use serde_json::{Value, json};
use sve_scenario_runner::model::{Expected, Question};
use sve_scenario_runner::{Engine, EngineError, Fixture, Step, View};

type Key = (String, String);

/// Plays back each scenario's expected results.
pub(crate) struct Oracle {
    scripts: HashMap<Key, Vec<Expected>>,
    current: Vec<Expected>,
    n: usize,
}

impl Oracle {
    pub(crate) fn new(questions: &[Question]) -> Self {
        let mut scripts = HashMap::new();
        for q in questions {
            for s in &q.scenarios {
                scripts.insert((q.id.clone(), s.name.clone()), s.expected.clone());
            }
        }
        Self {
            scripts,
            current: Vec::new(),
            n: 0,
        }
    }

    /// Checkpoint entries after the current decision.
    pub(crate) fn here(&self) -> Vec<&Expected> {
        self.current
            .iter()
            .filter(|e| e.decision_index().ok() == Some(self.n))
            .collect()
    }

    const fn view_name(view: View) -> &'static str {
        match view {
            View::Omniscient => "omniscient",
            View::P1 => "P1",
            View::P2 => "P2",
        }
    }
}

impl Engine for Oracle {
    fn load(&mut self, fixture: &Fixture) -> Result<(), EngineError> {
        self.current = self
            .scripts
            .get(&(fixture.question.clone(), fixture.scenario.clone()))
            .cloned()
            .ok_or_else(|| EngineError::Adapter("no script".into()))?;
        self.n = 0;
        Ok(())
    }

    fn decide(&mut self, _decision: &Value) -> Result<Step, EngineError> {
        self.n += 1;
        let here = self.here();
        let outcome = here
            .first()
            .map_or_else(|| "resolved".to_owned(), |e| e.outcome.clone());
        // Views at one checkpoint share one event range; the longest list covers them.
        let events = here
            .iter()
            .map(|e| e.events.clone())
            .max_by_key(Vec::len)
            .unwrap_or_default();
        Ok(Step { outcome, events })
    }

    fn query(&self, view: View, path: &str) -> Result<Option<Value>, EngineError> {
        let name = Self::view_name(view);
        // Several entries may share one checkpoint and view (contract 6.1).
        let Some(want) = self
            .here()
            .into_iter()
            .filter(|e| e.view == name)
            .find_map(|e| e.assert.get(path))
        else {
            return Ok(None);
        };
        if path == "knowledge" {
            return Ok(Some(
                json!({ "identifiable": want.get("knows").cloned().unwrap_or(json!([])) }),
            ));
        }
        Ok(Some(want.clone()))
    }

    fn awaiting(&self, view: View) -> Result<Option<Value>, EngineError> {
        let name = Self::view_name(view);
        let here = self.here();
        // Prefer what the question wrote for this view; fall back to any entry.
        let own = here
            .iter()
            .filter(|e| e.view == name)
            .find_map(|e| e.awaiting.clone());
        Ok(own.or_else(|| {
            let mut a = here.iter().find_map(|e| e.awaiting.clone())?;
            let by = a.get("by").and_then(Value::as_str).map(str::to_owned);
            if view != View::Omniscient
                && by.as_deref() != Some(name)
                && let Value::Object(m) = &mut a
            {
                m.remove("choices");
            }
            Some(a)
        }))
    }

    fn projection(&self, view: View) -> Result<Value, EngineError> {
        let name = Self::view_name(view);
        let mut out = serde_json::Map::new();
        for e in self.here().into_iter().filter(|e| e.view == name) {
            for (path, value) in &e.assert {
                if path != "knowledge" {
                    out.insert(path.clone(), value.clone());
                }
            }
            if let Some(a) = &e.awaiting {
                out.insert("awaiting".into(), a.clone());
            }
        }
        Ok(Value::Object(out))
    }
}

/// One way to break the oracle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mutation {
    WrongOutcome,
    DropEvent,
    SplitGroup,
    InjectForbidden,
    BumpNumber,
    AwaitingOtherPlayer,
    LeakKnowledge,
    LeakProjection,
    SwapDeck,
    OpponentChoices,
    FillerWithId,
    InterleaveGroup,
}

pub(crate) const ALL: &[Mutation] = &[
    Mutation::WrongOutcome,
    Mutation::DropEvent,
    Mutation::SplitGroup,
    Mutation::InjectForbidden,
    Mutation::BumpNumber,
    Mutation::AwaitingOtherPlayer,
    Mutation::LeakKnowledge,
    Mutation::LeakProjection,
    Mutation::SwapDeck,
    Mutation::OpponentChoices,
    Mutation::FillerWithId,
    Mutation::InterleaveGroup,
];

/// The oracle with one mutation applied at the first checkpoint where it applies.
pub(crate) struct Mutant {
    pub inner: Oracle,
    pub mutation: Mutation,
    target: Option<usize>,
}

impl Mutant {
    pub(crate) const fn new(inner: Oracle, mutation: Mutation) -> Self {
        Self {
            inner,
            mutation,
            target: None,
        }
    }

    /// Whether the mutation can apply to these checkpoint entries.
    pub(crate) fn applies(mutation: Mutation, entries: &[&Expected]) -> bool {
        entries.iter().any(|e| match mutation {
            Mutation::WrongOutcome => true,
            Mutation::DropEvent => !e.events.is_empty(),
            Mutation::SplitGroup | Mutation::InterleaveGroup => has_real_group(&e.events),
            Mutation::InjectForbidden => !e.forbidden_events.is_empty(),
            Mutation::BumpNumber => e
                .assert
                .iter()
                .any(|(p, v)| p != "knowledge" && first_number_path(v).is_some()),
            Mutation::AwaitingOtherPlayer => {
                e.awaiting.as_ref().and_then(|a| a.get("by")).is_some()
            }
            Mutation::LeakKnowledge | Mutation::LeakProjection => e
                .assert
                .get("knowledge")
                .and_then(|k| k.get("does_not_know"))
                .and_then(Value::as_array)
                .is_some_and(|a| !a.is_empty()),
            Mutation::SwapDeck => e.assert.iter().any(|(p, v)| is_deck(p) && swappable(v)),
            Mutation::OpponentChoices => e.awaiting.as_ref().is_some_and(|a| {
                e.view != "omniscient"
                    && a.get("by").and_then(Value::as_str) != Some(e.view.as_str())
            }),
            Mutation::FillerWithId => e.assert.values().any(has_filler),
        })
    }

    fn hidden(&self, view: View) -> Vec<Value> {
        let name = Oracle::view_name(view);
        self.inner
            .here()
            .into_iter()
            .filter(|e| e.view == name)
            .filter_map(|e| e.assert.get("knowledge").cloned())
            .filter_map(|k| k.get("does_not_know").and_then(Value::as_array).cloned())
            .flatten()
            .collect()
    }

    fn active(&self) -> bool {
        self.target == Some(self.inner.n)
    }
}

fn has_filler(v: &Value) -> bool {
    v.as_array().is_some_and(|a| a.iter().any(is_bare_filler))
}

fn is_bare_filler(v: &Value) -> bool {
    v.as_object()
        .is_some_and(|m| m.len() == 1 && m.contains_key("filler"))
}

fn is_deck(path: &str) -> bool {
    path.rsplit('.').next() == Some("deck")
}

fn has_real_group(events: &[Value]) -> bool {
    let mut seen: HashMap<String, usize> = HashMap::new();
    for e in events {
        if let Some(g) = e.get("group") {
            *seen.entry(g.to_string()).or_default() += 1;
        }
    }
    seen.values().any(|&n| n >= 2)
}

fn swappable(v: &Value) -> bool {
    v.as_array().is_some_and(|a| a.len() >= 2 && a[0] != a[1])
}

/// Bumps the first number found in a value; returns whether one was found.
fn bump(v: &mut Value) -> bool {
    match v {
        Value::Number(n) => {
            *v = json!(n.as_f64().unwrap_or(0.0) + 1.0);
            true
        }
        Value::Object(m) => m.values_mut().any(bump),
        Value::Array(a) => a.iter_mut().any(bump),
        Value::Null | Value::Bool(_) | Value::String(_) => false,
    }
}

fn first_number_path(v: &Value) -> Option<()> {
    let mut c = v.clone();
    bump(&mut c).then_some(())
}

impl Engine for Mutant {
    fn load(&mut self, fixture: &Fixture) -> Result<(), EngineError> {
        self.inner.load(fixture)?;
        let max = self
            .inner
            .current
            .iter()
            .filter_map(|e| e.decision_index().ok())
            .max()
            .unwrap_or(0);
        self.target = (1..=max).find(|&n| {
            let entries: Vec<&Expected> = self
                .inner
                .current
                .iter()
                .filter(|e| e.decision_index().ok() == Some(n))
                .collect();
            Self::applies(self.mutation, &entries)
        });
        Ok(())
    }

    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "each mutant only touches the calls it targets"
    )]
    fn decide(&mut self, decision: &Value) -> Result<Step, EngineError> {
        let mut step = self.inner.decide(decision)?;
        if !self.active() {
            return Ok(step);
        }
        match self.mutation {
            Mutation::WrongOutcome => {
                step.outcome = if step.outcome == "resolved" {
                    "paused".into()
                } else {
                    "resolved".into()
                };
            }
            Mutation::DropEvent => {
                // Drop an event that some entry here actually lists.
                if let Some(e) = self
                    .inner
                    .here()
                    .iter()
                    .find_map(|e| e.events.last().cloned())
                {
                    step.events.retain(|x| *x != e);
                }
            }
            Mutation::SplitGroup => {
                for (i, e) in step.events.iter_mut().enumerate() {
                    if let Value::Object(m) = e
                        && m.contains_key("group")
                    {
                        m.insert("group".into(), json!(format!("split-{i}")));
                    }
                }
            }
            Mutation::InterleaveGroup => {
                // Put an unrelated event between the first two members of a group.
                let label = step.events.iter().find_map(|e| e.get("group").cloned());
                if let Some(label) = label {
                    let at: Vec<usize> = (0..step.events.len())
                        .filter(|&i| step.events[i].get("group") == Some(&label))
                        .collect();
                    if at.len() >= 2 {
                        step.events
                            .insert(at[0] + 1, json!({"kind": "引く", "player": "P1"}));
                    }
                }
            }
            Mutation::InjectForbidden => {
                if let Some(f) = self
                    .inner
                    .here()
                    .iter()
                    .find_map(|e| e.forbidden_events.first().cloned())
                {
                    let mut f = f;
                    if let Value::Object(m) = &mut f {
                        m.remove("group");
                    }
                    step.events.push(f);
                }
            }
            _ => {}
        }
        Ok(step)
    }

    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "each mutant only touches the calls it targets"
    )]
    fn query(&self, view: View, path: &str) -> Result<Option<Value>, EngineError> {
        let got = self.inner.query(view, path)?;
        if !self.active() {
            return Ok(got);
        }
        let Some(mut v) = got else { return Ok(None) };
        match self.mutation {
            Mutation::BumpNumber if path != "knowledge" => {
                bump(&mut v);
            }
            Mutation::LeakKnowledge if path == "knowledge" => {
                let leaked = self.hidden(view);
                if let Some(Value::Array(ids)) = v.get_mut("identifiable") {
                    ids.extend(leaked);
                }
            }
            Mutation::FillerWithId => {
                if let Value::Array(a) = &mut v
                    && let Some(Value::Object(m)) = a.iter_mut().find(|i| is_bare_filler(i))
                {
                    m.insert("id".into(), json!("leaked"));
                }
            }
            Mutation::SwapDeck if is_deck(path) => {
                if let Value::Array(a) = &mut v
                    && a.len() >= 2
                {
                    a.swap(0, 1);
                }
            }
            _ => {}
        }
        Ok(Some(v))
    }

    fn projection(&self, view: View) -> Result<Value, EngineError> {
        let mut p = self.inner.projection(view)?;
        if self.active()
            && self.mutation == Mutation::LeakProjection
            && let Value::Object(m) = &mut p
        {
            m.insert("leaked".into(), Value::Array(self.hidden(view)));
        }
        Ok(p)
    }

    fn awaiting(&self, view: View) -> Result<Option<Value>, EngineError> {
        let got = self.inner.awaiting(view)?;
        if self.active() && self.mutation == Mutation::OpponentChoices {
            return Ok(got.map(|mut a| {
                if let Value::Object(m) = &mut a {
                    m.insert("choices".into(), json!([{"do": "pass"}]));
                }
                a
            }));
        }
        if !(self.active() && self.mutation == Mutation::AwaitingOtherPlayer) {
            return Ok(got);
        }
        Ok(got.map(|mut a| {
            if let Value::Object(m) = &mut a {
                let other = if m.get("by") == Some(&json!("P1")) {
                    "P2"
                } else {
                    "P1"
                };
                m.insert("by".into(), json!(other));
            }
            a
        }))
    }
}
