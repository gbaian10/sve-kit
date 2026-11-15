use super::super::player_pipeline::answer;
use super::nested_play::Scenario;
use super::presentation_fault::{ask_for_the_follower, prepared};
use super::presentation_nested::started;
use super::*;
use alloc::collections::BTreeSet;
use core::cell::Cell;
use sve_battle_contract::event::{self as wire, EventBatch, EventDelivery, EventGap};
use sve_battle_contract::observation::TurnView;
use sve_battle_contract::presentation::{CancelReason, PresentationBatch, PresentationOp};
use sve_battle_contract::{
    BatchId, CardRef, Int as WireInt, Nat as WireNat, ObjectHandle as WireHandle,
    ScopeId as WireScope, Seat, SourceRef,
};
use sve_engine::battle::{EventFeed, EventObjects, FeedUpdate, ProjectionError};
use sve_engine::game::player::events::EventCard;
use sve_engine::game::player::observation::{EffectSource, TurnObservation};
use sve_engine::game::player::{SubmissionKind, Viewer};

const SEATS: [Viewer; 2] = [Viewer::P1, Viewer::P2];

/// Stands in for the board projection. Every object code carries `era`, so a test can make
/// later lookups answer differently from earlier ones; `broken` makes every card lookup fail.
#[derive(Default)]
pub(super) struct Board {
    era: Cell<u64>,
    broken: Cell<bool>,
}

const fn seat(viewer: Viewer) -> Seat {
    match viewer {
        Viewer::P1 => Seat::P1,
        Viewer::P2 => Seat::P2,
    }
}

impl EventObjects for Board {
    fn card(&self, card: &EventCard) -> Result<wire::EventCard, ProjectionError> {
        if self.broken.get() {
            return Err(ProjectionError::Unavailable);
        }
        Ok(wire::EventCard {
            card: CardRef {
                printing_id: card.properties.card.clone(),
                face: WireNat::try_from(card.properties.face)?,
            },
            object: card
                .handle
                .as_ref()
                .map(|handle| WireHandle::new(format!("{}@{}", handle.as_str(), self.era.get()))),
        })
    }

    fn source(&self, source: &EffectSource) -> Result<SourceRef, ProjectionError> {
        Ok(SourceRef::Card {
            printing_id: source.card.clone(),
            face: None,
            line: source.line.map(WireNat::try_from).transpose()?,
            holder: None,
        })
    }

    fn turn(&self, turn: &TurnObservation) -> Result<TurnView, ProjectionError> {
        let [first, second] = turn.elapsed_turns;
        Ok(TurnView {
            active: turn.active.map(seat),
            first_player: turn.first_player.map(seat),
            phase: None,
            elapsed_turns: [WireNat::try_from(first)?, WireNat::try_from(second)?],
        })
    }
}

const fn quiet() -> FeedUpdate {
    FeedUpdate {
        events: EventDelivery::Available {
            batches: Vec::new(),
            animate: Vec::new(),
        },
        presentation: Vec::new(),
    }
}

const fn unavailable() -> FeedUpdate {
    FeedUpdate {
        events: EventDelivery::Unavailable {
            reason: EventGap::ProjectionGap,
        },
        presentation: Vec::new(),
    }
}

/// The committed batches of an update and the ones it asks to play; nothing when events
/// are unavailable.
fn committed(update: &FeedUpdate) -> (Vec<EventBatch>, Vec<BatchId>) {
    match &update.events {
        EventDelivery::Available { batches, animate } => (batches.clone(), animate.clone()),
        EventDelivery::Unavailable { .. } => (Vec::new(), Vec::new()),
    }
}

fn shown(update: &FeedUpdate) -> Vec<(WireScope, PresentationBatch)> {
    update
        .presentation
        .iter()
        .filter_map(|op| match op {
            PresentationOp::Provisional { scope_id, batch } => {
                Some((scope_id.clone(), batch.clone()))
            }
            PresentationOp::Promote { .. } | PresentationOp::Cancel { .. } => None,
        })
        .collect()
}

fn promoted(update: &FeedUpdate) -> Vec<WireScope> {
    update
        .presentation
        .iter()
        .filter_map(|op| match op {
            PresentationOp::Promote { scope_id } => Some(scope_id.clone()),
            PresentationOp::Provisional { .. } | PresentationOp::Cancel { .. } => None,
        })
        .collect()
}

fn cancelled(update: &FeedUpdate) -> Vec<WireScope> {
    update
        .presentation
        .iter()
        .filter_map(|op| match op {
            PresentationOp::Cancel {
                scope_id,
                reason: CancelReason::EngineFault,
            } => Some(scope_id.clone()),
            PresentationOp::Provisional { .. } | PresentationOp::Promote { .. } => None,
        })
        .collect()
}

