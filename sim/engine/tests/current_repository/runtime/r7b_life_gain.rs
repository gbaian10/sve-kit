#![expect(
    clippy::panic,
    reason = "Unexpected choices invalidate the complete Verdilia sequence."
)]
use super::event_cemetery::finish_checks;
use super::*;

fn settle(board: &mut Game, number: &str) {
    loop {
        finish_checks(board);
        let Some(q) = board.internal_pending_choice().unwrap() else {
            break;
        };
        let InputDomain::Pending(options) = &q.domain else {
            panic!("evolution pending")
        };
        assert!(
            options.iter().all(|option| option.card == number),
            "the actual evolved print owns both evolution triggers"
        );
        resolved(
            board
                .answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
                .unwrap(),
        );
    }
}

fn play_guidance(board: &mut Game, token: &str, enemy_present: bool) {
    let play = board
        .play_internal_card(token, PlayerId { seat: 1 })
        .unwrap();
    if let CardPlay::Awaiting(q) = play {
        let InputDomain::Targets(domain) = q.domain else {
            panic!("guidance target domain")
        };
        let targets = domain
            .members
            .iter()
            .take(1)
            .map(|member| member.handle)
            .collect();
        resolved(
            board
                .answer_internal_card(q.actor, q.token, Answer::Targets(targets))
                .unwrap(),
        );
    } else {
        assert!(
            !enemy_present,
            "only an empty opposing field can omit the formal question"
        );
        resolved(play);
    }
    finish_checks(board);
}

#[test]
fn r7b_verdilia_all_prints_evolve_and_super_evolve_into_the_complete_guidance_sequence() {
    for (base, evolved) in [("BP21-091", "BP21-092"), ("BP21-SL21", "BP21-SL22")] {
        for (super_evolution, enemy_present) in [(false, true), (true, true), (true, false)] {
            assert!(selection_with_rules().current.contains_key(evolved));
            assert!(selection_with_rules().current.contains_key("BP21-T09"));
            assert!(selection_with_rules().current.contains_key("BP21-P69"));
            let mut initial = draw_position();
            initial["turn"]["elapsed_turns"]["P1"] = json!(7_u8);
            initial["players"]["P1"]["leader"]["class"] = json!("ビショップ");
            initial["players"]["P1"]["sep"] = json!(1_u8);
            initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":base}]);
            initial["players"]["P1"]["zones"]["evolve_deck"] =
                json!([{"id":"form","card":evolved}]);
            initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"drawn","card":"BP01-037"}]);
            initial["players"]["P2"]["zones"]["field"] = json!([
                {"id":"enemy","card":"BP01-171","state":{"hp":20_u8,"max_hp":20_u8}}
            ]);
            if !super_evolution {
                initial["players"]["P1"]["zones"]["ex"] =
                    json!([{"id":"printed-guide","card":"BP21-P69"}]);
            }
            if !enemy_present {
                initial["players"]["P2"]["zones"]["field"] = json!([]);
            }
            let mut board = Game::from_selected_effects(
                selection_with_rules().clone(),
                &initial,
                &Value::Null,
                &Value::Null,
                "r7b-verdilia",
            )
            .unwrap();
            let mut action =
                json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}});
            if super_evolution {
                action["pay"]["sep"] = json!(1_u8);
            }
            assert_eq!(
                board.internal_rule_action(&action).unwrap().outcome,
                "resolved"
            );
            settle(&mut board, evolved);
            let before = board.projection(View::Referee).unwrap();
            assert_eq!(before["P1"]["leader"]["life"], 21_u8);
            assert_eq!(before["P1"]["hand"], json!(["drawn"]));
            assert!(
                before["objects"]["source"]["keywords"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("守護"))
            );
            let ex = before["P1"]["ex"].as_array().unwrap();
            assert_eq!(ex.len(), 1);
            let token = ex[0].as_str().unwrap();
            play_guidance(&mut board, token, enemy_present);
            let after = board.projection(View::Referee).unwrap();
            if enemy_present {
                assert_eq!(after["objects"]["enemy"]["damage"], 5_u8);
            }
            assert_eq!(
                after["objects"]["source"]["power"].as_i64().unwrap(),
                before["objects"]["source"]["power"].as_i64().unwrap() + 1
            );
            assert_eq!(
                after["objects"]["source"]["max_hp"].as_i64().unwrap(),
                before["objects"]["source"]["max_hp"].as_i64().unwrap() + 1
            );
        }
    }
}
