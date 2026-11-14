#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate the real-card scenarios."
)]
use super::*;

const FUSION: &str = "融合カウンター";

fn loaded(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "counter-damage-modifier",
    )
    .unwrap()
}

/// BP20-020 stands with `counters` fusion counters and BP20-021 waits in the evolve deck.
fn position(counters: u8) -> Value {
    let mut initial = draw_position();
    initial["turn"]["elapsed_turns"] = json!({"P1":7_u8,"P2":6_u8});
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"source","card":"BP20-020","state":{"counters":{FUSION: counters}}}
    ]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP20-021"}]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-037","state":{"hp":20_u8,"max_hp":20_u8}}]);
    initial
}

/// Picks the opposing follower and resolves every pending ability in order.
fn settle(game: &mut Game, mut step: Option<CardPlay>) {
    for _ in 0_u8..32_u8 {
        if let Some(CardPlay::Awaiting(q)) = step.take() {
            let answer = match &q.domain {
                InputDomain::Targets(d) => Answer::Targets(
                    d.members
                        .iter()
                        .filter(|m| m.player.seat == 2)
                        .map(|m| m.handle)
                        .take(1)
                        .collect(),
                ),
                InputDomain::Pending(options) => Answer::Pending(options[0].handle),
                InputDomain::Quick { .. } => Answer::Quick(None),
                _ => panic!("unexpected decision: {:?}", q.domain),
            };
            step = Some(game.answer_internal_card(q.actor, q.token, answer).unwrap());
            continue;
        }
        if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
            continue;
        }
        let Some(q) = game.internal_pending_choice().unwrap() else {
            return;
        };
        let InputDomain::Pending(options) = &q.domain else {
            panic!("expected a pending ability");
        };
        step = Some(
            game.answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
                .unwrap(),
        );
    }
    panic!("scenario did not settle");
}

#[test]
fn bp20_021_adds_two_ability_damage_per_fusion_counter() {
    // Evolution keeps the base object's counters; 3 printed damage plus 2 per counter.
    for (counters, damage) in [(0_u8, 3_u8), (1, 5), (2, 7)] {
        let mut game = loaded(&position(counters));
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}}),
        )
        .unwrap();
        settle(&mut game, None);
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["objects"]["enemy"]["damage"], damage, "{counters} counters");
    }
}

#[test]
fn bp20_021_leaves_attack_damage_unmodified() {
    // 「能力によるダメージ」: combat damage from the same source gets no fusion bonus.
    let mut game = loaded(&position(2));
    game.internal_rule_action(
        &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}}),
    )
    .unwrap();
    settle(&mut game, None);
    let evolved = game.projection(View::Referee).unwrap();
    assert_eq!(evolved["objects"]["enemy"]["damage"], 7_u8);
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"enemy"}))
            .unwrap()
            .outcome,
        "resolved"
    );
    settle(&mut game, None);
    while let Some(q) = game.internal_quick_choice().unwrap() {
        let step = game
            .answer_internal_card(q.actor, q.token, Answer::Quick(None))
            .unwrap();
        settle(&mut game, Some(step));
    }
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(
        view["objects"]["enemy"]["damage"].as_u64().unwrap(),
        7 + evolved["objects"]["source"]["power"].as_u64().unwrap()
    );
}
