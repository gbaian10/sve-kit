use super::event_cemetery::{move_material, position, prepared};
use super::*;

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
