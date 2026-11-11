use super::journal::index;
use super::{
    ActionDetails, ActionEvent, ActionKind, EventCard, EventError, EventKind, EventLocation,
    EventSubject, MovementBefore, MovementMethod, NumberFacet, Viewer,
};
use crate::game::Game;
use crate::game::player::observation::{seat, viewer, zone};
use crate::ir::EvolutionState;
use alloc::collections::{BTreeMap, BTreeSet};
use serde_json::Value;

type Projected = Result<Option<EventKind>, EventError>;

pub(super) const fn action(kind: ActionKind) -> ActionEvent {
    ActionEvent {
        kind,
        player: None,
        subjects: Vec::new(),
        source: None,
        numbers: Vec::new(),
        name: None,
        face_up: None,
        details: None,
    }
}

fn action_kind(tag: &str) -> Result<ActionKind, EventError> {
    Ok(match tag {
        "プレイ" => ActionKind::Play,
        "解決" => ActionKind::Resolve,
        "ダメージ" => ActionKind::Damage,
        "アクト" => ActionKind::Act,
        "スタンド" => ActionKind::Stand,
        "公開" => ActionKind::Reveal,
        "表向き／裏向き" => ActionKind::Facing,
        "攻撃" => ActionKind::Attack,
        "破壊" => ActionKind::Destroy,
        "消滅" => ActionKind::Banish,
        "取代" => ActionKind::Replacement,
        "消去" => ActionKind::Erase,
        "装備" => ActionKind::Equip,
        "進化" | "超進化" => ActionKind::Evolve,
        "カウンター" => ActionKind::Counter,
        "回復" | "PP最大値変化" | "EP変化" | "体力増加" => ActionKind::Resource,
        "サイコロ" => ActionKind::Dice,
        "ターンスキップ" => ActionKind::TurnSkipped,
        "支配変更" => ActionKind::Control,
        // These are frozen prototype continuation traces, outside typed player history.
        "待機" | "待機取消" | "費用成立" | "抑制" => {
            return Err(EventError::ProjectionGap);
        }
        _ => return Err(EventError::ProjectionGap),
    })
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Source adapters freeze permissions at the mutator."
)]
impl Game {
    pub(in crate::game) fn event_card(
        &self,
        perspective: Viewer,
        id: &str,
        memory: bool,
    ) -> Result<Option<EventCard>, EventError> {
        let known = self.player_knows_object(perspective, id);
        if !known && !memory {
            return Ok(None);
        }
        let handle = if known {
            Some(
                self.state
                    .knowledge
                    .get(seat(perspective))
                    .and_then(|knowledge| knowledge.handle(id))
                    .ok_or(EventError::ProjectionGap)?,
            )
        } else {
            None
        };
        let mut properties = self
            .observed_card_properties(perspective, id)
            .map_err(|_authority_error| EventError::ProjectionGap)?;
        if !known {
            properties.state = None;
        }
        Ok(Some(EventCard { handle, properties }))
    }

    pub(in crate::game) fn player_event_subject(
        &self,
        perspective: Viewer,
        id: &str,
    ) -> Result<Option<EventSubject>, EventError> {
        if let Some(seat) = id.strip_suffix(".leader") {
            return viewer(seat)
                .map(EventSubject::Leader)
                .map(Some)
                .map_err(|_authority_error| EventError::ProjectionGap);
        }
        self.event_card(perspective, id, false)
            .map(|card| card.map(|card| EventSubject::Card(Box::new(card))))
    }

    fn event_location(&self, perspective: Viewer, id: &str) -> Result<EventLocation, EventError> {
        let object = self
            .object(id)
            .map_err(|_authority_error| EventError::ProjectionGap)?;
        let player =
            viewer(&object.controller).map_err(|_authority_error| EventError::ProjectionGap)?;
        let kind = if object.zone == "void" {
            None
        } else {
            Some(zone(&object.zone).map_err(|_authority_error| EventError::ProjectionGap)?)
        };
        let card = self.event_card(perspective, id, false)?;
        let position = if card.is_some() && (object.zone != "hand" || perspective == player) {
            self.object_position(id)
                .map_err(|_authority_error| EventError::ProjectionGap)?
        } else {
            None
        };
        Ok(EventLocation {
            player,
            zone: kind,
            position,
            card,
        })
    }

    pub(in crate::game) fn player_movement_before(&self, ids: &[String]) -> MovementBefore {
        let mut cards = BTreeMap::new();
        let mut generations = BTreeMap::new();
        if self.player_events.active() {
            for id in ids {
                if let Ok(object) = self.object(id) {
                    generations.insert(id.clone(), object.generation);
                }
                cards.insert(
                    id.clone(),
                    [Viewer::P1, Viewer::P2]
                        .map(|perspective| self.event_location(perspective, id).ok()),
                );
            }
        }
        MovementBefore {
            start: self.player_events.pending.len(),
            cards,
            generations,
        }
    }

