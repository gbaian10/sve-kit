use super::player_pipeline::{answer, end_command, question};
use super::*;
use sve_engine::game::player::DecisionShape;
use sve_engine::game::player::events::{ActionKind, EventKind, MovementMethod};
use sve_engine::game::player::{
    Command, Input, QuestionPurpose, QuestionShape, SubmissionKind, Viewer,
};

fn board(next_deck: bool) -> Game {
    let mut initial = setup(1, json!([]));
    initial["players"]["P2"]["zones"]["deck"] = if next_deck {
        json!([{"id":"next-0","card":"unit-follower"},{"id":"next-1","card":"unit-follower"}])
    } else {
        json!([])
    };
    initial["players"]["P2"]["zones"]["field"][0]["state"] = json!({"acted":true});
    initial["players"]["P2"]["pp"] = json!({"current":0,"max":2});
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "player-closing",
    )
    .unwrap();
    let empty = WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![],
    })
    .unwrap();
    for number in ["unit-follower", "unit-spell"] {
        game.load_internal_card(number, &empty).unwrap();
    }
    game.advance_player_input();
    game
}

fn end(game: &mut Game) -> Command {
    let current = question(game, Viewer::P1);
    let token = current.token;
    let command = end_command(&current);
    assert_eq!(
        game.submit_player(Viewer::P2, command.clone()).kind,
        SubmissionKind::Rejected,
        "Only the active seat can end its main phase"
    );
    assert_eq!(
        question(game, Viewer::P1).token,
        token,
        "Wrong-seat submission preserves the question"
    );
    assert_eq!(
        game.submit_player(Viewer::P1, command.clone()).kind,
        SubmissionKind::Applied,
        "The active seat can begin the end phase"
    );
    command
}

#[test]
fn player_closing_expires_turn_effects_refreshes_and_draws_before_main_input() {
    let mut game = board(true);
    game.execute_internal(
        &checked(vec![numeric::modify(
            1,
            zone(ZoneKind::Field, 10),
            numeric::pair(2, 2),
            DurationEnd::CurrentTurnEnd {},
        )]),
        &source("synthetic"),
        context(1),
    )
    .unwrap();
    game.advance_player_input();
    assert_eq!(
        game.query(View::Referee, "P2.field.f.power").unwrap(),
        Some(json!(4))
    );
    let old = end(&mut game);
    assert!(matches!(
        question(&game, Viewer::P2).shape,
        QuestionShape::Decision(DecisionShape::Quick { .. })
    ));
    let next = answer(&mut game, Viewer::P2, Answer::Quick(None));
    assert_eq!(next.kind, SubmissionKind::Applied);
    assert!(matches!(
        question(&game, Viewer::P2).shape,
        QuestionShape::ActionWindow(_)
    ));
    let referee = game.projection(View::Referee).unwrap();
    assert_eq!(referee["turn"]["active"], "P2");
    assert_eq!(referee["turn"]["phase"], "main");
    assert_eq!(referee["P2"]["pp"], json!({"current":3,"max":3}));
    assert_eq!(referee["objects"]["f"]["acted"], false);
    assert_eq!(
        game.query(View::Referee, "P2.field.f.power").unwrap(),
        Some(json!(2))
    );
    assert_eq!(referee["P2"]["hand"], json!(["next-0"]));
    assert_eq!(referee["P2"]["deck"], json!(["next-1"]));
    let events = game.events(Viewer::P1, None).unwrap().collect::<Vec<_>>();
    let phases = events
        .iter()
        .filter_map(|event| match &event.kind {
            EventKind::Phase(turn) => turn.phase,
            EventKind::Movement { .. } | EventKind::Action(_) | EventKind::Result { .. } => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(phases, vec![Phase::End, Phase::Start, Phase::Main]);
    let stand = events.iter().position(|event| matches!(&event.kind, EventKind::Action(action) if action.kind == ActionKind::Stand)).unwrap();
    let draw = events
        .iter()
        .position(|event| {
            matches!(
                &event.kind,
                EventKind::Movement {
                    method: MovementMethod::Draw,
                    ..
                }
            )
        })
        .unwrap();
    assert!(
        stand < draw && draw < events.len() - 1,
        "Start phase stands and draws before Main begins"
    );

    assert_eq!(
        game.submit_player(Viewer::P2, old).kind,
        SubmissionKind::Rejected
    );
}

#[test]
fn player_closing_empty_next_deck_finishes_without_a_main_question() {
    let mut game = board(false);
    let old = end(&mut game);
    let terminal = answer(&mut game, Viewer::P2, Answer::Quick(None));
    assert_eq!(terminal.kind, SubmissionKind::Applied);
    assert_eq!(
        terminal.input,
        Input::Finished {
            winner: Some(Viewer::P1)
        }
    );
    assert!(game.internal_decision(View::Referee).is_none());
    assert_eq!(
        game.submit_player(Viewer::P1, old).kind,
        SubmissionKind::Rejected
    );
}

#[test]
fn player_closing_leader_defeats_are_checked_before_any_action_question() {
    for both in [false, true] {
        let mut initial = setup(1, json!([]));
        initial["players"]["P1"]["leader"]["life"] = json!(0);
        if both {
            initial["players"]["P2"]["leader"]["life"] = json!(0);
        }
        let mut game = Game::new(
            catalog(),
            &initial,
            &Value::Null,
            &Value::Null,
            "closing-defeat",
        )
        .unwrap();
        game.advance_player_input();
        for viewer in [Viewer::P1, Viewer::P2] {
            assert_eq!(
                game.input(viewer),
                Input::Finished {
                    winner: if both { None } else { Some(Viewer::P2) }
                }
            );
        }
        assert!(game.internal_decision(View::Referee).is_none());
        let losses = game
            .events(Viewer::P1, None)
            .unwrap()
            .filter(|event| matches!(event.kind, EventKind::Result { loser: Some(_), .. }))
            .count();
        assert_eq!(losses, if both { 2 } else { 1 });
        assert_eq!(
            game.submit_player(Viewer::P1, Command::Concede).kind,
            SubmissionKind::Rejected
        );
    }
}

#[test]
fn player_closing_attack_quick_returns_to_the_existing_main_phase() {
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"attacker","card":"unit-follower"}]);
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "closing-attack",
    )
    .unwrap();
    let empty = WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![],
    })
    .unwrap();
    for number in ["unit-follower", "unit-spell"] {
        game.load_internal_card(number, &empty).unwrap();
    }
    game.internal_rule_action(&json!({"do":"attack","attacker":"attacker","target":"P2.leader"}))
        .unwrap();
    game.advance_player_input();
    assert_eq!(
        question(&game, Viewer::P2).purpose,
        QuestionPurpose::QuickWindow
    );
    assert_eq!(
        answer(&mut game, Viewer::P2, Answer::Quick(None)).kind,
        SubmissionKind::Applied
    );
    assert!(matches!(
        question(&game, Viewer::P1).shape,
        QuestionShape::ActionWindow(_)
    ));
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
        18
    );
}

