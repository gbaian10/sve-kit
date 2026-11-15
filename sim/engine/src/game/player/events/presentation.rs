//! Complete uncommitted batches shown early for sound and animation.
//!
//! They are never rules history: committed readers, cursors and causal parents do not see
//! them, and showing one changes no rules state, payment or randomness.

use super::{EventCard, EventError, EventKind, EventSubject, PlayerEvent, Viewer};
use crate::game::Game;
use crate::game::player::ObjectHandle;
use alloc::collections::BTreeSet;
use alloc::sync::Arc;
use core::fmt;
use core::mem::take;

/// One viewer's reference to a provisional scope; never reused within a game.
#[derive(Clone)]
pub struct ScopeId {
    stream: Arc<()>,
    serial: u64,
}

impl ScopeId {
    /// Number among the scopes issued to its viewer.
    pub(crate) const fn sequence(&self) -> u64 {
        self.serial
    }
}

impl PartialEq for ScopeId {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.stream, &other.stream) && self.serial == other.serial
    }
}
impl Eq for ScopeId {}
impl fmt::Debug for ScopeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ScopeId")
            .field("sequence", &self.serial)
            .finish_non_exhaustive()
    }
}

/// One occurrence of a scope, independent of any committed event identity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PresentationId {
    scope: ScopeId,
    index: u64,
}

impl PresentationId {
    /// The scope this occurrence belongs to.
    pub(crate) const fn scope(&self) -> &ScopeId {
        &self.scope
    }

    /// Position among the occurrences of its scope.
    pub(crate) const fn position(&self) -> u64 {
        self.index
    }
}

/// An occurrence shown before it commits, with the permissions frozen when it happened.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProvisionalEvent {
    /// Identity within its scope.
    pub id: PresentationId,
    /// The same authorized result that will commit.
    pub kind: EventKind,
}

/// One complete producer-sealed batch; a scope never gains members later.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProvisionalBatch {
    /// The scope this batch opens.
    pub scope: ScopeId,
    /// All occurrences authorized for this viewer, in the order they will commit; never empty.
    pub events: Vec<ProvisionalEvent>,
}

/// One step in the life of a provisional scope.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PresentationOp {
    /// Show one complete batch now.
    Provisional(ProvisionalBatch),
    /// The scope's batch has committed unchanged; it becomes history and is not played again.
    Promote {
        /// The scope whose occurrences the committed events now refer to.
        scope: ScopeId,
    },
    /// An internal fault discarded the scope's batch; it never happened and never commits.
    /// A rules cancellation or an illegal play is never reported this way.
    Cancel {
        /// The withdrawn scope.
        scope: ScopeId,
    },
}

/// A position in one viewer's presentation record.
#[derive(Clone)]
pub struct PresentationCursor {
    stream: Arc<()>,
    index: u64,
}

impl PartialEq for PresentationCursor {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.stream, &other.stream) && self.index == other.index
    }
}
impl Eq for PresentationCursor {}
impl fmt::Debug for PresentationCursor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PresentationCursor")
            .field("sequence", &self.index)
            .finish_non_exhaustive()
    }
}

/// One recorded step, in delivery order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PresentationEntry {
    /// Cursor for the next read.
    pub cursor: PresentationCursor,
    /// What to do with the scope.
    pub op: PresentationOp,
}

/// One viewer's scopes. It travels with the published history across an origin rollback,
/// so a delivered step is never withdrawn silently and a scope number is never reissued.
#[derive(Clone, Debug, Default)]
pub(super) struct Presentation {
    issued: u64,
    /// Scopes that are neither committed nor withdrawn.
    open: Vec<Shown>,
    /// Scopes whose batch a restoration discarded, until the submission's result says why.
    discarded: Vec<ScopeId>,
    /// Source batches that a board handed to this viewer already contained without a scope.
    /// They are never shown early afterwards and commit as history. Only batches before the
    /// first one this viewer could not identify are recorded, so that batch keeps holding
    /// back everything after it.
    covered: Vec<u64>,
    /// Handles of the objects on the board this viewer was handed last.
    held: Arc<BTreeSet<ObjectHandle>>,
    record: Arc<[Arc<PresentationEntry>]>,
}

