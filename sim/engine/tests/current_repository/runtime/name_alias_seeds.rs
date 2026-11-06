#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected choices invalidate the printed alias scenarios."
)]
use super::*;

fn finish(game: &mut Game, mut play: CardPlay) -> CardPlay {
    while let CardPlay::Awaiting(q) = play {
        let answer = match &q.domain {
            InputDomain::OptionalCost => Answer::OptionalCost(true),
            InputDomain::CostMaterials(d) => {
                Answer::CostMaterials(d.members.iter().map(|c| c.handle).collect())
            }
            InputDomain::Targets(d) => {
                Answer::Targets(d.members.iter().map(|c| c.handle).collect())
            }
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            _ => panic!("unexpected printed choice"),
        };
        play = game.answer_internal_card(q.actor, q.token, answer).unwrap();
    }
    play
}
fn settle(game: &mut Game) {
    for _ in 0..6_usize {
        event_cemetery::finish_checks(game);
        let Some(q) = game.internal_pending_choice().unwrap() else {
            return;
        };
        let InputDomain::Pending(options) = &q.domain else {
            panic!("pending");
        };
        let play = game
            .answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
            .unwrap();
        resolved(finish(game, play));
    }
    panic!("trigger loop");
}

#[test]
fn orchis_and_zwei_pay_two_ex_puppets_then_buff_field_only_aliases_in_every_printing() {
    for number in [
        "BP16-003",
        "BP16-SL03",
        "BP16-U01",
        "SCS01-002",
        "BP19-004",
        "BP19-SL04",
    ] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"orchis","card":number}]);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"lloyd","card":"BP08-T01"}]);
        initial["players"]["P1"]["zones"]["ex"] =
            json!([{"id":"a","card":"BP08-T09"},{"id":"b","card":"BP08-T09"}]);
        initial["players"]["P1"]["zones"]["cemetery"] =
            json!([{"id":"g1","card":number},{"id":"g2","card":number},{"id":"g3","card":number}]);
        let mut game = game(&initial);
        let play = game
            .play_internal_card("orchis", PlayerId { seat: 1 })
            .unwrap();
        assert!(matches!(finish(&mut game, play), CardPlay::Entered(_)));
        settle(&mut game);
        let view = game.projection(View::Referee).unwrap();
        assert!(view["objects"]["a"].is_null());
        assert!(view["objects"]["b"].is_null());
        let ex = view["P1"]["ex"].as_array().unwrap();
        assert_eq!(ex.len(), 1);
        let expected_token = if number.starts_with("BP19") {
            "ヴィクトリア"
        } else {
            "ロイド"
        };
        assert_eq!(
            view["objects"][ex[0].as_str().unwrap()]["name"],
            expected_token
        );
        let definition = selection().current[number]
            .program
            .card()
            .abilities
            .iter()
            .find(|ability| ability.node.kind == AbilityKind::Activated)
            .unwrap()
            .node
            .id;
        let activation = game
            .activate_internal_card("orchis", definition, PlayerId { seat: 1 })
            .unwrap();
        resolved(finish(&mut game, activation));
        let buffed = game.projection(View::Referee).unwrap();
        assert_eq!(buffed["objects"]["lloyd"]["power"], 3_u8);
        assert_eq!(buffed["objects"]["lloyd"]["max_hp"], 5_u8);
        let keywords = buffed["objects"]["lloyd"]["keywords"].as_array().unwrap();
        for name in ["疾走", "必殺", "守護"] {
            assert_eq!(
                keywords.contains(&json!(name)),
                name == "守護" || number != "BP19-004" && number != "BP19-SL04"
            );
        }
    }
}

#[test]
fn lloyd_all_printings_heal_on_entry_and_keep_their_printed_ward() {
    for number in ["BP08-T01", "BP16-T11", "PR-188"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["leader"]["life"] = json!(16_u8);
        initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"lloyd","card":number}]);
        let mut game = game(&initial);
        let play = game
            .play_internal_card("lloyd", PlayerId { seat: 1 })
            .unwrap();
        assert!(matches!(finish(&mut game, play), CardPlay::Entered(_)));
        settle(&mut game);
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["P1"]["leader"]["life"], 18_u8);
        assert!(
            view["objects"]["lloyd"]["keywords"]
                .as_array()
                .unwrap()
                .contains(&json!("守護"))
        );
    }
}
#[test]
fn bakels_all_printings_draw_only_with_a_giant_ghost() {
    for number in ["BP03-078", "ETD02-001"] {
        for giant in [false, true] {
            let mut initial = draw_position();
            initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"bakels","card":number}]);
            initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"drawn","card":"BP01-037"}]);
            initial["players"]["P1"]["zones"]["field"] = if giant {
                json!([{"id":"observer","card":"BP03-074"},{"id":"giant","card":"BP03-T06"}])
            } else {
                json!([{"id":"observer","card":"BP03-074"}])
            };
            let mut game = game(&initial);
            let play = game
                .play_internal_card("bakels", PlayerId { seat: 1 })
                .unwrap();
            assert!(matches!(finish(&mut game, play), CardPlay::Entered(_)));
            settle(&mut game);
            let view = game.projection(View::Referee).unwrap();
            assert_eq!(view["objects"]["bakels"]["power"], 2_u8);
            assert_eq!(
                view["P1"]["hand"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("drawn")),
                giant
            );
        }
    }
}

#[test]
fn victoria_all_printings_draw_on_entry_and_keep_both_printed_keywords() {
    for number in ["BP08-T02", "BP19-T03", "PR-189"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"victoria","card":number}]);
        initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"drawn","card":"BP01-037"}]);
        let mut game = game(&initial);
        let play = game
            .play_internal_card("victoria", PlayerId { seat: 1 })
            .unwrap();
        assert!(matches!(finish(&mut game, play), CardPlay::Entered(_)));
        settle(&mut game);
        let view = game.projection(View::Referee).unwrap();
        assert!(
            view["P1"]["hand"]
                .as_array()
                .unwrap()
                .contains(&json!("drawn"))
        );
        for name in ["突進", "指定攻撃"] {
            assert!(
                view["objects"]["victoria"]["keywords"]
                    .as_array()
                    .unwrap()
                    .contains(&json!(name))
            );
        }
    }
}
