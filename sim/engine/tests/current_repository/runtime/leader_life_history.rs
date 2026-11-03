#![expect(
    clippy::panic,
    reason = "Unexpected questions invalidate printed life scenarios."
)]
use super::event_combat_life::life;
use super::*;

fn finish(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}
fn with_changes(initial: &Value, deltas: &[i64]) -> Game {
    let mut initial = initial.clone();
    initial["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"producer","card":"producer"}));
    let mut game = Game::from_selected_effects(
        producer_selection().clone(),
        &initial,
        &Value::Null,
        &Value::Null,
        "life-history",
    )
    .unwrap();
    let program = producer_steps(
        deltas
            .iter()
            .map(|&delta| life(PlayerRef::Controller {}, delta))
            .collect(),
    );
    game.load_internal_card("producer", &program).unwrap();
    activate(&mut game, "P1");
    finish(&mut game);
    game
}
fn resolve_trigger(game: &mut Game, accept: bool) {
    finish(game);
    let before = game.projection(View::Referee).unwrap()["P1"]["leader"]["life"].clone();
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("pending trigger")
    };
    let mut play = game
        .answer_internal_card(
            pending.actor,
            pending.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap();
    while let CardPlay::Awaiting(question) = play {
        let answer = match &question.domain {
            InputDomain::OptionalCost => Answer::OptionalCost(accept),
            InputDomain::Targets(domain) => {
                assert_eq!(
                    game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
                    before,
                    "formal targets are chosen before the life payment commits"
                );
                Answer::Targets(vec![domain.members[0].handle])
            }
            InputDomain::ResourceEntryPayment { .. }
            | InputDomain::WardEntry { .. }
            | InputDomain::Quick { .. }
            | InputDomain::CostMaterials(_)
            | InputDomain::BeforeDraw
            | InputDomain::Pending(_)
            | InputDomain::TriggerQuota(_)
            | InputDomain::Integer(_)
            | InputDomain::Order(_)
            | InputDomain::DamageReplacements(_)
            | InputDomain::DriveTrigger { .. }
            | InputDomain::Search(_)
            | InputDomain::SearchPartition(_)
            | InputDomain::Distribution(_)
            | InputDomain::SearchReplacement
            | InputDomain::TokenCapacity { .. }
            | InputDomain::Modes { .. } => panic!("printed life cost or target"),
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    assert!(
        matches!(play, CardPlay::Resolved(_) | CardPlay::Cancelled),
        "the selected trigger finishes or declines"
    );
    finish(game);
}

#[test]
fn bp05_069_attack_counts_decreases_instead_of_lost_life_and_checks_seven_afterwards() {
    let mut initial = draw_position();
    initial["players"]["P1"]["leader"]["life"] = json!(10_i64);
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"source","card":"BP05-069","state":{"entered_this_turn":false}}]);
    initial["players"]["P2"]["zones"]["field"] = json!([]);
    let mut game = with_changes(&initial, &[-1, -2, 0]);
    game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"P2.leader"}))
        .unwrap();
    resolve_trigger(&mut game, true);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["leader"]["life"], 18_i64);
    assert_eq!(state["objects"]["source"]["power"], 4_i64);
    assert_eq!(state["objects"]["source"]["hp"], 4_i64);
}

#[test]
fn bp05_081_evolution_can_decline_life_cost_and_counts_its_own_payment() {
    for accept in [false, true] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP05-080"}]);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP05-081"}]);
        initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-171"}]);
        let mut game = with_changes(&initial, &[-3]);
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_i64}}),
        )
        .unwrap();
        resolve_trigger(&mut game, accept);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P1"]["leader"]["life"],
            if accept { 16_i64 } else { 17_i64 }
        );
        assert_eq!(
            state["P2"]["field"]
                .as_array()
                .unwrap()
                .contains(&json!("enemy")),
            !accept
        );
    }
}

#[test]
fn sp01_039_requires_four_total_healing_even_when_damage_undoes_it() {
    for healing in [3, 4] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"SP01-039"}]);
        initial["players"]["P1"]["zones"]["field"] = json!([]);
        initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"drawn","card":"BP01-171"}]);
        initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-171"}]);
        let mut game = with_changes(&initial, &[1, healing - 1, -healing]);
        game.play_internal_card("source", PlayerId { seat: 1 })
            .unwrap();
        resolve_trigger(&mut game, true);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["leader"]["life"], 20_i64);
        assert_eq!(
            state["P2"]["field"].as_array().unwrap().len(),
            usize::from(healing < 4)
        );
        assert_eq!(
            state["P1"]["hand"].as_array().unwrap().len(),
            usize::from(healing == 4)
        );
    }
}
