//! Complete uncommitted batches shown early for sound and animation.
//!
//! They are never rules history: committed readers, cursors and causal parents do not see
//! them, and showing one changes no rules state, payment or randomness.

use super::{EventError, EventKind, Viewer};
use crate::game::Game;
use alloc::sync::Arc;
use core::fmt;

/// One viewer's reference to a provisional scope; never reused within a game.
#[derive(Clone)]
pub struct ScopeId {
    stream: Arc<()>,
    serial: u64,
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
    /// Source batches of the scopes that are neither committed nor withdrawn.
    open: Vec<u64>,
    record: Arc<[Arc<PresentationEntry>]>,
}

impl Presentation {
    /// Open a scope for one complete batch unless this viewer already has it.
    /// `None` reports identity exhaustion.
    pub(super) fn show(
        &mut self,
        stream: &Arc<()>,
        batch: u64,
        events: Vec<EventKind>,
    ) -> Option<()> {
        if events.is_empty() || self.open.contains(&batch) {
            return Some(());
        }
        let serial = self.issued.checked_add(1)?;
        let position = u64::try_from(self.record.len()).ok()?.checked_add(1)?;
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
            .collect();
        self.issued = serial;
        self.open.push(batch);
        let mut record = self.record.to_vec();
        record.push(Arc::new(PresentationEntry {
            cursor: PresentationCursor {
                stream: Arc::clone(stream),
                index: position,
            },
            op: PresentationOp::Provisional(ProvisionalBatch { scope, events }),
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
        if !self.provisional_stop() {
            return self
                .player_events
                .read_presentation(viewer, None)
                .map(|_entries| ());
        }
        self.player_events.present(viewer)
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
