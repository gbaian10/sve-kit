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
    assert!(
        matches!(
            play,
            CardPlay::Resolved(_) | CardPlay::Entered(_) | CardPlay::Continued(_)
        ),
        "the effect completes"
    );
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

fn storm_attack(card: &str, power: u8) {
    let mut initial = field_position(card);
    initial["players"]["P1"]["zones"]["field"][0]["state"] = json!({"entered_this_turn":true});
    let mut game = game(&initial);
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"P2.leader"}))
            .unwrap()
            .outcome,
        "resolved",
        "storm permits an immediate leader attack"
    );
    pass_quick(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
        20_u8.checked_sub(power).unwrap(),
        "storm deals the printed attack power"
    );
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

fn intimidate(card: &str) {
    let mut initial = field_position("BP01-037");
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-037","state":{"acted":true}}]);
    let mut control = game(&initial);
    assert_eq!(
        control
            .internal_rule_action(&json!({"do":"attack","attacker":"source","target":"enemy"}))
            .unwrap()
            .outcome,
        "resolved",
        "an ordinary rested follower is attackable"
    );
    initial["players"]["P2"]["zones"]["field"][0]["card"] = json!(card);
    let mut game = game(&initial);
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"enemy"}))
            .unwrap()
            .outcome,
        "cannot-attack",
        "intimidate prevents follower attacks"
    );
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"P2.leader"}))
            .unwrap()
            .outcome,
        "resolved",
        "intimidate does not protect the leader"
    );
}
#[test]
fn bp16_069_pays_one_pp_to_evolve() {
    evolve("BP16-069", "BP16-070", 1);
}
#[test]
fn bp16_079_pays_one_pp_to_evolve() {
    evolve("BP16-079", "BP16-080", 1);
}
#[test]
fn bp16_083_pays_one_pp_to_evolve() {
    evolve("BP16-083", "BP16-084", 1);
}
#[test]
fn bp16_097_pays_one_pp_to_evolve() {
    evolve("BP16-097", "BP16-098", 1);
}
#[test]
fn bp16_105_evolves_and_ward_protects() {
    evolve("BP16-105", "BP16-106", 1);
    ward("BP16-105");
}
#[test]
fn bp16_114_evolves_and_ward_protects() {
    evolve("BP16-114", "BP16-115", 1);
    ward("BP16-114");
}
#[test]
fn bp16_066_storms_and_intimidates() {
    storm_attack("BP16-066", 3);
    intimidate("BP16-066");
}
#[test]
fn bp16_t03_intimidates_without_protecting_its_leader() {
    intimidate("BP16-T03");
}
#[test]
fn bp16_p32_storm_deals_five_damage_immediately() {
    storm_attack("BP16-P32", 5);
}
#[test]
fn bp16_p31_lastword_heals_and_mills_after_token_disappears() {
    let game = departed("BP16-P31");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["leader"]["life"], 22_u8);
    assert_eq!(state["P2"]["leader"]["life"], 20_u8);
    assert_eq!(state["P1"]["deck"], json!(["P1-second", "P1-third"]));
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
            .contains(&json!("source"))
    );
}
#[test]
fn bp16_012_returning_to_hand_heals_and_restores_pp() {
    let mut initial = field_position("BP16-012");
    initial["players"]["P1"]["pp"]["current"] = json!(4_u8);
    let mut game = with_producer(&initial, movement(OwnerZone::Hand));
    activate(&mut game, "P1");
    game.resume_internal_card().unwrap();
    trigger(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["leader"]["life"], 21_u8);
    assert_eq!(state["P1"]["pp"]["current"], 5_u8);
    assert_eq!(state["P2"]["leader"]["life"], 20_u8);
    assert!(
        state["P1"]["hand"]
            .as_array()
            .unwrap()
            .contains(&json!("source"))
    );
}
#[test]
fn bp16_012_ignores_another_cards_departure() {
    let mut initial = field_position("BP16-012");
    initial["players"]["P1"]["pp"]["current"] = json!(4_u8);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"other","card":"BP01-037"}]);
    let mut objects = subjects();
    let Selector::Zone { players, .. } = &mut objects else {
        panic!("expected field selector");
    };
    players.relation = PlayerRelation::Opponents;
    players.cardinality = Cardinality {
        minimum: Nat(1),
        maximum: Some(Nat(1)),
    };
    let mut game = with_producer(&initial, Action::Destroy { objects });
    activate(&mut game, "P1");
    game.resume_internal_card().unwrap();
    assert!(game.internal_pending_choice().unwrap().is_none());
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["leader"]["life"], 20_u8);
    assert_eq!(state["P1"]["pp"]["current"], 4_u8);
    assert_eq!(state["P2"]["cemetery"], json!(["other"]));
}
#[test]
fn ebd04_t02_alias_uses_the_storm_token_program() {
    storm_attack("EBD04-T02", 5);
}

