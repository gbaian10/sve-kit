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

fn play_follower(game: &mut Game, object: &str) {
    let play = game.play_internal_card(object, actor()).unwrap();
    assert!(
        matches!(finish(game, play), CardPlay::Entered(_)),
        "the follower enters before its fanfare is resolved"
    );
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

#[test]
fn bp02_042_evolves_for_one_pp() {
    let mut initial = position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"base","card":"BP02-042"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP02-043"}]);
    let mut game = game(&initial);

    evolve(&mut game, "base", "form", 1);

    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        9_u8,
        "one PP is paid for evolution"
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["base"]["evolved"],
        true,
        "the object enters its evolved state"
    );
}

#[test]
fn bp02_042_rejects_evolution_without_the_printed_pp() {
    let mut initial = position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"base","card":"BP02-042"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP02-043"}]);
    let mut game = game(&initial);

    assert_ne!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"base","evolve_card":"form","pay":{"pp":0_u8}})
        )
        .unwrap()
        .outcome,
        "resolved",
        "evolution without the printed one PP is rejected"
    );
}

#[test]
fn bp02_110_enters_with_ward() {
    let mut initial = position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"angel","card":"BP02-110"}]);
    let mut game = game(&initial);

    play_follower(&mut game, "angel");

    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["angel"]["keywords"],
        json!(["守護"]),
        "BP02-110 grants Ward on entry"
    );
}

#[test]
fn bp02_118_evolves_for_three_pp() {
    let mut initial = position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"base","card":"BP02-118"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP02-119"}]);
    initial["players"]["P1"]["pp"]["current"] = json!(3_u8);
    let mut game = game(&initial);

    evolve(&mut game, "base", "form", 3);

    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        0_u8,
        "the printed evolution cost consumes three PP"
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["base"]["evolved"],
        true,
        "the object enters its evolved state"
    );
}

#[test]
fn bp02_119_evolved_face_has_ward() {
    let mut initial = position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"evolved","card":"BP02-119"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"attacker","card":"BP01-037"}]);
    let mut game = game(&initial);

    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["evolved"]["keywords"],
        json!(["守護"]),
        "the evolved printing retains its Ward keyword"
    );
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"attack","attacker":"attacker","target":"P1.leader"})
        )
        .unwrap()
        .outcome,
        "cannot-attack",
        "the evolved Ward protects its leader"
    );
}

#[test]
fn bp02_t06_rush_attacks_a_follower_the_turn_it_enters() {
    let mut initial = position();
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"dragon","card":"BP02-T06","state":{"entered_this_turn":true}}]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"target","card":"BP01-037","state":{"acted":true}}]);
    let mut game = game(&initial);

    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"dragon","target":"P2.leader"}))
            .unwrap()
            .outcome,
        "cannot-attack",
        "Rush does not permit attacking a leader on entry"
    );
    let attack = game
        .internal_rule_action(&json!({"do":"attack","attacker":"dragon","target":"target"}))
        .unwrap();

    assert_eq!(
        attack.outcome, "resolved",
        "Rush permits attacking a follower on entry"
    );
}

#[test]
fn bp02_046_fanfare_draws_then_discards_one_card() {
    let mut initial = position();
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"wizard","card":"BP02-046"},
        {"id":"kept","card":"BP01-037"}
    ]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"drawn","card":"BP01-037"}]);
    let mut game = game(&initial);

    play_follower(&mut game, "wizard");
    let triggered = trigger(&mut game);
    let result = finish(&mut game, triggered);
    assert!(
        matches!(result, CardPlay::Resolved(_)),
        "the fanfare completes after choosing one discard"
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        state["P1"]["hand"].as_array().unwrap().len(),
        1,
        "one card remains after drawing and discarding"
    );
    assert_eq!(
        state["P1"]["cemetery"].as_array().unwrap().len(),
        1,
        "one hand card is discarded"
    );
    assert_eq!(
        state["P1"]["deck"],
        json!([]),
        "the top card is drawn before the discard"
    );
}

#[test]
fn bp02_047_evolve_trigger_draws_then_discards_one_card() {
    let mut initial = position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"base","card":"BP02-046"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP02-047"}]);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"kept","card":"BP01-037"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"drawn","card":"BP01-037"}]);
    let mut game = game(&initial);

    evolve(&mut game, "base", "form", 1);
    let triggered = trigger(&mut game);
    let result = finish(&mut game, triggered);
    assert!(
        matches!(result, CardPlay::Resolved(_)),
        "the evolve trigger completes after one discard"
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        state["P1"]["hand"].as_array().unwrap().len(),
        1,
        "one card remains after drawing and discarding"
    );
    assert_eq!(
        state["P1"]["cemetery"].as_array().unwrap().len(),
        1,
        "one hand card is discarded"
    );
    assert_eq!(
        state["P1"]["deck"],
        json!([]),
        "the top card is drawn before the discard"
    );
}

#[test]
fn bp02_057_spell_adds_max_pp_heals_and_draws() {
    let mut initial = position();
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    initial["players"]["P1"]["pp"] = json!({"current":9_u8,"max":9_u8});
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"spell","card":"BP02-057"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"drawn","card":"BP01-037"}]);
    let mut game = game(&initial);

    let result = resolved(game.play_internal_card("spell", actor()).unwrap());
    game.resume_internal_card().unwrap();

    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        state["P1"]["pp"]["max"], 10_u8,
        "the spell raises maximum PP by one"
    );
    assert_eq!(
        state["P1"]["pp"]["current"], 4_u8,
        "increasing maximum PP does not restore the five PP paid for the spell"
    );
    assert_eq!(
        state["P1"]["leader"]["life"], 13_u8,
        "the spell restores three life"
    );
    assert_eq!(
        state["P1"]["hand"],
        json!(["drawn"]),
        "the spell draws one card"
    );
    assert!(
        !result.progress.receipts.is_empty(),
        "the spell records completed effects"
    );
}
