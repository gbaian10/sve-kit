use super::*;
use alloc::collections::BTreeMap;
use sve_engine::game::internal::InputDomain;
use sve_engine::ir::lowering::ExpandedLowerer;

fn tree(steps: &Value) -> Value {
    let mut tree = fanfare::expanded::spell_tree().clone();
    tree["cards"]["SD03-005"]["abilities"][0]["spell"] = json!({
        "targets":[{"bind":"t","from":{"who":"opp","zone":"field","kind":"follower"},"n":1}],
        "do":steps
    });
    tree
}

#[test]
fn target_set_changes_current_life_without_erasing_prior_damage() {
    let cards = ExpandedLowerer::new()
        .unwrap()
        .lower_with_printed_costs(
            &tree(&json!([
                {"damage":{"to":"t","n":2}},
                {"set_stat":{"on":"t","power":1,"hp":1}}
            ])),
            &BTreeMap::from([("SD03-005".to_owned(), Nat(1))]),
        )
        .unwrap();
    for seat in [1, 2] {
        let actor = if seat == 1 { "P1" } else { "P2" };
        let opponent = if seat == 1 { "P2" } else { "P1" };
        let (catalog, mut initial) =
            fanfare::fragment_setup(seat, "SD03-005", 10, json!([{"id":"f","card":"BP01-037"}]));
        initial["players"][actor]["zones"]["deck"] = json!([]);
        initial["players"][opponent]["zones"]["field"] = json!([{"id":"f","card":"BP01-037"}]);
        let mut game =
            Game::new(catalog, &initial, &Value::Null, &Value::Null, "target-set").unwrap();
        game.load_internal_card("SD03-005", &cards["SD03-005"])
            .unwrap();
        game.load_internal_card("BP01-037", &card(vec![])).unwrap();
        let CardPlay::Awaiting(question) = game.play_internal_card("s", PlayerId { seat }).unwrap()
        else {
            panic!("target");
        };
        assert!(matches!(question.domain, InputDomain::Targets(_)));
        resolved(
            game.answer_internal_card(question.actor, question.token, Answer::Targets(vec![0]))
                .unwrap(),
        );
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["f"]["power"], 1);
        assert_eq!(state["objects"]["f"]["hp"], 1);
        assert_eq!(state["objects"]["f"]["damage"], 2);
        assert_eq!(state["objects"]["f"]["max_hp"], 3);
    }
}

#[test]
fn bounce_rejects_hidden_follow_and_wrong_zones_before_execution() {
    for movement in [
        json!({"ref":"t","to":"hand","result":"secret"}),
        json!({"ref":{"follow":"secret"},"to":"hand"}),
        json!({"ref":{"who":"opp","zone":"hand"},"to":"hand"}),
        json!({"ref":"t","to":"hand","position":"top"}),
        json!({"ref":{"in":"looked"},"to":"hand"}),
    ] {
        ExpandedLowerer::new()
            .unwrap()
            .lower_with_printed_costs(
                &tree(&json!([{"move":movement}])),
                &BTreeMap::from([("SD03-005".to_owned(), Nat(1))]),
            )
            .unwrap_err();
    }
}

#[test]
fn field_damage_keeps_amulets_outside_the_follower_domain() {
    let mut tree = fanfare::expanded::tree().clone();
    let mut entry = tree["cards"]["BP08-038"]["abilities"][0].clone();
    entry.as_object_mut().unwrap().remove("fanfare");
    entry["activate"] = json!({"cost":{"pp":0},"do":[{"damage":{"to":{"who":"opp","zone":"field","kind":"follower"},"n":1}}]});
    let meta = tree["cards"]["BP08-038"]["meta"].clone();
    tree["cards"] = json!({"unit-follower":{"meta":meta,"abilities":[entry]}});
    let programs = ExpandedLowerer::new().unwrap().lower(&tree).unwrap();
    for seat in [1, 2] {
        let actor = if seat == 1 { "P1" } else { "P2" };
        let opponent = if seat == 1 { "P2" } else { "P1" };
        let mut initial = setup(seat, json!([]));
        initial["turn"]["active"] = json!(actor);
        initial["players"][actor]["zones"]["hand"] = json!([]);
        initial["players"][actor]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
        initial["players"][opponent]["zones"]["field"] =
            json!([{"id":"f","card":"unit-follower"},{"id":"a","card":"unit-amulet"}]);
        let mut game = Game::new(
            catalog(),
            &initial,
            &Value::Null,
            &Value::Null,
            "field-filter",
        )
        .unwrap();
        game.load_internal_card("unit-follower", &programs["unit-follower"])
            .unwrap();
        game.load_internal_card("unit-amulet", &card(vec![]))
            .unwrap();
        let result = activate(&mut game, seat);
        assert_eq!(
            result
                .progress
                .facts
                .iter()
                .filter(|fact| matches!(fact, Fact::Damage(_)))
                .count(),
            1
        );
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["f"]["damage"], 1);
        assert_eq!(state["objects"]["s"]["damage"], 0);
        assert_eq!(state[opponent]["field"], json!(["f", "a"]));
    }
}

#[test]
fn public_cemetery_moves_reject_hidden_sources_and_unimplemented_receipts() {
    for movement in [
        json!({"ref":{"who":"self","zone":"hand"},"to":"cemetery"}),
        json!({"ref":"t","to":"cemetery","result":"moved"}),
        json!({"ref":"t","to":"cemetery","position":"top"}),
    ] {
        ExpandedLowerer::new()
            .unwrap()
            .lower_with_printed_costs(
                &tree(&json!([{"move":movement}])),
                &BTreeMap::from([("SD03-005".to_owned(), Nat(1))]),
            )
            .unwrap_err();
    }
}
