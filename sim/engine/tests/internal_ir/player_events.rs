use super::player_pipeline::{answer, question};
use super::*;
use sve_battle_contract::event::{self as wire, EventBody, ResultReason};
use sve_battle_contract::observation::TurnView;
use sve_battle_contract::{Seat, SourceRef};
use sve_engine::battle::{EventObjects, ProjectedEvent, ProjectionError, project_batch};
use sve_engine::game::internal::SearchAnswer;
use sve_engine::game::player::DecisionShape;
use sve_engine::game::player::events::EventSubject;
use sve_engine::game::player::events::{
    ActionKind, EventCard, EventError, EventKind, MovementMethod, PlayerEvent,
};
use sve_engine::game::player::observation::{EffectSource, ObservedZone, TurnObservation};
use sve_engine::game::player::{Command, Input, QuestionShape, SubmissionKind, Viewer};

fn history(game: &Game, viewer: Viewer) -> Vec<PlayerEvent> {
    game.events(viewer, None).unwrap().cloned().collect()
}

/// For results that name no card, source or turn: the projection must not need any lookup.
struct NoLookups;

impl EventObjects for NoLookups {
    fn card(&self, _card: &EventCard) -> Result<wire::EventCard, ProjectionError> {
        Err(ProjectionError::Unavailable)
    }

    fn source(&self, _source: &EffectSource) -> Result<SourceRef, ProjectionError> {
        Err(ProjectionError::Unavailable)
    }

    fn turn(&self, _turn: &TurnObservation) -> Result<TurnView, ProjectionError> {
        Err(ProjectionError::Unavailable)
    }
}

/// Whether the wire says an effect instruction made this real draw; `None` when the result
/// is not projected as exactly one draw.
pub(super) fn drawn_by_effect(kind: &EventKind) -> Option<bool> {
    let projected = project_batch([kind], &NoLookups).ok()?;
    if let [
        ProjectedEvent {
            body:
                EventBody::Movement {
                    method: wire::MovementMethod::Draw,
                    by_effect,
                    ..
                },
            ..
        },
    ] = projected.as_slice()
    {
        Some(*by_effect)
    } else {
        None
    }
}

/// What the last committed batch of each viewer says on the wire, exactly as the producer
/// sealed it.
fn wire_ending(game: &Game) -> [Vec<EventBody>; 2] {
    [Viewer::P1, Viewer::P2].map(|viewer| {
        let batches = game
            .event_batches(viewer, None)
            .unwrap()
            .collect::<Vec<_>>();
        let last = batches.last().unwrap();
        project_batch(last.events.iter().map(|event| &event.kind), &NoLookups)
            .unwrap()
            .into_iter()
            .map(|event| event.body)
            .collect()
    })
}

const fn ended(winner: Option<Seat>, loser: Option<Seat>, reason: ResultReason) -> EventBody {
    EventBody::Result {
        winner,
        loser,
        reason,
        rule: None,
    }
}

/// A resolved effect that deals lethal damage to the leaders of `recipients`.
fn lethal(recipients: PlayerRelation) -> Game {
    let mut game = game(1, 1);
    game.advance_player_input();
    game.execute_internal(
        &checked(vec![damage(
            1,
            RecipientSet::Leaders {
                players: players(recipients),
            },
            20,
        )]),
        &source("synthetic"),
        context(1),
    )
    .unwrap();
    game.advance_player_input();
    game
}

