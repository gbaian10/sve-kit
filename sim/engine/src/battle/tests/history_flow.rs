//! Real history producers stay test-only while the host observes through Game.

use super::board_fixtures::exact_authority;
use super::{board_fixtures, history_fixture};
use crate::battle::history::project_history;
use crate::game::internal::{
    Activation, Answer, CardPlay, DecisionView, Fact, InputDomain, ResumeStage,
};
use crate::game::player::{Viewer, observation as native};
use crate::game::{Game, View};
use crate::ir::{
    AbilityId, Action, DieFace, Generation, Located, MoveDestination, MoveReason, MoveSpec, Nat,
    Node, ObjectOperand, PlayerId, PlayerRef, Posture, Selector, WellFormedProgram, ZoneKind,
    ZoneRef,
};
use crate::{EngineFailure, Result};
use history_fixture::{card, context, instruction, repeated, reroll, source};
use native::{CardState, HistoryEvent, ObservedZone, SubjectHistory};
use serde_json::{Value, json};
use sve_battle_contract::{Nat as WireNat, observation as wire};

fn checked(steps: Vec<Located<Node>>) -> Result<WellFormedProgram> {
    WellFormedProgram::check(history_fixture::body(1, steps))
        .map_err(|error| EngineFailure::Invalid(error.to_string()))
}
fn waiting(play: CardPlay) -> Option<DecisionView> {
    if let CardPlay::Awaiting(question) = play {
        Some(question)
    } else {
        None
    }
}
fn resolved(play: CardPlay) -> Option<Activation> {
    if let CardPlay::Resolved(activation) = play {
        Some(activation)
    } else {
        None
    }
}
fn answer(game: &mut Game, question: &DecisionView, value: u64) -> Result<CardPlay> {
    game.answer_internal_card(question.actor, question.token, Answer::Integer(Nat(value)))
}
fn field(game: &Game, seat: Viewer, position: usize) -> Option<CardState> {
    game.observe(seat)
        .ok()?
        .players
        .iter()
        .find(|player| player.player == seat)?
        .zones
        .iter()
        .find(|zone| zone.kind == ObservedZone::Field)?
        .cards
        .get(position)?
        .card
        .properties
        .state
        .clone()
}
fn pass(game: &mut Game) -> Option<()> {
    let question = game.internal_quick_choice().ok()??;
    assert!(
        matches!(
            game.answer_internal_card(question.actor, question.token, Answer::Quick(None))
                .ok()?,
            CardPlay::Continued(_)
        ),
        "the real Quick pass advances its rule"
    );
    Some(())
}

