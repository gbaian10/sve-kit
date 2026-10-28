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
fn bp20_110_spell_damages_all_opponents_heals_and_draws() {
    for print in ["BP20-110", "BP20-P54"] {
        let mut initial = position(print);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"ally","card":"BP01-037"}]);
        initial["players"]["P2"]["zones"]["field"] =
            json!([{"id":"enemy-a","card":"BP01-037"},{"id":"enemy-b","card":"BP01-037"}]);
        let mut game = game(&initial);
        play(&mut game);
        let state = game.projection(View::Referee).unwrap();
        for id in ["enemy-a", "enemy-b"] {
            assert_eq!(state["objects"][id]["damage"], 4_u8);
        }
        assert_eq!(state["objects"]["ally"]["damage"], 0_u8);
        assert_eq!(state["P1"]["leader"]["life"], 22_u8);
        assert_eq!(state["P2"]["leader"]["life"], 20_u8);
        assert_eq!(state["P1"]["hand"], json!(["P1-first"]));
    }
}

#[test]
fn bp20_017_storm_attack_damages_an_opposing_follower_before_combat() {
    let mut control = field_position("BP01-037");
    control["players"]["P1"]["zones"]["field"][0]["state"] = json!({"entered_this_turn":true});
    let mut denied = game(&control);
    assert_eq!(
        denied
            .internal_rule_action(&json!({"do":"attack","attacker":"source","target":"P2.leader"}))
            .unwrap()
            .outcome,
        "cannot-attack",
        "a plain follower cannot attack on entry"
    );
    for print in ["BP20-017", "BP20-P11"] {
        let mut initial = field_position(print);
        initial["players"]["P1"]["zones"]["field"][0]["state"] = json!({"entered_this_turn":true});
        initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
        let mut game = game(&initial);
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"attack","attacker":"source","target":"P2.leader"})
            )
            .unwrap()
            .outcome,
            "resolved",
            "storm permits an entry-turn leader attack"
        );
        trigger(&mut game);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["enemy"]["damage"], 4_u8);
        assert_eq!(state["P2"]["leader"]["life"], 20_u8);
        game.resume_internal_card().unwrap();
        pass_quick(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
            16_u8
        );
    }
}

#[test]
fn bp20_065_ward_protects_its_leader() {
    for print in ["BP20-065", "BP20-P34"] {
        ward(print);
    }
}

#[test]
fn bp20_068_fanfare_deals_one_to_an_opposing_follower() {
    for print in ["BP20-068", "BP20-P36"] {
        let mut initial = position(print);
        initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
        let mut game = game(&initial);
        play(&mut game);
        trigger(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
            1_u8
        );
    }
}

#[test]
fn bp20_038_fanfare_draws_one_for_each_print() {
    for print in ["BP20-038", "BP20-SL10"] {
        let mut game = game(&position(print));
        play(&mut game);
        trigger(&mut game);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["hand"], json!(["P1-first"]));
        assert_eq!(state["P1"]["deck"], json!(["P1-second", "P1-third"]));
    }
}

#[test]
fn bp20_106_ward_protects_its_leader() {
    ward("BP20-106");
}

#[test]
fn bp20_023_evolves_for_one_pp_for_each_print() {
    for print in ["BP20-023", "BP20-P13"] {
        evolve(print, "BP20-024", 1);
    }
}

#[test]
fn bp20_027_evolves_for_one_pp() {
    evolve("BP20-027", "BP20-028", 1);
}
