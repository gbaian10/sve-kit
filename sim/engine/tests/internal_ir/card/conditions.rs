use super::*;
use alloc::collections::BTreeMap;
use sve_engine::ir::lowering::ExpandedLowerer;

fn tree(steps: &Value) -> Value {
    let mut tree = fanfare::expanded::spell_tree().clone();
    tree["cards"]["SD03-005"]["abilities"][0]["spell"] = json!({"do":steps});
    tree
}

#[test]
fn boolean_conditions_use_current_pp_and_unfiltered_hidden_counts_without_exposing_cards() {
    let programs = ExpandedLowerer::new().unwrap().lower_with_printed_costs(
        &tree(&json!([{"if":{
            "cond":{"all":[
                {"cmp":{"left":{"resource":{"who":"self","kind":"pp"}},"op":"eq","right":2}},
                {"cmp":{"left":{"count":{"who":"self","zone":"deck"}},"op":"ge","right":1}},
                {"any":[
                    {"not":{"cmp":{"left":{"count":{"who":"opp","zone":"hand"}},"op":"eq","right":0}}},
                    {"cmp":{"left":1,"op":"gt","right":2}}
                ]}
            ]},
            "then":[{"draw":{"n":1}}],
            "else":[{"damage":{"to":{"leaders":"all"},"n":1}}]
        }}])),
        &BTreeMap::from([("SD03-005".to_owned(), Nat(1))]),
    ).unwrap();
    for hidden in [false, true] {
        let (catalog, mut initial) =
            fanfare::fragment_setup(1, "SD03-005", 3, json!([{"id":"drawn","card":"BP01-037"}]));
        initial["players"]["P2"]["zones"]["hand"] = if hidden {
            json!([{"id":"secret","card":"BP01-037"}])
        } else {
            json!([])
        };
        let mut game = Game::new(
            catalog,
            &initial,
            &Value::Null,
            &Value::Null,
            "body-condition",
        )
        .unwrap();
        game.load_internal_card("SD03-005", &programs["SD03-005"])
            .unwrap();
        game.load_internal_card("BP01-037", &card(vec![])).unwrap();
        resolved(game.play_internal_card("s", PlayerId { seat: 1 }).unwrap());
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P1"]["hand"],
            if hidden { json!(["drawn"]) } else { json!([]) }
        );
        assert_eq!(state["P2"]["leader"]["life"], if hidden { 20 } else { 19 });
        let public = game.projection(View::P1).unwrap();
        assert!(public["objects"].get("secret").is_none());
    }
}

#[test]
fn conditions_reject_filtered_hidden_counts_and_non_numeric_damage_replacements() {
    for steps in [
        json!([{"if":{"cond":{"cmp":{"left":{"count":{"who":"opp","zone":"hand","kind":"follower"}},"op":"ge","right":1}},"then":[{"draw":{"n":1}}]}}]),
        json!([{"damage":{"to":{"leaders":"all"},"n":1,"instead":[{"if":{"cmp":{"left":1,"op":"eq","right":1}},"to":{"leader":"self"}}]}}]),
        json!([{"damage":{"to":{"leaders":"all"},"n":1,"instead":[
            {"if":{"cmp":{"left":1,"op":"eq","right":1}},"n":2},
            {"if":{"cmp":{"left":1,"op":"eq","right":1}},"n":3}
        ]}}]),
    ] {
        ExpandedLowerer::new()
            .unwrap()
            .lower_with_printed_costs(
                &tree(&steps),
                &BTreeMap::from([("SD03-005".to_owned(), Nat(1))]),
            )
            .unwrap_err();
    }
}

#[test]
fn count_admission_does_not_authorize_hand_objects_as_effect_targets() {
    let hidden = Selector::Zone {
        players: PlayerSet {
            relation: PlayerRelation::Opponents,
            relative_to: PlayerRef::Controller {},
            cardinality: Cardinality {
                minimum: Nat(1),
                maximum: Some(Nat(1)),
            },
            text_quantifier: TextQuantifier::Implicit,
        },
        zone: ZoneKind::Hand,
        view: SelectionView::Current {},
        binding: Binding::new(99, ScopeId(1)),
        predicate: Box::new(Expr::Literal(Bool(true))),
    };
    let program = card(vec![ability(
        1,
        vec![instruction(1, Action::Destroy { objects: hidden })],
    )]);
    let mut game = Game::new(
        catalog(),
        &setup(1, json!([])),
        &Value::Null,
        &Value::Null,
        "hidden-selector",
    )
    .unwrap();
    game.load_internal_card("unit-follower", &program)
        .unwrap_err();
}

#[test]
fn leader_life_threshold_and_expression_read_after_the_preceding_life_change() {
    for (who, life) in [("self", 9_i64), ("opp", 20_i64)] {
        let programs = ExpandedLowerer::new()
            .unwrap()
            .lower_with_printed_costs(
                &tree(&json!([
                    {"life":{"who":"self","delta":-11}},
                    {"if":{"cond":{"all":[
                        {"leader_life":{"who":who,"eq":life}},
                        {"leader_life":{"who":who,"le":life}},
                        {"leader_life":{"who":who,"ge":life}},
                        {"cmp":{"left":{"leader_life":who},"op":"eq","right":life}}
                    ]},"then":[{"draw":{"n":1}}]}}
                ])),
                &BTreeMap::from([("SD03-005".to_owned(), Nat(1))]),
            )
            .unwrap();
        let (catalog, initial) =
            fanfare::fragment_setup(1, "SD03-005", 3, json!([{"id":"drawn","card":"BP01-037"}]));
        let mut game =
            Game::new(catalog, &initial, &Value::Null, &Value::Null, "live-life").unwrap();
        game.load_internal_card("SD03-005", &programs["SD03-005"])
            .unwrap();
        game.load_internal_card("BP01-037", &card(vec![])).unwrap();
        resolved(game.play_internal_card("s", PlayerId { seat: 1 }).unwrap());
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["leader"]["life"], 9_i64);
        assert_eq!(state["P1"]["hand"], json!(["drawn"]));
    }
}

#[test]
fn live_condition_syntax_rejects_false_awakening_and_ambiguous_thresholds() {
    for condition in [
        json!({"awakened":false}),
        json!({"leader_life":{"who":"self"}}),
        json!({"leader_life":{"who":"self","le":10,"ge":5}}),
        json!({"leader_life":{"who":"all","eq":10}}),
    ] {
        ExpandedLowerer::new()
            .unwrap()
            .lower_with_printed_costs(
                &tree(&json!([{"if":{"cond":condition,"then":[{"draw":{"n":1}}]}}])),
                &BTreeMap::from([("SD03-005".to_owned(), Nat(1))]),
            )
            .unwrap_err();
    }
}

#[test]
fn crimson_false_is_rejected_instead_of_becoming_a_negated_history_condition() {
    ExpandedLowerer::new()
        .unwrap()
        .lower_with_printed_costs(
            &tree(&json!([{"if":{"cond":{"crimson":false},"then":[{"draw":{"n":1}}]}}])),
            &BTreeMap::from([("SD03-005".to_owned(), Nat(1))]),
        )
        .unwrap_err();
}
