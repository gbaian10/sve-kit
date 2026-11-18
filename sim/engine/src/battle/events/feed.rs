//! One viewer's delivery of committed events and provisional presentation.
//!
//! The feed keeps the native cursors and the bodies its viewer was shown early, so a
//! committed event that promotes a shown one carries exactly the body that was shown.

use super::{EventObjects, EventReferences, ProjectedEvent, project_batch};
use crate::battle::{BattleReferences, ProjectionError};
use crate::game::Game;
use crate::game::player::Viewer;
use crate::game::player::events::{
    EventError, EventId, PlayerEvent, PlayerEventBatch, PresentationCursor, PresentationEntry,
    PresentationId, PresentationOp, ProvisionalBatch,
};
use alloc::collections::BTreeMap;
use sve_battle_contract::event::{BattleEvent, EventBatch, EventDelivery, EventGap};
use sve_battle_contract::presentation as wire;
use sve_battle_contract::{BatchId, CauseId, PresentationBatchId, ScopeId};

/// The events and presentation steps of one update for one viewer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FeedUpdate {
    /// Committed batches not delivered before, or the reason none are delivered.
    pub events: EventDelivery,
    /// Steps of provisional scopes, in the order they happened.
    pub presentation: Vec<wire::PresentationOp>,
}

impl FeedUpdate {
    const fn quiet() -> Self {
        Self {
            events: EventDelivery::Available {
                batches: Vec::new(),
                animate: Vec::new(),
            },
            presentation: Vec::new(),
        }
    }

    const fn unavailable() -> Self {
        Self {
            events: EventDelivery::Unavailable {
                reason: EventGap::ProjectionGap,
            },
            presentation: Vec::new(),
        }
    }
}

/// How a committed batch relates to what this viewer's client already has.
enum Seen {
    /// Shown early through this feed as this scope; these are the bodies that were shown.
    Shown {
        scope: u64,
        bodies: Vec<ProjectedEvent>,
    },
    /// Already contained in a board this viewer holds or held: shown early before its last
    /// baseline, or handed uncommitted without a scope.
    Covered,
    /// Never shown: this delivery is the first time it can be played.
    New,
}

/// Native numbering is already local to one viewer, so its numbers disclose nothing more
/// than the delivered stream itself does.
fn event_id(id: &EventId) -> sve_battle_contract::EventId {
    sve_battle_contract::EventId::new(format!("e{}", id.sequence()))
}

fn scope_id(scope: u64) -> ScopeId {
    ScopeId::new(format!("s{scope}"))
}

fn presentation_id(id: &PresentationId) -> sve_battle_contract::PresentationId {
    sve_battle_contract::PresentationId::new(format!(
        "p{}-{}",
        id.scope().sequence(),
        id.position()
    ))
}

const fn gap(_error: EventError) -> ProjectionError {
    ProjectionError::Unavailable
}

/// One viewer's event delivery across the updates of a local game.
#[derive(Debug)]
pub struct EventFeed {
    viewer: Viewer,
    committed: Option<EventId>,
    presented: Option<PresentationCursor>,
    /// Bodies of the open scopes this viewer's client holds, by scope number.
    shown: BTreeMap<u64, Vec<ProjectedEvent>>,
    /// Delivery ended for this viewer; a later baseline does not revive it.
    stopped: bool,
}

impl EventFeed {
    /// A feed that has delivered nothing; its first delivery must be a baseline.
    #[must_use]
    pub const fn new(viewer: Viewer) -> Self {
        Self {
            viewer,
            committed: None,
            presented: None,
            shown: BTreeMap::new(),
            stopped: false,
        }
    }

    /// Start over from the board this viewer is handed now: earlier history is not played,
    /// and scopes its rebuilt client no longer holds are forgotten.
    pub fn baseline(&mut self, game: &mut Game) -> FeedUpdate {
        if self.stopped {
            return FeedUpdate::unavailable();
        }
        match self.skip(game) {
            Ok(()) => FeedUpdate::quiet(),
            Err(ProjectionError::Unavailable | ProjectionError::Range) => self.stop(),
        }
    }

