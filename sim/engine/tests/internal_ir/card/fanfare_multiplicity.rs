use super::*;
use crate::player_pipeline::{answer, end_command, question};
use sve_engine::game::player::{
    Command, DecisionShape, Input, MainActionAnswer, MainIntentKind, QuestionShape, SubmissionKind,
    Viewer,
};

fn play(game: &mut Game, actor: Viewer, number: &str) {
    let question = question(game, actor);
    let QuestionShape::ActionWindow(actions) = question.shape else {
        panic!("main play window");
    };
    let intent = actions.intents.iter().find(|intent| {
        matches!(&intent.kind, MainIntentKind::Play { card } if card.properties.card == number)
    }).unwrap();
    assert_eq!(
        game.submit_player(
            actor,
            Command::MainAction {
                token: question.token,
                intent: intent.id.clone(),
                answer: MainActionAnswer::None,
            }
        )
        .kind,
        SubmissionKind::Applied,
        "playing {number} must enter the field"
    );
    while let Input::Question(pending) = game.input(actor) {
        let QuestionShape::Decision(DecisionShape::Pending(choices)) = pending.shape else {
            break;
        };
        assert_eq!(
            answer(game, actor, Answer::Pending(choices[0].handle)).kind,
            SubmissionKind::Applied,
            "each Fanfare occurrence is selected and resolved independently"
        );
    }
}

fn end_turn(game: &mut Game, actor: Viewer, opponent: Viewer) {
    let command = end_command(&question(game, actor));
    assert_eq!(
        game.submit_player(actor, command).kind,
        SubmissionKind::Applied,
        "the current player can enter end phase"
    );
    if let Input::Question(current) = game.input(opponent)
        && matches!(
            current.shape,
            QuestionShape::Decision(DecisionShape::Quick { .. })
        )
    {
        assert_eq!(
            answer(game, opponent, Answer::Quick(None)).kind,
            SubmissionKind::Applied,
            "the opponent can pass the Quick window"
        );
    }
}

fn modifier_step() -> Value {
    json!({"player_effect":{
        "who":"self",
        "effect":{"fanfare_triggers":2},
        "until":{"during_next_turn":{"who":"self"}}
    }})
}

fn board(abilities: &[(&str, Value)], extra_turn: bool) -> Game {
    let mut input = evolution_linked_movement::tree(abilities);
    let modifiers = evolution_linked_movement::tree(&[(
        "fanfare",
        json!({"do":[
            modifier_step(),
            {"move":{"ref":"this","to":"cemetery"}}
        ]}),
    )]);
    input["cards"]["unit-amulet"] = modifiers["cards"]["unit-follower"].clone();
    let programs = lowerer().lower(&input).unwrap();
    let mut initial = setup(1, json!([]));
    if extra_turn {
        initial["semantic_state"]["turn_schedule"] =
            json!({"extra":[{"player":"P1","count":1}],"skipped":[]});
    }
    for seat in ["P1", "P2"] {
        initial["players"][seat]["pp"] = json!({"current":8, "max":8});
        initial["players"][seat]["zones"]["field"] = json!([]);
        initial["players"][seat]["zones"]["deck"] = json!(
            (0_u8..8)
                .map(|index| json!({"id":format!("{seat}-draw-{index}"),"card":"unit-spell"}))
                .collect::<Vec<_>>()
        );
    }
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"modifier-a","card":"unit-amulet"}, {"id":"modifier-b","card":"unit-amulet"},
        {"id":"fanfare-initial","card":"unit-follower"}, {"id":"fanfare-next","card":"unit-follower"},
        {"id":"fanfare-later","card":"unit-follower"}
    ]);
    initial["players"]["P2"]["zones"]["hand"] =
        json!([{"id":"other-fanfare","card":"unit-follower"}]);
    let mut game = evolution_linked_movement::loaded(&initial, &programs["unit-follower"]);
    game.load_internal_card("unit-amulet", &programs["unit-amulet"])
        .unwrap();
    game.load_internal_card("unit-spell", &card(vec![]))
        .unwrap();
    game.advance_player_input();
    game
}

