use super::event_cemetery::finish_checks;
use super::*;

fn cost_position(other: Option<&str>) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"observer","card":"BP12-052"}]);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"payer","card":"BP15-074"}]);
    if let Some(other) = other {
        initial["players"]["P1"]["zones"]["hand"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"material","card":other,"state":{"cost":0_u8}}));
    }
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"d0","card":"BP01-171"},{"id":"d1","card":"BP01-171"},
        {"id":"spare","card":"BP01-171"}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([]);
    initial
}

fn game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "discard-timing",
    )
    .unwrap()
}

fn hand_activation() -> AbilityId {
    selection_with_rules().current["BP15-074"]
        .program
        .card()
        .abilities
        .iter()
        .find(|ability| {
            ability.node.kind == AbilityKind::Activated
                && ability.node.active_zones == [ZoneKind::Hand]
        })
        .unwrap()
        .node
        .id
}

#[test]
fn one_real_orb_cancer_cost_is_one_plesiosaurus_discard_batch() {
    let mut game = game(&cost_position(Some("BP01-080")));
    let CardPlay::Awaiting(question) = game
        .activate_internal_card("payer", hand_activation(), PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("the real hand activation chooses another discard material");
    };
    let InputDomain::CostMaterials(materials) = &question.domain else {
        panic!("private hand cost materials");
    };
    assert_eq!(
        materials.members.len(),
        1,
        "the source cannot pay both groups"
    );
    assert_eq!(materials.members[0].card.as_deref(), Some("BP01-080"));
    assert!(game.internal_decision(View::P2).is_none());
    resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::CostMaterials(vec![materials.members[0].handle]),
        )
        .unwrap(),
    );
    finish_checks(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["d0", "d1"]));
    assert_eq!(state["P1"]["cemetery"], json!(["payer", "material"]));
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("the printed observer must be pending");
    };
    assert_eq!(
        options.len(),
        1,
        "one printed two-card discard cost produces one batch occurrence"
    );
    let CardPlay::Awaiting(target) = game
        .answer_internal_card(
            pending.actor,
            pending.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap()
    else {
        panic!("the printed damage chooses an opponent");
    };
    let InputDomain::Targets(targets) = &target.domain else {
        panic!("damage target");
    };
    assert_eq!(
        targets.members.len(),
        1,
        "only the opposing leader is eligible"
    );
    resolved(
        game.answer_internal_card(
            target.actor,
            target.token,
            Answer::Targets(vec![targets.members[0].handle]),
        )
        .unwrap(),
    );
    finish_checks(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
        18_u8
    );
    assert!(game.internal_pending_choice().unwrap().is_none());
}

#[test]
fn orb_cancer_cannot_pay_with_itself_twice_or_with_an_unqualified_other_card() {
    for other in [None, Some("BP15-060"), Some("BP01-051")] {
        let mut initial = cost_position(other);
        if other.is_some() {
            initial["players"]["P1"]["zones"]["hand"][1]["state"]["cost"] = json!(99_u8);
        }
        let mut board = game(&initial);
        let before = board.projection(View::Referee).unwrap();
        board
            .activate_internal_card("payer", hand_activation(), PlayerId { seat: 1 })
            .unwrap_err();
        assert_eq!(
            board.projection(View::Referee).unwrap(),
            before,
            "one source, a low-original-cost Dragon, or another class cannot pay the printed cost"
        );
        assert!(board.internal_pending_choice().unwrap().is_none());
    }
}
