use super::event_cemetery::{
    finish_checks, move_material, move_named, position, prepared, resolve_event,
};
use super::*;

#[test]
fn bp18_069_native_evolution_creates_adele_who_observes_the_real_ians_cemetery_exit() {
    let mut initial = position("BP18-068", "P1", "movement-other", "P1", "hand");
    initial["turn"]["elapsed_turns"] = json!({"P1":7_u8,"P2":6_u8});
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"resource","card":"BP18-069"}]);
    let mut game = prepared(
        &initial,
        move_named(
            ZoneKind::Field,
            OwnerZone::Cemetery,
            "ドラゴンバスター・イアン",
        ),
    );
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"resource","pay":{"pp":1_u8}})
        )
        .unwrap()
        .outcome,
        "resolved",
        "the real Ian evolves through his printed ability"
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        9_u8,
        "evolution consumes one PP"
    );
    finish_checks(&mut game);
    resolve_event(&mut game, "BP18-069", 1);
    let entered = game.projection(View::Referee).unwrap();
    let adele = entered["P1"]["field"]
        .as_array()
        .unwrap()
        .iter()
        .find(|id| entered["objects"][id.as_str().unwrap()]["card"] == "BP18-T06")
        .unwrap();
    assert_eq!(
        entered["objects"][adele.as_str().unwrap()]["power"],
        4_u8,
        "evolution creates the actual complete Adele token"
    );
    activate(&mut game, "P1");
    finish_checks(&mut game);
    resolve_event(&mut game, "BP18-T06", 1);
    let after = game.projection(View::Referee).unwrap();
    assert!(
        after["P1"]["cemetery"]
            .as_array()
            .unwrap()
            .contains(&json!("source")),
        "the real evolved Ian reaches the cemetery"
    );
    assert_eq!(
        after["P2"]["leader"]["life"], 15_u8,
        "his native Adele damages the opponent for five"
    );
    assert_eq!(
        after["P1"]["leader"]["life"], 20_u8,
        "the controller's leader is not damaged"
    );
}

#[test]
fn bp18_t06_requires_an_allied_ian_to_reach_the_cemetery_on_either_turn() {
    for (active, side, material, to, expected) in [
        ("P1", "P1", "BP18-068", OwnerZone::Cemetery, true),
        ("P2", "P1", "BP18-068", OwnerZone::Cemetery, true),
        ("P1", "P2", "BP18-068", OwnerZone::Cemetery, false),
        ("P1", "P1", "movement-unit", OwnerZone::Cemetery, false),
        ("P1", "P1", "BP18-068", OwnerZone::Hand, false),
    ] {
        let action = if material == "BP18-068" {
            move_named(ZoneKind::Field, to, "ドラゴンバスター・イアン")
        } else {
            move_material(ZoneKind::Field, to)
        };
        let mut game = prepared(
            &position("BP18-T06", active, material, side, "field"),
            action,
        );
        activate(&mut game, active);
        finish_checks(&mut game);
        resolve_event(&mut game, "BP18-T06", usize::from(expected));
        let after = game.projection(View::Referee).unwrap();
        assert_eq!(
            after["P2"]["leader"]["life"],
            if expected { 15_u8 } else { 20_u8 },
            "only the allied named follower exit damages the opponent"
        );
        assert_eq!(after["P1"]["leader"]["life"], 20_u8);
    }
}

#[test]
fn bp18_095_observes_followers_of_either_controller_on_either_turn() {
    for (active, side, material, to, expected) in [
        ("P1", "P1", "movement-unit", OwnerZone::Cemetery, true),
        ("P1", "P2", "movement-unit", OwnerZone::Cemetery, true),
        ("P2", "P1", "movement-unit", OwnerZone::Cemetery, true),
        ("P1", "P1", "movement-amulet", OwnerZone::Cemetery, false),
        ("P1", "P1", "movement-unit", OwnerZone::Hand, false),
    ] {
        let mut game = prepared(
            &position("BP18-095", active, material, side, "field"),
            move_material(ZoneKind::Field, to),
        );
        let power = game.projection(View::Referee).unwrap()["objects"]["source"]["power"]
            .as_u64()
            .unwrap();
        activate(&mut game, active);
        finish_checks(&mut game);
        resolve_event(&mut game, "BP18-095", usize::from(expected));
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["source"]["power"],
            power + u64::from(expected),
            "every qualifying follower exit grants one power"
        );
    }
}

#[test]
fn bp18_095_native_fanfare_may_pay_three_pp_and_destroy_its_formal_target_then_observe_that_exit() {
    for accepted in [false, true] {
        let mut initial = position("BP18-095", "P1", "movement-other", "P1", "hand");
        initial["players"]["P1"]["zones"]["field"] = json!([]);
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP18-095"}]);
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
            "the real Hamster enters before its Fanfare"
        );
        finish_checks(&mut game);
        let before = game.projection(View::Referee).unwrap();
        let pp = before["P1"]["pp"]["current"].as_u64().unwrap();
        let power = before["objects"]["source"]["power"].as_u64().unwrap();
        let occurrence = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = &occurrence.domain else {
            panic!("Fanfare occurrence")
        };
        assert_eq!(options.len(), 1);
        let mut step = game
            .answer_internal_card(
                occurrence.actor,
                occurrence.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap();
        let mut offered_cost = false;
        let mut declared_target = false;
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
            } else {
                panic!("native optional cost or formal enemy target")
            };
            step = game
                .answer_internal_card(declaration.actor, declaration.token, answer)
                .unwrap();
        }
        assert!(offered_cost, "the printed PP payment is optional");
        assert_eq!(
            declared_target, accepted,
            "the accepted ability formally declares its enemy target"
        );
        if accepted {
            resolved(step);
        } else {
            assert_eq!(step, CardPlay::Cancelled);
        }
        finish_checks(&mut game);
        resolve_event(&mut game, "BP18-095", usize::from(accepted));
        let after = game.projection(View::Referee).unwrap();
        assert_eq!(
            after["P1"]["pp"]["current"],
            pp - if accepted { 3 } else { 0 },
            "only acceptance pays three PP"
        );
        assert_eq!(
            after["P2"]["cemetery"]
                .as_array()
                .is_some_and(|zone| zone.contains(&json!("enemy"))),
            accepted,
            "only the paid Fanfare destroys the formal target"
        );
        assert_eq!(
            after["objects"]["source"]["power"],
            power + u64::from(accepted),
            "the same native Hamster observes the destroyed follower"
        );
    }
}
