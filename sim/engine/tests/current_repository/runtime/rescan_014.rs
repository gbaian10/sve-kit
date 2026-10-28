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

fn immediate_attack(card: &str, keyword: &str, power: u8) {
    let mut initial = field_position(card);
    initial["players"]["P1"]["zones"]["field"][0]["state"] = json!({"entered_this_turn":true});
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-037","state":{"acted":true}}]);
    let mut game = game(&initial);
    let target = if keyword == "storm" {
        "P2.leader"
    } else {
        "enemy"
    };
    if keyword == "rush" {
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"attack","attacker":"source","target":"P2.leader"})
            )
            .unwrap()
            .outcome,
            "cannot-attack",
            "rush does not allow an immediate leader attack"
        );
    }
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":target}))
            .unwrap()
            .outcome,
        "resolved",
        "the printed keyword permits the immediate attack"
    );
    pass_quick(&mut game);
    if keyword == "storm" {
        assert_eq!(
            game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
            20_u8.checked_sub(power).unwrap(),
            "storm damages the opposing leader immediately"
        );
    } else if power >= 5 {
        assert_eq!(
            game.projection(View::Referee).unwrap()["P2"]["cemetery"],
            json!(["enemy"]),
            "the opposing follower is destroyed"
        );
    } else {
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
            power,
            "rush deals combat damage to its defender"
        );
    }
}

fn count_named(state: &Value, zone: &str, name: &str) -> usize {
    state["P1"][zone]
        .as_array()
        .unwrap()
        .iter()
        .filter(|id| state["objects"][id.as_str().unwrap()]["name"] == name)
        .count()
}

fn assail(card: &str) {
    let mut control = field_position("BP01-037");
    control["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    let mut control_game = game(&control);
    assert_eq!(
        control_game
            .internal_rule_action(&json!({"do":"attack","attacker":"source","target":"enemy"}))
            .unwrap()
            .outcome,
        "cannot-attack",
        "ordinary followers cannot attack active followers"
    );
    let mut initial = field_position(card);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    let mut game = game(&initial);
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"enemy"}))
            .unwrap()
            .outcome,
        "resolved",
        "assail permits attacking an active follower"
    );
}

#[test]
fn bp09_068_spell_damages_opponent_then_draws() {
    let mut initial = position("BP09-068");
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    let mut game = game(&initial);
    play(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["damage"], 2_u8);
    assert_eq!(state["P1"]["hand"], json!(["P1-first"]));
    assert_eq!(state["P1"]["deck"], json!(["P1-second", "P1-third"]));
}

#[test]
fn bp09_039_white_face_has_ward_without_black_face_assail() {
    ward("BP09-039");
    let mut initial = field_position("BP09-039");
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    let mut game = game(&initial);
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"enemy"}))
            .unwrap()
            .outcome,
        "cannot-attack",
        "the white face does not inherit black face assail"
    );
}

#[test]
fn bp09_030_real_evolution_deals_four_to_opposing_follower() {
    let mut initial = field_position("BP09-029");
    initial["players"]["P1"]["pp"]["current"] = json!(4_u8);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP09-030"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    let mut game = game(&initial);
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":4_u8}})
        )
        .unwrap()
        .outcome,
        "resolved",
        "the real base evolves"
    );
    trigger(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
        4_u8
    );
}