fn assert_wire_attack_links(game: &Game, current: bool) -> Option<()> {
    let before = exact_authority(game);
    for viewer in [Viewer::P1, Viewer::P2] {
        let history = project_history(&game.observe(viewer).ok()?).ok()?;
        let attacks: Vec<_> = history
            .subjects
            .iter()
            .filter(|fact| fact.event == wire::HistoryEvent::Attack)
            .collect();
        assert!(
            !attacks.is_empty(),
            "Committed attacks remain in public history"
        );
        assert!(
            attacks.iter().all(|fact| fact.current.is_some() == current),
            "Retired attack links must not reconnect to a later generation"
        );
    }
    assert_eq!(exact_authority(game), before, "Wire history is read-only");
    Some(())
}
fn assert_wire_counts(game: &Game, slot: usize, played: u64, union_bursts: u64) -> Option<()> {
    let mut expected_plays = [0, 0];
    let mut expected_bursts = [0, 0];
    *expected_plays.get_mut(slot)? = played;
    *expected_bursts.get_mut(slot)? = union_bursts;
    for viewer in [Viewer::P1, Viewer::P2] {
        let history = project_history(&game.observe(viewer).ok()?).ok()?;
        assert_eq!(
            history.played.map(WireNat::get),
            expected_plays,
            "Committed play counts keep seat order"
        );
        assert_eq!(
            history.union_bursts.map(WireNat::get),
            expected_bursts,
            "Committed Union Burst counts keep seat order"
        );
    }
    Some(())
}
#[test]
fn rerolls_replace_the_result_and_fixed_repeats_reset_the_permission() {
    let program = card(vec![repeated().unwrap(), reroll().unwrap()]).unwrap();
    let mut game = history_fixture::position_with_state(1, &program, &Value::Null).unwrap();
    let q1 = waiting(
        game.activate_internal_card("s", AbilityId(1), PlayerId { seat: 1 })
            .unwrap(),
    )
    .unwrap();
    assert!(matches!(
        q1.domain,
        InputDomain::DiceReroll {
            remaining: Nat(1),
            ..
        }
    ));
    assert_eq!(
        game.internal_decision(View::P2),
        Some(q1.clone()),
        "the visible die result is public"
    );
    assert_eq!(
        project_history(&game.observe(Viewer::P1).unwrap())
            .unwrap()
            .dice,
        Vec::<wire::DiceHistory>::new()
    );
    game.answer_internal_card(PlayerId { seat: 2 }, q1.token, Answer::Integer(Nat(1)))
        .unwrap_err();
    assert_eq!(game.internal_decision(View::P1), Some(q1.clone()));
    let q2 = waiting(answer(&mut game, &q1, 1).unwrap()).unwrap();
    assert!(matches!(
        q2.domain,
        InputDomain::DiceReroll {
            remaining: Nat(1),
            ..
        }
    ));
    game.answer_internal_card(q1.actor, q1.token, Answer::Integer(Nat(0)))
        .unwrap_err();
    assert_eq!(game.internal_decision(View::P1), Some(q2.clone()));
    let done = resolved(answer(&mut game, &q2, 0).unwrap()).unwrap();
    let dice = done
        .progress
        .facts
        .iter()
        .filter_map(|fact| {
            if let Fact::Dice(event) = fact {
                Some(event)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(dice.len(), 2, "two rolls rather than three attempts");
    assert_eq!(dice[0].attempts.len(), 2);
    assert_eq!(dice[0].accepted_attempt, Nat(1));
    assert_eq!(dice[0].attempts.last(), Some(&dice[0].value));
    assert_eq!(dice[1].attempts.len(), 1);
    assert_ne!(dice[0].roll, dice[1].roll);
    assert_eq!(dice[0].attempts, vec![DieFace::One, DieFace::Two]);
    assert_eq!(dice[0].value, DieFace::Two);
    assert_eq!(dice[1].value, DieFace::Five);
    let history = game.observe(Viewer::P1).unwrap().history;
    assert_eq!(
        history
            .dice
            .iter()
            .map(|roll| roll.value)
            .collect::<Vec<_>>(),
        [2, 5]
    );
    assert_eq!(history, game.observe(Viewer::P2).unwrap().history);
    for viewer in [Viewer::P1, Viewer::P2] {
        let projected = project_history(&game.observe(viewer).unwrap()).unwrap();
        assert_eq!(
            projected
                .dice
                .iter()
                .map(|roll| roll.value.get())
                .collect::<Vec<_>>(),
            [2, 5]
        );
    }
}
#[test]
fn public_history_is_read_only_and_does_not_depend_on_hidden_cards() {
    let mut game = history_fixture::public_history_board("unit-spell").unwrap();
    let mut other = history_fixture::public_history_board("unit-amulet").unwrap();
    let attack = json!({"do":"attack","attacker":"s","target":"P2.leader"});
    game.internal_rule_action(&attack).unwrap();
    other.internal_rule_action(&attack).unwrap();
    for board in [&mut game, &mut other] {
        while let Some(question) = board.internal_quick_choice().unwrap() {
            board
                .answer_internal_card(question.actor, question.token, Answer::Quick(None))
                .unwrap();
        }
        while board.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            board.resume_internal_card().unwrap();
        }
    }
    let before = exact_authority(&game);
    let observation = game.observe(Viewer::P1).unwrap();
    assert_eq!(observation, other.observe(Viewer::P1).unwrap());
    assert_eq!(observation, game.observe(Viewer::P1).unwrap());
    assert_eq!(before, exact_authority(&game));
    let attack_fact = observation
        .history
        .subjects
        .iter()
        .find(|fact| fact.event == HistoryEvent::Attack)
        .unwrap();
    assert!(attack_fact.current.is_some());
    assert_eq!(attack_fact.controller, Viewer::P1);
    assert_wire_attack_links(&game, true).unwrap();
    for viewer in [Viewer::P1, Viewer::P2] {
        assert_eq!(
            project_history(&game.observe(viewer).unwrap()).unwrap(),
            project_history(&other.observe(viewer).unwrap()).unwrap()
        );
    }
    let banish = checked(vec![instruction(
        1,
        Action::Banish {
            objects: Selector::Objects {
                objects: vec![ObjectOperand::SourceHolder {}],
            },
        },
    )])
    .unwrap();
    game.execute_internal(&banish, &source("synthetic"), context(1))
        .unwrap();
    assert_wire_attack_links(&game, false).unwrap();
    assert!(
        game.observe(Viewer::P1)
            .unwrap()
            .history
            .subjects
            .iter()
            .filter(|fact| fact.event == HistoryEvent::Attack)
            .all(|fact| fact.current.is_none())
    );
    let restore = checked(vec![instruction(
        2,
        Action::Move {
            objects: Selector::Objects {
                objects: vec![ObjectOperand::SourceHolder {}],
            },
            movement: MoveSpec {
                to: MoveDestination::Shared {
                    zone: ZoneRef {
                        player: PlayerRef::Controller {},
                        kind: ZoneKind::Field,
                    },
                },
                position: None,
                order: None,
                initial_posture: None,
                entry_effects: vec![],
            },
            reason: MoveReason::Put,
        },
    )])
    .unwrap();
    let mut returned = context(1);
    returned.generation = Generation(1);
    game.execute_internal(&restore, &source("synthetic"), returned)
        .unwrap();
    let after = game.observe(Viewer::P1).unwrap();
    assert_wire_attack_links(&game, false).unwrap();
    assert!(
        after
            .history
            .subjects
            .iter()
            .filter(|fact| fact.event == HistoryEvent::Attack)
            .all(|fact| fact.current.is_none())
    );
    assert_eq!(
        field(&game, Viewer::P1, 0).unwrap().posture,
        Posture::Standing
    );
}
#[test]
fn an_extra_turn_resets_committed_and_initial_history_even_for_the_same_active_player() {
    for (seat, active, other) in [(1, "P1", "P2"), (2, "P2", "P1")] {
        let mut initial = board_fixtures::setup(seat, json!([]));
        initial["turn"]["active"] = json!(active);
        initial["players"][active]["pp"] = json!({"current":10_i32,"max":10_i32});
        initial["players"][active]["zones"]["hand"] = json!([
            {"id":"first","card":"unit-spell"},{"id":"second","card":"unit-spell"}
        ]);
        initial["players"][active]["zones"]["deck"] = json!([
            {"id":"extra-draw","card":"unit-spell"},{"id":"later-draw","card":"unit-spell"}
        ]);
        initial["players"][other]["zones"]["deck"] =
            json!([{"id":"opposing-draw","card":"unit-spell"}]);
        initial["semantic_state"]["turn_schedule"] =
            json!({"extra":[{"player":active,"count":1_i32}],"skipped":[]});
        initial["semantic_state"]["counters_this_turn"][active] =
            json!({"cards_played":3_i32,"ub_activated":2_i32});
        let mut game = Game::new(
            board_fixtures::catalog().unwrap(),
            &initial,
            &Value::Null,
            &Value::Null,
            "extra-history",
        )
        .unwrap();
        game.load_internal_card(
            "unit-spell",
            &card(vec![history_fixture::spell(vec![]).unwrap()]).unwrap(),
        )
        .unwrap();
        game.load_internal_card("unit-follower", &card(vec![]).unwrap())
            .unwrap();
        let slot = usize::try_from(seat.checked_sub(1).unwrap()).unwrap();
        assert_eq!(game.observe(Viewer::P1).unwrap().history.played[slot], 3);
        assert_eq!(
            game.observe(Viewer::P1).unwrap().history.union_bursts[slot],
            2
        );
        assert_wire_counts(&game, slot, 3, 2).unwrap();
        resolved(game.play_internal_card("first", PlayerId { seat }).unwrap()).unwrap();
        game.resume_internal_card().unwrap();
        let before = game.observe(Viewer::P1).unwrap();
        assert_eq!(before.history.played[slot], 4);
        assert_eq!(before.history.subjects.len(), 1);
        assert_wire_counts(&game, slot, 4, 2).unwrap();
        game.internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap();
        pass(&mut game).unwrap();
        let extra = game.observe(Viewer::P1).unwrap();
        assert_eq!(
            game.projection(View::Referee).unwrap()["turn"]["active"],
            active
        );
        assert_eq!(
            extra.turn.elapsed_turns[slot],
            before.turn.elapsed_turns[slot] + 1
        );
        assert_eq!(extra.history.played, [0, 0]);
        assert_eq!(extra.history.union_bursts, [0, 0]);
        assert_eq!(extra.history.subjects, Vec::<SubjectHistory>::new());
        assert_wire_counts(&game, slot, 0, 0).unwrap();
        resolved(
            game.play_internal_card("second", PlayerId { seat })
                .unwrap(),
        )
        .unwrap();
        game.resume_internal_card().unwrap();
        assert_eq!(game.observe(Viewer::P1).unwrap().history.played[slot], 1);
        assert_wire_counts(&game, slot, 1, 0).unwrap();
        game.internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap();
        pass(&mut game).unwrap();
        assert_eq!(
            game.projection(View::Referee).unwrap()["turn"]["active"],
            other
        );
        assert_eq!(game.observe(Viewer::P1).unwrap().history.played, [0, 0]);
        assert_eq!(
            game.observe(Viewer::P1).unwrap().history.subjects,
            Vec::<SubjectHistory>::new()
        );
    }
}
