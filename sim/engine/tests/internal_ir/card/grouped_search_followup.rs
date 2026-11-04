use super::*;

fn tree(follow: bool) -> Value {
    let mut tree = fanfare::expanded::tree().clone();
    let mut source = tree["cards"]["BP01-041"].clone();
    let mut steps = json!([{"search":{
        "groups":[
            {"sel":{"who":"self","zone":"deck","kind":"follower","name":"白の王・イメラ"},"n":1_u8},
            {"sel":{"who":"self","zone":"deck","kind":"follower","name":"黒の女王・マグナス"},"n":1_u8}
        ],"to":"field"
    }}]);
    if follow {
        steps[0]["search"]["result"] = json!("entered");
        steps
            .as_array_mut()
            .unwrap()
            .push(json!({"buff":{"on":{"follow":"entered"},"power":1_u8,"hp":1_u8}}));
    }
    source["abilities"][0]["activate"] = json!({"cost":{"pp":0_u8},"do":steps});
    tree["cards"] = json!({"unit-follower":source});
    tree
}

fn lower_and_load(follow: bool) {
    let programs = lowerer().lower(&tree(follow)).unwrap();
    let mut game = Game::new(
        catalog(),
        &setup(1, json!([])),
        &Value::Null,
        &Value::Null,
        "named-group-search-follow",
    )
    .unwrap();
    game.load_internal_card("unit-follower", &programs["unit-follower"])
        .unwrap();
}

#[test]
fn named_two_group_field_search_without_follow_is_a_valid_lower_load_baseline() {
    lower_and_load(false);
}

#[test]
fn adding_follow_to_the_same_named_group_search_must_lower_and_load() {
    lower_and_load(true);
}

fn reject_consumer(input: &Value, index: usize) {
    let mut producer = input.clone();
    producer["cards"]["unit-follower"]["abilities"][0]["activate"]["do"]
        .as_array_mut()
        .unwrap()
        .truncate(index);
    lowerer().lower(&producer).unwrap();
    let error = lowerer().lower(input).unwrap_err();
    assert_eq!(
        error.code, "unsupported_construct",
        "valid Search does not grant this consumer permission"
    );
    assert!(
        error.path.ends_with(&format!("/do/{index}")),
        "rejection belongs to the consumer: {}",
        error.path
    );
}

#[test]
fn grouped_numeric_follow_does_not_authorize_effect_evolution_or_destroy() {
    for step in [
        json!({"evolve":{"ref":{"follow":"entered"}}}),
        json!({"destroy":{"follow":"entered"}}),
    ] {
        let mut input = tree(true);
        input["cards"]["unit-follower"]["abilities"][0]["activate"]["do"][1] = step;
        reject_consumer(&input, 1);
    }
}

#[test]
fn grouped_follow_rejects_nonfollowers_missing_kinds_larger_counts_and_distinct_names() {
    for change in [
        json!({"kind":"spell"}),
        json!({"kind":null}),
        json!({"n":2}),
        json!({"distinct_by":"name"}),
    ] {
        let mut input = tree(true);
        let search =
            &mut input["cards"]["unit-follower"]["abilities"][0]["activate"]["do"][0]["search"];
        if let Some(kind) = change.get("kind") {
            if kind.is_null() {
                search["groups"][1]["sel"]
                    .as_object_mut()
                    .unwrap()
                    .remove("kind");
            } else {
                search["groups"][1]["sel"]["kind"] = kind.clone();
            }
        } else if let Some(count) = change.get("n") {
            search["groups"][1]["n"] = count.clone();
        } else {
            search["distinct_by"] = change["distinct_by"].clone();
        }
        reject_consumer(&input, 1);
    }
}

#[test]
fn grouped_search_result_names_remain_unique_and_cannot_escape_optional_bodies() {
    let mut duplicate = tree(true);
    let steps = &mut duplicate["cards"]["unit-follower"]["abilities"][0]["activate"]["do"];
    steps[1] = steps[0].clone();
    reject_consumer(&duplicate, 1);
    let mut escaped = tree(true);
    let steps = &mut escaped["cards"]["unit-follower"]["abilities"][0]["activate"]["do"];
    steps[0] = json!({"optional":{"do":[steps[0].clone()]}});
    reject_consumer(&escaped, 1);
}

