#![expect(
    clippy::panic,
    reason = "An unexpected choice means the real-card scenario cannot continue."
)]

use super::*;

const fn actor() -> PlayerId {
    PlayerId { seat: 1 }
}

fn position(card: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    initial
}

fn finish_wards(game: &mut Game, mut play: CardPlay) -> CardPlay {
    while let CardPlay::Awaiting(question) = play {
        assert!(
            matches!(question.domain, InputDomain::WardEntry { .. }),
            "only Ward entry posture remains to choose"
        );
        play = game
            .answer_internal_card(question.actor, question.token, Answer::WardEntry(false))
            .unwrap();
    }
    play
}

fn enter(game: &mut Game) {
    let play = game.play_internal_card("source", actor()).unwrap();
    assert!(
        matches!(finish_wards(game, play), CardPlay::Entered(_)),
        "the complete real follower or amulet enters"
    );
}

fn trigger(game: &mut Game, card: &str) -> Activation {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("real-card trigger");
    };
    assert_eq!(options.len(), 1, "one matching trigger is pending");
    assert_eq!(options[0].card, card, "the real card supplies the ability");
    let play = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap();
    resolved(finish_wards(game, play))
}

fn activate(game: &mut Game, card: &str) -> Activation {
    let definition = selection().current[card]
        .program
        .card()
        .abilities
        .iter()
        .find(|ability| ability.node.kind == AbilityKind::Activated)
        .unwrap()
        .node
        .id;
    let play = game
        .activate_internal_card("source", definition, actor())
        .unwrap();
    resolved(finish_wards(game, play))
}

fn assert_tokens(state: &Value, zone: &str, card: &str, n: usize, power: i64, hp: i64) {
    let objects = state["P1"][zone]
        .as_array()
        .unwrap()
        .iter()
        .map(|id| &state["objects"][id.as_str().unwrap()])
        .filter(|object| object["card"] == card)
        .collect::<Vec<_>>();
    assert_eq!(objects.len(), n, "the expected real token reaches its zone");
    for object in objects {
        assert_eq!(
            object["power"], power,
            "token power comes from its snapshot"
        );
        assert_eq!(object["hp"], hp, "token health comes from its snapshot");
    }
}

#[test]
fn bp01_020_creates_fairy_on_field_and_in_ex() {
    let mut game = game(&position("BP01-020"));
    enter(&mut game);
    trigger(&mut game, "BP01-020");
    let state = game.projection(View::Referee).unwrap();
    assert_tokens(&state, "field", "BP01-T03", 1, 1, 1);
    assert_tokens(&state, "ex", "BP01-T03", 1, 1, 1);
    assert_eq!(state["P2"]["field"], json!([]));
    assert_eq!(state["P2"]["ex"], json!([]));
}

#[test]
fn bp02_024_pays_and_acts_to_create_two_ward_tokens() {
    let mut game = game(&position("BP02-024"));
    enter(&mut game);
    game.resume_internal_card().unwrap();
    activate(&mut game, "BP02-024");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["pp"]["current"], 5_i32);
    assert_eq!(state["objects"]["source"]["acted"], true);
    assert_tokens(&state, "field", "BP17-T13", 2, 1, 1);
    let field = state["P1"]["field"].as_array().unwrap();
    for id in field.iter().filter(|id| **id != json!("source")) {
        let token = &state["objects"][id.as_str().unwrap()];
        assert_eq!(token["keywords"], json!(["守護"]));
        assert_eq!(token["acted"], false);
    }
}

