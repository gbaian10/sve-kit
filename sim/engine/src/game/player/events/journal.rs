use super::presentation::{Presentation, PresentationCursor, PresentationEntry};
use super::{
    ActionKind, EventError, EventId, EventKind, EventLocation, PlayerEvent, PlayerEventBatch,
    Viewer,
};
use crate::game::Game;
use crate::game::player::ObjectHandle;
use alloc::collections::{BTreeMap, BTreeSet};
use alloc::sync::Arc;
use core::mem::take;
use serde_json::Value;

pub(super) const fn index(viewer: Viewer) -> usize {
    match viewer {
        Viewer::P1 => 0,
        Viewer::P2 => 1,
    }
}

fn coordinate(map: &mut Arc<BTreeMap<String, u64>>, key: &str) -> u64 {
    if let Some(value) = map.get(key) {
        return *value;
    }
    let value = u64::try_from(map.len()).unwrap_or(u64::MAX);
    Arc::make_mut(map).insert(key.into(), value);
    value
}

#[derive(Clone, Debug, Default)]
struct ViewerLog {
    stream: Arc<()>,
    chunks: Arc<[Arc<PlayerEventBatch>]>,
    count: u64,
    causes: Arc<BTreeMap<String, u64>>,
    groups: Arc<BTreeMap<String, u64>>,
    parents: Arc<BTreeMap<String, EventId>>,
    fault: bool,
    presentation: Presentation,
}

impl ViewerLog {
    /// Finish one committed batch. A batch this viewer was shown early must commit exactly
    /// as shown; otherwise this viewer's history stops rather than guess the correspondence.
    fn close(
        &mut self,
        serial: u64,
        mut events: Vec<PlayerEvent>,
        batches: &mut Vec<Arc<PlayerEventBatch>>,
    ) -> bool {
        let covered = self.presentation.uncover(serial);
        if !self.presentation.promote(&self.stream, serial, &mut events) {
            self.fault = true;
            return false;
        }
        if !events.is_empty() {
            batches.push(Arc::new(PlayerEventBatch { events, covered }));
        }
        true
    }
}

#[derive(Clone, Debug)]
pub(super) struct Pending {
    serial: u64,
    batch: u64,
    pub(super) step: Option<String>,
    cause: String,
    parent: Option<String>,
    group: String,
    pub(super) object: Option<String>,
    pub(super) generation: Option<u64>,
    pub(super) before_generation: Option<u64>,
    pub(super) sealed: bool,
    /// Part of a play's declaration and payment, up to and including being played.
    prefix: bool,
    ordering: BatchOrder,
    pub(super) views: [Result<Option<EventKind>, EventError>; 2],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BatchOrder {
    Fixed,
    Movement,
    Disclosure,
}

#[derive(Clone, Debug, Default)]
pub(in crate::game) struct Journal {
    active: bool,
    serial: u64,
    committed: u64,
    logs: [ViewerLog; 2],
    pub(super) pending: Vec<Pending>,
}

/// Only published prefixes cross an authority origin rollback.
#[derive(Clone, Debug)]
pub(in crate::game) struct PublishedEvents {
    active: bool,
    logs: [ViewerLog; 2],
    committed: u64,
}

pub(in crate::game) struct MovementBefore {
    pub(super) start: usize,
    pub(super) cards: BTreeMap<String, [Option<EventLocation>; 2]>,
    pub(super) generations: BTreeMap<String, u64>,
}

/// Rules order as one perspective may see it; shared by commitment and early presentation.
fn ordered(pending: &[Pending], perspective: usize) -> Vec<&Pending> {
    let mut ordered = pending.iter().collect::<Vec<_>>();
    let mut start = 0;
    while let Some(first) = ordered.get(start) {
        let is_unordered = first.ordering != BatchOrder::Fixed && first.object.is_some();
        let mut end = start.saturating_add(1);
        if is_unordered {
            while ordered.get(end).is_some_and(|entry| {
                entry.batch == first.batch
                    && entry.group == first.group
                    && entry.ordering == first.ordering
                    && entry.object.is_some()
            }) {
                end = end.saturating_add(1);
            }
            if let Some(unordered_batch) = ordered.get_mut(start..end) {
                // Private permutations cannot order another viewer's authorized revocations.
                unordered_batch.sort_by_cached_key(|entry| {
                    entry.views.get(perspective).map(super::ordering::key)
                });
            }
        }
        start = end;
    }
    ordered
}

/// One leading batch that is final and projectable for a perspective.
struct Sealed {
    batch: u64,
    /// Authorized members; a batch without one opens no scope.
    events: Vec<EventKind>,
    /// No member is part of a play's declaration and payment, whether that play has been
    /// played or is still declaring since `unplayed`.
    ordinary: bool,
}

/// Leading batches whose every member is final and projectable for this perspective.
fn sealed_batches(pending: &[Pending], perspective: usize, unplayed: Option<u64>) -> Vec<Sealed> {
    let mut batches: Vec<Sealed> = Vec::new();
    for entry in ordered(pending, perspective) {
        let view = match entry.views.get(perspective) {
            Some(Ok(view)) if entry.sealed => view,
            _ => {
                // The batch this member belongs to is not complete for this perspective.
                if batches
                    .last()
                    .is_some_and(|sealed| sealed.batch == entry.batch)
                {
                    batches.pop();
                }
                break;
            }
        };
        if batches
            .last()
            .is_none_or(|sealed| sealed.batch != entry.batch)
        {
            batches.push(Sealed {
                batch: entry.batch,
                events: Vec::new(),
                ordinary: true,
            });
        }
        if let Some(sealed) = batches.last_mut() {
            sealed.ordinary &= !entry.prefix && unplayed.is_none_or(|began| entry.serial <= began);
            if let Some(kind) = view {
                sealed.events.push(kind.clone());
            }
        }
    }
    batches
}

impl Journal {
    pub(in crate::game) const fn active(&self) -> bool {
        self.active
    }

