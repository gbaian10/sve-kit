#![expect(
    clippy::panic,
    reason = "Unexpected decisions indicate a broken game fixture."
)]
use super::*;

const fn actor() -> PlayerId {
    PlayerId { seat: 1 }
}

fn position(card: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    for seat in ["P1", "P2"] {
        initial["players"][seat]["zones"]["deck"] = json!([
            {"id":format!("{seat}-first"),"card":"BP01-037"},
            {"id":format!("{seat}-second"),"card":"BP01-037"},
            {"id":format!("{seat}-third"),"card":"BP01-037"}
        ]);
    }
    initial
}

fn field_position(card: &str) -> Value {
    let mut initial = position(card);
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":card}]);
    initial
}

fn finish(game: &mut Game, mut play: CardPlay, accept_optional: bool) {
    while let CardPlay::Awaiting(question) = play {
        let answer = if matches!(&question.domain, InputDomain::OptionalCost) {
            Answer::OptionalCost(accept_optional)
        } else if matches!(&question.domain, InputDomain::WardEntry { .. }) {
            Answer::WardEntry(false)
        } else if matches!(&question.domain, InputDomain::Modes { .. }) {
            Answer::Modes(vec![ModeId(1)])
        } else {
            let InputDomain::Targets(domain) = &question.domain else {
                panic!("expected target decision");
            };
            Answer::Targets(
                domain
                    .members
                    .iter()
                    .take(1)
                    .map(|member| member.handle)
                    .collect(),
            )
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
}

fn play(game: &mut Game) {
    let result = game.play_internal_card("source", actor()).unwrap();
    finish(game, result, true);
}

fn trigger(game: &mut Game) {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("expected pending trigger");
    };
    assert_eq!(options.len(), 1, "one card trigger is pending");
    let result = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap();
    finish(game, result, true);
}

fn pass_quick(game: &mut Game) {
    let question = game.internal_quick_choice().unwrap().unwrap();
    let result = game
        .answer_internal_card(question.actor, question.token, Answer::Quick(None))
        .unwrap();
    finish(game, result, true);
}

fn evolve(card: &str, form: &str, cost: u8) {
    let mut initial = field_position(card);
    initial["players"]["P1"]["pp"]["current"] = json!(cost);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
    if let Some(less) = cost.checked_sub(1) {
        let mut insufficient = initial.clone();
        insufficient["players"]["P1"]["pp"]["current"] = json!(less);
        let mut denied = game(&insufficient);
        if let Ok(step) = denied.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":less}}),
        ) {
            assert_ne!(
                step.outcome, "resolved",
                "less than the printed cost cannot evolve"
            );
        }
        assert_eq!(
            denied.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
            less,
            "rejected evolution preserves PP"
        );
    }
    let mut game = game(&initial);
    let step = game
        .internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":cost}}),
        )
        .unwrap();
    assert_eq!(
        step.outcome, "resolved",
        "the exact printed cost permits evolution"
    );
    let event = step
        .events
        .iter()
        .find(|event| event["kind"] == "進化")
        .unwrap();
    assert_eq!(event["object"], "source", "the real base evolves");
    assert_eq!(
        event["with"], "form",
        "the matching evolution resource is used"
    );
}

fn ward(card: &str) {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"attacker","card":"BP01-037"}]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"ward","card":card,"state":{"acted":true}}]);
    let mut game = game(&initial);
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"attack","attacker":"attacker","target":"P2.leader"})
        )
        .unwrap()
        .outcome,
        "cannot-attack",
        "rested ward protects the leader"
    );
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"attacker","target":"ward"}))
            .unwrap()
            .outcome,
        "resolved",
        "the ward remains attackable"
    );
}

#[test]
fn bp18_068_ward_and_fanfare_destroy_a_five_health_target() {
    ward("BP18-068");
    let mut initial = position("BP18-068");
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    let mut game = game(&initial);
    play(&mut game);
    trigger(&mut game);
    game.resume_internal_card().unwrap();
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["cemetery"], json!(["enemy"]));
    assert_eq!(state["P1"]["leader"]["life"], 20_u8);
}

#[test]
fn bp18_121_fanfare_deals_four_to_the_opposing_follower() {
    let mut initial = position("BP18-121");
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    let mut game = game(&initial);
    play(&mut game);
    trigger(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
        4_u8
    );
}

#[test]
fn bp18_107_fanfare_heals_its_controller_for_four() {
    for print in ["BP18-107", "PR-536"] {
        let mut game = game(&position(print));
        play(&mut game);
        trigger(&mut game);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["leader"]["life"], 24_u8);
        assert_eq!(state["P2"]["leader"]["life"], 20_u8);
    }
}

