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

fn intimidate(card: &str) {
    let mut control = field_position("BP01-037");
    control["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-037","state":{"acted":true}}]);
    let mut ordinary = game(&control);
    assert_eq!(
        ordinary
            .internal_rule_action(&json!({"do":"attack","attacker":"source","target":"enemy"}))
            .unwrap()
            .outcome,
        "resolved",
        "a plain rested follower can be attacked"
    );
    let mut initial = control;
    initial["players"]["P2"]["zones"]["field"][0]["card"] = json!(card);
    let mut game = game(&initial);
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"enemy"}))
            .unwrap()
            .outcome,
        "cannot-attack",
        "intimidate prevents a follower attack"
    );
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"P2.leader"}))
            .unwrap()
            .outcome,
        "resolved",
        "intimidate does not protect the leader"
    );
}

fn attack_debuff(card: &str, power: u8) {
    let mut initial = field_position(card);
    initial["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"ally","card":"BP01-037"}));
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    let mut game = game(&initial);
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"P2.leader"}))
            .unwrap()
            .outcome,
        "resolved",
        "the printed card can attack the leader"
    );
    trigger(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        state["objects"]["enemy"]["power"], 4_u8,
        "the opposing follower loses one power"
    );
    assert_eq!(
        state["objects"]["enemy"]["hp"], 4_u8,
        "the opposing follower loses one health"
    );
    assert_eq!(
        state["objects"]["ally"]["power"], 5_u8,
        "an own follower is not debuffed"
    );
    assert_eq!(
        state["P2"]["leader"]["life"], 20_u8,
        "the trigger resolves before combat"
    );
    game.resume_internal_card().unwrap();
    pass_quick(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
        20_u8.checked_sub(power).unwrap(),
        "the leader then receives combat damage"
    );
}

fn storm_attack_draw(card: &str, power: u8) {
    let mut plain = field_position("BP01-037");
    plain["players"]["P1"]["zones"]["field"][0]["state"] = json!({"entered_this_turn":true});
    let mut denied = game(&plain);
    assert_eq!(
        denied
            .internal_rule_action(&json!({"do":"attack","attacker":"source","target":"P2.leader"}))
            .unwrap()
            .outcome,
        "cannot-attack",
        "a plain follower cannot attack on entry"
    );
    let mut initial = field_position(card);
    initial["players"]["P1"]["zones"]["field"][0]["state"] = json!({"entered_this_turn":true});
    let mut game = game(&initial);
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"P2.leader"}))
            .unwrap()
            .outcome,
        "resolved",
        "storm permits attacking on entry"
    );
    trigger(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        state["P1"]["hand"],
        json!(["P1-first"]),
        "the attack draws from its controller's deck"
    );
    assert_eq!(
        state["P2"]["deck"],
        json!(["P2-first", "P2-second", "P2-third"]),
        "the opponent deck is unchanged"
    );
    game.resume_internal_card().unwrap();
    pass_quick(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
        20_u8.checked_sub(power).unwrap(),
        "combat follows the draw trigger"
    );
}

#[test]
fn bp19_013_ward_and_fanfare_damage_only_the_opposing_follower() {
    ward("BP19-013");
    let mut initial = position("BP19-013");
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    let mut game = game(&initial);
    play(&mut game);
    trigger(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
        3_u8
    );
}

#[test]
fn bp19_014_ward_and_real_evolution_damage_an_opposing_follower() {
    ward("BP19-014");
    let mut initial = field_position("BP19-013");
    initial["players"]["P1"]["pp"]["current"] = json!(1_u8);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP19-014"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    let mut game = game(&initial);
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
        )
        .unwrap()
        .outcome,
        "resolved"
    );
    trigger(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["damage"], 3_u8);
    assert_eq!(state["P1"]["pp"]["current"], 0_u8);
}

#[test]
fn bp19_015_rush_and_fanfare_draw_two() {
    immediate_attack("BP19-015", "rush", 4);
    let mut game = game(&position("BP19-015"));
    play(&mut game);
    trigger(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["P1-first", "P1-second"]));
    assert_eq!(state["P1"]["deck"], json!(["P1-third"]));
}