#[test]
fn player_events_seal_a_shared_defeat_so_its_wire_results_name_no_winner() {
    let both = lethal(PlayerRelation::All);
    assert_eq!(
        both.input(Viewer::P1),
        Input::Finished { winner: None },
        "both leaders reached zero at the same check"
    );
    for ending in wire_ending(&both) {
        assert_eq!(
            ending,
            vec![
                ended(None, Some(Seat::P1), ResultReason::SimultaneousDefeat),
                ended(None, Some(Seat::P2), ResultReason::SimultaneousDefeat),
            ],
            "the two defeats are one sealed batch: nobody is named the winner"
        );
    }

    let one = lethal(PlayerRelation::Opponents);
    for ending in wire_ending(&one) {
        assert_eq!(
            ending,
            vec![ended(Some(Seat::P1), Some(Seat::P2), ResultReason::Defeat)],
            "a single defeat still makes the other player win"
        );
    }

    let mut conceded = game(1, 1);
    conceded.advance_player_input();
    assert_eq!(
        conceded.submit_player(Viewer::P2, Command::Concede).kind,
        SubmissionKind::Applied,
        "a seat may concede at any stop"
    );
    for ending in wire_ending(&conceded) {
        assert_eq!(
            ending,
            vec![ended(
                Some(Seat::P1),
                Some(Seat::P2),
                ResultReason::Concession
            )],
            "a concession keeps its own reason"
        );
    }
}

fn life(id: u64, amount: i64) -> Located<Node> {
    instruction(
        id,
        Action::Life {
            player: PlayerRef::Controller {},
            change: NumericChange::Add {
                amount: Expr::Literal(Int(amount)),
            },
        },
    )
}

pub(super) fn reveal_hand() -> WellFormedProgram {
    checked(vec![instruction(
        1,
        Action::Reveal {
            objects: Selector::Zone {
                players: PlayerSet {
                    cardinality: Cardinality {
                        minimum: Nat(1),
                        maximum: Some(Nat(1)),
                    },
                    ..players(PlayerRelation::SelfPlayer)
                },
                zone: ZoneKind::Hand,
                view: SelectionView::Current {},
                binding: Binding::new(90, ScopeId(1)),
                predicate: Box::new(Expr::Literal(Bool(true))),
            },
        },
    )])
}

#[test]
fn player_events_unknown_draws_ignore_private_prints_and_physical_ids() {
    let mut observed = Vec::new();
    for (id, number) in [
        ("private-left", "unit-follower"),
        ("private-right", "unit-amulet"),
    ] {
        let initial = setup(1, json!([{"id":id,"card":number}]));
        let mut game = Game::new(
            catalog(),
            &initial,
            &Value::Null,
            &Value::Null,
            "events-hidden-pair",
        )
        .unwrap();
        game.advance_player_input();
        game.execute_internal(
            &checked(vec![ordering::look(1, 1), draw(2, 1), life(3, 1)]),
            &source("synthetic"),
            context(1),
        )
        .unwrap();
        game.advance_player_input();
        let events = history(&game, Viewer::P2);
        assert_eq!(events.len(), 2);
        observed.push(format!("{events:?}"));
    }
    assert_eq!(
        observed[0], observed[1],
        "Every public field and local coordinate ignores the unknown identity"
    );
}

#[test]
fn player_events_unconditional_private_search_publishes_only_anonymous_move_and_shuffle() {
    let mut game = game(1, 3);
    game.advance_player_input();
    game.execute_internal(
        &checked(vec![search::search(
            Expr::Literal(Bool(true)),
            search::cardinality(1, 1),
            search::hand(),
        )]),
        &source("synthetic"),
        context(1),
    )
    .unwrap();
    game.advance_player_input();
    assert_eq!(history(&game, Viewer::P2), vec![]);
    assert_eq!(
        answer(
            &mut game,
            Viewer::P1,
            Answer::Search(SearchAnswer::Ungrouped(vec![0]))
        )
        .kind,
        SubmissionKind::Applied
    );
    let events = history(&game, Viewer::P2);
    assert_eq!(events.len(), 2);
    assert!(
        matches!(&events[0].kind, EventKind::Movement { before, after, .. } if before.card.is_none() && after.card.is_none())
    );
    assert!(
        matches!(&events[1].kind, EventKind::Action(action) if action.kind == ActionKind::Shuffle)
    );
}

