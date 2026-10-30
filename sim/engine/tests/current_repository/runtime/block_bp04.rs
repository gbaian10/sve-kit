use super::event_cemetery::finish_checks;
use super::*;
use sve_engine::game::internal::InstructionOutcome;

fn initial(card: &str, spells: usize) -> Value {
    let mut value = draw_position();
    value["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    value["players"]["P1"]["zones"]["field"] = json!([{"id":"ally","card":"BP01-037"}]);
    value["players"]["P1"]["zones"]["cemetery"] = json!(
        (0..spells)
            .map(|i| json!({"id":format!("spell-{i}"),"card":"BP01-065"}))
            .chain([json!({"id":"wrong-type","card":"BP01-037"})])
            .collect::<Vec<_>>()
    );
    value["players"]["P1"]["zones"]["ex"] = json!([{"id":"outside-spell","card":"BP01-065"}]);
    value["players"]["P2"]["zones"]["cemetery"] =
        json!([{"id":"opposing-spell","card":"BP01-065"}]);
    value["players"]["P2"]["zones"]["field"] = json!([{"id":"first","card":"BP01-037"},{"id":"second","card":"BP01-037"},{"id":"amulet","card":"BP09-102"}]);
    value
}

fn game(value: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        value,
        &Value::Null,
        &Value::Null,
        "block-bp04",
    )
    .unwrap()
}

#[expect(
    clippy::panic,
    reason = "Neither printed ability asks for a target or mode."
)]
fn trigger(game: &mut Game) -> Activation {
    let occurrence = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &occurrence.domain else {
        panic!("printed triggered ability")
    };
    assert_eq!(options.len(), 1, "exactly one printed ability is pending");
    resolved(
        game.answer_internal_card(
            occurrence.actor,
            occurrence.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap(),
    )
}

#[test]
fn bp04_040_printings_choose_one_frozen_spellchain_damage_batch_at_each_threshold() {
    for printing in ["BP04-040", "BP04-SL09"] {
        for (spells, amount) in [
            (9_usize, 0_u8),
            (10, 5),
            (19, 5),
            (20, 10),
            (29, 10),
            (30, 30),
        ] {
            let mut value = initial(printing, spells);
            value["players"]["P2"]["leader"]["life"] = json!(1_u8);
            let mut game = game(&value);
            assert!(matches!(
                game.play_internal_card("source", PlayerId { seat: 1 })
                    .unwrap(),
                CardPlay::Entered(_)
            ));
            finish_checks(&mut game);
            let result = trigger(&mut game);
            assert_eq!(
                result
                    .progress
                    .receipts
                    .values()
                    .filter(|receipt| matches!(receipt, InstructionOutcome::Damage(_)))
                    .count(),
                usize::from(amount > 0),
                "the threshold selects exactly one joint damage action or none"
            );
            let hits = result
                .progress
                .facts
                .iter()
                .filter_map(|fact| {
                    if let Fact::Damage(event) = fact {
                        Some(event)
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();
            assert_eq!(
                hits.len(),
                if amount > 0 { 3 } else { 0 },
                "lethal leader damage never prevents either opposing follower's simultaneous damage"
            );
            assert!(
                hits.iter()
                    .all(|hit| hit.amount.0.get() == u64::from(amount)),
                "all recipients receive the highest applicable frozen SC amount"
            );
            let view = game.projection(View::Referee).unwrap();
            for id in ["first", "second"] {
                assert_eq!(view["objects"][id]["damage"], amount);
            }
            for id in ["ally", "amulet"] {
                assert_eq!(view["objects"][id]["damage"], 0_u8);
            }
            assert_eq!(view["P1"]["leader"]["life"], 20_u8);
            assert_eq!(view["P1"]["pp"]["current"], 3_u8);
        }
    }
}

#[test]
fn bp04_052_real_base_evolves_for_one_pp_and_heals_only_with_five_own_cemetery_spells() {
    for spells in [4_usize, 5] {
        let mut value = initial("BP04-052", spells);
        value["players"]["P1"]["leader"]["life"] = json!(10_u8);
        value["players"]["P1"]["zones"]["hand"] = json!([]);
        value["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP04-051"}]);
        value["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP04-052"}]);
        let mut game = game(&value);
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
            )
            .unwrap()
            .outcome,
            "resolved"
        );
        finish_checks(&mut game);
        trigger(&mut game);
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(
            view["P1"]["leader"]["life"],
            if spells == 5 { 12_u8 } else { 10_u8 }
        );
        assert_eq!(view["P1"]["pp"]["current"], 9_u8);
    }
}
