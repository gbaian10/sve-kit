#![expect(
    clippy::panic,
    reason = "Unexpected choice domains invalidate these card scenarios."
)]

use super::*;

const fn actor() -> PlayerId {
    PlayerId { seat: 1 }
}

fn position() -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"drawn","card":"BP01-037"},
        {"id":"next","card":"BP01-037"}
    ]);
    initial
}

fn finish(game: &mut Game, mut play: CardPlay) -> CardPlay {
    while let CardPlay::Awaiting(question) = play {
        let answer = match &question.domain {
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            InputDomain::Pending(options) => {
                assert_eq!(
                    options.len(),
                    1,
                    "only the scenario card trigger is pending"
                );
                Answer::Pending(options[0].handle)
            }
            InputDomain::Targets(domain) => {
                assert!(!domain.members.is_empty(), "the effect has a legal target");
                Answer::Targets(vec![domain.members[0].handle])
            }
            InputDomain::TokenCapacity { .. }
            | InputDomain::MealPayment { .. }
            | InputDomain::DriveTrigger { .. }
            | InputDomain::Quick { .. }
            | InputDomain::OptionalCost
            | InputDomain::CostMaterials(_)
            | InputDomain::BeforeDraw
            | InputDomain::TriggerQuota(_)
            | InputDomain::Integer(_)
            | InputDomain::Order(_)
            | InputDomain::Search(_)
            | InputDomain::SearchPartition(_)
            | InputDomain::Distribution(_)
            | InputDomain::SearchReplacement
            | InputDomain::Modes { .. } => panic!("unexpected choice in the card scenario"),
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    play
}

fn trigger(game: &mut Game) -> CardPlay {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("expected the played or evolved card trigger");
    };
    assert_eq!(
        options.len(),
        1,
        "only the scenario card trigger is pending"
    );
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::Pending(options[0].handle),
    )
    .unwrap()
}

fn evolve(game: &mut Game, base: &str, form: &str, pp: u8) {
    let step = game
        .internal_rule_action(
            &json!({"do":"evolve","source":base,"evolve_card":form,"pay":{"pp":pp}}),
        )
        .unwrap();
    assert_eq!(
        step.outcome, "resolved",
        "the printed evolution cost is payable"
    );
}

fn evolve_position(base: &str, form: &str, pp: u8) -> Value {
    let mut initial = position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"base","card":base}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
    initial["players"]["P1"]["pp"]["current"] = json!(pp);
    initial
}

#[test]
fn bp03_014_evolves_for_one_pp() {
    let mut game = game(&evolve_position("BP03-014", "BP03-015", 1));
    evolve(&mut game, "base", "form", 1);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        0_u8
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["base"]["evolved"],
        true
    );
}

#[test]
fn bp03_042_creates_a_pawn_on_field_and_in_ex() {
    let mut initial = position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"queen","card":"BP03-042"}]);
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    initial["players"]["P1"]["zones"]["ex"] = json!([]);
    let mut game = game(&initial);

    let play = game.play_internal_card("queen", actor()).unwrap();
    let entered = finish(&mut game, play);
    assert!(matches!(entered, CardPlay::Entered(_)));
    let triggered = trigger(&mut game);
    let result = finish(&mut game, triggered);
    assert!(matches!(result, CardPlay::Resolved(_)));
    let state = game.projection(View::Referee).unwrap();
    let field_ids = state["P1"]["field"].as_array().unwrap();
    assert_eq!(
        field_ids.len(),
        2,
        "the queen and created pawn are on the field"
    );
    assert!(field_ids.iter().any(|id| {
        state["objects"][id.as_str().unwrap()]["name"].as_str() == Some("マジカルポーン")
    }));
    assert_eq!(state["P1"]["ex"].as_array().unwrap().len(), 1);
    let ex_id = state["P1"]["ex"][0].as_str().unwrap();
    assert_eq!(state["objects"][ex_id]["name"], "マジカルポーン");
}

#[test]
fn bp03_049_evolves_for_zero_pp() {
    let mut game = game(&evolve_position("BP03-049", "BP03-050", 0));
    evolve(&mut game, "base", "form", 0);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        0_u8
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["base"]["evolved"],
        true
    );
}

#[test]
fn bp03_050_evolve_trigger_draws_one_card() {
    let mut game = game(&evolve_position("BP03-049", "BP03-050", 0));
    evolve(&mut game, "base", "form", 0);
    let triggered = trigger(&mut game);
    let result = finish(&mut game, triggered);
    assert!(matches!(result, CardPlay::Resolved(_)));
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["hand"],
        json!(["drawn"])
    );
}

