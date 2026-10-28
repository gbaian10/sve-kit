#![expect(
    clippy::panic,
    reason = "A missing decision fails the scenario immediately."
)]

use super::*;
use sve_engine::game::internal::{DecisionView, InputDomain};
use sve_engine::ir::lowering::ExpandedLowerer;

fn tree(choice: &Value, action: &Value) -> Value {
    let mut tree = fanfare::expanded::tree().clone();
    let mut entry = tree["cards"]["BP08-038"]["abilities"][0].clone();
    entry.as_object_mut().unwrap().remove("fanfare");
    entry["activate"] = json!({"cost":{"pp":1},"do":[{"choose":choice},action]});
    let meta = tree["cards"]["BP08-038"]["meta"].clone();
    tree["cards"] = json!({"unit-follower":{"meta":meta,"abilities":[entry]}});
    tree
}

fn awaiting(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(question) = play else {
        panic!("resolution choice")
    };
    question
}

#[test]
fn resolution_exact_choice_uses_available_field_objects_and_keeps_payment() {
    let programs = ExpandedLowerer::new()
        .unwrap()
        .lower(&tree(
            &json!({"bind":"t","from":{"who":"opp","zone":"field","kind":"follower"},"n":2}),
            &json!({"damage":{"to":"t","n":1}}),
        ))
        .unwrap();
    for count in [0_u64, 1] {
        let mut initial = setup(1, json!([]));
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["pp"] = json!({"current":10,"max":10});
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
        initial["players"]["P2"]["zones"]["field"] = json!(
            (0..count)
                .map(|n| json!({"id":format!("enemy{n}"),"card":"unit-follower"}))
                .collect::<Vec<_>>()
        );
        let mut game = Game::new(
            catalog(),
            &initial,
            &Value::Null,
            &Value::Null,
            "resolution-count",
        )
        .unwrap();
        game.load_internal_card("unit-follower", &programs["unit-follower"])
            .unwrap();
        let question = awaiting(
            game.activate_internal_card("s", AbilityId(1), PlayerId { seat: 1 })
                .unwrap(),
        );
        let InputDomain::Targets(domain) = &question.domain else {
            panic!("field choice")
        };
        assert_eq!(domain.members.len(), usize::try_from(count).unwrap());
        assert_eq!(domain.cardinality.minimum, Nat(count));
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
            9
        );
        let before = format!("{game:?}");
        game.answer_internal_card(
            PlayerId { seat: 2 },
            question.token,
            Answer::Targets(vec![]),
        )
        .unwrap_err();
        assert!(
            format!("{game:?}") == before,
            "invalid answers preserve authority state"
        );
        if count == 1 {
            game.answer_internal_card(question.actor, question.token, Answer::Targets(vec![]))
                .unwrap_err();
            assert!(
                format!("{game:?}") == before,
                "an available required object cannot be omitted"
            );
        }
        resolved(
            game.answer_internal_card(
                question.actor,
                question.token,
                Answer::Targets((0..count).collect()),
            )
            .unwrap(),
        );
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["pp"]["current"], 9);
        if count == 1 {
            assert_eq!(state["objects"]["enemy0"]["damage"], 1);
        }
    }
}

#[test]
fn resolution_amulet_choice_reuses_public_handles_and_destroy() {
    let programs = ExpandedLowerer::new()
        .unwrap()
        .lower(&tree(
            &json!({"bind":"t","from":{"who":"opp","zone":"field","kind":"amulet"},"n":1}),
            &json!({"destroy":"t"}),
        ))
        .unwrap();
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"unit-follower"},{"id":"amulet","card":"unit-amulet"}]);
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "resolution-amulet",
    )
    .unwrap();
    game.load_internal_card("unit-follower", &programs["unit-follower"])
        .unwrap();
    game.load_internal_card("unit-amulet", &card(vec![]))
        .unwrap();
    let question = awaiting(
        game.activate_internal_card("s", AbilityId(1), PlayerId { seat: 1 })
            .unwrap(),
    );
    assert!(matches!(&question.domain, InputDomain::Targets(domain) if domain.members.len() == 1));
    let before = format!("{game:?}");
    game.answer_internal_card(question.actor, question.token, Answer::Targets(vec![99]))
        .unwrap_err();
    assert!(
        format!("{game:?}") == before,
        "invalid answers preserve authority state"
    );
    resolved(
        game.answer_internal_card(question.actor, question.token, Answer::Targets(vec![0]))
            .unwrap(),
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["field"], json!(["enemy"]));
    assert_eq!(state["P2"]["cemetery"], json!(["amulet"]));
}