/// Every card identity a result names for its viewer.
fn cards(kind: &EventKind) -> Vec<&EventCard> {
    match kind {
        EventKind::Movement { before, after, .. } => {
            before.card.iter().chain(after.card.iter()).collect()
        }
        EventKind::Action(action) => action
            .subjects
            .iter()
            .flatten()
            .filter_map(|subject| match subject {
                EventSubject::Card(card) => Some(card.as_ref()),
                EventSubject::Leader(_) => None,
            })
            .collect(),
        EventKind::Phase(_) | EventKind::Result { .. } => Vec::new(),
    }
}

/// What a viewer was shown, kept to prove that the committed batch is the same one.
#[derive(Clone, Debug)]
struct Shown {
    /// Source batch in the uncommitted journal.
    batch: u64,
    scope: ScopeId,
    events: Arc<[ProvisionalEvent]>,
}

impl Presentation {
    /// Whether this viewer can identify every card of these results from the board it has
    /// now or the one it was last handed. A card revealed and hidden again inside the
    /// uncommitted window is known only from memory; without separate evidence that the
    /// viewer saw it, its result waits for commitment.
    fn identifiable(&self, live: &BTreeSet<ObjectHandle>, events: &[EventKind]) -> bool {
        events.iter().all(|kind| {
            let source = match kind {
                EventKind::Action(action) => action
                    .source
                    .as_ref()
                    .and_then(|source| source.holder.as_ref()),
                EventKind::Movement { .. } | EventKind::Phase(_) | EventKind::Result { .. } => None,
            };
            cards(kind)
                .into_iter()
                .map(|card| card.handle.as_ref())
                .chain(source.map(Some))
                .all(|handle| {
                    handle.is_some_and(|known| live.contains(known) || self.held.contains(known))
                })
        })
    }

    /// Remember the board this viewer is handed now.
    pub(super) fn hold(&mut self, live: BTreeSet<ObjectHandle>) {
        if *self.held != live {
            self.held = Arc::new(live);
        }
    }

    /// Open a scope for one complete batch unless this viewer already has it.
    /// `Some(false)` means this batch, and therefore every later one, must wait for
    /// commitment; `None` reports identity exhaustion.
    pub(super) fn show(
        &mut self,
        stream: &Arc<()>,
        live: &BTreeSet<ObjectHandle>,
        batch: u64,
        events: Vec<EventKind>,
    ) -> Option<bool> {
        if events.is_empty() || self.settled_before(batch) {
            return Some(true);
        }
        if !self.identifiable(live, &events) {
            return Some(false);
        }
        let serial = self.issued.checked_add(1)?;
        let scope = ScopeId {
            stream: Arc::clone(stream),
            serial,
        };
        let events = (0_u64..)
            .zip(events)
            .map(|(index, kind)| ProvisionalEvent {
                id: PresentationId {
                    scope: scope.clone(),
                    index,
                },
                kind,
            })
            .collect::<Vec<_>>();
        self.issued = serial;
        self.open.push(Shown {
            batch,
            scope: scope.clone(),
            events: Arc::from(events.clone()),
        });
        self.append(
            stream,
            PresentationOp::Provisional(ProvisionalBatch { scope, events }),
        )
        .map(|()| true)
    }

    /// Link one committed batch to the scope it was shown as. A batch that was never shown
    /// stays unlinked; `false` means the shown and committed batches differ, so the exact
    /// correspondence this viewer was promised is lost.
    pub(super) fn promote(
        &mut self,
        stream: &Arc<()>,
        batch: u64,
        events: &mut [PlayerEvent],
    ) -> bool {
        let Some(position) = self.open.iter().position(|shown| shown.batch == batch) else {
            return true;
        };
        let shown = self.open.remove(position);
        if shown.events.len() != events.len()
            || shown
                .events
                .iter()
                .zip(events.iter())
                .any(|(before, after)| before.kind != after.kind)
        {
            return false;
        }
        for (event, before) in events.iter_mut().zip(shown.events.iter()) {
            event.presentation = Some(before.id.clone());
        }
        self.append(stream, PresentationOp::Promote { scope: shown.scope })
            .is_some()
    }

