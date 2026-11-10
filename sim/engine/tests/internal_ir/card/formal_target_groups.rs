use super::*;
use alloc::collections::BTreeMap;
use sve_engine::game::internal::InputDomain;
use sve_engine::ir::lowering::LoweringError;

fn lower(tree: &Value) -> Result<BTreeMap<String, WellFormedCardProgram>, LoweringError> {
    // Spells lower only with their printed PP cost, matching the synthetic catalog.
    lowerer().lower_with_printed_costs(tree, &BTreeMap::from([("unit-spell".to_owned(), Nat(1))]))
}

fn tree(count: u8, required: bool) -> Value {
    let mut tree = fanfare::expanded::tree().clone();
    let mut entry = tree["cards"]["BP08-038"]["abilities"][0].clone();
    entry.as_object_mut().unwrap().remove("fanfare");
    let mut target = json!({
        "bind":"chosen","from":{"who":"self","zone":"cemetery","kind":"follower"},
        "groups":vec![json!({"orig_cost_le":3}); usize::from(count)]
    });
    target[if required { "n" } else { "upto" }] = json!(count);
    entry["spell"] = json!({"targets":[target],"do":[{"move":{"ref":"chosen","to":"field","result":"entered"}}]});
    let meta = tree["cards"]["BP08-038"]["meta"].clone();
    tree["cards"] = json!({"unit-spell":{"meta":meta,"abilities":[entry]}});
    tree
}

fn loaded(count: u8, required: bool, candidates: usize) -> Game {
    let typed = lower(&tree(count, required))
        .unwrap()
        .remove("unit-spell")
        .unwrap();
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["cemetery"] = json!(
        (0..candidates)
            .map(|index| json!({"id":format!("c{index}"),"card":"unit-follower"}))
            .collect::<Vec<_>>()
    );
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "grouped-targets",
    )
    .unwrap();
    game.load_internal_card("unit-spell", &typed).unwrap();
    game.load_internal_card("unit-follower", &card(vec![]))
        .unwrap();
    game
}

#[test]
fn required_groups_match_distinct_objects_in_a_large_cemetery() {
    let mut game = loaded(2, true, 40);
    let CardPlay::Awaiting(question) = game.play_internal_card("s", PlayerId { seat: 1 }).unwrap()
    else {
        panic!("targets")
    };
    game.answer_internal_card(question.actor, question.token, Answer::Targets(vec![0, 0]))
        .unwrap_err();
    resolved(
        game.answer_internal_card(question.actor, question.token, Answer::Targets(vec![0, 1]))
            .unwrap(),
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["field"], json!(["c0", "c1"]));
    // 40 candidates minus the two entered followers, plus the resolved spell itself.
    let cemetery = state["P1"]["cemetery"].as_array().unwrap();
    assert_eq!(cemetery.len(), 39);
    assert!(cemetery.contains(&json!("s")));
}

#[test]
fn one_object_cannot_supply_two_required_groups() {
    let mut game = loaded(2, true, 1);
    game.play_internal_card("s", PlayerId { seat: 1 })
        .unwrap_err();
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["hand"],
        json!(["s"])
    );
}

#[test]
fn three_optional_groups_allow_zero_and_overlap_only_once_per_card() {
    for handles in [vec![], vec![0], vec![0, 1, 2]] {
        let mut game = loaded(3, false, 12);
        let CardPlay::Awaiting(question) =
            game.play_internal_card("s", PlayerId { seat: 1 }).unwrap()
        else {
            panic!("targets")
        };
        let InputDomain::Targets(domain) = &question.domain else {
            panic!("targets")
        };
        assert_eq!(
            domain.cardinality,
            Cardinality {
                minimum: Nat(0),
                maximum: Some(Nat(3))
            }
        );
        game.answer_internal_card(question.actor, question.token, Answer::Targets(vec![0, 0]))
            .unwrap_err();
        let expected = handles.len();
        resolved(
            game.answer_internal_card(question.actor, question.token, Answer::Targets(handles))
                .unwrap(),
        );
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["field"]
                .as_array()
                .unwrap()
                .len(),
            expected
        );
    }
}

#[test]
fn grouped_targets_reject_unneeded_zones_counts_and_combined_constraints() {
    lower(&tree(2, true)).unwrap();
    for change in [
        json!({"n":1}),
        json!({"cost_sum_le":5}),
        json!({"distinct_by":"name"}),
        json!({"from":{"who":"self","zone":"hand","kind":"follower"}}),
        json!({"from":{"who":"opp","zone":"cemetery","kind":"follower"}}),
        json!({"groups":[{"orig_cost_le":3}]}),
        json!({"groups":[{"orig_cost_le":3,"orig_cost_eq":1},{"orig_cost_le":3}]}),
        json!({"groups":[{"orig_cost_le":3},{"orig_cost_le":3},{"orig_cost_le":3}],"n":3}),
        json!({"n":null,"upto":1}),
        json!({"from":{"who":"self","zone":"cemetery","kind":"follower","orig_cost_le":5}}),
    ] {
        let mut input = tree(2, true);
        let target = &mut input["cards"]["unit-spell"]["abilities"][0]["spell"]["targets"][0];
        for (key, value) in change.as_object().unwrap() {
            target[key] = value.clone();
        }
        lower(&input).unwrap_err();
    }
}