#[test]
fn player_events_late_revelation_reads_keep_the_original_authorization_and_retire_identity() {
    let mut game = game(1, 1);
    game.advance_player_input();
    game.execute_internal(&reveal_hand(), &source("synthetic"), context(1))
        .unwrap();
    game.advance_player_input();
    let first = history(&game, Viewer::P2);
    let EventKind::Action(action) = &first[0].kind else {
        panic!("reveal");
    };
    let Some(EventSubject::Card(card)) = &action.subjects[0] else {
        panic!("public face");
    };
    let old = card.handle.clone().unwrap();
    assert_eq!(
        game.observe(Viewer::P2).unwrap().players[0]
            .zones
            .iter()
            .find(|zone| zone.kind == ObservedZone::Hand)
            .unwrap()
            .cards,
        vec![]
    );
    game.execute_internal(&reveal_hand(), &source("synthetic"), context(1))
        .unwrap();
    game.advance_player_input();
    let second = game
        .events(Viewer::P2, Some(&first[0].id))
        .unwrap()
        .collect::<Vec<_>>();
    assert_eq!(
        second.len(),
        1,
        "Equal-looking distinct occurrences must not be deduplicated"
    );
    let EventKind::Action(action) = &second[0].kind else {
        panic!("reveal");
    };
    let Some(EventSubject::Card(card)) = &action.subjects[0] else {
        panic!("public face");
    };
    assert_ne!(card.handle.as_ref(), Some(&old));
    assert_eq!(
        history(&game, Viewer::P2)[0],
        first[0],
        "A later identity period cannot rewrite the authorized historical snapshot"
    );
}

#[test]
fn player_events_projection_gap_is_fixed_and_does_not_stop_the_other_viewer_or_input() {
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["hand"][0]["state"] = json!({"counters":{"unreadable":-1}});
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "events-gap",
    )
    .unwrap();
    game.advance_player_input();
    game.execute_internal(&reveal_hand(), &source("synthetic"), context(1))
        .unwrap();
    game.advance_player_input();
    assert!(matches!(
        game.events(Viewer::P1, None),
        Err(EventError::ProjectionGap)
    ));
    assert_eq!(history(&game, Viewer::P2).len(), 1);
    assert!(matches!(game.input(Viewer::P1), Input::Question(_)));
    assert_eq!(
        game.submit_player(Viewer::P2, Command::Concede).kind,
        SubmissionKind::Applied
    );
    assert!(matches!(
        game.events(Viewer::P1, None),
        Err(EventError::ProjectionGap)
    ));
    assert_eq!(history(&game, Viewer::P2).len(), 2);
}

#[test]
fn player_events_unite_step_moves_and_typed_life_without_duplicate_draws() {
    let mut game = game(1, 3);
    game.advance_player_input();
    assert_eq!(history(&game, Viewer::P2), vec![]);
    game.execute_internal(
        &checked(vec![life(1, 3), draw(2, 2)]),
        &source("synthetic"),
        context(1),
    )
    .unwrap();
    game.advance_player_input();
    let owner = history(&game, Viewer::P1);
    let opponent = history(&game, Viewer::P2);
    assert_eq!(
        owner.len(),
        3,
        "One life change and two draws, with no Step/Fact duplicate"
    );
    assert_eq!(opponent.len(), owner.len());
    assert!(
        matches!(&owner[0].kind, EventKind::Action(event) if event.kind == ActionKind::Life && event.numbers[0].amount == 3)
    );
    for event in &opponent[1..] {
        assert!(
            matches!(&event.kind, EventKind::Movement { method: MovementMethod::Effect(MoveMethod::Draw), before, after }
            if before.card.is_none() && after.card.is_none() && after.position.is_none()),
            "an instruction's draw records that an effect drew"
        );
        assert_eq!(
            drawn_by_effect(&event.kind),
            Some(true),
            "the wire says an effect instruction drew"
        );
    }
    for event in &owner[1..] {
        assert!(matches!(&event.kind, EventKind::Movement { after, .. } if after.card.is_some()));
    }
    let cursor = owner.last().unwrap().id.clone();
    assert_eq!(game.events(Viewer::P1, Some(&cursor)).unwrap().count(), 0);
    assert!(matches!(
        game.events(Viewer::P2, Some(&cursor)),
        Err(EventError::InvalidCursor)
    ));
    let mut other = super::game(1, 3);
    other.advance_player_input();
    assert!(
        matches!(
            other.events(Viewer::P1, Some(&cursor)),
            Err(EventError::InvalidCursor)
        ),
        "Even an identical seed creates another history"
    );
    for _ in 0..3 {
        game.advance_player_input();
        assert_eq!(
            history(&game, Viewer::P1),
            owner,
            "Reads and automatic checkpoints cannot re-emit accumulated facts"
        );
    }
}