    pub(in crate::game) fn published(&self) -> PublishedEvents {
        PublishedEvents {
            active: self.active,
            logs: self.logs.clone(),
            committed: self.committed,
        }
    }

    pub(in crate::game) fn restore(&mut self, published: &PublishedEvents) {
        self.active |= published.active;
        self.logs.clone_from(&published.logs);
        self.committed = published.committed;
        self.serial = self.serial.max(self.committed);
        self.pending.retain(|entry| entry.serial > self.committed);
        let pending = &self.pending;
        for log in &mut self.logs {
            log.presentation
                .discard(|batch| pending.iter().any(|entry| entry.batch == batch));
        }
    }

    /// Close the scopes a restoration discarded: withdrawn after a fault, otherwise the
    /// viewer's history stops because a shown result was undone by rules.
    pub(in crate::game) fn settle_presentation(&mut self, fault: bool) {
        for log in &mut self.logs {
            if !log.presentation.settle(&log.stream, fault) {
                log.fault = true;
            }
        }
    }

    pub(super) fn push(
        &mut self,
        views: [Result<Option<EventKind>, EventError>; 2],
        step: Option<String>,
        cause: &Value,
        group: String,
        object: Option<String>,
        sealed: bool,
    ) {
        if !self.active {
            return;
        }
        let Some(serial) = self.serial.checked_add(1) else {
            for log in &mut self.logs {
                log.fault = true;
            }
            return;
        };
        self.serial = serial;
        let ordering = if !sealed {
            BatchOrder::Movement
        } else if views.iter().any(|view| matches!(view, Ok(Some(EventKind::Action(action))) if action.kind == ActionKind::Reveal)) {
            BatchOrder::Disclosure
        } else {
            BatchOrder::Fixed
        };
        self.pending.push(Pending {
            serial,
            batch: serial,
            step,
            cause: cause.to_string(),
            parent: cause["event"].as_str().map(str::to_owned),
            group,
            object,
            generation: None,
            before_generation: None,
            sealed,
            prefix: false,
            ordering,
            views,
        });
    }

