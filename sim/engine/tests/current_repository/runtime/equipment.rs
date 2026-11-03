#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate the equipment scenarios."
)]
use super::event_cemetery::finish_checks;
use super::union_burst_batch1::finish;
use super::*;

fn settle(game: &mut Game) {
    for _ in 0_u8..4 {
        finish_checks(game);
        let Some(question) = game.internal_pending_choice().unwrap() else {
            return;
        };
        let InputDomain::Pending(options) = &question.domain else {
            panic!("printed trigger");
        };
        let play = game
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap();
        resolved(finish(game, play));
    }
    panic!("unexpected trigger loop");
}

#[test]
fn kokkoro_all_printings_equip_and_keep_ameth_through_normal_evolution() {
    for (card, form) in [
        ("CP04-001", "CP04-002"),
        ("CP04-SL01", "CP04-SL02"),
        ("PCS01-001", "PCS01-002"),
    ] {
        let mut initial = draw_position();
        initial["players"]["P1"]["construction"] = json!("title");
        initial["players"]["P1"]["title_code"] = json!("princess_connect_re_dive");
        initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"host","card":card}]);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
        initial["players"]["P1"]["zones"]["deck"] =
            json!([{"id":"drawn","card":"BP01-037"},{"id":"spare","card":"BP01-037"}]);
        let mut game = Game::from_selected_effects(
            selection_with_rules().clone(),
            &initial,
            &Value::Null,
            &Value::Null,
            "equipment-runtime",
        )
        .unwrap();
        assert!(matches!(
            game.play_internal_card("host", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ));
        settle(&mut game);
        let equipped = game.projection(View::Referee).unwrap();
        assert_eq!(equipped["P1"]["leader"]["life"], 11_u8);
        assert_eq!(equipped["P1"]["pp"]["current"], 7_u8);
        assert_eq!(equipped["P1"]["field"], json!(["host"]));
        let equipment = equipped["P1"]["equipment"].as_array().unwrap();
        assert_eq!(equipment.len(), 1);
        let id = equipment[0].as_str().unwrap();
        assert_eq!(equipped["objects"][id]["equipped_to"], "host");
        let step = game
            .internal_rule_action(
                &json!({"do":"evolve","source":"host","evolve_card":"form","pay":{"pp":1_u8}}),
            )
            .unwrap();
        assert_eq!(step.outcome, "resolved");
        settle(&mut game);
        let evolved = game.projection(View::Referee).unwrap();
        assert_eq!(evolved["P1"]["equipment"], equipped["P1"]["equipment"]);
        assert_eq!(evolved["objects"][id]["equipped_to"], "host");
        assert_eq!(evolved["P1"]["hand"], json!(["drawn"]));
        let source = game.internal_activated_sources("host").unwrap().remove(0);
        resolved(
            game.activate_internal_card_instance("host", source.instance, PlayerId { seat: 1 })
                .unwrap(),
        );
        let activated = game.projection(View::Referee).unwrap();
        assert_eq!(activated["objects"]["host"]["power"], 3_u8);
        assert_eq!(activated["objects"]["host"]["acted"], true);
    }
}

#[test]
fn ameth_all_printings_grant_a_host_payment_and_buff_only_own_field_followers() {
    for card in ["CP04-T01", "CP04-PR15", "CP04-PR27"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"host","card":"BP01-037"},{"id":"ally","card":"BP01-037"},{"id":"amulet","card":"BP01-041"}]);
        initial["players"]["P1"]["zones"]["equipment"] =
            json!([{"id":"equipment","card":card,"state":{"equipped_to":"host"}}]);
        initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
        let mut game = game(&initial);
        let before = game.projection(View::Referee).unwrap();
        let source = game.internal_activated_sources("host").unwrap().remove(0);
        resolved(
            game.activate_internal_card_instance("host", source.instance, PlayerId { seat: 1 })
                .unwrap(),
        );
        let after = game.projection(View::Referee).unwrap();
        for id in ["host", "ally"] {
            assert_eq!(
                after["objects"][id]["power"].as_u64().unwrap(),
                before["objects"][id]["power"].as_u64().unwrap() + 1
            );
            assert_eq!(
                after["objects"][id]["max_hp"].as_u64().unwrap(),
                before["objects"][id]["max_hp"].as_u64().unwrap() + 1
            );
        }
        assert_eq!(after["objects"]["host"]["acted"], true);
        assert_eq!(
            after["objects"]["equipment"]["acted"],
            before["objects"]["equipment"]["acted"]
        );
        for id in ["amulet", "enemy"] {
            assert_eq!(after["objects"][id], before["objects"][id]);
        }
    }
}

#[test]
fn princess_sword_all_printings_modify_outgoing_and_incoming_combat_damage() {
    for card in ["CP04-T02", "CP04-PR16", "CP04-PR28"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["field"] =
            json!([{"id":"host","card":"BP01-037","state":{"hp":10_u8,"max_hp":10_u8}}]);
        initial["players"]["P1"]["zones"]["equipment"] =
            json!([{"id":"equipment","card":card,"state":{"equipped_to":"host"}}]);
        initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037","state":{"acted":true,"hp":10_u8,"max_hp":10_u8}}]);
        let mut game = game(&initial);
        let before = game.projection(View::Referee).unwrap();
        let power = before["objects"]["host"]["power"].as_u64().unwrap();
        game.internal_rule_action(&json!({"do":"attack","attacker":"host","target":"enemy"}))
            .unwrap();
        let question = game.internal_quick_choice().unwrap().unwrap();
        game.answer_internal_card(question.actor, question.token, Answer::Quick(None))
            .unwrap();
        let after = game.projection(View::Referee).unwrap();
        assert_eq!(after["objects"]["enemy"]["damage"], power + 2);
        assert_eq!(after["objects"]["host"]["damage"], power.saturating_sub(2));
    }
}