fn scopes(update: &FeedUpdate) -> Vec<WireScope> {
    shown(update).into_iter().map(|(scope, _)| scope).collect()
}

/// Both seats observe the game before the ability is activated, then are handed the stop
/// where its controller is asked for a follower.
fn handed_the_stop(game: &mut Game, board: &Board) -> ([EventFeed; 2], [FeedUpdate; 2]) {
    let mut feeds = SEATS.map(EventFeed::new);
    for feed in &mut feeds {
        assert_eq!(
            feed.baseline(game),
            quiet(),
            "a first observation plays no history and shows nothing early"
        );
    }
    ask_for_the_follower(game);
    let early = feeds.each_mut().map(|feed| feed.deliver(game, board));
    (feeds, early)
}

#[test]
fn a_shown_batch_commits_with_the_body_it_was_shown_and_is_not_played_again() {
    let mut game = prepared(false);
    let board = Board::default();
    let (mut feeds, early) = handed_the_stop(&mut game, &board);
    for update in &early {
        assert_eq!(
            committed(update),
            (Vec::new(), Vec::new()),
            "nothing has committed while the play is unresolved"
        );
        assert!(
            promoted(update).is_empty() && cancelled(update).is_empty(),
            "a scope that was just opened has not ended"
        );
    }
    assert!(
        shown(&early[0]).len() >= 2
            && shown(&early[0])
                .iter()
                .any(|(_, batch)| batch.events.iter().any(|event| !event.about.is_empty())),
        "the acting seat was shown the play and its own drawn card: {:?}",
        early[0]
    );

    // Whatever the board projection answers from now on must not change what was shown.
    board.era.set(1);
    assert_eq!(
        answer(&mut game, Viewer::P1, Answer::Targets(vec![0])).kind,
        SubmissionKind::Applied,
        "the chosen follower is damaged and the ability resolves"
    );
    for (feed, before) in feeds.iter_mut().zip(&early) {
        let update = feed.deliver(&mut game, &board);
        assert_eq!(
            promoted(&update),
            scopes(before),
            "every shown scope is promoted once, in the order it was shown"
        );
        assert!(
            shown(&update).is_empty() && cancelled(&update).is_empty(),
            "a finished play shows nothing new and withdraws nothing"
        );
        let (batches, animate) = committed(&update);
        let mut linked = Vec::new();
        for (_, batch) in shown(before) {
            let first = batch.events.first().map(|event| &event.id);
            let matching = batches
                .iter()
                .filter(|candidate| {
                    candidate
                        .events
                        .first()
                        .is_some_and(|event| event.presentation_id.as_ref() == first)
                })
                .collect::<Vec<_>>();
            let [promotion] = matching.as_slice() else {
                panic!("exactly one committed batch promotes each scope, got {matching:?}");
            };
            assert_eq!(
                promotion
                    .events
                    .iter()
                    .map(|event| (event.presentation_id.as_ref(), &event.body, &event.about))
                    .collect::<Vec<_>>(),
                batch
                    .events
                    .iter()
                    .map(|event| (Some(&event.id), &event.body, &event.about))
                    .collect::<Vec<_>>(),
                "member for member, the committed events carry what was shown, unchanged"
            );
            linked.push(promotion.id.clone());
        }
        let fresh = batches
            .iter()
            .filter(|batch| !linked.contains(&batch.id))
            .collect::<Vec<_>>();
        assert!(
            !fresh.is_empty()
                && fresh.iter().all(|batch| batch
                    .events
                    .iter()
                    .all(|event| event.presentation_id.is_none())),
            "the damage and what followed committed without having been shown"
        );
        assert_eq!(
            animate,
            fresh
                .iter()
                .map(|batch| batch.id.clone())
                .collect::<Vec<_>>(),
            "only the batches that were never shown are played now"
        );
        assert_eq!(
            feed.deliver(&mut game, &board),
            quiet(),
            "nothing is delivered twice"
        );
    }
}