#[test]
fn player_events_publish_a_multi_question_origin_only_when_it_commits() {
    let mut game = game(1, 3);
    game.advance_player_input();
    game.execute_internal(
        &checked(vec![life(1, 1), up_to(2, 2)]),
        &source("synthetic"),
        context(1),
    )
    .unwrap();
    game.advance_player_input();
    assert_eq!(history(&game, Viewer::P1), vec![]);
    let first = question(&game, Viewer::P1);
    assert_eq!(
        game.submit_player(
            Viewer::P2,
            Command::Answer {
                token: first.token,
                answer: Answer::Stepwise(StepwiseAnswer::Continue)
            }
        )
        .kind,
        SubmissionKind::Rejected
    );
    assert_eq!(
        answer(
            &mut game,
            Viewer::P1,
            Answer::Stepwise(StepwiseAnswer::Continue)
        )
        .kind,
        SubmissionKind::Applied
    );
    for viewer in [Viewer::P1, Viewer::P2] {
        assert!(
            history(&game, viewer).is_empty(),
            "The draw is visible in observe but is still inside a rollback origin"
        );
    }
    assert_eq!(
        game.observe(Viewer::P1).unwrap().players[0]
            .zones
            .iter()
            .find(|zone| zone.kind == ObservedZone::Hand)
            .unwrap()
            .count,
        2
    );
    assert_eq!(
        answer(
            &mut game,
            Viewer::P1,
            Answer::Stepwise(StepwiseAnswer::Stop)
        )
        .kind,
        SubmissionKind::Applied
    );
    assert_eq!(history(&game, Viewer::P1).len(), 2);
    assert_eq!(history(&game, Viewer::P2).len(), 2);
}

#[test]
fn player_events_never_present_a_procedure_that_is_not_a_play() {
    let mut game = game(1, 3);
    game.advance_player_input();
    game.execute_internal(
        &checked(vec![life(1, 1), draw(2, 1), up_to(3, 1)]),
        &source("synthetic"),
        context(1),
    )
    .unwrap();
    game.advance_player_input();
    assert!(
        matches!(game.input(Viewer::P1), Input::Question(_)),
        "The optional draw is a real stop after completed results"
    );
    for viewer in [Viewer::P1, Viewer::P2] {
        game.present_player_events(viewer).unwrap();
        assert_eq!(
            game.presentation(viewer, None).unwrap().count(),
            0,
            "Completed results of a procedure without a played step stay inside its origin"
        );
    }
    assert_eq!(
        answer(
            &mut game,
            Viewer::P1,
            Answer::Stepwise(StepwiseAnswer::Stop)
        )
        .kind,
        SubmissionKind::Applied,
        "Declining the optional draw completes the procedure"
    );
    for viewer in [Viewer::P1, Viewer::P2] {
        assert_eq!(
            history(&game, viewer).len(),
            2,
            "The life change and the draw commit as ordinary history"
        );
        assert_eq!(
            game.presentation(viewer, None).unwrap().count(),
            0,
            "Committing never creates a scope after the fact"
        );
        assert!(
            game.event_batches(viewer, None)
                .unwrap()
                .all(|batch| batch.covered),
            "Both seats were handed the board at the stop, so neither result is new to them"
        );
    }
}

