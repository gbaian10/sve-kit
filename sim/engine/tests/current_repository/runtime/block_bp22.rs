use super::event_cemetery::{
    finish_checks, move_material, move_named, position, prepared, resolve_event,
};
use super::*;

#[expect(
    clippy::panic,
    reason = "Unexpected questions invalidate the native Goblin Fanfare."
)]
fn resolve_fanfare(game: &mut Game, accepted: bool) {
    let occurrence = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &occurrence.domain else {
        panic!("native Goblin Fanfare")
    };
    assert_eq!(options.len(), 1, "only the native Fanfare is pending");
    let mut step = game
        .answer_internal_card(
            occurrence.actor,
            occurrence.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap();
    let mut offered_cost = false;
    let mut declared_target = false;
    let mut paid_material = false;
    while let CardPlay::Awaiting(declaration) = step {
        let answer = if matches!(&declaration.domain, InputDomain::OptionalCost) {
            offered_cost = true;
            Answer::OptionalCost(accepted)
        } else if let InputDomain::Targets(domain) = &declaration.domain {
            declared_target = true;
            let enemy = domain
                .members
                .iter()
                .find(|m| m.card.as_deref() == Some("movement-enemy"))
                .unwrap();
            Answer::Targets(vec![enemy.handle])
        } else if let InputDomain::CostMaterials(domain) = &declaration.domain {
            assert!(
                declared_target,
                "the formal enemy target is fixed before paying the field material"
            );
            assert_eq!(
                domain.members.len(),
                1,
                "the source itself and the opposing Goblin cannot pay this cost"
            );
            assert_eq!(
                domain.members[0].card.as_deref(),
                Some("BP22-089"),
                "the allied native Goblin is the eligible payment"
            );
            paid_material = true;
            Answer::CostMaterials(vec![domain.members[0].handle])
        } else {
            panic!("native optional cost, formal target, or Goblin payment")
        };
        step = game
            .answer_internal_card(declaration.actor, declaration.token, answer)
            .unwrap();
    }
    assert!(offered_cost, "the Fanfare material payment is optional");
    assert_eq!(
        declared_target, accepted,
        "only acceptance declares the formal enemy target"
    );
    assert_eq!(
        paid_material, accepted,
        "only acceptance selects a Goblin cost material"
    );
    if accepted {
        resolved(step);
    } else {
        assert_eq!(
            step,
            CardPlay::Cancelled,
            "declining the payment cancels the Fanfare"
        );
    }
    finish_checks(game);
}

#[test]
fn bp22_081_native_fanfare_targets_before_paying_another_allied_goblin_and_observes_that_payment() {
    for accepted in [false, true] {
        let mut initial = position("BP22-081", "P1", "BP22-089", "P1", "field");
        initial["players"]["P1"]["leader"]["life"] = json!(15_u8);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"material","card":"BP22-089"}]);
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP22-081"}]);
        initial["players"]["P2"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"opposing-goblin","card":"BP22-089"}));
        let mut game = prepared(
            &initial,
            move_material(ZoneKind::Field, OwnerZone::Cemetery),
        );
        assert!(
            matches!(
                game.play_internal_card("source", PlayerId { seat: 1 })
                    .unwrap(),
                CardPlay::Entered(_)
            ),
            "the real Goblin enters before its Fanfare"
        );
        finish_checks(&mut game);
        let before = game.projection(View::Referee).unwrap();
        assert!(
            before["objects"]["source"]["keywords"]
                .as_array()
                .unwrap()
                .contains(&json!("疾走")),
            "the new whole-card program retains its native Storm"
        );
        resolve_fanfare(&mut game, accepted);
        resolve_event(&mut game, "BP22-081", usize::from(accepted));
        let after = game.projection(View::Referee).unwrap();
        assert_eq!(
            after["P1"]["pp"]["current"], before["P1"]["pp"]["current"],
            "the optional payment uses only the allied Goblin"
        );
        assert_eq!(
            after["P1"]["cemetery"]
                .as_array()
                .is_some_and(|zone| zone.contains(&json!("material"))),
            accepted,
            "only the allied Goblin pays the accepted cost"
        );
        assert_eq!(
            after["P2"]["cemetery"]
                .as_array()
                .is_some_and(|zone| zone.contains(&json!("enemy"))),
            accepted,
            "only payment destroys the formal target"
        );
        assert!(
            after["P2"]["field"]
                .as_array()
                .unwrap()
                .contains(&json!("opposing-goblin")),
            "the opposing Goblin is never a cost material"
        );
        assert_eq!(
            after["P1"]["leader"]["life"],
            if accepted { 16_u8 } else { 15_u8 },
            "the native observer heals for its paid Goblin's exit"
        );
        assert_eq!(
            after["P2"]["leader"]["life"],
            if accepted { 19_u8 } else { 20_u8 },
            "that exit also damages the opponent"
        );
    }
}

#[test]
fn bp22_081_requires_an_allied_goblin_follower_to_reach_the_cemetery_on_its_own_turn() {
    for (active, side, material, to, expected) in [
        ("P1", "P1", "BP22-089", OwnerZone::Cemetery, true),
        ("P2", "P1", "BP22-089", OwnerZone::Cemetery, false),
        ("P1", "P2", "BP22-089", OwnerZone::Cemetery, false),
        ("P1", "P1", "movement-unit", OwnerZone::Cemetery, false),
        ("P1", "P1", "BP22-089", OwnerZone::Hand, false),
    ] {
        let mut initial = position("BP22-081", active, material, side, "field");
        initial["players"]["P1"]["leader"]["life"] = json!(15_u8);
        let action = if material == "BP22-089" {
            move_named(ZoneKind::Field, to, "ゴブリンゾンビ")
        } else {
            move_material(ZoneKind::Field, to)
        };
        let mut game = prepared(&initial, action);
        activate(&mut game, active);
        finish_checks(&mut game);
        resolve_event(&mut game, "BP22-081", usize::from(expected));
        let after = game.projection(View::Referee).unwrap();
        assert_eq!(
            after["P1"]["leader"]["life"],
            if expected { 16_u8 } else { 15_u8 },
            "only the matching exit heals its controller"
        );
        assert_eq!(
            after["P2"]["leader"]["life"],
            if expected { 19_u8 } else { 20_u8 },
            "only the matching exit damages the opponent"
        );
    }
}
