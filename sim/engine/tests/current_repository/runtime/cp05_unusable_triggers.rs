
use super::*;
use sve_engine::game::player::{
    Command, DecisionShape, Input, MainActionAnswer, MainIntentKind, Question, QuestionShape,
    SubmissionKind,
};

const P1: PlayerId = PlayerId { seat: 1 };

fn question(game: &Game, actor: Viewer) -> Question {
    let Input::Question(question) = game.input(actor) else {
        panic!("expected the player's next question");
    };
    *question
}

fn play(game: &mut Game, printing: &str) {
    game.advance_player_input();
    let main = question(game, Viewer::P1);
    let QuestionShape::ActionWindow(actions) = main.shape else {
        panic!("expected main actions");
    };
    let intent = actions
        .intents
        .into_iter()
        .find(|intent| {
            matches!(&intent.kind, MainIntentKind::Play { card } if card.properties.card == printing)
        })
        .unwrap();
    assert_eq!(
        game.submit_player(
            Viewer::P1,
            Command::MainAction {
                token: main.token,
                intent: intent.id,
                answer: MainActionAnswer::None,
            },
        )
        .kind,
        SubmissionKind::Applied,
        "the follower must enter before its Fanfare is checked"
    );
}

fn assert_main(game: &Game) {
    let prompt = question(game, Viewer::P1);
    assert!(
        matches!(prompt.shape, QuestionShape::ActionWindow(_)),
        "expected main actions, got {:?}",
        prompt.purpose
    );
}

fn cancel_unusable_trigger(game: &mut Game) {
    let order = question(game, Viewer::P1);
    let QuestionShape::Decision(DecisionShape::Pending(options)) = &order.shape else {
        panic!("expected the pending ability");
    };
    assert_eq!(options.len(), 1, "only the tested ability should be pending");
    assert_eq!(
        game.submit_player(
            Viewer::P1,
            Command::Answer {
                token: order.token,
                answer: Answer::Pending(options[0].handle),
            },
        )
        .kind,
        SubmissionKind::Cancelled,
        "the pending ability cannot open a payment or target question"
    );
}

#[test]
fn cp05_unusable_triggers_skip_evolution_with_only_one_discard() {
    let mut initial = draw_position();
    let zones = &mut initial["players"]["P1"]["zones"];
    zones["field"] = json!([{"id":"source","card":"CP05-074"}]);
    zones["evolve_deck"] = json!([{"id":"form","card":"CP05-075"}]);
    zones["hand"] = json!([{"id":"only","card":"BP01-171"}]);
    let mut game = game(&initial);
    assert_eq!(
        game.internal_rule_action(&json!({
            "do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}
        }))
        .unwrap()
        .outcome,
        "resolved"
    );
    game.advance_player_input();
    cancel_unusable_trigger(&mut game);
    assert_main(&game);
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["objects"]["source"]["evolved_with"], "form");
    assert_eq!(view["P1"]["hand"], json!(["only"]));
}

#[test]
fn cp05_unusable_triggers_skip_fanfare_with_only_two_reveals() {
    let mut initial = draw_position();
    let p1 = &mut initial["players"]["P1"];
    p1["leader"]["life"] = json!(15_u8);
    p1["zones"]["hand"] = json!([
        {"id":"source","card":"CP05-013"},
        {"id":"fire1","card":"CP05-044"},
        {"id":"fire2","card":"CP05-051"},
        {"id":"other","card":"BP01-037"}
    ]);
    let mut game = game(&initial);
    play(&mut game, "CP05-013");
    cancel_unusable_trigger(&mut game);
    assert_main(&game);
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["field"], json!(["source"]));
    assert_eq!(view["P1"]["hand"], json!(["fire1", "fire2", "other"]));
    assert_eq!(view["P1"]["leader"]["life"], 15_u8);
    assert_eq!(view["P1"]["ex"], json!([]));
}

#[test]
fn cp05_unusable_triggers_skip_fanfare_when_no_discard_matches() {
    let mut initial = draw_position();
    let zones = &mut initial["players"]["P1"]["zones"];
    zones["hand"] = json!([{"id":"meg","card":"CP05-058"},{"id":"other","card":"BP01-171"}]);
    zones["deck"] = json!([{"id":"top","card":"BP01-171"}]);
    let mut game = game(&initial);
    play(&mut game, "CP05-058");
    cancel_unusable_trigger(&mut game);
    assert_main(&game);
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["field"], json!(["meg"]));
    assert_eq!(view["P1"]["hand"], json!(["other"]));
    assert_eq!(view["P1"]["deck"], json!(["top"]));
}

