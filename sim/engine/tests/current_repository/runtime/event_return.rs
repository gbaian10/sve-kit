use super::event_cemetery::{
    finish_checks, move_material, move_named, position, prepared, resolve_event,
};
use super::*;

#[test]
fn may_printings_deal_damage_after_their_own_field_return() {
    for number in ["BP01-012", "BP01-P02", "ETD03-007"] {
        for (from, to, expected) in [
            (ZoneKind::Field, OwnerZone::Hand, true),
            (ZoneKind::Field, OwnerZone::Cemetery, false),
            (ZoneKind::Cemetery, OwnerZone::Hand, false),
        ] {
            let mut initial = position(number, "P1", "movement-unit", "P1", "field");
            if from == ZoneKind::Cemetery {
                initial["players"]["P1"]["zones"]["field"] = json!([]);
                initial["players"]["P1"]["zones"]["cemetery"] =
                    json!([{"id":"source","card":number}]);
            }
            let mut game = prepared(&initial, move_named(from, to, "ベビーエルフ・メイ"));
            activate(&mut game, "P1");
            finish_checks(&mut game);
            resolve_event(&mut game, number, usize::from(expected));
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["objects"]["enemy"]["damage"], u8::from(expected));
            if expected {
                assert!(
                    state["P1"]["hand"]
                        .as_array()
                        .unwrap()
                        .contains(&json!("source"))
                );
            }
        }
    }
}

#[test]
fn gerbera_bear_printings_heal_only_for_allied_field_returns() {
    for number in ["BP03-009", "BP03-P02", "ETD03-003"] {
        for (side, from, to, expected) in [
            ("P1", ZoneKind::Field, OwnerZone::Hand, true),
            ("P2", ZoneKind::Field, OwnerZone::Hand, false),
            ("P1", ZoneKind::Cemetery, OwnerZone::Hand, false),
            ("P1", ZoneKind::Field, OwnerZone::Cemetery, false),
        ] {
            let mut initial = position(
                number,
                "P1",
                "movement-unit",
                side,
                if from == ZoneKind::Field {
                    "field"
                } else {
                    "cemetery"
                },
            );
            initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
            let mut game = prepared(&initial, move_material(from, to));
            activate(&mut game, "P1");
            finish_checks(&mut game);
            resolve_event(&mut game, number, usize::from(expected));
            assert_eq!(
                game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
                10_u8 + u8::from(expected)
            );
        }
    }
}

#[test]
fn gerbera_bears_observe_their_own_return_before_the_source_leaves() {
    for number in ["BP03-009", "BP03-P02", "ETD03-003"] {
        let mut initial = position(number, "P1", "movement-unit", "P1", "field");
        initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
        let mut game = prepared(
            &initial,
            move_named(ZoneKind::Field, OwnerZone::Hand, "ガーベラベアー"),
        );
        activate(&mut game, "P1");
        finish_checks(&mut game);
        resolve_event(&mut game, number, 1);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["leader"]["life"], 11_u8);
        assert!(
            state["P1"]["hand"]
                .as_array()
                .unwrap()
                .contains(&json!("source"))
        );
    }
}

#[test]
fn evolved_gerbera_bear_returns_its_base_and_heals_once() {
    use sve_engine::game::internal::SearchAnswer;
    for (base, form) in [
        ("BP03-009", "BP03-010"),
        ("BP03-P02", "BP03-P03"),
        ("ETD03-003", "ETD03-004"),
    ] {
        let mut initial = position(base, "P1", "movement-unit", "P1", "field");
        initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
        initial["players"]["P1"]["zones"]["deck"] =
            json!([{"id":"flower","card":"BP03-016"}, {"id":"other","card":"BP01-037"}]);
        let mut game = prepared(
            &initial,
            move_named(ZoneKind::Field, OwnerZone::Hand, "ガーベラベアー"),
        );
        let step = game
            .internal_rule_action(
                &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}}),
            )
            .unwrap();
        assert_eq!(step.outcome, "resolved");
        let question = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = &question.domain else {
            panic!("evolved search trigger")
        };
        assert_eq!(options.len(), 1);
        assert_eq!(options[0].card, form);
        let CardPlay::Awaiting(search) = game
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap()
        else {
            panic!("printed search")
        };
        let InputDomain::Search(domain) = &search.domain else {
            panic!("deck search")
        };
        assert_eq!(domain.cards.len(), 1);
        assert_eq!(domain.cards[0].card, "BP03-016");
        resolved(
            game.answer_internal_card(
                search.actor,
                search.token,
                Answer::Search(SearchAnswer::Ungrouped(vec![domain.cards[0].handle])),
            )
            .unwrap(),
        );
        finish_checks(&mut game);
        activate(&mut game, "P1");
        finish_checks(&mut game);
        resolve_event(&mut game, form, 1);
        assert!(
            game.internal_pending_choice().unwrap().is_none(),
            "one return is one occurrence"
        );
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["leader"]["life"], 11_u8);
        assert_eq!(state["P1"]["evolve_deck"], json!(["form"]));
        assert!(
            state["P1"]["hand"]
                .as_array()
                .unwrap()
                .contains(&json!("source"))
        );
        assert!(
            state["P1"]["hand"]
                .as_array()
                .unwrap()
                .contains(&json!("flower"))
        );
        assert!(
            !state["P1"]["hand"]
                .as_array()
                .unwrap()
                .contains(&json!("form"))
        );
    }
}

#[test]
fn gerbera_bear_observes_a_token_return_before_rule_removal() {
    let mut initial = position("BP03-009", "P1", "movement-token", "P1", "field");
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    let mut game = prepared(&initial, move_material(ZoneKind::Field, OwnerZone::Hand));
    activate(&mut game, "P1");
    finish_checks(&mut game);
    resolve_event(&mut game, "BP03-009", 1);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["leader"]["life"], 11_u8);
    for zone in ["field", "hand", "cemetery"] {
        assert!(
            !state["P1"][zone]
                .as_array()
                .unwrap()
                .contains(&json!("material"))
        );
    }
}