#[test]
fn delivered_identities_are_unique_and_parents_point_at_delivered_events() {
    let mut game = prepared(false);
    let board = Board::default();
    let (mut feeds, _early) = handed_the_stop(&mut game, &board);
    assert_eq!(
        answer(&mut game, Viewer::P1, Answer::Targets(vec![0])).kind,
        SubmissionKind::Applied,
        "the ability resolves"
    );
    for feed in &mut feeds {
        let (batches, _animate) = committed(&feed.deliver(&mut game, &board));
        let events = batches
            .iter()
            .flat_map(|batch| &batch.events)
            .collect::<Vec<_>>();
        let ids = events
            .iter()
            .map(|event| &event.id)
            .collect::<BTreeSet<_>>();
        assert_eq!(
            ids.len(),
            events.len(),
            "every committed event has its own identity"
        );
        assert_eq!(
            batches
                .iter()
                .map(|batch| &batch.id)
                .collect::<BTreeSet<_>>()
                .len(),
            batches.len(),
            "every batch has its own identity"
        );
        assert!(
            events
                .iter()
                .filter_map(|event| event.parent.as_ref())
                .all(|parent| ids.contains(parent)),
            "a causal parent is an event this viewer was delivered"
        );
    }
}

#[test]
fn a_fault_withdraws_exactly_the_shown_scopes_and_delivers_no_history() {
    let mut game = prepared(true);
    let board = Board::default();
    let (mut feeds, early) = handed_the_stop(&mut game, &board);
    assert_eq!(
        answer(&mut game, Viewer::P1, Answer::Targets(vec![0])).kind,
        SubmissionKind::EngineFault,
        "the oversized draw is an internal fault"
    );
    for (feed, before) in feeds.iter_mut().zip(&early) {
        let update = feed.deliver(&mut game, &board);
        assert!(
            !scopes(before).is_empty(),
            "each seat had been shown something before the fault"
        );
        assert_eq!(
            cancelled(&update),
            scopes(before),
            "every shown scope is withdrawn once, in order, because of the fault"
        );
        assert!(
            promoted(&update).is_empty() && shown(&update).is_empty(),
            "a withdrawn scope is never promoted and nothing new is shown"
        );
        assert_eq!(
            committed(&update),
            (Vec::new(), Vec::new()),
            "the faulted play left no history"
        );
        assert_eq!(
            feed.deliver(&mut game, &board),
            quiet(),
            "a withdrawal is reported once"
        );
    }
}

#[test]
fn a_client_rebuilt_at_the_stop_receives_the_shown_batches_as_plain_history() {
    let mut game = prepared(false);
    let board = Board::default();
    let ([mut kept, _other], _early) = handed_the_stop(&mut game, &board);
    // A rebuilt client starts from the board at the stop and holds none of the open scopes.
    let mut rebuilt = EventFeed::new(Viewer::P1);
    assert_eq!(
        rebuilt.baseline(&mut game),
        quiet(),
        "a rebuilt client replays nothing that its new board already contains"
    );
    assert_eq!(
        answer(&mut game, Viewer::P1, Answer::Targets(vec![0])).kind,
        SubmissionKind::Applied,
        "the ability resolves"
    );
    let linked = kept.deliver(&mut game, &board);
    let plain = rebuilt.deliver(&mut game, &board);
    assert!(
        !promoted(&linked).is_empty() && plain.presentation.is_empty(),
        "no step is reported for a scope this client never held"
    );
    let (with_links, played) = committed(&linked);
    let (without_links, replayed) = committed(&plain);
    assert_eq!(
        replayed, played,
        "what the baseline board already contained is history, not something to play"
    );
    assert_eq!(
        without_links,
        with_links
            .into_iter()
            .map(|mut batch| {
                for event in &mut batch.events {
                    event.presentation_id = None;
                }
                batch
            })
            .collect::<Vec<_>>(),
        "the same batches arrive without a link to scopes the client does not know"
    );
}

#[test]
fn a_failed_projection_stops_one_viewers_events_for_good_without_stopping_the_game() {
    let mut game = prepared(false);
    let board = Board::default();
    let mut feeds = SEATS.map(EventFeed::new);
    for feed in &mut feeds {
        assert_eq!(
            feed.baseline(&mut game),
            quiet(),
            "both seats start available"
        );
    }
    ask_for_the_follower(&mut game);
    let [first, second] = &mut feeds;
    board.broken.set(true);
    assert_eq!(
        first.deliver(&mut game, &board),
        unavailable(),
        "a body that cannot be projected safely stops the events, with no partial delivery"
    );
    board.broken.set(false);
    assert!(
        !shown(&second.deliver(&mut game, &board)).is_empty(),
        "the other viewer's events are unaffected"
    );
    assert_eq!(
        first.deliver(&mut game, &board),
        unavailable(),
        "stopped events do not resume when lookups work again"
    );
    assert_eq!(
        first.baseline(&mut game),
        unavailable(),
        "a later baseline does not revive them either"
    );
    assert_eq!(
        answer(&mut game, Viewer::P1, Answer::Targets(vec![0])).kind,
        SubmissionKind::Applied,
        "the board and the question keep working"
    );
    assert!(
        !promoted(&second.deliver(&mut game, &board)).is_empty(),
        "the other viewer's shown scopes are still promoted"
    );
}