#[test]
fn cp05_unusable_triggers_skip_discard_effect_at_zero_pp() {
    let mut initial = draw_position();
    let p1 = &mut initial["players"]["P1"];
    p1["pp"]["current"] = json!(2_u8);
    p1["zones"]["hand"] = json!([
        {"id":"meg","card":"CP05-058"},
        {"id":"vaseraga","card":"CP05-082"}
    ]);
    p1["zones"]["deck"] = json!([{"id":"drawn","card":"BP01-171"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    let mut game = game(&initial);
    let CardPlay::Entered(_) = game.play_internal_card("meg", P1).unwrap() else {
        panic!("Meg enters before her Fanfare");
    };
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("Meg's Fanfare");
    };
    let CardPlay::Awaiting(payment) = game
        .answer_internal_card(pending.actor, pending.token, Answer::Pending(options[0].handle))
        .unwrap()
    else {
        panic!("Meg can discard Vaseraga");
    };
    let CardPlay::Awaiting(materials) = game
        .answer_internal_card(payment.actor, payment.token, Answer::OptionalCost(true))
        .unwrap()
    else {
        panic!("Meg must choose the discard");
    };
    let InputDomain::CostMaterials(domain) = &materials.domain else {
        panic!("expected discard materials");
    };
    resolved(
        game.answer_internal_card(
            materials.actor,
            materials.token,
            Answer::CostMaterials(vec![domain.members[0].handle]),
        )
        .unwrap(),
    );
    game.advance_player_input();
    cancel_unusable_trigger(&mut game);
    assert_main(&game);
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["hand"], json!(["drawn"]));
    assert_eq!(view["P1"]["cemetery"], json!(["vaseraga"]));
    assert_eq!(view["P1"]["pp"]["current"], 0_u8);
    assert_eq!(view["objects"]["enemy"]["damage"], 0_u8);
}

#[test]
fn cp05_unusable_triggers_skip_fanfare_without_a_required_target() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"source","card":"CP05-002"},
        {"id":"fire1","card":"CP05-044"},
        {"id":"fire2","card":"CP05-051"},
        {"id":"fire3","card":"CP05-013"}
    ]);
    let mut game = game(&initial);
    play(&mut game, "CP05-002");
    cancel_unusable_trigger(&mut game);
    assert_main(&game);
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["field"], json!(["source"]));
    assert_eq!(view["P2"]["leader"]["life"], 20_u8);
    assert!(game.projection(View::P2).unwrap()["known_cards"]
        .as_array().unwrap().iter().all(|card| card["id"] == "source"));
}

#[test]
fn cp05_unusable_triggers_continue_the_attack_without_a_required_target() {
    let mut initial = draw_position();
    let zones = &mut initial["players"]["P1"]["zones"];
    zones["field"] = json!([{"id":"attacker","card":"CP05-068"}]);
    zones["hand"] = json!([
        {"id":"fire1","card":"CP05-044"},
        {"id":"fire2","card":"CP05-051"},
        {"id":"fire3","card":"CP05-013"}
    ]);
    let mut game = game(&initial);
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"attacker","target":"P2.leader"}))
            .unwrap()
            .outcome,
        "resolved"
    );
    game.advance_player_input();
    cancel_unusable_trigger(&mut game);
    let quick = question(&game, Viewer::P2);
    assert_eq!(
        game.submit_player(
            Viewer::P2,
            Command::Answer {
                token: quick.token,
                answer: Answer::Quick(None),
            },
        )
        .kind,
        SubmissionKind::Applied
    );
    assert_main(&game);
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P2"]["leader"]["life"], 14_u8);
    assert_eq!(view["P1"]["hand"], json!(["fire1", "fire2", "fire3"]));
}

#[test]
fn cp05_unusable_triggers_keep_the_limit_break_when_its_ability_cannot_be_played() {
    for (pp, enemy) in [(2_u8, true), (3, false)] {
        let mut initial = draw_position();
        let p1 = &mut initial["players"]["P1"];
        p1["pp"]["current"] = json!(pp);
        p1["zones"]["field"] = json!([{"id":"source","card":"CP05-107"}]);
        p1["zones"]["ex"] = json!([{"id":"material","card":"CP05-T15"}]);
        if enemy {
            initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
        }
        let mut game = game(&initial);
        let activation = game.internal_activated_sources("material").unwrap()[0].definition;
        let CardPlay::Awaiting(target) =
            game.activate_internal_card("material", activation, P1).unwrap()
        else {
            panic!("the material must target Gran and Djeeta");
        };
        let InputDomain::Targets(domain) = &target.domain else {
            panic!("expected an allied follower target");
        };
        assert_eq!(domain.members.len(), 1);
        resolved(
            game.answer_internal_card(
                target.actor,
                target.token,
                Answer::Targets(vec![domain.members[0].handle]),
            )
            .unwrap(),
        );
        game.advance_player_input();
        cancel_unusable_trigger(&mut game);
        assert_main(&game);
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["P1"]["limit_break"], json!(["material"]));
        assert_eq!(view["P1"]["ex"], json!([]));
        assert_eq!(view["P1"]["pp"]["current"], pp);
        assert_eq!(view["P2"]["leader"]["life"], 20_u8);
    }
}
