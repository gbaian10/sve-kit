#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate the equipment Play scenarios."
)]
use super::event_cemetery::finish_checks;
use super::union_burst_batch1::finish;
use super::*;

fn settle(game: &mut Game) {
    for _ in 0_u8..5 {
        finish_checks(game);
        let Some(question) = game.internal_pending_choice().unwrap() else {
            return;
        };
        let InputDomain::Pending(options) = &question.domain else {
            panic!("pending");
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
    panic!("trigger loop");
}

fn titled(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "equipment-played-real",
    )
    .unwrap()
}

#[test]
fn kyaru_all_printings_pay_ub_and_equip_grimoire_then_trigger_from_a_spell() {
    for card in ["CP04-037", "CP04-SL09", "PCS01-015"] {
        let mut initial = draw_position();
        initial["turn"]["elapsed_turns"]["P1"] = json!(10_u8);
        initial["players"]["P1"]["construction"] = json!("title");
        initial["players"]["P1"]["title_code"] = json!("princess_connect_re_dive");
        initial["players"]["P1"]["zones"]["hand"] =
            json!([{"id":"host","card":card},{"id":"discard","card":"BP01-065"}]);
        initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"drawn","card":"BP01-065"},{"id":"d1","card":"BP01-037"},{"id":"d2","card":"BP01-037"},{"id":"spare","card":"BP01-037"}]);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"CP04-038"}]);
        initial["players"]["P2"]["zones"]["field"] =
            json!([{"id":"enemy","card":"BP01-037","state":{"hp":10_u8,"max_hp":10_u8}}]);
        let mut game = titled(&initial);
        assert!(matches!(
            game.play_internal_card("host", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ));
        settle(&mut game);
        let equipped = game.projection(View::Referee).unwrap();
        assert_eq!(equipped["P1"]["hand"], json!(["drawn"]));
        assert_eq!(equipped["P1"]["cemetery"], json!(["discard"]));
        assert_eq!(equipped["P1"]["pp"]["current"], 4_u8);
        assert_eq!(equipped["objects"]["enemy"]["damage"], 3_u8);
        let equipment = equipped["P1"]["equipment"].as_array().unwrap();
        assert_eq!(equipment.len(), 1);
        let token = equipment[0].as_str().unwrap();
        assert_eq!(equipped["objects"][token]["equipped_to"], "host");
        let play = game
            .play_internal_card("drawn", PlayerId { seat: 1 })
            .unwrap();
        resolved(finish(&mut game, play));
        settle(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
            18_u8
        );
        game.internal_rule_action(
            &json!({"do":"evolve","source":"host","evolve_card":"form","pay":{"pp":1_u8}}),
        )
        .unwrap();
        settle(&mut game);
        let evolved = game.projection(View::Referee).unwrap();
        assert_eq!(evolved["P1"]["pp"]["current"], 0_u8);
        assert_eq!(evolved["P1"]["equipment"], equipped["P1"]["equipment"]);
        assert_eq!(evolved["objects"]["host"]["evolved"], true);
        assert_eq!(evolved["objects"]["enemy"]["damage"], 8_u8);
    }
}

#[test]
fn grimoire_all_printings_limit_the_host_spell_trigger_to_two_plays() {
    for card in ["CP04-T06", "CP04-PR20", "CP04-PR32"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["field"] =
            json!([{"id":"host","card":"BP01-037","state":{"power":3_u8}}]);
        initial["players"]["P1"]["zones"]["equipment"] =
            json!([{"id":"equipment","card":card,"state":{"equipped_to":"host"}}]);
        initial["players"]["P1"]["zones"]["hand"] = json!(
            (0_u8..3)
                .map(|i| json!({"id":format!("spell{i}"),"card":"BP01-065"}))
                .collect::<Vec<_>>()
        );
        initial["players"]["P1"]["zones"]["deck"] = json!(
            (0_u8..7)
                .map(|i| json!({"id":format!("deck{i}"),"card":"BP01-037"}))
                .collect::<Vec<_>>()
        );
        let mut game = game(&initial);
        for (spell, life) in [("spell0", 18_u8), ("spell1", 16), ("spell2", 16)] {
            let play = game
                .play_internal_card(spell, PlayerId { seat: 1 })
                .unwrap();
            resolved(finish(&mut game, play));
            settle(&mut game);
            assert_eq!(
                game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
                life
            );
        }
    }
}

#[test]
fn ameth_stops_granting_its_activated_ability_when_the_host_loses_abilities() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"host","card":"BP01-037"}]);
    initial["players"]["P1"]["zones"]["equipment"] =
        json!([{"id":"equipment","card":"CP04-T01","state":{"equipped_to":"host"}}]);
    let mut active = game(&initial);
    let definition = active
        .internal_activated_sources("host")
        .unwrap()
        .remove(0)
        .definition;
    initial["players"]["P1"]["zones"]["field"][0]["state"]["silenced"] = json!(true);
    let mut silenced = game(&initial);
    assert_eq!(
        silenced.internal_activated_sources("host").unwrap(),
        Vec::<RuntimeAbilitySource>::new()
    );
    let before = silenced.projection(View::Referee).unwrap();
    silenced
        .activate_internal_card("host", definition, PlayerId { seat: 1 })
        .unwrap_err();
    assert_eq!(silenced.projection(View::Referee).unwrap(), before);
}
