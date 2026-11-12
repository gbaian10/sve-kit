#![expect(
    clippy::panic,
    reason = "Unexpected public questions invalidate the printed prayer scenario."
)]
use super::*;
use alloc::collections::BTreeSet;
use sve_engine::game::player::observation::{InstalledChange, NumberChange, ObservedZone};
use sve_engine::game::player::{
    Command, DecisionShape, Input, MainActionAnswer, MainIntentKind, Question, QuestionShape,
    SubmissionKind, Viewer,
};

const CARD: &str = "BP18-104";
const CARD_ID: &str = "c:590db19b18125325bd8ce192d709786e";
const FACE: &str = "f:fa7eb2837bea53088903efca57420635";

#[test]
fn bp18_104_current_covers_both_lines_and_exactly_the_registered_printings() {
    let selected = selection_with_rules();
    let author = inputs().2["authors"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["status"] == "accepted" && a["input"]["card_id"] == CARD_ID)
        .unwrap();
    let printings = author["printings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p.as_str().unwrap())
        .collect::<BTreeSet<_>>();
    assert_eq!(printings, BTreeSet::from([CARD, "BP18-P43"]));
    let prototypes = serde_json::to_value(&*selected.prototypes).unwrap();
    for printing in printings {
        assert_eq!(selected.current[printing].card_id, CARD_ID);
        assert!(prototypes["programs"].get(printing).is_none());
    }
    let card = &selected.current[CARD];
    let faces = card.source_payload["faces"].as_array().unwrap();
    assert_eq!(faces.len(), 1);
    assert_eq!(faces[0]["face_id"], FACE);
    assert_eq!(card.program.card().abilities.len(), 2);
    for (line, path) in [
        (1, "0/fanfare/do/0/look"),
        (1, "0/fanfare/do/5/player_effect"),
        (2, "1/activate/do/0/life"),
    ] {
        let source = card
            .source_at(&format!("/cards/{CARD}/faces/{FACE}/abilities/{path}"))
            .unwrap();
        assert_eq!(source.source.card_id, CARD_ID);
        assert_eq!(source.source.face, FACE);
        assert_eq!(source.source.field, "effect");
        assert_eq!(source.source.line, line);
        assert_eq!(source.source.section, None);
    }
}

fn question(game: &Game) -> Question {
    let Input::Question(q) = game.input(Viewer::P1) else {
        panic!("public question")
    };
    *q
}

fn submit(game: &mut Game, command: Command) {
    assert_eq!(
        game.submit_player(Viewer::P1, command).kind,
        SubmissionKind::Applied,
        "the public choice applies at this stage"
    );
}

fn answer(game: &mut Game, answer: Answer) {
    submit(
        game,
        Command::Answer {
            token: question(game).token,
            answer,
        },
    );
}

fn main_action(game: &mut Game, number: &str, activate: bool) {
    let q = question(game);
    let QuestionShape::ActionWindow(actions) = q.shape else {
        panic!("main actions")
    };
    let intent = actions
        .intents
        .iter()
        .find(|i| match &i.kind {
            MainIntentKind::Play { card } => !activate && card.properties.card == number,
            MainIntentKind::Activate { holder, line, .. } => {
                activate && holder.properties.card == number && *line == 2
            }
            MainIntentKind::Evolve { .. }
            | MainIntentKind::Attack { .. }
            | MainIntentKind::EndPhase => false,
        })
        .unwrap();
    submit(
        game,
        Command::MainAction {
            token: q.token,
            intent: intent.id.clone(),
            answer: MainActionAnswer::None,
        },
    );
}

fn quote(game: &Game, number: &str) -> i64 {
    let observed = game.observe(Viewer::P1).unwrap();
    let handle = &observed
        .players
        .iter()
        .find(|p| p.player == Viewer::P1)
        .unwrap()
        .zones
        .iter()
        .find(|z| z.kind == ObservedZone::Hand)
        .unwrap()
        .cards
        .iter()
        .find(|c| c.card.properties.card == number)
        .unwrap()
        .card
        .handle;
    observed
        .costs
        .cards
        .iter()
        .find(|q| &q.card == handle)
        .unwrap()
        .cost
        .total_pp
        .unwrap()
}

fn resolve_look(game: &mut Game) {
    let pending = question(game);
    let QuestionShape::Decision(DecisionShape::Pending(options)) = pending.shape else {
        panic!("printed fanfare pending")
    };
    assert_eq!(options.len(), 1, "only the printed fanfare is pending");
    assert_eq!(options[0].card, CARD, "the source owns the pending trigger");
    assert_eq!(options[0].line, 1_u64, "only the fanfare line is pending");
    answer(game, Answer::Pending(options[0].handle));
    let look = question(game);
    let QuestionShape::Decision(DecisionShape::Targets(targets)) = look.shape else {
        panic!("printed private Look")
    };
    assert_eq!(
        targets
            .members
            .iter()
            .map(|m| m.card.as_deref().unwrap())
            .collect::<Vec<_>>(),
        ["BP18-105", "BP18-097"],
        "exactly the printed groups are offered"
    );
    assert!(
        matches!(game.input(Viewer::P2), Input::Waiting(_)),
        "the opponent cannot answer the private Look"
    );
    answer(
        game,
        Answer::Targets(targets.members.iter().map(|m| m.handle).collect()),
    );
    let order = question(game);
    let QuestionShape::Decision(DecisionShape::Order(cards)) = order.shape else {
        panic!("printed bottom order")
    };
    assert_eq!(cards.len(), 2, "the two unselected cards need ordering");
    let known = game.projection(View::P2).unwrap();
    for id in ["trait", "named"] {
        assert!(
            known["known_cards"]
                .as_array()
                .unwrap()
                .iter()
                .any(|c| c["id"] == id),
            "the selected card is revealed"
        );
    }
    for id in ["same", "rest"] {
        assert!(
            !known["known_cards"]
                .as_array()
                .unwrap()
                .iter()
                .any(|c| c["id"] == id),
            "unselected cards remain private"
        );
    }
    answer(
        game,
        Answer::Order(cards.iter().rev().map(|m| m.handle).collect()),
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["deck"],
        json!(["rest", "same"]),
        "the actor reverses the remainder order"
    );
}

#[test]
fn bp18_104_look_then_next_tokyo_discount_survives_departure_and_commits_once() {
    let mut initial = draw_position();
    initial["players"]["P1"]["leader"]["class"] = json!("ビショップ");
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"source","card":CARD},{"id":"goblin","card":"BP01-171"},
        {"id":"first","card":"BP18-097"},{"id":"second","card":"BP18-097"}
    ]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"same","card":CARD},{"id":"trait","card":"BP18-105"},
        {"id":"named","card":"BP18-097"},{"id":"rest","card":"BP01-171"}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-171"}]);
    let mut game = Game::from_selected_effects(
        selection_with_rules().clone(),
        &initial,
        &Value::Null,
        &Value::Null,
        "printed-prayer-cost",
    )
    .unwrap();
    game.advance_player_input();
    let enemy_before = game.projection(View::Referee).unwrap()["objects"]["enemy"].clone();
    main_action(&mut game, CARD, false);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        7_u8
    );
    resolve_look(&mut game);
    let before = format!("{game:?}");
    for _ in 0_u8..3 {
        assert_eq!(quote(&game, "BP18-097"), 1);
        assert_eq!(quote(&game, "BP01-171"), 1);
        question(&game);
    }
    assert_eq!(format!("{game:?}"), before);
    main_action(&mut game, "BP01-171", false);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        6_u8
    );
    assert_eq!(quote(&game, "BP18-097"), 1);
    main_action(&mut game, CARD, true);
    let departed = game.projection(View::Referee).unwrap();
    assert_eq!(departed["P1"]["pp"]["current"], 5_u8);
    assert_eq!(departed["P1"]["cemetery"], json!(["source"]));
    assert_eq!(departed["P1"]["leader"]["life"], 21_u8);
    assert!(
        game.observe(Viewer::P1)
            .unwrap()
            .players
            .iter()
            .find(|p| p.player == Viewer::P1)
            .unwrap()
            .effects
            .iter()
            .any(|e| e.change
                == InstalledChange::PlayerCost {
                    change: NumberChange::Add(-2),
                    limited: true
                })
    );
    assert_eq!(quote(&game, "BP18-097"), 1);
    main_action(&mut game, "BP18-097", false);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        4_u8
    );
    assert_eq!(quote(&game, "BP18-097"), 3);
    main_action(&mut game, "BP18-097", false);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["pp"]["current"], 1_u8);
    assert_eq!(state["P1"]["field"], json!(["goblin", "first", "second"]));
    assert_eq!(state["P1"]["leader"]["life"], 21_u8);
    assert_eq!(state["P2"]["field"], json!(["enemy"]));
    assert_eq!(state["objects"]["enemy"], enemy_before);
    assert!(matches!(
        question(&game).shape,
        QuestionShape::ActionWindow(_)
    ));
}