    /// Whether a delivery now would bring this viewer committed batches it has not received,
    /// or the first report that its events are unavailable. Reads without recording that the
    /// viewer was handed this stop, so a host can decide to deliver before calling
    /// [`Self::deliver`].
    #[must_use]
    pub fn has_news(&self, game: &Game) -> bool {
        !self.stopped
            && game
                .event_batches(self.viewer, self.committed.as_ref())
                .map_or(true, |mut batches| batches.next().is_some())
    }

    /// Everything that happened for this viewer since its previous delivery, naming each
    /// card as its event froze it under the game's own catalogue.
    ///
    /// A failure, including references bound to another catalogue, stops this viewer's
    /// events for good and reports them unavailable; the board and the question are
    /// unaffected.
    pub fn deliver(&mut self, game: &mut Game, references: &BattleReferences) -> FeedUpdate {
        // Sharing the catalogue lets the lookups outlive the exclusive use of the game.
        let catalog = game.shared_catalog();
        match EventReferences::new(&catalog, references) {
            Ok(objects) => self.deliver_with(game, &objects),
            Err(ProjectionError::Unavailable | ProjectionError::Range) => self.stop(),
        }
    }

    /// [`Self::deliver`] with identities supplied by the caller.
    pub fn deliver_with<O>(&mut self, game: &mut Game, objects: &O) -> FeedUpdate
    where
        O: EventObjects,
    {
        if self.stopped {
            return FeedUpdate::unavailable();
        }
        match self.collect(game, objects) {
            Ok(update) => update,
            Err(ProjectionError::Unavailable | ProjectionError::Range) => self.stop(),
        }
    }

    fn stop(&mut self) -> FeedUpdate {
        self.stopped = true;
        self.shown.clear();
        FeedUpdate::unavailable()
    }

    fn skip(&mut self, game: &mut Game) -> Result<(), ProjectionError> {
        // The engine must learn which board this viewer holds even when nothing is played.
        game.present_player_events(self.viewer).map_err(gap)?;
        self.committed = game
            .event_batches(self.viewer, None)
            .map_err(gap)?
            .last()
            .and_then(|batch| batch.events.last())
            .map(|event| event.id.clone());
        self.presented = game
            .presentation(self.viewer, None)
            .map_err(gap)?
            .last()
            .map(|entry| entry.cursor.clone());
        self.shown.clear();
        Ok(())
    }

    fn collect<O>(&mut self, game: &mut Game, objects: &O) -> Result<FeedUpdate, ProjectionError>
    where
        O: EventObjects,
    {
        game.present_player_events(self.viewer).map_err(gap)?;
        let mut presentation = Vec::new();
        let mut promoted = BTreeMap::new();
        let presented = self.presented.clone();
        for entry in game
            .presentation(self.viewer, presented.as_ref())
            .map_err(gap)?
        {
            if let Some(op) = self.step(entry, objects, &mut promoted)? {
                presentation.push(op);
            }
            self.presented = Some(entry.cursor.clone());
        }

        let mut batches = Vec::new();
        let mut animate = Vec::new();
        let committed = self.committed.clone();
        for batch in game
            .event_batches(self.viewer, committed.as_ref())
            .map_err(gap)?
        {
            let last = batch.events.last().ok_or(ProjectionError::Unavailable)?;
            let id = BatchId::new(format!("b{}", last.id.sequence()));
            let unseen = if batch.covered {
                Seen::Covered
            } else {
                Seen::New
            };
            let seen = batch
                .events
                .first()
                .and_then(|event| event.presentation.as_ref())
                .map_or(unseen, |first| {
                    let scope = first.scope().sequence();
                    promoted
                        .remove(&scope)
                        .map_or(Seen::Covered, |bodies| Seen::Shown { scope, bodies })
                });
            if matches!(seen, Seen::New) {
                animate.push(id.clone());
            }
            batches.push(EventBatch {
                id,
                events: committed_events(batch, seen, objects)?,
            });
            self.committed = Some(last.id.clone());
        }
        // A promoted scope whose committed batch did not arrive with it has lost the exact
        // correspondence its viewer was promised.
        if !promoted.is_empty() {
            return Err(ProjectionError::Unavailable);
        }
        Ok(FeedUpdate {
            events: EventDelivery::Available { batches, animate },
            presentation,
        })
    }