#[test]
fn bp03_068_intimidate_prevents_a_low_cost_follower_from_attacking_it() {
    let mut initial = position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"attacker","card":"BP06-048"}]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"dragon","card":"BP03-068","state":{"acted":true}}]);
    let mut game = game(&initial);

    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"attacker","target":"dragon"}))
            .unwrap()
            .outcome,
        "cannot-attack"
    );
}

#[test]
fn bp03_069_evolves_for_one_pp() {
    let mut game = game(&evolve_position("BP03-069", "BP03-070", 1));
    evolve(&mut game, "base", "form", 1);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        0_u8
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["base"]["evolved"],
        true
    );
}

#[test]
fn bp03_094_evolves_for_two_pp_and_ward_protects_its_leader() {
    let mut ward_position = position();
    ward_position["players"]["P1"]["zones"]["field"] = json!([{"id":"attacker","card":"BP01-037"}]);
    ward_position["players"]["P2"]["zones"]["field"] =
        json!([{"id":"ward","card":"BP03-094","state":{"acted":true}}]);
    let mut ward_game = game(&ward_position);
    assert_eq!(
        ward_game
            .internal_rule_action(
                &json!({"do":"attack","attacker":"attacker","target":"P2.leader"})
            )
            .unwrap()
            .outcome,
        "cannot-attack",
        "an active defender with Ward protects its leader"
    );

    let mut control_position = position();
    control_position["players"]["P1"]["zones"]["field"] =
        json!([{"id":"attacker","card":"BP01-037"}]);
    let mut control = game(&control_position);
    assert_eq!(
        control
            .internal_rule_action(
                &json!({"do":"attack","attacker":"attacker","target":"P2.leader"})
            )
            .unwrap()
            .outcome,
        "resolved",
        "without a Ward defender, the same attack can reach the leader"
    );

    let mut evolved = game(&evolve_position("BP03-094", "BP03-095", 2));
    evolve(&mut evolved, "base", "form", 2);
    assert_eq!(
        evolved.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        0_u8
    );
}

#[test]
fn bp03_103_assail_attacks_an_active_follower() {
    let mut control_position = position();
    control_position["players"]["P1"]["zones"]["field"] =
        json!([{"id":"prince","card":"BP01-037"}]);
    control_position["players"]["P2"]["zones"]["field"] =
        json!([{"id":"target","card":"BP01-037"}]);
    let mut control = game(&control_position);
    assert_eq!(
        control
            .internal_rule_action(&json!({"do":"attack","attacker":"prince","target":"target"}))
            .unwrap()
            .outcome,
        "cannot-attack",
        "ordinary followers cannot attack an active follower"
    );

    let mut initial = position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"prince","card":"BP03-103"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"target","card":"BP01-037"}]);
    let mut game = game(&initial);

    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"prince","target":"target"}))
            .unwrap()
            .outcome,
        "resolved"
    );
}

#[test]
fn bp03_119_fanfare_damages_an_opposing_follower_and_evolves_for_one_pp() {
    let mut initial = position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"angel","card":"BP03-119"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"target","card":"BP01-037"}]);
    let mut fanfare_game = game(&initial);
    let play = fanfare_game.play_internal_card("angel", actor()).unwrap();
    assert!(matches!(
        finish(&mut fanfare_game, play),
        CardPlay::Entered(_)
    ));
    let triggered = trigger(&mut fanfare_game);
    finish(&mut fanfare_game, triggered);
    assert_eq!(
        fanfare_game.projection(View::Referee).unwrap()["objects"]["target"]["hp"],
        4_i32
    );

    let mut evolved = game(&evolve_position("BP03-119", "BP03-120", 1));
    evolve(&mut evolved, "base", "form", 1);
    assert_eq!(
        evolved.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        0_u8
    );
}

#[test]
fn bp03_120_evolve_trigger_damages_the_opposing_leader() {
    let mut game = game(&evolve_position("BP03-119", "BP03-120", 1));
    evolve(&mut game, "base", "form", 1);
    let triggered = trigger(&mut game);
    let result = finish(&mut game, triggered);
    assert!(matches!(result, CardPlay::Resolved(_)));
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["leader"]["life"], 20_u8);
    assert_eq!(state["P2"]["leader"]["life"], 19_u8);
}