    pub(in crate::game) fn player_movement_after(&mut self, before: MovementBefore) {
        if !self.player_events.active() {
            return;
        }
        let MovementBefore {
            start,
            cards,
            generations,
        } = before;
        let endpoints = cards
            .keys()
            .map(|id| {
                (
                    id.clone(),
                    [Viewer::P1, Viewer::P2]
                        .map(|perspective| self.event_location(perspective, id)),
                )
            })
            .collect::<BTreeMap<_, _>>();
        for entry in self
            .player_events
            .pending
            .iter_mut()
            .skip(start)
            .filter(|entry| !entry.sealed)
        {
            let Some(id) = &entry.object else {
                continue;
            };
            entry.before_generation = generations.get(id).copied();
            for perspective in [Viewer::P1, Viewer::P2] {
                let slot = index(perspective);
                let projected = (|| {
                    let old = cards
                        .get(id)
                        .and_then(|views| views.get(slot))
                        .and_then(Option::as_ref)
                        .ok_or(EventError::ProjectionGap)?
                        .clone();
                    let new = endpoints
                        .get(id)
                        .and_then(|views| views.get(slot))
                        .ok_or(EventError::ProjectionGap)?
                        .clone()?;
                    if old.player == new.player
                        && old.zone == new.zone
                        && old.card.is_none()
                        && new.card.is_none()
                    {
                        return Ok(None);
                    }
                    Ok(Some(EventKind::Movement {
                        method: MovementMethod::Procedure,
                        before: Box::new(old),
                        after: Box::new(new),
                    }))
                })();
                if let Some(view) = entry.views.get_mut(slot) {
                    *view = projected;
                }
            }
            entry.sealed = true;
        }
    }

    pub(in crate::game) fn player_movement_method(
        &mut self,
        start: usize,
        ids: &[String],
        method: MovementMethod,
    ) {
        let mut marked = BTreeSet::new();
        for entry in self.player_events.pending.iter_mut().skip(start) {
            if entry
                .views
                .iter()
                .any(|view| matches!(view, Ok(Some(EventKind::Movement { .. }))))
                && entry
                    .object
                    .as_ref()
                    .is_some_and(|id| ids.contains(id) && marked.insert(id.clone()))
            {
                for view in &mut entry.views {
                    if let Ok(Some(EventKind::Movement {
                        method: recorded, ..
                    })) = view
                    {
                        *recorded = method;
                    }
                }
            }
        }
    }

    pub(in crate::game) fn player_movement_private_hand(&mut self, start: usize, ids: &[String]) {
        let endpoints = ids
            .iter()
            .map(|id| {
                (
                    id.clone(),
                    [Viewer::P1, Viewer::P2].map(|viewer| self.event_location(viewer, id)),
                )
            })
            .collect::<BTreeMap<_, _>>();
        for entry in self.player_events.pending.iter_mut().skip(start) {
            let Some(views) = entry.object.as_ref().and_then(|id| endpoints.get(id)) else {
                continue;
            };
            for (slot, view) in entry.views.iter_mut().enumerate() {
                if let Ok(Some(EventKind::Movement { after, .. })) = view
                    && after.zone == Some(super::super::observation::ObservedZone::Hand)
                {
                    match views.get(slot) {
                        Some(Ok(current)) => **after = current.clone(),
                        _ => *view = Err(EventError::ProjectionGap),
                    }
                }
            }
        }
    }

    pub(in crate::game) fn record_player_opening_reveal(&mut self, ids: &[String]) {
        if !self.player_events.active() {
            return;
        }
        for id in ids {
            let views = [Viewer::P1, Viewer::P2].map(|viewer| {
                let mut result = action(ActionKind::Reveal);
                result.face_up = Some(true);
                result.subjects.push(self.player_event_subject(viewer, id)?);
                Ok(Some(EventKind::Action(result)))
            });
            self.record_player_fact_views(views, &serde_json::json!({"rule":"14.4"}), None);
        }
    }

    pub(in crate::game) const fn player_event_mark(&self) -> usize {
        self.player_events.pending.len()
    }

