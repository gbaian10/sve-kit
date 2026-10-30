use super::*;
use sve_engine::game::internal::InstructionOutcome;

fn position(seat: &str, source: &str, material: &str, count: usize, life: u8) -> Value {
    let opponent = if seat == "P1" { "P2" } else { "P1" };
    let mut initial = draw_position();
    initial["turn"]["active"] = json!(seat);
    initial["players"][seat]["zones"]["hand"] = json!([{"id":"source","card":source}]);
    initial["players"][seat]["zones"]["field"] = json!([{"id":"ally","card":"BP04-117"}]);
    initial["players"][seat]["zones"]["cemetery"] = json!(
        (0..count)
            .map(|i| json!({"id":format!("grave-{i}"),"card":material}))
            .chain([json!({"id":"wrong","card":"BP01-037"})])
            .collect::<Vec<_>>()
    );
    initial["players"][seat]["zones"]["ex"] = json!([{"id":"outside","card":material}]);
    initial["players"][opponent]["leader"]["life"] = json!(life);
    initial["players"][opponent]["zones"]["hand"] = json!([]);
    initial["players"][opponent]["zones"]["field"] = json!([
        {"id":"first","card":"BP04-117"},{"id":"second","card":"BP04-117"},
        {"id":"amulet","card":"BP09-102"}
    ]);
    initial["players"][opponent]["zones"]["cemetery"] = json!([{"id":"foreign","card":material}]);
    initial
}

fn assert_batch(game: &Game, result: &Activation, seat: &str, life: u8, amount: u8) {
    let opponent = if seat == "P1" { "P2" } else { "P1" };
    let quantities = result
        .progress
        .receipts
        .values()
        .filter_map(|receipt| {
            if let InstructionOutcome::Damage(quantity) = receipt {
                Some(quantity)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(
        quantities.as_slice(),
        [&DamageQuantityOutcome {
            requested: Int(i64::from(amount).checked_mul(3).unwrap()),
            actual: Nat(u64::from(amount).checked_mul(3).unwrap())
        }],
        "leader and followers produce one joint damage receipt"
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
        3,
        "only one enemy leader and two enemy followers take damage"
    );
    assert!(
        hits.iter()
            .all(|event| event.amount.0.get() == u64::from(amount)),
        "every recipient receives the same evaluated amount"
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        state[opponent]["leader"]["life"],
        i64::from(life).checked_sub(i64::from(amount)).unwrap(),
        "The opposing leader takes the same batch amount"
    );
    for id in ["first", "second"] {
        assert_eq!(
            state["objects"][id]["damage"], amount,
            "Both enemy followers receive the batch damage"
        );
    }
    assert_eq!(
        state["objects"]["ally"]["damage"], 0_u8,
        "Friendly followers are outside the recipients"
    );
    if life == 1 {
        assert_eq!(
            state["game"]["ended"], true,
            "leader defeat is checked after the joint damage application"
        );
        assert_eq!(
            state["game"]["winner"], seat,
            "Lethal damage awards the batch controller the win"
        );
    }
}

#[test]
fn bp18_088_same_damage_batch_hits_followers_even_when_leader_damage_is_lethal() {
    for printing in ["BP18-088", "BP18-P37"] {
        for seat in ["P1", "P2"] {
            for count in [9_usize, 10] {
                for life in [1_u8, 20] {
                    let mut game = game(&position(seat, printing, "BP18-088", count, life));
                    let result = resolved(
                        game.play_internal_card(
                            "source",
                            PlayerId {
                                seat: seat_number(seat),
                            },
                        )
                        .unwrap(),
                    );
                    assert_batch(&game, &result, seat, life, if count == 10 { 2 } else { 1 });
                }
            }
        }
    }
}

#[test]
fn bp22_083_vampire_count_changes_the_whole_leader_and_follower_damage_batch() {
    for printing in ["BP22-083", "BP22-P45"] {
        for count in [4_usize, 5] {
            for life in [1_u8, 20] {
                let mut initial = position("P1", "BP22-082", "BP22-082", count, life);
                initial["players"]["P1"]["zones"]["hand"] = json!([]);
                initial["players"]["P1"]["zones"]["field"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!({"id":"source","card":"BP22-082"}));
                initial["players"]["P1"]["zones"]["evolve_deck"] =
                    json!([{"id":"form","card":printing}]);
                let mut game = game(&initial);
                assert_eq!(game.internal_rule_action(&json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})).unwrap().outcome,"resolved");
                let pending = game.internal_pending_choice().unwrap().unwrap();
                let InputDomain::Pending(options) = &pending.domain else {
                    panic!("evolve trigger")
                };
                let result = resolved(
                    game.answer_internal_card(
                        pending.actor,
                        pending.token,
                        Answer::Pending(options[0].handle),
                    )
                    .unwrap(),
                );
                assert_batch(&game, &result, "P1", life, if count == 5 { 4 } else { 2 });
            }
        }
    }
}

#[test]
fn cp02_035_nine_original_costs_change_one_joint_batch_and_act_the_source() {
    for printing in ["CP02-035", "CP02-SL07"] {
        for count in [8_usize, 9] {
            for life in [1_u8, 20] {
                let mut initial = position("P1", printing, "BP01-037", 0, life);
                initial["players"]["P1"]["zones"]["hand"] = json!([]);
                initial["players"]["P1"]["zones"]["field"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!({"id":"source","card":printing}));
                initial["players"]["P1"]["zones"]["cemetery"] = json!((0..count)
                    .map(|i|json!({"id":format!("grave-{i}"),"card":format!("distinct-{i}"),"state":{"cost":0_u8}}))
                    .chain([json!({"id":"same-cost","card":"distinct-same-cost"})]).collect::<Vec<_>>());
                initial["players"]["P1"]["zones"]["ex"] =
                    json!([{"id":"outside","card":"distinct-8"}]);
                initial["players"]["P2"]["zones"]["cemetery"] =
                    json!([{"id":"foreign","card":"distinct-8"}]);
                let mut game = distinct_count::fixture(&initial);
                let ability = selection().current[printing]
                    .program
                    .card()
                    .abilities
                    .iter()
                    .find(|ability| ability.node.kind == AbilityKind::Activated)
                    .unwrap()
                    .node
                    .id;
                let result = resolved(
                    game.activate_internal_card("source", ability, PlayerId { seat: 1 })
                        .unwrap(),
                );
                assert_batch(&game, &result, "P1", life, if count == 9 { 7 } else { 3 });
                assert_eq!(
                    game.projection(View::Referee).unwrap()["objects"]["source"]["acted"],
                    true
                );
                if life == 20 {
                    game.resume_internal_card().unwrap();
                    let before = game.projection(View::Referee).unwrap();
                    game.activate_internal_card("source", ability, PlayerId { seat: 1 })
                        .unwrap_err();
                    assert_eq!(game.projection(View::Referee).unwrap(), before);
                }
            }
        }
    }
}
