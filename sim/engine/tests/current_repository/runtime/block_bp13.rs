use super::event_cemetery::{finish_checks, move_material, position, prepared, resolve_event};
use super::*;

#[test]
fn bp13_108_damages_only_the_opposing_leader_for_own_follower_exits_on_its_turn() {
    for (active, side, material, to, expected) in [
        ("P1", "P1", "movement-unit", OwnerZone::Cemetery, true),
        ("P1", "P2", "movement-unit", OwnerZone::Cemetery, false),
        ("P2", "P1", "movement-unit", OwnerZone::Cemetery, false),
        ("P1", "P1", "movement-amulet", OwnerZone::Cemetery, false),
        ("P1", "P1", "movement-unit", OwnerZone::Hand, false),
    ] {
        let mut game = prepared(
            &position("BP13-108", active, material, side, "field"),
            move_material(ZoneKind::Field, to),
        );
        activate(&mut game, active);
        finish_checks(&mut game);
        resolve_event(&mut game, "BP13-108", usize::from(expected));
        let after = game.projection(View::Referee).unwrap();
        assert_eq!(
            after["P2"]["leader"]["life"],
            if expected { 19_u8 } else { 20_u8 },
            "only a matching own-turn follower exit damages the opponent"
        );
        assert_eq!(
            after["P1"]["leader"]["life"], 20_u8,
            "the observer's controller never takes its own leader damage"
        );
    }
}

#[test]
fn bp13_108_real_evolution_rejects_one_pp_and_pays_its_printed_two_pp() {
    let mut initial = position("BP13-108", "P1", "movement-unit", "P1", "field");
    initial["turn"]["elapsed_turns"] = json!({"P1":7_u8,"P2":6_u8});
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"resource","card":"BP13-109"}]);
    let mut game = prepared(
        &initial,
        move_material(ZoneKind::Field, OwnerZone::Cemetery),
    );
    let rejected = game
        .internal_rule_action(&json!({
            "do":"evolve","source":"source","evolve_card":"resource","pay":{"pp":1_u8}
        }))
        .unwrap();
    assert_eq!(
        rejected.outcome, "cannot-evolve",
        "one PP does not meet the printed cost"
    );
    assert!(
        rejected.events.is_empty(),
        "underpayment never reveals or moves the evolution resource"
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        10_u8,
        "the rejected declaration preserves PP"
    );
    let paid = game
        .internal_rule_action(&json!({
            "do":"evolve","source":"source","evolve_card":"resource","pay":{"pp":2_u8}
        }))
        .unwrap();
    assert_eq!(
        paid.outcome, "resolved",
        "two PP pays for the real evolution"
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        8_u8,
        "the printed two PP is actually spent"
    );
}