    pub(in crate::game) fn player_event_memory(
        &self,
        perspective: Viewer,
        id: &str,
        generation: u64,
    ) -> Option<EventSubject> {
        for entry in self
            .player_events
            .pending
            .iter()
            .rev()
            .filter(|entry| entry.object.as_deref() == Some(id))
        {
            let Some(Ok(Some(kind))) = entry.views.get(index(perspective)) else {
                continue;
            };
            let card = match kind {
                EventKind::Movement { before, after, .. }
                    if entry.before_generation == Some(generation) =>
                {
                    before.card.as_ref().or_else(|| {
                        (entry.generation == Some(generation))
                            .then_some(after.card.as_ref())
                            .flatten()
                    })
                }
                EventKind::Movement { after, .. } if entry.generation == Some(generation) => {
                    after.card.as_ref()
                }
                EventKind::Action(action) if entry.generation == Some(generation) => action
                    .subjects
                    .first()
                    .and_then(Option::as_ref)
                    .and_then(|subject| match subject {
                        EventSubject::Card(card) => Some(card.as_ref()),
                        EventSubject::Leader(_) => None,
                    }),
                EventKind::Movement { .. }
                | EventKind::Action(_)
                | EventKind::Phase(_)
                | EventKind::Result { .. } => None,
            };
            if let Some(card) = card {
                let mut memory = card.clone();
                memory.handle = None;
                return Some(EventSubject::Card(Box::new(memory)));
            }
        }
        None
    }

    pub(in crate::game) fn player_subject_before(
        &self,
        id: &str,
    ) -> [Result<Option<EventSubject>, EventError>; 2] {
        if !self.player_events.active() {
            return [Ok(None), Ok(None)];
        }
        [Viewer::P1, Viewer::P2].map(|viewer| self.player_event_subject(viewer, id))
    }

    pub(in crate::game) fn player_step_actor(&mut self, step: &str, actor: &str) {
        let actor = viewer(actor).map_err(|_authority_error| EventError::ProjectionGap);
        if let Some(entry) = self
            .player_events
            .pending
            .iter_mut()
            .find(|entry| entry.step.as_deref() == Some(step))
        {
            for view in &mut entry.views {
                match (view, &actor) {
                    (Ok(Some(EventKind::Action(action))), Ok(actor)) => {
                        action.player = Some(*actor);
                    }
                    (view, Err(error)) => *view = Err(*error),
                    _ => {}
                }
            }
        }
    }

    pub(in crate::game) fn player_source_before(
        &self,
        id: &str,
        generation: Option<u64>,
    ) -> [Result<Option<EventSubject>, EventError>; 2] {
        if !self.player_events.active() {
            return [Ok(None), Ok(None)];
        }
        [Viewer::P1, Viewer::P2].map(|perspective| {
            let Some(generation) = generation else {
                return Ok(None);
            };
            let object = self
                .object(id)
                .map_err(|_authority_error| EventError::ProjectionGap)?;
            if object.generation != generation {
                return Ok(self.player_event_memory(perspective, id, generation));
            }
            if matches!(
                object.zone.as_str(),
                "deck" | "hand" | "evolve_deck" | "void"
            ) {
                return Ok(None);
            }
            self.player_event_subject(perspective, id)
        })
    }

    pub(in crate::game) fn player_source_after(
        &mut self,
        step: &str,
        subjects: [Result<Option<EventSubject>, EventError>; 2],
    ) {
        if let Some(entry) = self
            .player_events
            .pending
            .iter_mut()
            .find(|entry| entry.step.as_deref() == Some(step))
        {
            for (view, subject) in entry.views.iter_mut().zip(subjects) {
                match (view, subject) {
                    (Ok(Some(EventKind::Action(action))), Ok(subject)) => {
                        if let Some(slot) = action.subjects.first_mut() {
                            *slot = subject;
                        }
                    }
                    (view, Err(error)) => *view = Err(error),
                    _ => {}
                }
            }
        }
    }

    pub(in crate::game) fn player_subject_after(
        &mut self,
        step: &str,
        subjects: [Result<Option<EventSubject>, EventError>; 2],
    ) {
        if let Some(entry) = self
            .player_events
            .pending
            .iter_mut()
            .find(|entry| entry.step.as_deref() == Some(step))
        {
            for (view, subject) in entry.views.iter_mut().zip(subjects) {
                match (view, subject) {
                    (Ok(Some(EventKind::Action(action))), Ok(subject)) => {
                        action.subjects = vec![subject];
                    }
                    (view, Err(error)) => *view = Err(error),
                    _ => {}
                }
            }
        }
    }

    pub(in crate::game) fn enrich_player_step(
        &mut self,
        step: &str,
        kind: Option<ActionKind>,
        details: &ActionDetails,
        numbers: &[NumberFacet],
    ) {
        if !self.player_events.active() {
            return;
        }
        let mut found = false;
        for entry in &mut self.player_events.pending {
            if entry.step.as_deref() == Some(step) {
                found = true;
                for view in &mut entry.views {
                    match view {
                        Ok(Some(EventKind::Action(action))) => {
                            if let Some(kind) = kind {
                                action.kind = kind;
                            }
                            action.details = Some(details.clone());
                            action.numbers.extend(numbers.iter().copied());
                        }
                        Ok(None) | Err(_) => {}
                        Ok(Some(_)) => *view = Err(EventError::ProjectionGap),
                    }
                }
            }
        }
        if !found {
            self.record_player_fact_views(
                [
                    Err(EventError::ProjectionGap),
                    Err(EventError::ProjectionGap),
                ],
                &Value::Null,
                None,
            );
        }
    }