/// The committed batches of `update` that change a life total by `delta`.
fn life_changes(update: &FeedUpdate, delta: i64) -> Vec<BatchId> {
    let wanted = WireInt::try_from(delta).ok();
    committed(update)
        .0
        .into_iter()
        .filter(|batch| {
            batch.events.iter().any(|event| {
                matches!(&event.body, wire::EventBody::LifeChanged { change, .. }
                    if Some(change.delta) == wanted)
            })
        })
        .map(|batch| batch.id)
        .collect()
}

/// A parent ability gains 1 life and then plays a spell from the cemetery. At the spell's
/// target question the spell has not been played, so nothing can be shown early; the board
/// handed there already shows the life gain. Returns the game at that question.
fn child_is_declaring(feeds: &mut [EventFeed; 2]) -> Game {
    let mut game = started("unit-spell", true, Scenario::Plain);
    for feed in feeds.iter_mut() {
        assert_eq!(
            feed.baseline(&mut game),
            quiet(),
            "both seats observe the parent's target question first"
        );
    }
    assert_eq!(
        answer(&mut game, Viewer::P1, Answer::Targets(vec![0])).kind,
        SubmissionKind::Applied,
        "the parent is played, gains 1 life and starts playing the spell"
    );
    game
}

#[test]
fn a_result_already_on_a_handed_board_is_history_when_it_commits() {
    let board = Board::default();
    let mut feeds = SEATS.map(EventFeed::new);
    let mut game = child_is_declaring(&mut feeds);
    let [acting, rebuilt] = &mut feeds;
    // One seat is handed the stop by an ordinary delivery, the other by a new baseline.
    assert_eq!(
        acting.deliver(&mut game, &board),
        quiet(),
        "the spell can still be undone by rules, so nothing is shown and nothing committed"
    );
    assert_eq!(
        rebuilt.baseline(&mut game),
        quiet(),
        "a baseline at the same stop plays nothing either"
    );
    assert_eq!(
        answer(&mut game, Viewer::P1, Answer::Targets(vec![0])).kind,
        SubmissionKind::Applied,
        "the spell and then the parent resolve completely"
    );
    for feed in &mut feeds {
        let update = feed.deliver(&mut game, &board);
        assert!(
            update.presentation.is_empty(),
            "nothing was shown early, so there is nothing to promote"
        );
        let (batches, animate) = committed(&update);
        let first_gain = life_changes(&update, 1);
        let [old] = first_gain.as_slice() else {
            panic!("the parent's first life gain is one committed batch: {batches:?}");
        };
        assert!(
            !animate.contains(old),
            "the board handed at the stop already showed that gain; it is history only"
        );
        assert_eq!(
            animate,
            batches
                .iter()
                .map(|batch| batch.id.clone())
                .filter(|id| id != old)
                .collect::<Vec<_>>(),
            "everything else plays now, in order: both plays with their costs, which keep \
             their first presentation, and every result completed after the stop"
        );
        assert!(
            life_changes(&update, 2)
                .iter()
                .chain(&life_changes(&update, 10))
                .all(|later| animate.contains(later))
                && batches.iter().any(|batch| {
                    animate.contains(&batch.id)
                        && batch.events.iter().any(|event| {
                            matches!(event.body, wire::EventBody::AbilityPlayed { .. })
                        })
                }),
            "the later life gains and the parent's own play are among what plays"
        );
    }
}

#[test]
fn a_seat_that_was_not_handed_the_stop_plays_every_batch() {
    let board = Board::default();
    let mut feeds = SEATS.map(EventFeed::new);
    let mut game = child_is_declaring(&mut feeds);
    let [acting, waiting] = &mut feeds;
    assert_eq!(
        acting.deliver(&mut game, &board),
        quiet(),
        "only the acting seat is handed the spell's private target question"
    );
    assert_eq!(
        answer(&mut game, Viewer::P1, Answer::Targets(vec![0])).kind,
        SubmissionKind::Applied,
        "the spell and then the parent resolve completely"
    );
    let update = waiting.deliver(&mut game, &board);
    let (batches, animate) = committed(&update);
    assert_eq!(
        animate,
        batches
            .iter()
            .map(|batch| batch.id.clone())
            .collect::<Vec<_>>(),
        "a seat whose board never showed the first life gain sees all of it for the first time"
    );
    assert_eq!(
        life_changes(&update, 1).len(),
        1,
        "including the parent's first life gain"
    );
}