    fn commit(&mut self) {
        let mut logs = take(&mut self.logs);
        for (perspective, log) in logs.iter_mut().enumerate() {
            if log.fault {
                continue;
            }
            let mut batches: Vec<Arc<PlayerEventBatch>> = Vec::new();
            let mut batch = Vec::new();
            let mut previous = None;
            for entry in ordered(&self.pending, perspective) {
                if let Some(serial) = previous.filter(|serial| *serial != entry.batch)
                    && !log.close(serial, take(&mut batch), &mut batches)
                {
                    break;
                }
                previous = Some(entry.batch);
                let Some(view) = entry.views.get(perspective) else {
                    log.fault = true;
                    break;
                };
                if !entry.sealed {
                    log.fault = true;
                    break;
                }
                let kind = match view {
                    Ok(None) => continue,
                    Ok(Some(kind)) => kind.clone(),
                    Err(_) => {
                        log.fault = true;
                        break;
                    }
                };
                let Some(next) = log.count.checked_add(1) else {
                    log.fault = true;
                    break;
                };
                let id = EventId {
                    stream: Arc::clone(&log.stream),
                    index: next,
                };
                let cause = coordinate(&mut log.causes, &entry.cause);
                let group = coordinate(&mut log.groups, &entry.group);
                let parent = entry
                    .parent
                    .as_ref()
                    .and_then(|key| log.parents.get(key))
                    .cloned();
                if let Some(step) = &entry.step {
                    Arc::make_mut(&mut log.parents).insert(step.clone(), id.clone());
                }
                batch.push(PlayerEvent {
                    id,
                    cause,
                    group,
                    parent,
                    kind,
                    presentation: None,
                });
                log.count = next;
            }
            if let Some(serial) = previous.filter(|_| !log.fault) {
                log.close(serial, batch, &mut batches);
            }
            // Everything uncommitted has now committed, so a scope left open lost its source.
            log.fault |= !log.presentation.settled();
            if !log.fault {
                let mut chunks = log.chunks.to_vec();
                chunks.extend(batches);
                log.chunks = Arc::from(chunks);
            }
        }
        self.logs = logs;
        self.committed = self.serial;
        self.pending.clear();
    }

    pub(in crate::game) fn read_batches(
        &self,
        viewer: Viewer,
        after: Option<&EventId>,
    ) -> Result<impl Iterator<Item = &PlayerEventBatch>, EventError> {
        let (log, offset) = self.reader(viewer, after)?;
        if let Some(id) = after
            && !log
                .chunks
                .iter()
                .any(|batch| batch.events.last().is_some_and(|event| event.id == *id))
        {
            return Err(EventError::InvalidCursor);
        }
        Ok(log.chunks.iter().map(Arc::as_ref).filter(move |batch| {
            batch
                .events
                .last()
                .is_some_and(|event| event.id.index > offset)
        }))
    }

    pub(in crate::game) fn read(
        &self,
        viewer: Viewer,
        after: Option<&EventId>,
    ) -> Result<impl Iterator<Item = &PlayerEvent>, EventError> {
        let (log, offset) = self.reader(viewer, after)?;
        Ok(log
            .chunks
            .iter()
            .flat_map(|chunk| chunk.events.iter())
            .filter(move |event| event.id.index > offset))
    }

    /// One handoff to this viewer: when `ready`, record every complete batch it has not been
    /// shown yet, in order, up to the first one it cannot identify; then remember `live`,
    /// the handles it holds now, for the next handoff.
    ///
    /// The board handed now already contains every complete batch. One that gets no scope
    /// here is therefore not new to this viewer when it commits, unless it belongs to a
    /// play's declaration and payment, which keep their first presentation; `unplayed` is
    /// where a play that has not been played yet began.
    pub(super) fn present(
        &mut self,
        viewer: Viewer,
        ready: bool,
        live: BTreeSet<ObjectHandle>,
        unplayed: Option<u64>,
    ) -> Result<(), EventError> {
        let perspective = index(viewer);
        let log = self
            .logs
            .get_mut(perspective)
            .ok_or(EventError::ProjectionGap)?;
        if log.fault {
            return Err(EventError::ProjectionGap);
        }
        let sealed = sealed_batches(&self.pending, perspective, unplayed);
        if ready {
            for batch in &sealed {
                match log
                    .presentation
                    .show(&log.stream, &live, batch.batch, batch.events.clone())
                {
                    Some(true) => {}
                    Some(false) => break,
                    None => {
                        log.fault = true;
                        return Err(EventError::ProjectionGap);
                    }
                }
            }
        }
        for batch in sealed {
            if batch.ordinary && !batch.events.is_empty() {
                log.presentation.cover(batch.batch);
            }
        }
        log.presentation.hold(live);
        Ok(())
    }