#[test]
fn bp09_061_optional_fanfare_pays_only_when_destroying_an_amulet() {
    for accepted in [false, true] {
        let mut initial = position("BP09-061");
        initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP18-115"}]);
        let mut game = game(&initial);
        play(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
            6_u8
        );
        let pending = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = &pending.domain else {
            panic!("expected fanfare trigger");
        };
        assert_eq!(options.len(), 1);
        let declaration = game
            .answer_internal_card(
                pending.actor,
                pending.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap();
        let CardPlay::Awaiting(payment) = declaration else {
            panic!("expected optional PP declaration");
        };
        assert_eq!(payment.domain, InputDomain::OptionalCost);
        let play = game
            .answer_internal_card(payment.actor, payment.token, Answer::OptionalCost(accepted))
            .unwrap();
        finish(&mut game, play, accepted);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P1"]["pp"]["current"],
            if accepted { 4_u8 } else { 6_u8 }
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
fn bp09_063_awakening_buffs_only_power() {
    for max in [6_u8, 7] {
        let mut initial = position("BP09-063");
        initial["players"]["P1"]["pp"] = json!({"current":max,"max":max});
        let mut game = game(&initial);
        play(&mut game);
        trigger(&mut game);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["objects"]["source"]["power"],
            if max == 7 { 3_u8 } else { 1_u8 }
        );
        assert_eq!(state["objects"]["source"]["hp"], 2_u8);
    }
}

#[test]
fn bp09_025_assail_attacks_an_active_follower() {
    assail("BP09-025");
}

#[test]
fn bp09_029_evolves_for_four_pp() {
    evolve("BP09-029", "BP09-030", 4);
}

#[test]
fn bp09_076_evolves_for_one_pp() {
    evolve("BP09-076", "BP09-077", 1);
}

#[test]
fn bp09_080_evolves_for_one_pp() {
    evolve("BP09-080", "BP09-081", 1);
}

#[test]
fn bp09_093_evolves_for_one_pp() {
    evolve("BP09-093", "BP09-094", 1);
}

#[test]
fn bp09_114_rush_attacks_on_entry() {
    immediate_attack("BP09-114", "rush", 3);
}

#[test]
fn bp09_115_ward_protects_its_leader() {
    ward("BP09-115");
}

#[test]
fn bp09_058_spell_draws_before_discarding() {
    let mut initial = position("BP09-058");
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    let mut game = game(&initial);
    play(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["damage"], 3_u8);
    assert_eq!(state["P1"]["hand"], json!([]));
    assert_eq!(state["P1"]["deck"], json!(["P1-second", "P1-third"]));
    assert!(
        state["P1"]["cemetery"]
            .as_array()
            .unwrap()
            .contains(&json!("P1-first")),
        "the drawn card is discarded"
    );
    assert!(
        state["P1"]["cemetery"]
            .as_array()
            .unwrap()
            .contains(&json!("source")),
        "the resolved spell reaches cemetery"
    );
}

#[test]
fn bp09_103_fanfare_mills_top_two() {
    let mut game = game(&position("BP09-103"));
    play(&mut game);
    trigger(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["cemetery"], json!(["P1-first", "P1-second"]));
    assert_eq!(state["P1"]["deck"], json!(["P1-third"]));
}

#[test]
fn bp09_085_quick_spell_mills_its_controller_during_opponent_turn() {
    let mut initial = position("BP09-085");
    initial["turn"]["active"] = json!("P2");
    let mut game = game(&initial);
    assert_eq!(
        game.internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap()
            .outcome,
        "resolved",
        "the opponent end phase opens Quick"
    );
    let quick = game.internal_quick_choice().unwrap().unwrap();
    let InputDomain::Quick { options, .. } = &quick.domain else {
        panic!("expected Quick window");
    };
    assert_eq!(options.len(), 1);
    let play = game
        .answer_internal_card(quick.actor, quick.token, Answer::Quick(Some(0)))
        .unwrap();
    finish(&mut game, play, true);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["deck"], json!(["P1-third"]));
    assert_eq!(state["P1"]["cemetery"].as_array().unwrap().len(), 3);
    assert_eq!(
        state["P2"]["deck"],
        json!(["P2-first", "P2-second", "P2-third"])
    );
}

#[test]
fn bp09_021_fanfare_creates_an_admitted_knight() {
    let mut game = game(&position("BP09-021"));
    play(&mut game);
    trigger(&mut game);
    assert_eq!(
        count_named(&game.projection(View::Referee).unwrap(), "field", "ナイト"),
        1,
        "one knight token enters the field"
    );
}