#[test]
fn bp19_032_storm_attack_draws_before_combat() {
    storm_attack_draw("BP19-032", 2);
}

#[test]
fn bp19_033_storm_draw_and_real_evolution_hit_only_opposing_followers() {
    storm_attack_draw("BP19-033", 3);
    let mut initial = field_position("BP19-032");
    initial["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"ally","card":"BP01-037"}));
    initial["players"]["P1"]["pp"]["current"] = json!(1_u8);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP19-033"}]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy-a","card":"BP01-037"},{"id":"enemy-b","card":"BP01-037"}]);
    let mut game = game(&initial);
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
        )
        .unwrap()
        .outcome,
        "resolved"
    );
    trigger(&mut game);
    let state = game.projection(View::Referee).unwrap();
    for id in ["enemy-a", "enemy-b"] {
        assert_eq!(state["objects"][id]["damage"], 1_u8);
    }
    assert_eq!(state["objects"]["ally"]["damage"], 0_u8);
    assert_eq!(state["P2"]["leader"]["life"], 20_u8);
}

#[test]
fn bp19_068_intimidate_and_attack_debuff_an_opponent() {
    intimidate("BP19-068");
    attack_debuff("BP19-068", 3);
}

#[test]
fn bp19_069_intimidate_attack_debuff_and_real_evolution_act_target() {
    intimidate("BP19-069");
    attack_debuff("BP19-069", 4);
    let mut initial = field_position("BP19-068");
    initial["players"]["P1"]["pp"]["current"] = json!(1_u8);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP19-069"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    let mut game = game(&initial);
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
        )
        .unwrap()
        .outcome,
        "resolved"
    );
    trigger(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["acted"], true);
    assert_eq!(state["objects"]["enemy"]["power"], 5_u8);
}

#[test]
fn bp19_076_storm_attacks_the_leader_on_entry() {
    for print in ["BP19-076", "BP19-SL20"] {
        immediate_attack(print, "storm", 6);
    }
}

#[test]
fn bp19_083_lastword_mills_after_an_opponent_spell_destroys_it() {
    let mut initial = field_position("BP19-083");
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
fn bp19_093_fanfare_draws_two_for_the_printed_controller() {
    for print in ["BP19-093", "BP19-SL23"] {
        let mut game = game(&position(print));
        play(&mut game);
        trigger(&mut game);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["hand"], json!(["P1-first", "P1-second"]));
        assert_eq!(state["P1"]["deck"], json!(["P1-third"]));
    }
}

#[test]
fn bp19_117_ward_and_fanfare_draw_one() {
    ward("BP19-117");
    let mut game = game(&position("BP19-117"));
    play(&mut game);
    trigger(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["P1-first"]));
    assert_eq!(state["P1"]["deck"], json!(["P1-second", "P1-third"]));
}

#[test]
fn bp15_120_real_evolution_draws_two() {
    let mut initial = field_position("BP15-119");
    initial["players"]["P1"]["pp"]["current"] = json!(1_u8);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP15-120"}]);
    let mut game = game(&initial);
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
        )
        .unwrap()
        .outcome,
        "resolved"
    );
    trigger(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["pp"]["current"], 0_u8);
    assert_eq!(state["P1"]["hand"], json!(["P1-first", "P1-second"]));
    assert_eq!(state["P1"]["deck"], json!(["P1-third"]));
    assert_eq!(
        state["P2"]["deck"],
        json!(["P2-first", "P2-second", "P2-third"])
    );
}

#[test]
fn bp18_092_fanfare_creates_one_admitted_forest_bat_in_ex() {
    let mut game = game(&position("BP18-092"));
    play(&mut game);
    trigger(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        count_named(&state, "ex", "フォレストバット"),
        1,
        "exactly one forest bat enters its controller EX"
    );
    assert_eq!(state["P2"]["ex"], json!([]), "the opponent EX is unchanged");
}