#[test]
fn bp02_028_damages_its_target_then_creates_knight_in_ex() {
    let mut initial = position("BP02-028");
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    let mut game = game(&initial);
    let before = game.projection(View::Referee).unwrap()["objects"]["enemy"]["hp"]
        .as_i64()
        .unwrap();
    let CardPlay::Awaiting(question) = game.play_internal_card("source", actor()).unwrap() else {
        panic!("formal spell target");
    };
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("follower targets");
    };
    assert_eq!(domain.members.len(), 1);
    assert_eq!(domain.members[0].player.seat, 2);
    resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Targets(vec![domain.members[0].handle]),
        )
        .unwrap(),
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["hp"], before - 3);
    assert_tokens(&state, "ex", "BP17-T11", 1, 1, 1);
    assert_eq!(state["P1"]["cemetery"], json!(["source"]));
}

#[test]
fn bp02_078_attack_creates_forest_bat() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP02-078"}]);
    let mut game = game(&initial);
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"P2.leader"}))
            .unwrap()
            .outcome,
        "resolved"
    );
    trigger(&mut game, "BP02-078");
    let state = game.projection(View::Referee).unwrap();
    assert_tokens(&state, "field", "BP01-T15", 1, 1, 1);
    assert_eq!(state["P1"]["ex"], json!([]));
}

#[test]
fn bp02_079_real_evolution_and_attack_create_forest_bats_in_distinct_zones() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP02-078"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP02-079"}]);
    let mut game = game(&initial);
    let evolved = game
        .internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}}),
        )
        .unwrap();
    assert_eq!(evolved.outcome, "resolved");
    assert!(
        evolved
            .events
            .iter()
            .any(|event| event["kind"] == "進化" && event["with"] == "form")
    );
    trigger(&mut game, "BP02-079");
    game.resume_internal_card().unwrap();
    let after_evolution = game.projection(View::Referee).unwrap();
    assert_eq!(after_evolution["P1"]["pp"]["current"], 9_i32);
    assert_tokens(&after_evolution, "ex", "BP01-T15", 1, 1, 1);
    assert_tokens(&after_evolution, "field", "BP01-T15", 0, 1, 1);
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"P2.leader"}))
            .unwrap()
            .outcome,
        "resolved"
    );
    trigger(&mut game, "BP02-079");
    let after_attack = game.projection(View::Referee).unwrap();
    assert_tokens(&after_attack, "field", "BP01-T15", 1, 1, 1);
    assert_tokens(&after_attack, "ex", "BP01-T15", 1, 1, 1);
}

#[test]
fn bp06_070_creates_two_orcas_on_field() {
    let mut game = game(&position("BP06-070"));
    enter(&mut game);
    trigger(&mut game, "BP06-070");
    let state = game.projection(View::Referee).unwrap();
    assert_tokens(&state, "field", "BP02-T05", 2, 2, 2);
    assert_eq!(state["P1"]["ex"], json!([]));
}

#[test]
fn bp09_017_fanfare_creates_wisp_then_sacrifices_itself_for_fairy() {
    let mut game = game(&position("BP09-017"));
    enter(&mut game);
    trigger(&mut game, "BP09-017");
    game.resume_internal_card().unwrap();
    let entry = game.projection(View::Referee).unwrap();
    assert_tokens(&entry, "ex", "BP01-T02", 1, 1, 1);
    assert_tokens(&entry, "field", "BP01-T03", 0, 1, 1);
    activate(&mut game, "BP09-017");
    let exit = game.projection(View::Referee).unwrap();
    assert_eq!(exit["P1"]["cemetery"], json!(["source"]));
    assert_tokens(&exit, "field", "BP01-T03", 1, 1, 1);
    assert_tokens(&exit, "ex", "BP01-T02", 1, 1, 1);
}

#[test]
fn bp09_067_creates_five_five_dragon() {
    let mut game = game(&position("BP09-067"));
    enter(&mut game);
    trigger(&mut game, "BP09-067");
    let state = game.projection(View::Referee).unwrap();
    assert_tokens(&state, "field", "BP01-T11", 1, 5, 5);
    assert_eq!(state["objects"]["source"]["power"], 1_i32);
    assert_eq!(state["objects"]["source"]["hp"], 1_i32);
}
