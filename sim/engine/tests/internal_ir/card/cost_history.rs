use super::*;
use alloc::collections::BTreeMap;
use sve_engine::game::internal::InputDomain;

fn tree(steps: &Value) -> Value {
    let mut tree = fanfare::expanded::spell_tree().clone();
    let mut candidate = tree["cards"]["SD03-005"].take();
    candidate["abilities"][0]["spell"] = json!({
        "targets":[{"bind":"t","from":{"who":"self","zone":"field","kind":"follower"},"n":1}],
        "do":steps
    });
    tree["cards"] = json!({"unit-spell":candidate});
    tree
}

fn discount() -> Value {
    json!({"apply":{"on":{"follow":"moved"},"effect":{"cost_mod":{"when_playing":{"in":"affected"},"delta":-2}},"until":"turn"}})
}

#[test]
fn moved_ex_results_discount_only_successful_after_members() {
    let programs = lowerer()
        .lower_with_printed_costs(
            &tree(&json!([
                {"move":{"ref":"t","to":"ex","result":"moved"}}, discount()
            ])),
            &BTreeMap::from([("unit-spell".to_owned(), Nat(3))]),
        )
        .unwrap();
    for full in [false, true] {
        let mut initial = setup(1, json!([]));
        initial["players"]["P1"]["pp"] = json!({"current":10,"max":10});
        initial["players"]["P1"]["zones"]["field"] =
            json!([{"id":"selected","card":"unit-follower"}]);
        if full {
            initial["players"]["P1"]["zones"]["ex"] = json!(
                (0..5)
                    .map(|i| json!({"id":format!("old-{i}"),"card":"unit-follower"}))
                    .collect::<Vec<_>>()
            );
        }
        let mut game = Game::new(
            catalog_with_cost(&json!("3")),
            &initial,
            &Value::Null,
            &Value::Null,
            "moved-discount",
        )
        .unwrap();
        game.load_internal_card("unit-spell", &programs["unit-spell"])
            .unwrap();
        game.load_internal_card("unit-follower", &card(vec![]))
            .unwrap();
        let CardPlay::Awaiting(question) =
            game.play_internal_card("s", PlayerId { seat: 1 }).unwrap()
        else {
            panic!("formal target")
        };
        let InputDomain::Targets(domain) = &question.domain else {
            panic!("target domain")
        };
        resolved(
            game.answer_internal_card(
                question.actor,
                question.token,
                Answer::Targets(vec![domain.members[0].handle]),
            )
            .unwrap(),
        );
        while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["pp"]["current"], 7);
        assert_eq!(
            state["P1"]["ex"]
                .as_array()
                .unwrap()
                .contains(&json!("selected")),
            !full
        );
        if !full {
            game.play_internal_card("selected", PlayerId { seat: 1 })
                .unwrap();
            assert_eq!(
                game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
                6,
                "only the successful moved generation receives minus two"
            );
        }
    }
}

#[test]
fn a_follow_discount_rejects_missing_future_and_non_ex_outputs() {
    for steps in [
        json!([discount()]),
        json!([discount(), {"move":{"ref":"t","to":"ex","result":"moved"}}]),
        json!([{"move":{"ref":"t","to":"cemetery"}}, discount()]),
        json!([{"create":{"token":"unit-token","to":"field","result":"moved"}}, discount()]),
    ] {
        assert!(
            lowerer().lower(&tree(&steps)).is_err(),
            "only an already declared successful EX output can be followed"
        );
    }
}