    /// The board handed now contains this batch: unless it is a play's declaration and
    /// payment, it is not new when it commits. `false`: the viewer cannot identify it.
    pub(super) fn cover(
        &mut self,
        live: &BTreeSet<ObjectHandle>,
        batch: u64,
        events: &[EventKind],
        ordinary: bool,
    ) -> bool {
        if events.is_empty() || self.settled_before(batch) {
            return true;
        }
        if !self.identifiable(live, events) {
            return false;
        }
        if ordinary {
            self.covered.push(batch);
        }
        true
    }

    /// Whether this viewer already has the batch, as a scope or on a board it was handed.
    fn settled_before(&self, batch: u64) -> bool {
        self.covered.contains(&batch) || self.open.iter().any(|shown| shown.batch == batch)
    }

    /// Whether the batch committing now was already on a board this viewer was handed.
    pub(super) fn uncover(&mut self, batch: u64) -> bool {
        let before = self.covered.len();
        self.covered.retain(|covered| *covered != batch);
        self.covered.len() != before
    }

    /// Whether every shown scope has reached the end of its life.
    pub(super) const fn settled(&self) -> bool {
        self.open.is_empty()
    }

    /// After a restoration, set aside the scopes whose source batch no longer exists.
    /// Their batch numbers may be issued again, so they must stop matching new batches.
    pub(super) fn discard(&mut self, kept: impl Fn(u64) -> bool) {
        let (open, gone) = take(&mut self.open)
            .into_iter()
            .partition(|shown| kept(shown.batch));
        self.open = open;
        self.discarded
            .extend(gone.into_iter().map(|shown: Shown| shown.scope));
        self.covered.retain(|batch| kept(*batch));
    }

    /// Give the discarded scopes their end. Only a fault withdraws a shown scope; `false`
    /// means rules took back something this viewer had already been shown, which no
    /// presentation step can describe.
    pub(super) fn settle(&mut self, stream: &Arc<()>, fault: bool) -> bool {
        let discarded = take(&mut self.discarded);
        if discarded.is_empty() {
            return true;
        }
        fault
            && discarded.into_iter().all(|scope| {
                self.append(stream, PresentationOp::Cancel { scope })
                    .is_some()
            })
    }

    fn append(&mut self, stream: &Arc<()>, op: PresentationOp) -> Option<()> {
        let index = u64::try_from(self.record.len()).ok()?.checked_add(1)?;
        let mut record = self.record.to_vec();
        record.push(Arc::new(PresentationEntry {
            cursor: PresentationCursor {
                stream: Arc::clone(stream),
                index,
            },
            op,
        }));
        self.record = Arc::from(record);
        Some(())
    }

    pub(super) fn read(
        &self,
        stream: &Arc<()>,
        after: Option<&PresentationCursor>,
    ) -> Result<impl Iterator<Item = &PresentationEntry>, EventError> {
        let offset = match after {
            None => 0,
            Some(cursor) if Arc::ptr_eq(&cursor.stream, stream) => {
                usize::try_from(cursor.index).map_err(|_overflow| EventError::InvalidCursor)?
            }
            Some(_) => return Err(EventError::InvalidCursor),
        };
        self.record
            .get(offset..)
            .map(|entries| entries.iter().map(Arc::as_ref))
            .ok_or(EventError::InvalidCursor)
    }
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Presentation shares the game authority without changing its readers."
)]
impl Game {
    /// Record for this viewer every complete uncommitted batch that rules can no longer take
    /// back, then read it with [`Game::presentation`].
    ///
    /// Call this only at the handoff where this viewer receives an update. A batch that is
    /// never requested commits as ordinary history; nothing accumulates for later.
    ///
    /// # Errors
    /// A perspective-local projection gap; input and observation are unaffected.
    pub fn present_player_events(&mut self, viewer: Viewer) -> Result<(), EventError> {
        let live = self.observable_handles(viewer);
        let ready = self.provisional_stop();
        let unplayed = self.unplayed_since();
        self.player_events.present(viewer, ready, live, unplayed)
    }

    /// Read this viewer's recorded presentation steps after a cursor, without changing rules,
    /// identities or randomness.
    ///
    /// # Errors
    /// Rejects a foreign or future cursor and a fixed perspective-local projection gap.
    pub fn presentation(
        &self,
        viewer: Viewer,
        after: Option<&PresentationCursor>,
    ) -> Result<impl Iterator<Item = &PresentationEntry>, EventError> {
        self.player_events.read_presentation(viewer, after)
    }
}