#[expect(
    clippy::panic,
    reason = "Grouped search fixtures require their declared private domains."
)]
#[expect(
    clippy::too_many_lines,
    reason = "One authority trace compares the private search, capacity choice and exact successful-entry effects."
)]
fn run_case(free: u8, find: u8, survivor: usize) {
    use sve_engine::game::internal::{InputDomain, SearchAnswer};
    let mut input = tree(true);
    let steps = &mut input["cards"]["unit-follower"]["abilities"][0]["activate"]["do"];
    for group in steps[0]["search"]["groups"].as_array_mut().unwrap() {
        group["sel"]["name"] = json!("unit-follower");
    }
    steps
        .as_array_mut()
        .unwrap()
        .push(json!({"buff":{"on":"this","power":1,"hp":1}}));
    let program = lowerer()
        .lower(&input)
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"unit-follower"}]);
    for n in 0..4_u8.checked_sub(free).unwrap() {
        initial["players"]["P1"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":format!("old-{n}"),"card":"unit-follower"}));
    }
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"a","card":"unit-follower"},{"id":"b","card":"unit-follower"},{"id":"left","card":"unit-follower"}
    ]);
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "grouped-follow",
    )
    .unwrap();
    game.load_internal_card("unit-follower", &program).unwrap();
    let CardPlay::Awaiting(q) = game
        .activate_internal_card("source", AbilityId(1), PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("private grouped Search");
    };
    assert!(
        game.internal_decision(View::P2).is_none(),
        "opponent cannot inspect private search handles"
    );
    let InputDomain::Search(domain) = &q.domain else {
        panic!("Search domain")
    };
    let handles = domain.cards.iter().map(|c| c.handle).collect::<Vec<_>>();
    for (actor, answer) in [
        (q.actor, vec![vec![u64::MAX], vec![]]),
        (q.actor, vec![vec![handles[0]], vec![handles[0]]]),
        (PlayerId { seat: 2 }, vec![vec![], vec![]]),
    ] {
        let before = format!("{game:?}");
        game.answer_internal_card(
            actor,
            q.token,
            Answer::Search(SearchAnswer::Grouped(answer)),
        )
        .unwrap_err();
        assert_eq!(
            format!("{game:?}"),
            before,
            "invalid grouped answers preserve authority and state"
        );
    }
    let picks = vec![
        if find >= 1 { vec![handles[0]] } else { vec![] },
        if find == 2 { vec![handles[1]] } else { vec![] },
    ];
    let result = game
        .answer_internal_card(
            q.actor,
            q.token,
            Answer::Search(SearchAnswer::Grouped(picks)),
        )
        .unwrap();
    let result = if let CardPlay::Awaiting(q) = result {
        let InputDomain::SearchPartition(domain) = &q.domain else {
            panic!("partial Field capacity")
        };
        assert_eq!(free, 1, "only partial capacity needs a choice");
        assert_eq!(
            domain.cards.len(),
            2,
            "both found cards remain available for allocation"
        );
        let before = format!("{game:?}");
        game.answer_internal_card(q.actor, q.token, Answer::SearchPartition(vec![u64::MAX]))
            .unwrap_err();
        assert_eq!(
            format!("{game:?}"),
            before,
            "foreign partition handles cannot alter authority"
        );
        game.answer_internal_card(
            q.actor,
            q.token,
            Answer::SearchPartition(vec![domain.cards[survivor].handle]),
        )
        .unwrap()
    } else {
        result
    };
    resolved(result);
    let view = game.projection(View::Referee).unwrap();
    let field = view["P1"]["field"].as_array().unwrap();
    assert_eq!(
        view["objects"]["source"]["power"], 3,
        "independent source buff runs even when follow is empty"
    );
    for id in ["a", "b", "left"] {
        let entered = field.contains(&json!(id));
        assert_eq!(
            view["objects"][id]["power"],
            if entered { 3 } else { 2 },
            "only actual successful entry {id} receives power"
        );
        assert_eq!(
            view["objects"][id]["hp"],
            if entered { 4 } else { 3 },
            "only actual successful entry {id} receives life"
        );
        assert_eq!(
            view["objects"][id]["generation"],
            u8::from(entered),
            "only movement changes {id}'s generation"
        );
    }
    assert!(
        !field.contains(&json!("left")),
        "an unselected same-name card stays in the deck"
    );
    for n in 0..4_u8.checked_sub(free).unwrap() {
        assert_eq!(
            view["objects"][format!("old-{n}")]["power"],
            2,
            "existing same-name objects do not receive follow effects"
        );
    }
    assert_eq!(
        field.len(),
        usize::from(
            5_u8.checked_sub(free)
                .unwrap()
                .checked_add(find.min(free))
                .unwrap()
        ),
        "only capacity-limited successful movements enter"
    );
}

#[test]
fn grouped_follow_modifies_only_successful_entries_with_zero_one_or_two_field_spaces() {
    for free in 0..=2 {
        for find in 0..=2 {
            run_case(free, find, 0);
        }
    }
    run_case(1, 2, 1);
}