#[test]
fn resolution_choice_rejects_hidden_sources_foreign_actors_and_optional_instructions() {
    for choice in [
        json!({"bind":"t","from":{"who":"opp","zone":"hand"},"n":1}),
        json!({"bind":"t","from":{"who":"opp","zone":"field"},"n":1,"by":"opp"}),
        json!({"bind":"t","from":{"who":"opp","zone":"field"},"n":1,"may":true}),
        json!({"bind":"t","from":{"who":"opp","zone":"field"},"n":0}),
    ] {
        ExpandedLowerer::new()
            .unwrap()
            .lower(&tree(&choice, &json!({"destroy":"t"})))
            .unwrap_err();
    }
}

#[test]
fn resolution_choice_cannot_replace_an_existing_observation_binding() {
    let choice = json!({"bind":"t","from":{"who":"opp","zone":"field"},"n":1});
    let mut duplicate = tree(&choice, &json!({"destroy":"t"}));
    duplicate["cards"]["unit-follower"]["abilities"][0]["activate"]["do"] = json!([
        {"look":{"top":{"who":"self","n":1},"bind":"t"}},
        {"choose":choice},
        {"destroy":"t"}
    ]);
    let error = ExpandedLowerer::new()
        .unwrap()
        .lower(&duplicate)
        .unwrap_err();
    assert_eq!(error.code, "unsupported_construct");
}

#[test]
fn named_field_choices_reject_shared_names_and_accept_distinct_names() {
    for (substring, expected_count, handles, remaining) in [
        ("unit-", 3, vec![0, 2], json!(["duplicate"])),
        ("follower", 2, vec![0], json!(["duplicate", "different"])),
    ] {
        let programs = ExpandedLowerer::new().unwrap().lower(&tree(
        &json!({"bind":"t","from":{"who":"opp","zone":"field","name_contains":substring},"upto":2,"distinct_by":"name"}),
        &json!({"destroy":"t"}),
    )).unwrap();
        let mut initial = setup(1, json!([]));
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
        initial["players"]["P2"]["zones"]["field"] = json!([
            {"id":"first","card":"unit-follower"}, {"id":"duplicate","card":"unit-follower"}, {"id":"different","card":"unit-amulet"}
        ]);
        let mut game = Game::new(
            catalog(),
            &initial,
            &Value::Null,
            &Value::Null,
            "distinct-names",
        )
        .unwrap();
        game.load_internal_card("unit-follower", &programs["unit-follower"])
            .unwrap();
        game.load_internal_card("unit-amulet", &card(vec![]))
            .unwrap();
        let question = awaiting(
            game.activate_internal_card("s", AbilityId(1), PlayerId { seat: 1 })
                .unwrap(),
        );
        let InputDomain::Targets(domain) = &question.domain else {
            panic!("named targets");
        };
        assert_eq!(domain.members.len(), expected_count);
        assert!(domain.distinct_names);
        let before = format!("{game:?}");
        game.answer_internal_card(question.actor, question.token, Answer::Targets(vec![0, 1]))
            .unwrap_err();
        assert!(
            format!("{game:?}") == before,
            "invalid named selection is atomic"
        );
        resolved(
            game.answer_internal_card(question.actor, question.token, Answer::Targets(handles))
                .unwrap(),
        );
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P2"]["field"], remaining);
    }
}

#[test]
fn exact_distinct_name_resolution_uses_the_largest_compatible_selection() {
    let programs = ExpandedLowerer::new().unwrap().lower(&tree(
        &json!({"bind":"t","from":{"who":"opp","zone":"field","name":"unit-follower"},"n":2,"distinct_by":"name"}),
        &json!({"damage":{"to":"t","n":1}}),
    )).unwrap();
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"first","card":"unit-follower"},{"id":"second","card":"unit-follower"},{"id":"unrelated","card":"unit-amulet"}]);
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "distinct-shortage",
    )
    .unwrap();
    game.load_internal_card("unit-follower", &programs["unit-follower"])
        .unwrap();
    game.load_internal_card("unit-amulet", &card(vec![]))
        .unwrap();
    let question = awaiting(
        game.activate_internal_card("s", AbilityId(1), PlayerId { seat: 1 })
            .unwrap(),
    );
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("named targets");
    };
    assert_eq!(
        domain.members.len(),
        2,
        "exact names exclude the unrelated amulet"
    );
    assert_eq!(domain.cardinality.minimum, Nat(1));
    resolved(
        game.answer_internal_card(question.actor, question.token, Answer::Targets(vec![0]))
            .unwrap(),
    );
}
