use super::*;

#[test]
fn draws_only_for_its_own_attack() {
    // CR 8.4.1 and 8.4.5: the attack event is produced by the attacking follower.
    for seat in ["P1", "P2"] {
        let opponent = if seat == "P1" { "P2" } else { "P1" };
        let mut initial = draw_position();
        initial["turn"]["active"] = json!(seat);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"watcher","card":"BP10-122"}]);
        initial["players"]["P2"]["zones"]["field"] = json!([{"id":"watcher","card":"BP10-122"}]);
        initial["players"][seat]["zones"]["field"] = json!([{"id":"attacker","card":"BP10-122"}]);
        for owner in ["P1", "P2"] {
            initial["players"][owner]["zones"]["deck"] = json!([
                {"id":format!("{owner}-d1"),"card":"CP02-068"},
                {"id":format!("{owner}-d2"),"card":"BP10-122"}
            ]);
        }
        let mut game = game(&initial);
        let attack = game
            .internal_rule_action(
                &json!({"do":"attack","attacker":"attacker","target":format!("{opponent}.leader")}),
            )
            .unwrap();
        assert_eq!(attack.outcome, "resolved");

        let question = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = &question.domain else {
            panic!("attack trigger pending")
        };
        assert_eq!(options.len(), 1);
        assert_eq!(options[0].card, "BP10-122");
        assert_eq!(options[0].line, 1);
        assert!(options[0].event.contains("Attack"));
        assert_eq!(question.actor.seat, if seat == "P1" { 1 } else { 2 });
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
                .facts
                .iter()
                .any(|fact| matches!(fact, Fact::Draw(_)))
        );
        for view in [View::P1, View::P2, View::Referee] {
            let state = game.projection(view).unwrap();
            assert_eq!(projected_zone_count(&state[seat]["hand"]), 1_u64);
            assert_eq!(projected_zone_count(&state[seat]["deck"]), 1_u64);
            assert_eq!(projected_zone_count(&state[opponent]["hand"]), 0_u64);
            assert_eq!(projected_zone_count(&state[opponent]["deck"]), 2_u64);
            if view == if seat == "P1" { View::P2 } else { View::P1 } {
                assert!(
                    !state[seat]["hand"].to_string().contains("-d1"),
                    "the opponent never sees the drawn card"
                );
            }
            if view == View::Referee {
                assert_eq!(state[seat]["hand"], json!([format!("{seat}-d1")]));
                assert_eq!(state[seat]["deck"], json!([format!("{seat}-d2")]));
            }
        }
    }
}

#[test]
fn playing_bp10_122_from_hand_does_not_trigger_attack_time() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"follower","card":"BP10-122"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"d1","card":"CP02-068"}]);
    let mut game = game(&initial);
    assert!(matches!(
        game.play_internal_card("follower", PlayerId { seat: 1 })
            .unwrap(),
        CardPlay::Entered(_)
    ));
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["deck"],
        json!(["d1"])
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["hand"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
    game.resume_internal_card().unwrap();
    assert!(game.internal_pending_choice().unwrap().is_none());
    for view in [View::P1, View::P2, View::Referee] {
        let state = game.projection(view).unwrap();
        assert_eq!(projected_zone_count(&state["P1"]["hand"]), 0_u64);
        assert_eq!(projected_zone_count(&state["P1"]["deck"]), 1_u64);
        if view == View::Referee {
            assert_eq!(state["P1"]["deck"], json!(["d1"]));
        }
    }
}

#[test]
fn source_mutations_cannot_change_attack_head_actor_count_or_provenance() {
    const REJECTIONS: [&str; 4] = [
        "current_ability_closure_mismatch",
        "unsupported_construct",
        "current_ability_closure_mismatch",
        "invalid_current_source",
    ];
    // head moved, actor flipped (unsupported), count changed, provenance dropped.
    let (snapshot, keywords, accepted) = inputs();
    for (probe, rejection) in REJECTIONS.iter().enumerate() {
        let mut selection = accepted.clone();
        let author = selection["authors"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|author| author["input"]["card_no"] == "BP10-122")
            .unwrap();
        let candidate = &mut author["input"];
        match probe {
            0 => {
                let ability = &mut candidate["tree"]["cards"]["BP10-122"]["abilities"][0];
                let body = ability
                    .as_object_mut()
                    .unwrap()
                    .remove("attack_time")
                    .unwrap();
                ability
                    .as_object_mut()
                    .unwrap()
                    .insert("evolve_time".into(), body);
            }
            1 => {
                candidate["tree"]["cards"]["BP10-122"]["abilities"][0]["attack_time"]["do"][0]["draw"]
                    ["who"] = json!("opponent");
            }
            2 => {
                candidate["tree"]["cards"]["BP10-122"]["abilities"][0]["attack_time"]["do"][0]["draw"]
                    ["n"] = json!(2_u8);
            }
            // The last probe drops the source sidecar.
            _ => candidate["sidecar"] = json!([]),
        }
        assert!(
            failure(Catalog::from_current_selection(
                &snapshot, &keywords, &selection
            ))
            .contains(*rejection),
            "mutated input {probe} must be rejected for its own reason"
        );
    }
}