#[test]
fn next_own_turn_fanfare_doubling_does_not_stack_and_expires() {
    let mut game = board(
        &[
            ("fanfare", json!({"do":[{"life":{"who":"self","delta":1}}]})),
            (
                "auto",
                json!({"on":{"enter_field":{"ref":"this"}},"do":[
                    {"life":{"who":"self","delta":2}}
                ]}),
            ),
        ],
        false,
    );
    for _ in 0..2 {
        play(&mut game, Viewer::P1, "unit-amulet");
    }
    let installed = game.projection(View::Referee).unwrap();
    assert_eq!(
        installed["P1"]["cemetery"],
        json!(["modifier-a", "modifier-b"])
    );
    play(&mut game, Viewer::P1, "unit-follower");
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        23
    );
    end_turn(&mut game, Viewer::P1, Viewer::P2);
    play(&mut game, Viewer::P2, "unit-follower");
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
        23
    );
    end_turn(&mut game, Viewer::P2, Viewer::P1);
    play(&mut game, Viewer::P1, "unit-follower");
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        27,
        "Fanfare resolves twice and the ordinary entry ability resolves once"
    );
    end_turn(&mut game, Viewer::P1, Viewer::P2);
    end_turn(&mut game, Viewer::P2, Viewer::P1);
    play(&mut game, Viewer::P1, "unit-follower");
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        30
    );
}

#[test]
fn an_immediate_extra_turn_is_the_next_own_turn_for_fanfare_multiplicity() {
    let mut game = board(
        &[("fanfare", json!({"do":[{"life":{"who":"self","delta":1}}]}))],
        true,
    );
    play(&mut game, Viewer::P1, "unit-amulet");
    end_turn(&mut game, Viewer::P1, Viewer::P2);
    assert_eq!(
        game.projection(View::Referee).unwrap()["turn"]["active"],
        "P1"
    );
    play(&mut game, Viewer::P1, "unit-follower");
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        22
    );
    end_turn(&mut game, Viewer::P1, Viewer::P2);
    end_turn(&mut game, Viewer::P2, Viewer::P1);
    play(&mut game, Viewer::P1, "unit-follower");
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        23
    );
}

#[test]
fn next_own_turn_fanfare_multiplier_rejects_typed_ir_mutations() {
    let input = evolution_linked_movement::tree(&[("fanfare", json!({"do":[modifier_step()]}))]);
    let programs = lowerer().lower(&input).unwrap();
    player_hand_limit::assert_player_rule_rejects_mutations(&programs["unit-follower"]);
}

#[test]
fn next_own_turn_fanfare_multiplier_rejects_other_subjects_durations_and_counts() {
    for (player, count, duration) in [
        ("opp", 2, json!({"during_next_turn":{"who":"self"}})),
        ("self", 2, json!("game")),
        ("self", 2, json!({"during_next_turn":{"who":"opp"}})),
        ("self", 3, json!({"during_next_turn":{"who":"self"}})),
    ] {
        let input = evolution_linked_movement::tree(&[(
            "fanfare",
            json!({"do":[{"player_effect":{
                "who":player,
                "effect":{"fanfare_triggers":count},
                "until":duration
            }}]}),
        )]);
        lowerer().lower(&input).unwrap_err();
    }
}

#[test]
fn doubled_chronos_fanfare_schedules_two_extra_turns() {
    let mut game = board(
        &[(
            "fanfare",
            json!({"do":[{"if":{
                "cond":{"entered_by":{"ref":"this","by":"play"}},
                "then":[{"extra_turn":{"who":"self"}}]
            }}]}),
        )],
        false,
    );
    play(&mut game, Viewer::P1, "unit-amulet");
    end_turn(&mut game, Viewer::P1, Viewer::P2);
    end_turn(&mut game, Viewer::P2, Viewer::P1);
    play(&mut game, Viewer::P1, "unit-follower");
    for _ in 0..2 {
        end_turn(&mut game, Viewer::P1, Viewer::P2);
        assert_eq!(
            game.projection(View::Referee).unwrap()["turn"]["active"],
            "P1"
        );
    }
    end_turn(&mut game, Viewer::P1, Viewer::P2);
    assert_eq!(
        game.projection(View::Referee).unwrap()["turn"]["active"],
        "P2"
    );
}

#[test]
fn a_doubled_source_transfer_leaves_the_follower_with_the_first_recipient() {
    let mut game = board(
        &[(
            "fanfare",
            json!({"do":[
                {"give":{"ref":"this","to":"other_players"}},
                {"life":{"who":"self","delta":1}}
            ]}),
        )],
        true,
    );
    play(&mut game, Viewer::P1, "unit-amulet");
    end_turn(&mut game, Viewer::P1, Viewer::P2);
    play(&mut game, Viewer::P1, "unit-follower");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["leader"]["life"], 22);
    assert_eq!(state["P2"]["leader"]["life"], 20);
    assert_eq!(state["P1"]["field"], json!([]));
    assert_eq!(state["P2"]["field"], json!(["fanfare-initial"]));
    assert_eq!(state["objects"]["fanfare-initial"]["generation"], 2);
}
