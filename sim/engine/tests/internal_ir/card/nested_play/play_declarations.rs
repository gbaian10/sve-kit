use super::*;
use sve_engine::game::internal::PlayDeclaration;

fn awaiting(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(q) = play else {
        panic!("a nested declaration or body stop");
    };
    q
}

fn declared_play_programs(child_fails: bool) -> BTreeMap<String, WellFormedCardProgram> {
    let mut document = tree("self", "unit-spell", child_fails, false);
    let parent = &mut document["cards"]["unit-amulet"]["abilities"][0]["activate"];
    parent["x"] = json!({});
    parent["cost"] = json!({"pp":"x"});
    parent["do"]
        .as_array_mut()
        .unwrap()
        .push(json!({"optional":{"do":[{"life":{"who":"self","delta":4}}]}}));
    if !child_fails {
        // The cancellation fixture keeps its admitted additional-cost shape without modes.
        let child = &mut document["cards"]["unit-spell"]["abilities"][0]["spell"];
        child.as_object_mut().unwrap().remove("targets");
        child["do"] = json!([{"choice":{"n":1,"options":[
            [{"optional":{"do":[{"life":{"who":"self","delta":2}}]}}],
            [{"life":{"who":"self","delta":3}}]
        ]}}]);
    }
    lowerer()
        .lower_with_printed_costs(&document, &BTreeMap::from([("unit-spell".into(), Nat(1))]))
        .unwrap()
}

#[test]
fn a_nested_childs_modes_do_not_replace_or_extend_its_parents_x() {
    for child_fails in [false, true] {
        let (mut game, definition) = build_custom(
            "self",
            "unit-spell",
            true,
            Scenario::ProcessingPause { child_fails },
            |programs| *programs = declared_play_programs(child_fails),
        );
        let x = awaiting(
            game.activate_internal_card("parent", definition, PlayerId { seat: 1 })
                .unwrap(),
        );
        let outer = x.token.execution;
        let parent = awaiting(
            game.answer_internal_card(x.actor, x.token, Answer::Integer(Nat(1)))
                .unwrap(),
        );
        let expected_parent = [PlayDeclaration::X {
            player: PlayerId { seat: 1 },
            value: Nat(1),
        }];
        assert_eq!(
            game.internal_play_declarations().unwrap(),
            expected_parent.as_slice()
        );
        let next = awaiting(select(&mut game, &parent));
        let parent = if next.token.execution == outer {
            assert!(
                child_fails,
                "only an unpayable child skips its mode decision"
            );
            next
        } else {
            assert!(
                game.internal_play_declarations().unwrap().is_empty(),
                "child starts without its parent's X"
            );
            let InputDomain::Modes { modes, .. } = &next.domain else {
                panic!("child declares modes");
            };
            let selected = vec![*modes.first().unwrap()];
            let expected_child = [PlayDeclaration::Modes {
                player: next.actor,
                decision: next.token.decision,
                modes: selected.clone(),
            }];
            let after = awaiting(
                game.answer_internal_card(next.actor, next.token, Answer::Modes(selected))
                    .unwrap(),
            );
            if after.token.execution == outer {
                assert!(
                    child_fails,
                    "only the unpayable child cancels after its declaration"
                );
                after
            } else {
                assert!(!child_fails, "the unpayable child cannot resolve its body");
                assert_eq!(
                    game.internal_play_declarations().unwrap(),
                    expected_child.as_slice()
                );
                awaiting(
                    game.answer_internal_card(after.actor, after.token, Answer::Integer(Nat(0)))
                        .unwrap(),
                )
            }
        };
        assert_eq!(parent.token.execution, outer);
        assert_eq!(
            game.internal_play_declarations().unwrap(),
            expected_parent.as_slice()
        );
        resolved(
            game.answer_internal_card(parent.actor, parent.token, Answer::Integer(Nat(0)))
                .unwrap(),
        );
        assert_eq!(game.internal_play_declarations(), None);
    }
}
