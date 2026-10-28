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
fn bp14_028_fanfare_damages_then_draws() {
    let mut initial = position("BP14-028");
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    let mut game = game(&initial);
    play(&mut game);
    trigger(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["damage"], 4_u8);
    assert_eq!(state["P1"]["hand"], json!(["P1-first"]));
    assert_eq!(state["P1"]["deck"], json!(["P1-second", "P1-third"]));
}

#[test]
fn bp14_086_quick_destroys_target_and_damages_its_own_leader() {
    let mut initial = position("BP14-086");
    initial["turn"]["active"] = json!("P2");
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    let mut game = game(&initial);
    assert_eq!(
        game.internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap()
            .outcome,
        "resolved"
    );
    let quick = game.internal_quick_choice().unwrap().unwrap();
    let InputDomain::Quick { options, .. } = &quick.domain else {
        panic!("expected Quick");
    };
    assert_eq!(options.len(), 1);
    let result = game
        .answer_internal_card(quick.actor, quick.token, Answer::Quick(Some(0)))
        .unwrap();
    finish(&mut game, result, true);
    game.resume_internal_card().unwrap();
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["cemetery"], json!(["enemy"]));
    assert_eq!(state["P1"]["leader"]["life"], 17_u8);
    assert_eq!(state["P2"]["leader"]["life"], 20_u8);
}

#[test]
fn bp14_016_assail_attack_damages_opponent_and_buffs_before_combat() {
    assail("BP14-016");
    let mut game = game(&field_position("BP14-016"));
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"P2.leader"}))
            .unwrap()
            .outcome,
        "resolved"
    );
    trigger(&mut game);
    let triggered = game.projection(View::Referee).unwrap();
    assert_eq!(triggered["P1"]["leader"]["life"], 20_u8);
    assert_eq!(triggered["P2"]["leader"]["life"], 17_u8);
    assert_eq!(triggered["objects"]["source"]["power"], 5_u8);
    assert_eq!(triggered["objects"]["source"]["hp"], 5_u8);
    game.resume_internal_card().unwrap();
    pass_quick(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
        12_u8,
        "the attack uses its buffed power"
    );
}

#[test]
fn bp14_058_always_selects_but_damages_only_when_awakened() {
    for max in [6_u8, 7] {
        let mut initial = position("BP14-058");
        initial["players"]["P1"]["pp"] = json!({"current":max,"max":max});
        initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
        let mut game = game(&initial);
        play(&mut game);
        trigger(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
            if max == 7 { 3_u8 } else { 0_u8 }
        );
    }
}

#[test]
fn bp14_p29_ward_and_lastword_damage_opposing_follower() {
    for print in ["BP14-P29", "BP14-T01"] {
        ward(print);
    }
    let mut initial = field_position("BP14-P29");
    initial["turn"]["active"] = json!("P2");
    initial["players"]["P2"]["zones"]["hand"] = json!([{"id":"removal","card":"BP01-169"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    let mut game = game(&initial);
    let result = game
        .play_internal_card("removal", PlayerId { seat: 2 })
        .unwrap();
    finish(&mut game, result, true);
    game.resume_internal_card().unwrap();
    trigger(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
        2_u8
    );
}

#[test]
fn bp14_110_quick_heals_and_draws_for_its_controller() {
    let mut initial = position("BP14-110");
    initial["turn"]["active"] = json!("P2");
    let mut game = game(&initial);
    assert_eq!(
        game.internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap()
            .outcome,
        "resolved"
    );
    let quick = game.internal_quick_choice().unwrap().unwrap();
    let InputDomain::Quick { options, .. } = &quick.domain else {
        panic!("expected Quick");
    };
    assert_eq!(options.len(), 1);
    let result = game
        .answer_internal_card(quick.actor, quick.token, Answer::Quick(Some(0)))
        .unwrap();
    finish(&mut game, result, true);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["leader"]["life"], 22_u8);
    assert_eq!(state["P2"]["leader"]["life"], 20_u8);
    assert_eq!(state["P1"]["hand"], json!(["P1-first", "P1-second"]));
    assert_eq!(state["P1"]["deck"], json!(["P1-third"]));
    assert_eq!(
        state["P2"]["deck"],
        json!(["P2-first", "P2-second", "P2-third"])
    );
}

#[test]
fn bp14_t06_ward_protects_its_leader() {
    for print in ["BP14-T06", "PR-377"] {
        ward(print);
    }
}

#[test]
fn bp14_033_fanfare_acts_only_the_opposing_follower() {
    let mut initial = position("BP14-033");
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    let mut game = game(&initial);
    play(&mut game);
    trigger(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["acted"], true);
    assert_eq!(state["objects"]["source"]["acted"], false);
}

#[test]
fn bp14_020_bane_destroys_a_larger_defender() {
    for print in ["BP14-020", "BP14-SL06"] {
        let mut initial = field_position(print);
        initial["players"]["P2"]["zones"]["field"] =
            json!([{"id":"enemy","card":"BP01-037","state":{"acted":true}}]);
        let mut game = game(&initial);
        assert_eq!(
            game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"enemy"}))
                .unwrap()
                .outcome,
            "resolved"
        );
        pass_quick(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P2"]["cemetery"],
            json!(["enemy"])
        );
    }
}

#[test]
fn bp14_060_evolves_for_one_pp() {
    for print in ["BP14-060", "BP14-P14"] {
        evolve(print, "BP14-061", 1);
    }
}

#[test]
fn bp14_064_evolves_for_two_pp() {
    evolve("BP14-064", "BP14-065", 2);
}

#[test]
fn bp14_078_evolves_for_one_pp() {
    for print in ["BP14-078", "BP14-P17"] {
        evolve(print, "BP14-079", 1);
    }
}

#[test]
fn bp14_017_quick_spell_draws_before_discarding() {
    let mut initial = position("BP14-017");
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
            .contains(&json!("P1-first"))
    );
}

#[test]
fn bp14_082_evolves_for_five_and_fanfare_damages_then_discards() {
    evolve("BP14-082", "BP14-083", 5);
    let mut initial = position("BP14-082");
    initial["players"]["P1"]["zones"]["hand"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"discarded","card":"BP01-037"}));
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    let mut game = game(&initial);
    play(&mut game);
    trigger(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["damage"], 2_u8);
    assert_eq!(state["P1"]["hand"], json!([]));
    assert_eq!(state["P1"]["cemetery"], json!(["discarded"]));
    assert_eq!(
        state["P1"]["deck"],
        json!(["P1-first", "P1-second", "P1-third"])
    );
}

#[test]
fn bp14_012_fanfare_creates_one_admitted_puppet_in_ex() {
    let mut game = game(&position("BP14-012"));
    play(&mut game);
    trigger(&mut game);
    assert_eq!(
        count_named(&game.projection(View::Referee).unwrap(), "ex", "操り人形"),
        1
    );
}