    fn play_prefix(&mut self, began: u64) {
        for entry in &mut self.pending {
            if entry.serial > began {
                entry.prefix = true;
            }
        }
    }

    pub(super) fn read_presentation(
        &self,
        viewer: Viewer,
        after: Option<&PresentationCursor>,
    ) -> Result<impl Iterator<Item = &PresentationEntry>, EventError> {
        let log = self
            .logs
            .get(index(viewer))
            .ok_or(EventError::ProjectionGap)?;
        if log.fault {
            return Err(EventError::ProjectionGap);
        }
        log.presentation.read(&log.stream, after)
    }

    fn reader(
        &self,
        viewer: Viewer,
        after: Option<&EventId>,
    ) -> Result<(&ViewerLog, u64), EventError> {
        let log = self
            .logs
            .get(index(viewer))
            .ok_or(EventError::ProjectionGap)?;
        if log.fault {
            return Err(EventError::ProjectionGap);
        }
        let offset = match after {
            None => 0,
            Some(id) if Arc::ptr_eq(&id.stream, &log.stream) && id.index <= log.count => id.index,
            Some(_) => return Err(EventError::InvalidCursor),
        };
        Ok((log, offset))
    }
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Journal publication follows the player authority boundary."
)]
impl Game {
    /// The journal position after the last recorded result.
    pub(in crate::game) const fn player_event_serial(&self) -> u64 {
        self.player_events.serial
    }

    /// A play that began at `began` has just been played: what it recorded since then is its
    /// declaration and payment, whose first presentation an earlier board does not replace.
    pub(in crate::game) fn player_play_prefix(&mut self, began: u64) {
        self.player_events.play_prefix(began);
    }

    // Completion belongs to the mutator, not to shared cause or group numbers.
    pub(in crate::game) fn seal_player_event_batch(&mut self, start: usize) {
        let Some(first) = self.player_events.pending.get(start) else {
            return;
        };
        let batch = first.serial;
        for entry in self.player_events.pending.iter_mut().skip(start) {
            entry.batch = batch;
        }
    }

    pub(in crate::game) fn record_player_fact_views(
        &mut self,
        views: [Result<Option<EventKind>, EventError>; 2],
        cause: &Value,
        group: Option<String>,
    ) {
        let serial = self.player_events.serial;
        self.player_events.push(
            views,
            None,
            cause,
            group.unwrap_or_else(|| format!("fact/{serial}")),
            None,
            true,
        );
    }

    pub(in crate::game) const fn enable_player_events(&mut self) {
        if self.player_events.active {
            return;
        }
        self.player_events.active = true;
    }

    pub(in crate::game) fn publish_player_events(&mut self) {
        // Whatever a restoration discarded and no answer accounted for was lost to a fault.
        self.player_events
            .settle_presentation(self.player_fault.is_some());
        // Every suspended execution may still restore an origin, including nested plays.
        if self.player_events.active && (self.typed.is_none() || self.state.game["ended"] == true) {
            self.player_events.commit();
        }
    }

    pub(in crate::game) fn record_player_phase(&mut self) {
        if !self.player_events.active {
            return;
        }
        let views = [Viewer::P1, Viewer::P2].map(|viewer| {
            self.observe(viewer)
                .map(|observation| Some(EventKind::Phase(observation.turn)))
                .map_err(|_authority_error| EventError::ProjectionGap)
        });
        let serial = self.player_events.serial;
        self.player_events.push(
            views,
            None,
            &serde_json::json!({"rule":"phase"}),
            format!("phase/{serial}"),
            None,
            true,
        );
    }
}

#[cfg(test)]
mod tests;