#[test]
fn bp18_037_optional_payment_buffs_and_draws_only_when_paid() {
    ward("BP18-037");
    for accepted in [false, true] {
        let mut game = game(&position("BP18-037"));
        play(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
            8_u8
        );
        let pending = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = &pending.domain else {
            panic!("expected fanfare trigger");
        };
        assert_eq!(options.len(), 1, "only the optional fanfare is pending");
        let declaration = game
            .answer_internal_card(
                pending.actor,
                pending.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap();
        let CardPlay::Awaiting(payment) = declaration else {
            panic!("expected optional PP choice");
        };
        assert_eq!(payment.domain, InputDomain::OptionalCost);
        let result = game
            .answer_internal_card(payment.actor, payment.token, Answer::OptionalCost(accepted))
            .unwrap();
        finish(&mut game, result, accepted);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P1"]["pp"]["current"],
            if accepted { 6_u8 } else { 8_u8 }
        );
        assert_eq!(
            state["objects"]["source"]["power"],
            if accepted { 4_u8 } else { 2_u8 }
        );
        assert_eq!(
            state["objects"]["source"]["hp"],
            if accepted { 5_u8 } else { 3_u8 }
        );
        assert_eq!(
            state["P1"]["hand"],
            if accepted {
                json!(["P1-first"])
            } else {
                json!([])
            }
        );
        assert_eq!(
            state["P1"]["deck"].as_array().unwrap().len(),
            if accepted { 2 } else { 3 }
        );
    }
}

#[test]
fn bp18_113_destroys_opposing_follower_and_heals_only_its_controller() {
    let mut initial = position("BP18-113");
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    let mut game = game(&initial);
    play(&mut game);
    trigger(&mut game);
    game.resume_internal_card().unwrap();
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["cemetery"], json!(["enemy"]));
    assert_eq!(state["P1"]["leader"]["life"], 23_u8);
    assert_eq!(state["P2"]["leader"]["life"], 20_u8);
}

#[test]
fn bp18_030_optional_payment_destroys_only_when_paid() {
    for accepted in [false, true] {
        let mut initial = position("BP18-030");
        initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
        let mut game = game(&initial);
        play(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
            9_u8
        );
        let pending = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = &pending.domain else {
            panic!("expected fanfare trigger");
        };
        assert_eq!(options.len(), 1, "only the optional fanfare is pending");
        let declaration = game
            .answer_internal_card(
                pending.actor,
                pending.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap();
        let CardPlay::Awaiting(payment) = declaration else {
            panic!("expected optional PP choice");
        };
        assert_eq!(payment.domain, InputDomain::OptionalCost);
        let result = game
            .answer_internal_card(payment.actor, payment.token, Answer::OptionalCost(accepted))
            .unwrap();
        finish(&mut game, result, accepted);
        game.resume_internal_card().unwrap();
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P1"]["pp"]["current"],
            if accepted { 6_u8 } else { 9_u8 }
        );
        assert_eq!(
            state["P2"]["cemetery"],
            if accepted {
                json!(["enemy"])
            } else {
                json!([])
            }
        );
        assert_eq!(
            state["P2"]["field"],
            if accepted {
                json!([])
            } else {
                json!(["enemy"])
            }
        );
    }
}

#[test]
fn bp18_124_fanfare_acts_only_the_opposing_follower() {
    let mut initial = position("BP18-124");
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    let mut game = game(&initial);
    play(&mut game);
    trigger(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["acted"], true);
    assert_eq!(state["objects"]["source"]["acted"], false);
}

#[test]
fn bp18_018_attack_buffs_before_combat_damage() {
    let mut game = game(&field_position("BP18-018"));
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"P2.leader"}))
            .unwrap()
            .outcome,
        "resolved"
    );
    trigger(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["source"]["power"], 6_u8);
    assert_eq!(state["objects"]["source"]["hp"], 7_u8);
    assert_eq!(state["P2"]["leader"]["life"], 20_u8);
    game.resume_internal_card().unwrap();
    pass_quick(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
        14_u8,
        "combat uses the buffed power"
    );
}

#[test]
fn bp18_011_evolves_for_one_pp() {
    evolve("BP18-011", "BP18-012", 1);
}

#[test]
fn bp18_024_ward_and_real_evolution_use_the_printed_card() {
    for print in ["BP18-024", "BP18-P09"] {
        ward(print);
        evolve(print, "BP18-025", 1);
    }
}

#[test]
fn bp18_053_lastword_mills_for_original_controller_on_opponent_turn() {
    let mut initial = field_position("BP18-053");
    initial["turn"]["active"] = json!("P2");
    initial["players"]["P2"]["zones"]["hand"] = json!([{"id":"removal","card":"BP01-169"}]);
    let mut game = game(&initial);
    let attempt = game
        .play_internal_card("removal", PlayerId { seat: 2 })
        .unwrap();
    finish(&mut game, attempt, true);
    game.resume_internal_card().unwrap();
    trigger(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["deck"], json!(["P1-second", "P1-third"]));
    assert_eq!(
        state["P2"]["deck"],
        json!(["P2-first", "P2-second", "P2-third"])
    );
    assert!(
        state["P1"]["cemetery"]
            .as_array()
            .unwrap()
            .contains(&json!("P1-first"))
    );
}

#[test]
fn bp18_077_fanfare_mills_top_two_for_its_controller() {
    for print in ["BP18-077", "BP18-SL17", "PR-482"] {
        let mut game = game(&position(print));
        play(&mut game);
        trigger(&mut game);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["cemetery"], json!(["P1-first", "P1-second"]));
        assert_eq!(state["P1"]["deck"], json!(["P1-third"]));
        assert_eq!(
            state["P2"]["deck"],
            json!(["P2-first", "P2-second", "P2-third"])
        );
    }
}
