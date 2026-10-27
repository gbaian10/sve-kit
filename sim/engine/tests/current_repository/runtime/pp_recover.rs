use super::*;
use sve_engine::game::internal::InstructionOutcome;

#[test]
fn bp11_111_attack_recovers_only_its_controllers_pp_and_respects_the_cap() {
    for seat in ["P1", "P2"] {
        let opponent = if seat == "P1" { "P2" } else { "P1" };
        for current in [0_u8, 4, 5] {
            let mut initial = draw_position();
            initial["turn"]["active"] = json!(seat);
            initial["players"][seat]["pp"] = json!({"current":current,"max":5_u8});
            initial["players"][seat]["zones"]["field"] = json!([
                {"id":"attacker","card":"BP11-111"}, {"id":"ally-watcher","card":"BP11-111"}
            ]);
            initial["players"][opponent]["zones"]["field"] = json!([
                {"id":"enemy","card":"BP01-037","state":{"acted":false}},
                {"id":"enemy-watcher","card":"BP11-111"}
            ]);
            let mut game = game(&initial);
            let attack = game
                .internal_rule_action(
                    &json!({"do":"attack","attacker":"attacker","target":"enemy"}),
                )
                .unwrap();
            assert_eq!(
                attack.outcome, "resolved",
                "assail permits the active enemy target"
            );
            let question = game.internal_pending_choice().unwrap().unwrap();
            let InputDomain::Pending(options) = &question.domain else {
                panic!("own attack trigger");
            };
            assert_eq!(options.len(), 1);
            assert_eq!(options[0].card, "BP11-111");
            assert_eq!(options[0].line, 2_u64);
            let before = format!("{game:?}");
            game.answer_internal_card(
                PlayerId {
                    seat: seat_number(opponent),
                },
                question.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap_err();
            assert!(
                format!("{game:?}") == before,
                "invalid actor preserves authority"
            );
            let result = resolved(
                game.answer_internal_card(
                    question.actor,
                    question.token,
                    Answer::Pending(options[0].handle),
                )
                .unwrap(),
            );
            let recoveries = result
                .progress
                .receipts
                .values()
                .filter_map(|outcome| {
                    if let InstructionOutcome::Resource(recovery) = outcome {
                        Some(recovery)
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();
            assert_eq!(recoveries.len(), 1);
            assert_eq!(recoveries[0].requested, Int(3));
            assert_eq!(recoveries[0].before, Int(i64::from(current)));
            assert_eq!(recoveries[0].actual, Int(i64::from(3_u8.min(5 - current))));
            for view in [View::P1, View::P2, View::Referee] {
                let state = game.projection(view).unwrap();
                assert_eq!(
                    state[seat]["pp"]["current"],
                    current.saturating_add(3).min(5)
                );
                assert_eq!(state[seat]["pp"]["max"], 5_u8);
                assert_eq!(state[opponent]["pp"]["current"], 10_u8);
            }
        }
    }
}

#[test]
fn bp11_111_entry_draws_once_without_recovering_pp_before_an_attack() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"follower","card":"BP11-111"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"drawn","card":"CP02-068"}]);
    let mut game = game(&initial);
    assert!(matches!(
        game.play_internal_card("follower", PlayerId { seat: 1 })
            .unwrap(),
        CardPlay::Entered(_)
    ));
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("entry draw");
    };
    assert_eq!(options.len(), 1);
    assert_eq!(options[0].line, 3_u64);
    let result = resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap(),
    );
    assert!(
        result
            .progress
            .receipts
            .values()
            .all(|outcome| !matches!(outcome, InstructionOutcome::Resource(_)))
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["pp"]["current"], 6_u8);
    assert_eq!(state["P1"]["hand"], json!(["drawn"]));
}