#[test]
fn player_events_fault_discards_the_whole_uncommitted_origin_and_keeps_the_prefix() {
    let mut game = game(1, 3);
    game.advance_player_input();
    game.execute_internal(&checked(vec![life(1, 1)]), &source("synthetic"), context(1))
        .unwrap();
    game.advance_player_input();
    let prefix = history(&game, Viewer::P1);
    game.execute_internal(
        &checked(vec![draw(1, 1), up_to(2, 1), draw(3, 10_001)]),
        &source("synthetic"),
        context(1),
    )
    .unwrap();
    game.advance_player_input();
    assert_eq!(history(&game, Viewer::P1), prefix);
    assert_eq!(
        answer(
            &mut game,
            Viewer::P1,
            Answer::Stepwise(StepwiseAnswer::Stop)
        )
        .kind,
        SubmissionKind::EngineFault
    );
    assert_eq!(history(&game, Viewer::P1), prefix);
    assert_eq!(history(&game, Viewer::P2).len(), prefix.len());
    assert_eq!(
        game.submit_player(Viewer::P2, Command::Concede).kind,
        SubmissionKind::Applied
    );
    let ended = history(&game, Viewer::P1);
    assert_eq!(&ended[..prefix.len()], prefix);
    assert!(matches!(
        ended.last().unwrap().kind,
        EventKind::Result {
            loser: Some(Viewer::P2),
            ..
        }
    ));
    assert_eq!(
        game.events(Viewer::P1, Some(&prefix.last().unwrap().id))
            .unwrap()
            .count(),
        1
    );
}

#[test]
fn player_events_private_look_and_order_do_not_change_the_opponents_sequence() {
    let mut initial = game(1, 4);
    initial.advance_player_input();
    let mut variants = Vec::new();
    for reverse in [false, true] {
        let mut branch = initial.clone();
        branch
            .execute_internal(
                &checked(vec![
                    ordering::look(1, 3),
                    ordering::order(2, ordering::looked(1), Position::Bottom {}),
                ]),
                &source("synthetic"),
                context(1),
            )
            .unwrap();
        branch.advance_player_input();
        let QuestionShape::Decision(DecisionShape::Order(cards)) =
            question(&branch, Viewer::P1).shape
        else {
            panic!("order");
        };
        let mut handles = cards.iter().map(|card| card.handle).collect::<Vec<_>>();
        if reverse {
            handles.reverse();
        }
        assert_eq!(
            answer(&mut branch, Viewer::P1, Answer::Order(handles)).kind,
            SubmissionKind::Applied
        );
        assert_eq!(history(&branch, Viewer::P2), vec![]);
        branch
            .execute_internal(&checked(vec![draw(1, 1)]), &source("synthetic"), context(1))
            .unwrap();
        branch.advance_player_input();
        variants.push(history(&branch, Viewer::P2));
    }
    assert_eq!(
        variants[0], variants[1],
        "Private order, card identity and input count cannot alter public cursors or causes"
    );
}

#[test]
fn player_events_simultaneous_reveal_does_not_publish_the_opponents_hand_order() {
    let mut histories = Vec::new();
    for reversed in [false, true] {
        let mut initial = setup(1, json!([]));
        let mut hand = vec![
            json!({"id":"s","card":"unit-spell"}),
            json!({"id":"private-other","card":"unit-follower"}),
        ];
        if reversed {
            hand.reverse();
        }
        initial["players"]["P1"]["zones"]["hand"] = json!(hand);
        let mut game = Game::new(
            catalog(),
            &initial,
            &Value::Null,
            &Value::Null,
            "events-hand-pair",
        )
        .unwrap();
        game.advance_player_input();
        game.execute_internal(&reveal_hand(), &source("synthetic"), context(1))
            .unwrap();
        game.advance_player_input();
        histories.push(format!("{:?}", history(&game, Viewer::P2)));
    }
    assert_eq!(
        histories[0], histories[1],
        "Simultaneous disclosure must not serialize private hand slots"
    );
}