    /// One recorded step, as far as this viewer's client knows the scope it is about.
    fn step<O>(
        &mut self,
        entry: &PresentationEntry,
        objects: &O,
        promoted: &mut BTreeMap<u64, Vec<ProjectedEvent>>,
    ) -> Result<Option<wire::PresentationOp>, ProjectionError>
    where
        O: EventObjects,
    {
        match &entry.op {
            PresentationOp::Provisional(batch) => {
                let events = project_batch(batch.events.iter().map(|event| &event.kind), objects)?;
                let op = provisional(batch, &events);
                self.shown.insert(batch.scope.sequence(), events);
                Ok(Some(op))
            }
            PresentationOp::Promote { scope } => {
                let number = scope.sequence();
                Ok(self.shown.remove(&number).map(|events| {
                    promoted.insert(number, events);
                    wire::PresentationOp::Promote {
                        scope_id: scope_id(number),
                    }
                }))
            }
            PresentationOp::Cancel { scope } => {
                let number = scope.sequence();
                Ok(self
                    .shown
                    .remove(&number)
                    .map(|_events| wire::PresentationOp::Cancel {
                        scope_id: scope_id(number),
                        reason: wire::CancelReason::EngineFault,
                    }))
            }
        }
    }
}

fn provisional(batch: &ProvisionalBatch, events: &[ProjectedEvent]) -> wire::PresentationOp {
    let scope = batch.scope.sequence();
    wire::PresentationOp::Provisional {
        scope_id: scope_id(scope),
        batch: wire::PresentationBatch {
            id: PresentationBatchId::new(format!("pb{scope}")),
            events: batch
                .events
                .iter()
                .zip(events)
                .map(|(native, projected)| wire::PresentationEvent {
                    id: presentation_id(&native.id),
                    body: projected.body.clone(),
                    about: projected.about.clone(),
                })
                .collect(),
        },
    }
}

/// The wire events of one committed batch.
fn committed_events<O>(
    batch: &PlayerEventBatch,
    seen: Seen,
    objects: &O,
) -> Result<Vec<BattleEvent>, ProjectionError>
where
    O: EventObjects,
{
    let (bodies, linked) = match seen {
        Seen::Shown { scope, bodies } => {
            // The engine promotes a scope only as a whole and in order; anything else here
            // means the bodies kept for it belong to some other batch.
            let aligned = bodies.len() == batch.events.len()
                && (0_u64..).zip(&batch.events).all(|(position, event)| {
                    event.presentation.as_ref().is_some_and(|shown| {
                        shown.scope().sequence() == scope && shown.position() == position
                    })
                });
            if !aligned {
                return Err(ProjectionError::Unavailable);
            }
            (bodies, true)
        }
        Seen::Covered | Seen::New => (
            project_batch(batch.events.iter().map(|event| &event.kind), objects)?,
            false,
        ),
    };
    Ok(batch
        .events
        .iter()
        .zip(bodies)
        .map(|(native, projected)| battle_event(native, projected, linked))
        .collect())
}

fn battle_event(native: &PlayerEvent, projected: ProjectedEvent, linked: bool) -> BattleEvent {
    BattleEvent {
        id: event_id(&native.id),
        cause: CauseId::new(format!("c{}", native.cause)),
        parent: native.parent.as_ref().map(event_id),
        body: projected.body,
        about: projected.about,
        presentation_id: native
            .presentation
            .as_ref()
            .filter(|_shown| linked)
            .map(presentation_id),
    }
}