#[test]
fn player_closing_effect_termination_keeps_the_prefix_and_never_asks_for_the_suffix() {
    let mut game = game(1, 4);
    let win = instruction(
        2,
        Action::GameResult {
            players: PlayerSet {
                relation: PlayerRelation::SelfPlayer,
                relative_to: PlayerRef::Controller {},
                cardinality: Cardinality {
                    minimum: Nat(1),
                    maximum: Some(Nat(1)),
                },
                text_quantifier: TextQuantifier::Implicit,
            },
            result: Outcome::Win,
            source: AbilitySource {
                holder: ObjectOperand::SourceHolder {},
                origin: AbilityOrigin::Printed {
                    face: FaceId("0".into()),
                    line: Nat(1),
                },
                controller: PlayerRef::Controller {},
            },
        },
    );
    game.execute_internal(
        &checked(vec![up_to(1, 1), win, draw(3, 1)]),
        &source("synthetic"),
        context(1),
    )
    .unwrap();
    game.advance_player_input();
    let before = question(&game, Viewer::P1);
    let result = answer(
        &mut game,
        Viewer::P1,
        Answer::Stepwise(StepwiseAnswer::Stop),
    );
    assert_eq!(result.kind, SubmissionKind::Applied);
    assert_eq!(
        result.input,
        Input::Finished {
            winner: Some(Viewer::P1)
        }
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["deck"]
            .as_array()
            .unwrap()
            .len(),
        4
    );
    assert!(game.internal_decision(View::Referee).is_none());
    assert_eq!(
        game.submit_player(
            Viewer::P1,
            Command::Answer {
                token: before.token,
                answer: Answer::Stepwise(StepwiseAnswer::Continue)
            }
        )
        .kind,
        SubmissionKind::Rejected
    );
}
