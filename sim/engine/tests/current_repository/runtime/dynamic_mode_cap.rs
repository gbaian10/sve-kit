#![expect(
    clippy::panic,
    reason = "Unexpected questions invalidate the dynamic printed mode scenario."
)]
use super::*;
use sve_engine::game::internal::DecisionView;

fn position(card: &str, other_count: u8, enemy: bool) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{ "id":"draw", "card":"BP01-037" }]);
    initial["players"]["P1"]["zones"]["field"] = json!(
        (0..other_count)
            .map(|i| json!({"id":format!("family{i}"),"card":"ECP01-048"}))
            .collect::<Vec<_>>()
    );
    initial["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"unrelated","card":"BP01-171"}));
    initial["players"]["P2"]["zones"]["field"] = if enemy {
        json!([{"id":"enemy","card":"BP01-037"},{"id":"opposing-family","card":"ECP01-048"}])
    } else {
        json!([])
    };
    initial
}

fn check_result(game: &Game, cap: u64, enemy: bool) {
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        state["P1"]["leader"]["life"],
        if cap > 1 { 12_u8 } else { 10 },
        "healing follows the selected third mode"
    );
    assert_eq!(
        state["P1"]["hand"].as_array().unwrap().len(),
        usize::from(cap > 1),
        "drawing follows the selected third mode"
    );
    assert_eq!(
        state["P2"]["banish"],
        if enemy && cap > 1 {
            json!(["enemy"])
        } else {
            json!([])
        },
        "banishment follows the selected first mode"
    );
    assert_eq!(
        state["objects"]["source"]["keywords"]
            .as_array()
            .unwrap()
            .contains(&json!("疾走")),
        cap != 2 || !enemy,
        "storm follows the selected second mode"
    );
}

fn validate_modes(game: &mut Game, choice: &DecisionView, cap: u64, enemy: bool) {
    let InputDomain::Modes { count, modes, .. } = &choice.domain else {
        panic!("mode limit");
    };
    assert_eq!(
        *count,
        Nat(cap),
        "only the controller's matching cards count, capped by the three printed modes"
    );
    assert_eq!(
        *modes,
        if enemy {
            vec![ModeId(1), ModeId(2), ModeId(3)]
        } else {
            vec![ModeId(2), ModeId(3)]
        },
        "a missing formal follower target excludes mode one"
    );
    let before = game.projection(View::Referee).unwrap();
    assert!(
        game.answer_internal_card(
            choice.actor,
            choice.token,
            Answer::Modes(vec![ModeId(2), ModeId(2)])
        )
        .is_err(),
        "a mode cannot be chosen twice"
    );
    if cap == 1 {
        assert!(
            game.answer_internal_card(
                choice.actor,
                choice.token,
                Answer::Modes(vec![ModeId(2), ModeId(3)])
            )
            .is_err(),
            "the evaluated cap is binding"
        );
    }
    assert_eq!(
        game.projection(View::Referee).unwrap(),
        before,
        "invalid answers preserve the whole authority position"
    );
}

fn run_case(card: &str, other_count: u8, enemy: bool) {
    let mut game = Game::from_selected_effects(
        selection_with_rules().clone(),
        &position(card, other_count, enemy),
        &Value::Null,
        &Value::Null,
        "dynamic-modes",
    )
    .unwrap();
    assert!(
        matches!(
            game.play_internal_card("source", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ),
        "the real base card enters before its printed fanfare"
    );
    if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("printed fanfare");
    };
    let CardPlay::Awaiting(choice) = game
        .answer_internal_card(
            pending.actor,
            pending.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap()
    else {
        panic!("dynamic mode declaration");
    };
    let cap = u64::from(other_count.saturating_add(1)).min(3);
    validate_modes(&mut game, &choice, cap, enemy);
    let chosen = if !enemy {
        vec![ModeId(3), ModeId(2)]
    } else if cap == 1 {
        vec![ModeId(2)]
    } else if cap == 2 {
        vec![ModeId(3), ModeId(1)]
    } else {
        vec![ModeId(3), ModeId(2), ModeId(1)]
    };
    let mut play = game
        .answer_internal_card(choice.actor, choice.token, Answer::Modes(chosen))
        .unwrap();
    if enemy && cap > 1 {
        let CardPlay::Awaiting(target) = play else {
            panic!("formal target before healing or drawing");
        };
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
            10_u8,
            "formal targets are declared before mode effects resolve"
        );
        let InputDomain::Targets(domain) = &target.domain else {
            panic!("banish target");
        };
        let member = domain
            .members
            .iter()
            .find(|member| member.card.as_deref() == Some("BP01-037"))
            .unwrap();
        play = game
            .answer_internal_card(
                target.actor,
                target.token,
                Answer::Targets(vec![member.handle]),
            )
            .unwrap();
    }
    resolved(play);
    if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
    check_result(&game, cap, enemy);
}

#[test]
fn ecp01_048_counts_own_mejiro_cards_and_declares_distinct_modes_before_targets() {
    for (card, other_count, enemy) in [
        ("ECP01-048", 0_u8, true),
        ("ECP01-SL23", 1, true),
        ("ECP01-SP23", 3, true),
        ("ECP01-SSP23", 1, false),
    ] {
        run_case(card, other_count, enemy);
    }
}
