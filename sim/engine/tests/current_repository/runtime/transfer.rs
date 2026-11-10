#![expect(
    clippy::wildcard_enum_match_arm,
    reason = "Native steal must declare its follower and all three payment cards."
)]
use super::*;

fn position(number: &str, full: bool) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"source","card":number},{"id":"observer","card":"BP01-043"}]);
    if full {
        initial["players"]["P1"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .extend((0_u8..3_u8).map(|n| json!({"id":format!("filler{n}"),"card":"BP01-037"})));
    }
    initial["players"]["P1"]["zones"]["cemetery"] = json!(
        (0_u8..6_u8)
            .map(|n| json!({"id":format!("payment{n}"),"card":number}))
            .collect::<Vec<_>>()
    );
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP02-119"}]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"victim","card":"BP02-118","state":{"acted":true,"damage":1_u8}}]);
    initial
}

// Both printings share the expensive source reconciliation; each runs the whole native procedure.
#[test]
fn chaotic_angel_transfer_all_printings_keep_payment_capacity_and_new_owner_evolution_separate() {
    for number in ["BP03-109", "BP03-SL22"] {
        for full in [false, true] {
            let initial = position(number, full);
            let mut game = Game::from_selected_effects(
                selection_with_rules().clone(),
                &initial,
                &Value::Null,
                &Value::Null,
                "native-transfer",
            )
            .unwrap();
            let before = game.projection(View::Referee).unwrap();
            let source = game.internal_activated_sources("source").unwrap()[0];
            let mut play = game
                .activate_internal_card_instance("source", source.instance, PlayerId { seat: 1 })
                .unwrap();
            while let CardPlay::Awaiting(question) = play {
                let answer = match &question.domain {
                    InputDomain::Targets(domain) => Answer::Targets(vec![
                        domain
                            .members
                            .iter()
                            .find(|member| member.card.as_deref() == Some("BP02-118"))
                            .unwrap()
                            .handle,
                    ]),
                    InputDomain::CostMaterials(domain) => {
                        assert_eq!(domain.cardinality.minimum, Nat(3));
                        Answer::CostMaterials(
                            domain
                                .members
                                .iter()
                                .take(3)
                                .map(|member| member.handle)
                                .collect(),
                        )
                    }
                    _ => panic!("native steal target or payment"),
                };
                play = game
                    .answer_internal_card(question.actor, question.token, answer)
                    .unwrap();
            }
            resolved(play);
            event_cemetery::finish_checks(&mut game);
            assert!(
                game.internal_pending_choice().unwrap().is_none(),
                "steal is not follower entry"
            );
            let after = game.projection(View::Referee).unwrap();
            assert_eq!(after["P1"]["cemetery"].as_array().unwrap().len(), 3_usize);
            assert_eq!(after["P1"]["banish"].as_array().unwrap().len(), 3_usize);
            assert_eq!(after["P1"]["pp"]["current"], 10_u8);
            let controller = if full { "P2" } else { "P1" };
            assert!(
                after[controller]["field"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("victim"))
            );
            assert_eq!(after["objects"]["victim"]["acted"], false);
            assert_eq!(
                after["objects"]["victim"]["generation"],
                before["objects"]["victim"]["generation"]
            );
            assert_eq!(after["objects"]["victim"]["damage"], 1_u8);
            for stat in ["power", "hp", "max_hp"] {
                assert_eq!(
                    after["objects"]["observer"][stat],
                    before["objects"]["observer"][stat]
                );
            }
            game.activate_internal_card_instance("source", source.instance, PlayerId { seat: 1 })
                .unwrap_err();
            if !full {
                game.internal_rule_action(&json!({"do":"evolve","source":"victim","evolve_card":"form","pay":{"pp":3_u8}})).unwrap();
                event_cemetery::finish_checks(&mut game);
                let evolved = game.projection(View::Referee).unwrap();
                assert_eq!(evolved["P1"]["pp"]["current"], 7_u8);
                assert_eq!(evolved["objects"]["victim"]["evolved_with"], "form");
                assert_eq!(evolved["P1"]["evolution"], json!(["form"]));
                assert_eq!(evolved["objects"]["victim"]["damage"], 1_u8);
            }
        }
    }
}
