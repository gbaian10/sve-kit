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

#[test]
fn bp01_178_evolution_heals_two() {
    let mut initial = position("BP01-177");
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP01-177"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP01-178"}]);
    let mut game = game(&initial);
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
        )
        .unwrap()
        .outcome,
        "resolved",
        "the printed card permits this rule action"
    );
    trigger(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        22_u8
    );
}

#[test]
fn bp01_116_sacrifices_own_follower_and_draws_two() {
    let mut initial = position("BP01-116");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"offering","card":"BP01-037"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    let mut game = game(&initial);
    play(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["drawn", "next"]));
    assert_eq!(state["P1"]["field"], json!([]));
    assert_eq!(state["P2"]["field"], json!(["enemy"]));
}

#[test]
fn bp01_049_buffs_only_selected_ally_and_draws() {
    let mut initial = position("BP01-049");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"ally","card":"BP01-037"}]);
    let mut game = game(&initial);
    play(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["ally"]["power"], 6_u8);
    assert_eq!(state["objects"]["ally"]["hp"], 6_u8);
    assert_eq!(state["P1"]["hand"], json!(["drawn"]));
}

#[test]
fn bp01_157_fanfare_then_end_phase_buff_all_allies() {
    let mut initial = position("BP01-157");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"ally","card":"BP01-037"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    let mut game = game(&initial);
    play(&mut game);
    trigger(&mut game);
    game.resume_internal_card().unwrap();
    let before = game.projection(View::Referee).unwrap();
    assert_eq!(before["objects"]["ally"]["power"], 6_u8);
    assert_eq!(
        game.internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap()
            .outcome,
        "resolved",
        "the printed card permits this rule action"
    );
    trigger(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["ally"]["power"], 7_u8);
    assert_eq!(state["objects"]["source"]["power"], 2_u8);
    assert_eq!(state["objects"]["enemy"]["power"], 5_u8);
}

fn enemy_position(card: &str) -> Value {
    let mut initial = position(card);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    initial
}

#[test]
fn bp01_029_fanfare_rests_the_enemy() {
    let mut game = game(&enemy_position("BP01-029"));
    play(&mut game);
    trigger(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["enemy"]["acted"],
        true
    );
}

#[test]
fn bp01_038_fanfare_and_activation_rest_enemies() {
    let mut game = game(&enemy_position("BP01-038"));
    play(&mut game);
    trigger(&mut game);
    game.resume_internal_card().unwrap();
    let entered_state = game.projection(View::Referee).unwrap();
    assert_eq!(entered_state["objects"]["enemy"]["acted"], true);
    assert_eq!(entered_state["objects"]["source"]["acted"], false);
    let mut activated_position = enemy_position("BP01-038");
    activated_position["players"]["P1"]["zones"]["hand"] = json!([]);
    activated_position["players"]["P1"]["zones"]["field"] =
        json!([{ "id":"source", "card":"BP01-038" }]);
    let mut activated_game = super::game(&activated_position);
    let activated = activated_game
        .activate_internal_card("source", AbilityId(4), actor())
        .unwrap();
    finish(&mut activated_game, activated);
    let state = activated_game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["acted"], true);
    assert_eq!(state["objects"]["source"]["acted"], true);
}

fn evolves_at_printed_cost(base: &str, form: &str, cost: u8) {
    let mut initial = position(base);
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":base}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
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
}

#[test]
fn bp01_034_evolves_for_two_pp() {
    evolves_at_printed_cost("BP01-034", "BP01-035", 2);
}

#[test]
fn bp01_092_evolves_for_zero_pp() {
    evolves_at_printed_cost("BP01-092", "BP01-093", 0);
}

#[test]
fn bp01_080_storm_attacks_leader_on_entry_turn() {
    let mut game = game(&position("BP01-080"));
    play(&mut game);
    game.resume_internal_card().unwrap();
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"P2.leader"}))
            .unwrap()
            .outcome,
        "resolved",
        "the printed card permits this rule action"
    );
    let question = game.internal_quick_choice().unwrap().unwrap();
    game.answer_internal_card(question.actor, question.token, Answer::Quick(None))
        .unwrap();
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
        10_u8
    );
}

#[test]
fn bp01_082_assail_attacks_standing_enemy_and_bane_destroys_it() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP01-082"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    let mut game = game(&initial);
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"enemy"}))
            .unwrap()
            .outcome,
        "resolved",
        "the printed card permits this rule action"
    );
    let question = game.internal_quick_choice().unwrap().unwrap();
    game.answer_internal_card(question.actor, question.token, Answer::Quick(None))
        .unwrap();
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["cemetery"],
        json!(["enemy"])
    );
}
