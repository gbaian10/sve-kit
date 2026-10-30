use super::event_cemetery::{
    finish_checks, move_material, move_named, position, prepared, resolve_event,
};
use super::*;
use sve_engine::game::internal::SearchAnswer;

fn babies(game: &Game) -> usize {
    let state = game.projection(View::Referee).unwrap();
    state["P1"]["field"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|id| state["objects"][id.as_str().unwrap()]["card"] == "BP16-T03")
        .count()
}

#[test]
fn bp16_070_native_evolution_searches_an_egg_and_its_real_cost_exit_creates_a_baby_once() {
    let mut initial = position("BP16-069", "P1", "BP04-078", "P1", "field");
    initial["turn"]["elapsed_turns"] = json!({"P1":7_u8,"P2":6_u8});
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"resource","card":"BP16-070"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"searched-egg","card":"BP04-078"},
        {"id":"draw-one","card":"movement-other"},
        {"id":"draw-two","card":"movement-other"}
    ]);
    let mut game = prepared(
        &initial,
        move_material(ZoneKind::Field, OwnerZone::Cemetery),
    );
    let evolution = game
        .internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"resource","pay":{"pp":1_u8}}),
        )
        .unwrap();
    assert_eq!(
        evolution.outcome, "resolved",
        "the native base evolves for one PP"
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        9_u8,
        "the real evolution consumes its printed PP"
    );
    finish_checks(&mut game);
    let occurrence = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &occurrence.domain else {
        panic!("native evolution occurrence")
    };
    assert_eq!(
        options.len(),
        1,
        "only the printed evolution ability is pending"
    );
    assert_eq!(options[0].card, "BP16-070");
    let step = game
        .answer_internal_card(
            occurrence.actor,
            occurrence.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap();
    let CardPlay::Awaiting(search) = step else {
        panic!("native egg search")
    };
    let InputDomain::Search(domain) = &search.domain else {
        panic!("native egg search domain")
    };
    assert_eq!(
        domain.cards.len(),
        1,
        "only the actual egg in the deck qualifies"
    );
    resolved(
        game.answer_internal_card(
            search.actor,
            search.token,
            Answer::Search(SearchAnswer::Ungrouped(vec![domain.cards[0].handle])),
        )
        .unwrap(),
    );
    finish_checks(&mut game);
    assert!(
        game.projection(View::Referee).unwrap()["P1"]["field"]
            .as_array()
            .unwrap()
            .contains(&json!("searched-egg")),
        "the native search places its selection on the field"
    );
    for (id, pending) in [("searched-egg", 1), ("material", 0)] {
        resolved(
            game.activate_internal_card(id, AbilityId(1), PlayerId { seat: 1 })
                .unwrap(),
        );
        finish_checks(&mut game);
        resolve_event(&mut game, "BP16-070", pending);
        let state = game.projection(View::Referee).unwrap();
        assert!(
            state["P1"]["cemetery"]
                .as_array()
                .unwrap()
                .contains(&json!(id)),
            "the actual amulet pays its own burial cost"
        );
        assert_eq!(
            babies(&game),
            1,
            "the second egg in the turn cannot create a second native Baby"
        );
    }
}

#[test]
fn bp16_070_accepts_own_named_amulets_on_either_turn_but_requires_the_name_owner_and_destination() {
    for (active, side, material, to, expected) in [
        ("P1", "P1", "BP04-078", OwnerZone::Cemetery, true),
        ("P2", "P1", "BP04-078", OwnerZone::Cemetery, true),
        ("P1", "P2", "BP04-078", OwnerZone::Cemetery, false),
        ("P1", "P1", "movement-amulet", OwnerZone::Cemetery, false),
        ("P1", "P1", "BP04-078", OwnerZone::Hand, false),
    ] {
        let action = if material == "BP04-078" {
            move_named(ZoneKind::Field, to, "ドラゴンの卵")
        } else {
            move_material(ZoneKind::Field, to)
        };
        let mut game = prepared(
            &position("BP16-070", active, material, side, "field"),
            action,
        );
        activate(&mut game, active);
        finish_checks(&mut game);
        resolve_event(&mut game, "BP16-070", usize::from(expected));
        assert_eq!(
            babies(&game),
            usize::from(expected),
            "only a named allied egg reaching the cemetery creates the native token"
        );
    }
}
