#![expect(
    clippy::panic,
    reason = "Unexpected decisions indicate a broken game fixture."
)]
use super::*;

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
fn bp15_002_ward_protects_its_leader() {
    for print in ["BP15-002", "BP15-SL02", "EBD01-003"] {
        ward(print);
    }
}

#[test]
fn bp15_010_evolves_for_one_pp() {
    evolve("BP15-010", "BP15-011", 1);
}

#[test]
fn bp15_033_ward_protects_its_leader() {
    ward("BP15-033");
}

#[test]
fn bp15_051_evolves_for_one_pp() {
    evolve("BP15-051", "BP15-052", 1);
}

#[test]
fn bp15_070_evolves_for_three_pp() {
    evolve("BP15-070", "BP15-071", 3);
}

#[test]
fn bp15_119_evolves_for_one_pp() {
    evolve("BP15-119", "BP15-120", 1);
}

#[test]
fn bp15_123_evolves_for_one_pp() {
    evolve("BP15-123", "BP15-124", 1);
}

#[test]
fn bp15_p32_rush_and_assail_permit_their_printed_attacks() {
    for print in ["BP15-P32", "BP15-T04", "BP20-T17", "EBD03-T03"] {
        immediate_attack(print, "rush", 2);
        assail(print);
    }
}

#[test]
fn bp15_093_quick_destroys_opponent_and_mills_for_controller() {
    let mut initial = position("BP15-093");
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
    assert_eq!(options.len(), 1, "only this Quick spell is available");
    let result = game
        .answer_internal_card(quick.actor, quick.token, Answer::Quick(Some(0)))
        .unwrap();
    finish(&mut game, result, true);
    game.resume_internal_card().unwrap();
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["cemetery"], json!(["enemy"]));
    assert_eq!(state["P1"]["deck"], json!(["P1-third"]));
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
    assert!(
        state["P1"]["cemetery"]
            .as_array()
            .unwrap()
            .contains(&json!("P1-second"))
    );
}

#[test]
fn bp15_088_lastword_mills_for_original_controller_on_opponent_turn() {
    for print in ["BP15-088", "EBD03-019"] {
        let mut initial = field_position(print);
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
        assert!(
            !state["P1"]["cemetery"]
                .as_array()
                .unwrap()
                .contains(&json!("P1-second"))
        );
    }
}
