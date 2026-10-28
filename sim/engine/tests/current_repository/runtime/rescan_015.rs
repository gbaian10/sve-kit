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

fn departed(card: &str) -> Game {
    let mut game = with_producer(
        &field_position(card),
        Action::Destroy {
            objects: subjects(),
        },
    );
    activate(&mut game, "P1");
    game.resume_internal_card().unwrap();
    trigger(&mut game);
    game
}

#[test]
fn bp10_t01_ward_and_lastword_protect_and_damage() {
    ward("BP10-T01");
    let game = departed("BP10-T01");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["leader"]["life"], 20_u8);
    assert_eq!(state["P2"]["leader"]["life"], 16_u8);
}

#[test]
fn bp10_089_fanfare_damages_only_its_controller() {
    let mut entry = game(&position("BP10-089"));
    play(&mut entry);
    trigger(&mut entry);
    let fanfare_state = entry.projection(View::Referee).unwrap();
    assert_eq!(fanfare_state["P1"]["leader"]["life"], 19_u8);
    assert_eq!(fanfare_state["P2"]["leader"]["life"], 20_u8);
    let mut initial = field_position("BP10-089");
    initial["turn"]["active"] = json!("P2");
    initial["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"ally","card":"BP01-037"}));
    initial["players"]["P2"]["zones"]["hand"] = json!([{"id":"removal","card":"BP01-169"}]);
    let mut death = game(&initial);
    let attempt = death
        .play_internal_card("removal", PlayerId { seat: 2 })
        .unwrap();
    let CardPlay::Awaiting(choice) = attempt else {
        panic!("expected removal target");
    };
    let InputDomain::Targets(domain) = &choice.domain else {
        panic!("expected target domain");
    };
    let source = domain
        .members
        .iter()
        .find(|member| member.card.as_deref() == Some("BP10-089"))
        .unwrap();
    let play = death
        .answer_internal_card(
            choice.actor,
            choice.token,
            Answer::Targets(vec![source.handle]),
        )
        .unwrap();
    finish(&mut death, play, true);
    death.resume_internal_card().unwrap();
    trigger(&mut death);
    let state = death.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["cemetery"], json!(["source"]));
    assert_eq!(state["objects"]["ally"]["power"], 6_u8);
    assert_eq!(state["objects"]["ally"]["hp"], 6_u8);
}

#[test]
fn bp10_119_fanfare_damages_opposing_follower() {
    let mut initial = position("BP10-119");
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
fn bp10_032_lastword_draws_for_its_controller() {
    let game = departed("BP10-032");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["P1-first"]));
    assert_eq!(state["P2"]["hand"], json!([]));
}

#[test]
fn bp10_048_fanfare_draws_one_card() {
    let mut game = game(&position("BP10-048"));
    play(&mut game);
    trigger(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["P1-first"]));
    assert_eq!(state["P1"]["deck"], json!(["P1-second", "P1-third"]));
}

#[test]
fn bp10_028_storm_attacks_leader_on_entry() {
    immediate_attack("BP10-028", "storm", 4);
}

#[test]
fn bp10_068_evolves_for_one_pp() {
    evolve("BP10-068", "BP10-069", 1);
}

#[test]
fn bp10_086_evolves_for_one_pp() {
    evolve("BP10-086", "BP10-087", 1);
}

#[test]
fn bp10_095_evolves_for_one_pp() {
    evolve("BP10-095", "BP10-096", 1);
}