fn fairy_count(game: &Game, expected: usize) {
    let state = game.projection(View::Referee).unwrap();
    let ex = state["P1"]["ex"].as_array().unwrap();
    assert_eq!(
        ex.len(),
        expected,
        "the printed number of fairies is created"
    );
    for id in ex {
        assert_eq!(
            state["objects"][id.as_str().unwrap()]["card"],
            "BP01-T03",
            "the admitted Fairy template is used"
        );
    }
}
#[test]
fn bp16_001_fanfare_creates_two_fairies_and_evolves_for_one_pp() {
    let mut game = game(&position("BP16-001"));
    play(&mut game);
    trigger(&mut game);
    fairy_count(&game, 2);
    evolve("BP16-001", "BP16-002", 1);
}

#[test]
fn bp16_051_evolution_draws_two_before_discarding_two() {
    let mut initial = field_position("BP16-050");
    initial["players"]["P1"]["pp"]["current"] = json!(1_u8);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"spare","card":"BP01-037"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP16-051"}]);
    let mut game = game(&initial);
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
        )
        .unwrap()
        .outcome,
        "resolved"
    );
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("expected evolution trigger");
    };
    assert_eq!(options.len(), 1, "the evolved form supplies one trigger");
    let play = game
        .answer_internal_card(
            pending.actor,
            pending.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap();
    let CardPlay::Awaiting(question) = play else {
        panic!("expected discard decision");
    };
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("expected hand selection");
    };
    assert_eq!(
        domain.members.len(),
        3,
        "two cards are drawn before discarding"
    );
    let _denied = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Targets(vec![domain.members[0].handle]),
        )
        .err()
        .unwrap();
    let answer = Answer::Targets(
        domain
            .members
            .iter()
            .take(2)
            .map(|member| member.handle)
            .collect(),
    );
    let result = game
        .answer_internal_card(question.actor, question.token, answer)
        .unwrap();
    finish(&mut game, result, true);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"].as_array().unwrap().len(), 1);
    assert_eq!(state["P1"]["cemetery"].as_array().unwrap().len(), 2);
    assert_eq!(state["P1"]["deck"], json!(["P1-third"]));
    assert_eq!(state["P1"]["pp"]["current"], 0_u8);
}

#[test]
fn bp16_105_real_evolution_triggers_the_admitted_106_form() {
    let mut initial = field_position("BP16-105");
    initial["players"]["P1"]["pp"]["current"] = json!(1_u8);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP16-106"}]);
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
    assert_eq!(state["P1"]["leader"]["life"], 20_u8);
    assert_eq!(state["P2"]["leader"]["life"], 16_u8);
    assert_eq!(state["P1"]["pp"]["current"], 0_u8);
}

#[test]
fn bp16_p29_spell_token_targets_draws_discards_and_ceases_to_exist() {
    let mut initial = position("BP16-P29");
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"source","card":"BP16-P29"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    let mut game = game(&initial);
    let play = game.play_internal_card("source", actor()).unwrap();
    let CardPlay::Awaiting(question) = play else {
        panic!("expected Spell token target");
    };
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("expected opposing follower choices");
    };
    assert_eq!(domain.members.len(), 1);
    let before = game.projection(View::Referee).unwrap();
    let _denied = game
        .answer_internal_card(
            PlayerId { seat: 2 },
            question.token,
            Answer::Targets(vec![domain.members[0].handle]),
        )
        .err()
        .unwrap();
    assert_eq!(game.projection(View::Referee).unwrap(), before);
    finish(&mut game, CardPlay::Awaiting(question), true);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["damage"], 2_u8);
    assert_eq!(state["P1"]["pp"]["current"], 10_u8);
    assert_eq!(state["P1"]["hand"], json!([]));
    assert_eq!(state["P1"]["deck"], json!(["P1-second", "P1-third"]));
    assert_eq!(state["P1"]["cemetery"], json!(["P1-first"]));
    assert_eq!(state["P1"]["ex"], json!([]));
    assert!(state["objects"]["source"].is_null());
}
