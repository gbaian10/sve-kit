#![expect(
    clippy::panic,
    reason = "Unexpected choices fail these game scenarios."
)]

use super::*;

const fn actor() -> PlayerId {
    PlayerId { seat: 1 }
}

fn position(card: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"drawn","card":"BP01-037"},{"id":"next","card":"BP01-037"}
    ]);
    initial
}

fn finish(game: &mut Game, mut play: CardPlay) {
    while let CardPlay::Awaiting(question) = play {
        let answer = if matches!(&question.domain, InputDomain::WardEntry { .. }) {
            Answer::WardEntry(false)
        } else {
            let InputDomain::Targets(domain) = &question.domain else {
                panic!("expected a field target");
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
    let entered = game.play_internal_card("source", actor()).unwrap();
    finish(game, entered);
}

fn trigger(game: &mut Game) {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("pending trigger")
    };
    assert_eq!(
        options.len(),
        1,
        "only the migrated card trigger is pending"
    );
    let play = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap();
    finish(game, play);
}

fn evolve_position(base: &str, form: &str) -> Value {
    let mut initial = position(base);
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":base}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
    initial
}

fn evolves_at_printed_cost(base: &str, form: &str, cost: u8) {
    let mut initial = evolve_position(base, form);
    initial["players"]["P1"]["pp"]["current"] = json!(cost);
    let mut game = game(&initial);
    let step = game
        .internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":cost}}),
        )
        .unwrap();
    assert_eq!(
        step.outcome, "resolved",
        "evolution succeeds with exactly the printed PP"
    );
    let event = step
        .events
        .iter()
        .find(|event| event["kind"] == "進化")
        .unwrap();
    assert_eq!(event["object"], "source", "the base follower evolves");
    assert_eq!(event["with"], "form", "the matching resource is consumed");
    let mut insufficient = evolve_position(base, form);
    insufficient["players"]["P1"]["pp"]["current"] = json!(0_u8);
    if cost > 0 {
        let mut rejected = super::game(&insufficient);
        assert_ne!(
            rejected
                .internal_rule_action(
                    &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":cost}})
                )
                .unwrap()
                .outcome,
            "resolved",
            "underpayment cannot evolve"
        );
    }
}

#[test]
fn bp01_122_evolves_for_zero_pp() {
    evolves_at_printed_cost("BP01-122", "BP01-123", 0);
}

#[test]
fn bp01_135_evolves_for_two_pp() {
    evolves_at_printed_cost("BP01-135", "BP01-136", 2);
}

fn ward_protects_leader(card: &str) {
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
        "a rested ward protects its leader"
    );
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"attacker","target":"ward"}))
            .unwrap()
            .outcome,
        "resolved",
        "the ward itself remains attackable"
    );
}

fn aura_prevents_enemy_targeting(card: &str) {
    let mut initial = position("BP01-169");
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"protected","card":card}]);
    let mut game = game(&initial);
    assert!(
        game.play_internal_card("source", actor()).is_err(),
        "aura leaves no legal enemy spell target"
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        10_u8,
        "an illegal target does not spend PP"
    );
}

#[test]
fn bp01_138_ward_and_aura_protect_against_attacks_and_targeted_spells() {
    ward_protects_leader("BP01-138");
    aura_prevents_enemy_targeting("BP01-138");
}

#[test]
fn bp01_145_aura_prevents_targeted_destruction() {
    aura_prevents_enemy_targeting("BP01-145");
}

#[test]
fn bp01_165_ward_protects_leader() {
    ward_protects_leader("BP01-165");
}

#[test]
fn bp01_t04_ward_token_protects_leader() {
    ward_protects_leader("BP01-T04");
}

#[test]
fn bp01_t06_storm_token_attacks_leader_on_entry_turn() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"source","card":"BP01-T06","state":{"entered_this_turn":true}}]);
    let mut game = game(&initial);
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"P2.leader"}))
            .unwrap()
            .outcome,
        "resolved",
        "storm permits attacking on entry"
    );
    let question = game.internal_quick_choice().unwrap().unwrap();
    game.answer_internal_card(question.actor, question.token, Answer::Quick(None))
        .unwrap();
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
        17_u8
    );
}

#[test]
fn bp01_086_fanfare_draws_two_then_discards_one() {
    let mut game = game(&position("BP01-086"));
    play(&mut game);
    trigger(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["next"]));
    assert_eq!(state["P1"]["cemetery"], json!(["drawn"]));
    assert_eq!(state["P1"]["deck"], json!([]));
}

#[test]
fn bp01_087_evolution_heals_five() {
    let mut game = game(&evolve_position("BP01-086", "BP01-087"));
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":2_u8}})
        )
        .unwrap()
        .outcome,
        "resolved",
        "Shenlong evolves normally"
    );
    trigger(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        25_u8
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

#[test]
fn bp01_026_fanfare_creates_a_ward_token() {
    let mut game = game(&position("BP01-026"));
    play(&mut game);
    trigger(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        count_named(&state, "field", "乙姫お守り隊"),
        1,
        "one guard enters the field"
    );
}

#[test]
fn bp01_050_damages_enemy_and_creates_knight_in_ex() {
    let mut initial = position("BP01-050");
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    let mut game = game(&initial);
    play(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["damage"], 5_u8);
    assert_eq!(
        count_named(&state, "ex", "ナイト"),
        1,
        "one knight enters EX"
    );
}
