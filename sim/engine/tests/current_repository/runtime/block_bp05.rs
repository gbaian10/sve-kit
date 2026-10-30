use super::event_cemetery::{finish_checks, move_material, position, prepared, resolve_event};
use super::*;

#[test]
fn bp05_077_real_evolution_creates_two_puppets_and_observes_the_next_own_follower_exit() {
    let mut initial = position("BP05-076", "P1", "movement-unit", "P1", "field");
    initial["turn"]["elapsed_turns"] = json!({"P1":7_u8,"P2":6_u8});
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"resource","card":"BP05-077"}]);
    let mut game = prepared(
        &initial,
        move_material(ZoneKind::Field, OwnerZone::Cemetery),
    );
    let step = game
        .internal_rule_action(&json!({
            "do":"evolve","source":"source","evolve_card":"resource","pay":{"pp":1_u8}
        }))
        .unwrap();
    assert_eq!(step.outcome, "resolved", "the real base evolves for one PP");
    finish_checks(&mut game);
    resolve_event(&mut game, "BP05-077", 1);
    let after_evolution = game.projection(View::Referee).unwrap();
    assert_eq!(
        after_evolution["P1"]["pp"]["current"], 9_u8,
        "the printed evolution cost is actually paid"
    );
    let field_cards = after_evolution["P1"]["field"]
        .as_array()
        .unwrap()
        .iter()
        .map(|id| {
            after_evolution["objects"][id.as_str().unwrap()]["card"]
                .as_str()
                .unwrap()
        })
        .collect::<Vec<_>>();
    let puppets = field_cards
        .iter()
        .filter(|card| matches!(**card, "BP05-T03" | "BP08-T09"))
        .count();
    assert_eq!(
        puppets, 2,
        "two actual Puppet printings enter the field: {field_cards:?}"
    );
    let power = after_evolution["objects"]["source"]["power"]
        .as_u64()
        .unwrap();
    activate(&mut game, "P1");
    finish_checks(&mut game);
    resolve_event(&mut game, "BP05-077", 1);
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["source"]["power"],
        power.checked_add(1).unwrap(),
        "the evolved print gains one power for its own follower entering the cemetery"
    );
}

#[test]
fn bp05_077_rejects_opposing_followers_other_turns_amulets_and_other_destinations() {
    for (active, side, material, to) in [
        ("P1", "P2", "movement-unit", OwnerZone::Cemetery),
        ("P2", "P1", "movement-unit", OwnerZone::Cemetery),
        ("P1", "P1", "movement-amulet", OwnerZone::Cemetery),
        ("P1", "P1", "movement-unit", OwnerZone::Hand),
    ] {
        let mut game = prepared(
            &position("BP05-077", active, material, side, "field"),
            move_material(ZoneKind::Field, to),
        );
        let before = game.projection(View::Referee).unwrap()["objects"]["source"]["power"].clone();
        activate(&mut game, active);
        finish_checks(&mut game);
        resolve_event(&mut game, "BP05-077", 0);
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["source"]["power"],
            before,
            "the cemetery subscription excludes the injected counterexample"
        );
    }
}