    pub(in crate::game) fn omit_player_step(&mut self, step: &str) {
        if let Some(entry) = self
            .player_events
            .pending
            .iter_mut()
            .find(|entry| entry.step.as_deref() == Some(step))
        {
            entry.views = [Ok(None), Ok(None)];
        }
    }

    pub(in crate::game) fn record_player_step(&mut self, event: &Value) {
        if !self.player_events.active() {
            return;
        }
        let tag = event["kind"].as_str().unwrap_or_default();
        // The draw/discard adapters enrich the move itself; their declaration traces are not extra moves.
        if matches!(tag, "引く" | "捨てる") {
            return;
        }
        let movement = matches!(tag, "移動" | "場に出す");
        let views = if movement {
            [Ok(None), Ok(None)]
        } else {
            [Viewer::P1, Viewer::P2].map(|perspective| self.project_player_step(perspective, event))
        };
        self.player_events.push(
            views,
            event["id"].as_str().map(str::to_owned),
            &event["cause"],
            format!("step/{}", event["group"]),
            event["object"].as_str().map(str::to_owned),
            !movement,
        );
        let generation = event["object"]
            .as_str()
            .and_then(|id| self.state.objects.get(id))
            .map(|object| object.generation);
        if let Some(entry) = self.player_events.pending.last_mut() {
            entry.generation = generation;
        }
    }

    fn project_player_step(&self, perspective: Viewer, event: &Value) -> Projected {
        let tag = event["kind"].as_str().ok_or(EventError::ProjectionGap)?;
        if matches!(tag, "勝利" | "敗北") {
            let player = event["player"].as_str().ok_or(EventError::ProjectionGap)?;
            let player = viewer(player).map_err(|_authority_error| EventError::ProjectionGap)?;
            return Ok(Some(EventKind::Result {
                winner: (tag == "勝利").then_some(player),
                loser: (tag == "敗北").then_some(player),
                rule: event["by"].as_str().map(str::to_owned),
            }));
        }
        let kind = action_kind(tag)?;
        let mut result = action(kind);
        result.player = event["player"]
            .as_str()
            .map(viewer)
            .transpose()
            .map_err(|_authority_error| EventError::ProjectionGap)?;
        if kind == ActionKind::Reveal {
            let permitted = event["to"] == "all" || event["to"] == seat(perspective);
            if !permitted {
                return Ok(None);
            }
            let id = event["object"].as_str().ok_or(EventError::ProjectionGap)?;
            result.subjects.push(
                self.event_card(perspective, id, true)?
                    .map(|card| EventSubject::Card(Box::new(card))),
            );
        } else {
            for key in [
                "object",
                "source",
                "attacker",
                "target",
                "equipment",
                "with",
            ] {
                if let Some(id) = event[key].as_str() {
                    result.subjects.push(
                        if key == "source" || (key == "object" && kind == ActionKind::Resolve) {
                            None
                        } else {
                            self.player_event_subject(perspective, id)?
                        },
                    );
                }
            }
        }
        if matches!(kind, ActionKind::Counter | ActionKind::Resource) {
            result.name = if kind == ActionKind::Counter {
                event["name"].as_str().map(str::to_owned)
            } else {
                Some(
                    match tag {
                        "回復" => "pp",
                        "PP最大値変化" => "maximum_pp",
                        "EP変化" => "ep",
                        _ => "life",
                    }
                    .into(),
                )
            };
        }
        let number = event["amount"]
            .as_i64()
            .or_else(|| event["delta"].as_i64())
            .or_else(|| event["result"].as_i64());
        if let Some(amount) = number {
            result.numbers.push(NumberFacet {
                stat: None,
                before: event["before"].as_i64(),
                after: event["after"].as_i64(),
                amount,
            });
        }
        if kind == ActionKind::Evolve {
            result.details = Some(ActionDetails::Evolution(if tag == "超進化" {
                EvolutionState::SuperEvolved
            } else {
                EvolutionState::Evolved
            }));
        }
        if kind == ActionKind::TurnSkipped {
            result.details = Some(ActionDetails::TurnSkipped {
                extra: event["extra"].as_bool().ok_or(EventError::ProjectionGap)?,
            });
        }
        result.face_up = event["face_up"].as_bool();
        Ok(Some(EventKind::Action(result)))
    }
}
