use super::event_cemetery::{finish_checks, move_named, position, prepared, resolve_event};
use super::*;

fn puppet_ids(state: &Value, zone: &str) -> Vec<Value> {
    state["P1"][zone]
        .as_array()
        .unwrap()
        .iter()
        .filter(|id| {
            matches!(
                state["objects"][id.as_str().unwrap()]["card"].as_str(),
                Some("BP05-T03" | "BP08-T09" | "BP14-T08")
            )
        })
        .cloned()
        .collect()
}

#[test]
fn bp15_006_real_fanfare_and_007_evolution_create_puppets_whose_exit_debuffs_a_formal_target() {
    let mut initial = position("BP15-006", "P1", "movement-unit", "P1", "field");
    initial["turn"]["elapsed_turns"] = json!({"P1":7_u8,"P2":6_u8});
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP15-006"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"resource","card":"BP15-007"}]);
    let mut game = prepared(
        &initial,
        move_named(ZoneKind::Field, OwnerZone::Cemetery, "操り人形"),
    );
    let entered = game
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap();
    assert!(
        matches!(entered, CardPlay::Entered(_)),
        "the real base enters from hand using its own printed program"
    );
    finish_checks(&mut game);
    resolve_event(&mut game, "BP15-006", 1);
    let after_fanfare = game.projection(View::Referee).unwrap();
    assert_eq!(
        puppet_ids(&after_fanfare, "ex").len(),
        1,
        "the base Fanfare creates one actual Puppet in EX"
    );
    let pp = after_fanfare["P1"]["pp"]["current"].as_u64().unwrap();
    let evolved = game
        .internal_rule_action(&json!({
            "do":"evolve","source":"source","evolve_card":"resource","pay":{"pp":1_u8}
        }))
        .unwrap();
    assert_eq!(
        evolved.outcome, "resolved",
        "the real base evolves for one PP"
    );
    finish_checks(&mut game);
    resolve_event(&mut game, "BP15-007", 1);
    let after_evolution = game.projection(View::Referee).unwrap();
    let field_puppets = puppet_ids(&after_evolution, "field");
    assert_eq!(field_puppets.len(), 1, "evolution creates one field Puppet");
    assert_eq!(
        puppet_ids(&after_evolution, "ex").len(),
        2,
        "evolution adds one EX Puppet to the Fanfare output"
    );
    assert_eq!(
        after_evolution["P1"]["pp"]["current"],
        pp.checked_sub(1).unwrap(),
        "evolution actually consumes its printed PP"
    );
    activate(&mut game, "P1");
    finish_checks(&mut game);
    resolve_event(&mut game, "BP15-007", 1);
    let after_exit = game.projection(View::Referee).unwrap();
    assert_eq!(
        after_exit["objects"]["enemy"]["power"], 9_u8,
        "the formal target loses one power"
    );
    assert_eq!(
        after_exit["objects"]["enemy"]["hp"], 19_u8,
        "the formal target loses one health"
    );
    for id in field_puppets {
        assert!(
            !after_exit["P1"]["field"].as_array().unwrap().contains(&id)
                && !after_exit["P1"]["cemetery"]
                    .as_array()
                    .unwrap()
                    .contains(&id),
            "the real Puppet is erased after its exit still produces an occurrence"
        );
    }
    assert_eq!(
        puppet_ids(&after_exit, "ex").len(),
        2,
        "EX Puppets do not move with the field token"
    );
}
