#![expect(
    clippy::panic,
    reason = "Unexpected fixture domains must fail the focused payment scenarios."
)]
use super::*;
use crate::player_pipeline::{answer, question};
use std::sync::OnceLock;
use sve_engine::game::internal::{DecisionView, InputDomain};
use sve_engine::game::player::{
    Command, DecisionShape, Input, MainActionAnswer, MainIntentKind, QuestionShape, SubmissionKind,
    Viewer,
};
use sve_engine::policy::select;

fn tree(groups: &[&str], later_failure: bool) -> Value {
    let mut tree = fanfare::expanded::tree().clone();
    let mut ability = tree["cards"]["BP08-038"]["abilities"][0].clone();
    ability.as_object_mut().unwrap().remove("fanfare");
    let groups = groups
        .iter()
        .map(|name| json!({"sel":{"who":"self","zone":"hand","trait":name},"n":1}))
        .collect::<Vec<_>>();
    let mut steps = vec![
        json!({"pp":1}),
        json!({"act_self":true}),
        json!({"discard":{"groups":groups}}),
    ];
    if later_failure {
        steps.push(json!({"pp":99}));
    }
    ability["activate"] = json!({"cost":{"steps":steps},"do":[{"draw":{"n":1}}]});
    let meta = tree["cards"]["BP08-038"]["meta"].clone();
    tree["cards"] = json!({"unit-follower":{"meta":meta,"abilities":[ability]}});
    tree
}

fn program(groups: &[&str], later_failure: bool) -> WellFormedCardProgram {
    lowerer()
        .lower(&tree(groups, later_failure))
        .unwrap()
        .remove("unit-follower")
        .unwrap()
}

fn grouped_catalog() -> Arc<Catalog> {
    static CATALOG: OnceLock<Arc<Catalog>> = OnceLock::new();
    Arc::clone(CATALOG.get_or_init(|| {
        let (snapshot, registry, document) = catalog_documents(&json!("1"));
        let snapshot = snapshot
            .lines()
            .map(|line| {
                let mut row: Value = serde_json::from_str(line).unwrap();
                row["faces"][0]["traits"] = match row["number"].as_str().unwrap() {
                    "unit-spell" => json!(["a", "b", "c"]),
                    "unit-amulet" => json!(["c"]),
                    _ => json!(["a"]),
                };
                row.to_string()
            })
            .collect::<Vec<_>>()
            .join("\n");
        Arc::new(
            Catalog::from_documents(&snapshot, &registry, &[("groups.yaml".into(), document)])
                .unwrap(),
        )
    }))
}

fn board(program: &WellFormedCardProgram, hand: Value) -> Game {
    let mut initial = setup(1, json!([{"id":"drawn","card":"unit-follower"}]));
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
    initial["players"]["P1"]["zones"]["hand"] = hand;
    let mut game = Game::new(
        grouped_catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "hand-groups",
    )
    .unwrap();
    game.load_internal_card("unit-follower", program).unwrap();
    for name in ["unit-spell", "unit-amulet"] {
        game.load_internal_card(name, &card(vec![])).unwrap();
    }
    game
}

#[test]
fn player_policy_pays_non_prefix_groups_in_a_large_hand_and_keeps_bad_answers_private() {
    for groups in [vec!["a", "b"], vec!["a", "b", "c"]] {
        let mut hand = (0..20)
            .map(|i| json!({"id":format!("a{i}"),"card":"unit-follower"}))
            .collect::<Vec<_>>();
        hand.extend([
            json!({"id":"b","card":"unit-spell"}),
            json!({"id":"c","card":"unit-amulet"}),
        ]);
        let mut game = board(&program(&groups, false), json!(hand));
        game.advance_player_input();
        let main = question(&game, Viewer::P1);
        let QuestionShape::ActionWindow(actions) = &main.shape else {
            panic!("main");
        };
        let intent = actions
            .intents
            .iter()
            .find(|intent| matches!(intent.kind, MainIntentKind::Activate { .. }))
            .unwrap();
        assert_eq!(
            game.submit_player(
                Viewer::P1,
                Command::MainAction {
                    token: main.token,
                    intent: intent.id.clone(),
                    answer: MainActionAnswer::None
                }
            )
            .kind,
            SubmissionKind::Applied
        );
        let materials = question(&game, Viewer::P1);
        let QuestionShape::Decision(DecisionShape::Discard {
            cards: selection, ..
        }) = &materials.shape
        else {
            panic!("materials");
        };
        assert_eq!(selection.groups.len(), groups.len());
        assert_eq!(selection.members.len(), 19 + groups.len());
        assert!(matches!(game.input(Viewer::P2), Input::Waiting(_)));
        let before = exact_authority(&game);
        for handles in [
            vec![0; groups.len()],
            (0..u64::try_from(groups.len()).unwrap()).collect(),
            vec![999],
        ] {
            assert_eq!(
                answer(&mut game, Viewer::P1, Answer::CostMaterials(handles)).kind,
                SubmissionKind::Rejected
            );
            assert_eq!(exact_authority(&game), before);
        }
        assert_eq!(
            game.submit_player(
                Viewer::P2,
                Command::Answer {
                    token: materials.token,
                    answer: Answer::CostMaterials(vec![0, 20])
                }
            )
            .kind,
            SubmissionKind::Rejected
        );
        let legal = select(selection).unwrap();
        assert!(legal.targets.contains(&20));
        assert_eq!(
            answer(&mut game, Viewer::P1, Answer::CostMaterials(legal.targets)).kind,
            SubmissionKind::Applied
        );
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P1"]["cemetery"].as_array().unwrap().len(),
            groups.len()
        );
        assert_eq!(state["P1"]["pp"]["current"], 1);
        assert_eq!(state["objects"]["s"]["acted"], true);
    }
}

#[test]
fn overlapping_traits_need_three_physical_cards_and_late_failure_restores_all_costs() {
    for (hand, available) in [
        (json!([{"id":"flex","card":"unit-spell"}]), false),
        (
            json!([{"id":"flex","card":"unit-spell"},{"id":"a0","card":"unit-follower"},{"id":"a1","card":"unit-follower"}]),
            false,
        ),
        (
            json!([{"id":"flex","card":"unit-spell"},{"id":"a","card":"unit-follower"},{"id":"c","card":"unit-amulet"}]),
            true,
        ),
    ] {
        for failure in [false, true] {
            let mut game = board(&program(&["a", "b", "c"], failure), hand.clone());
            let before = game.projection(View::Referee).unwrap();
            let start = game.activate_internal_card("s", AbilityId(1), PlayerId { seat: 1 });
            if !available {
                start.unwrap_err();
                assert_eq!(game.projection(View::Referee).unwrap(), before);
                continue;
            }
            let CardPlay::Awaiting(q) = start.unwrap() else {
                panic!("materials");
            };
            let pending = game.projection(View::Referee).unwrap();
            assert_eq!(pending["P1"]["pp"]["current"], 2);
            assert_eq!(pending["objects"]["s"]["acted"], false);
            assert_eq!(pending["P1"]["cemetery"], json!([]));
            let paid =
                game.answer_internal_card(q.actor, q.token, Answer::CostMaterials(vec![0, 1, 2]));
            if failure {
                paid.unwrap_err();
                assert_eq!(game.projection(View::Referee).unwrap(), before);
                assert!(game.internal_decision(View::Referee).is_none());
            } else {
                let result = resolved(paid.unwrap());
                assert_eq!(
                    result
                        .progress
                        .facts
                        .iter()
                        .filter(|f| matches!(f, Fact::Discard(_)))
                        .count(),
                    3
                );
            }
        }
    }
}

fn raw_optional() -> WellFormedCardProgram {
    let mut value = tree(&["a", "b", "c"], false);
    let entry = &mut value["cards"]["unit-follower"]["abilities"][0];
    entry.as_object_mut().unwrap().remove("activate");
    entry["fanfare"] = json!({"optional_cost":{"discard":{"sel":{"who":"self","zone":"hand"},"n":2},"result":"paid"},"do":[{"draw":{"n":1}}]});
    let mut raw = lowerer()
        .lower(&value)
        .unwrap()
        .remove("unit-follower")
        .unwrap()
        .card()
        .clone();
    let grouped = program(&["a", "b", "c"], false);
    let CostSpec::Ordered { steps: group_steps } = &grouped.card().abilities[0]
        .node
        .play
        .as_ref()
        .unwrap()
        .costs
    else {
        panic!("costs");
    };
    let Action::Discard {
        mode: DiscardMode::Chosen {
            constraint: template,
        },
        ..
    } = &group_steps[2].instruction.action
    else {
        panic!("groups");
    };
    let Node::Control {
        control:
            Control::Pay {
                costs: CostSpec::Ordered { steps },
                ..
            },
    } = &mut raw.abilities[0].node.body.as_mut().unwrap().steps[0].node
    else {
        panic!("pay");
    };
    for (offset, group) in template.groups.iter().enumerate().skip(1) {
        let mut step = steps[0].clone();
        step.instruction.id = InstructionId(
            1000_u64
                .checked_add(u64::try_from(offset).unwrap())
                .unwrap(),
        );
        step.instruction.receipt = ReceiptId(
            1000_u64
                .checked_add(u64::try_from(offset).unwrap())
                .unwrap(),
        );
        let Action::Discard {
            mode: DiscardMode::Chosen { constraint },
            ..
        } = &mut step.instruction.action
        else {
            panic!("discard");
        };
        constraint.groups = vec![group.clone()];
        constraint.cardinality = group.cardinality;
        steps.push(step);
    }
    WellFormedCardProgram::check(raw).unwrap()
}

fn optional_start() -> (Game, DecisionView) {
    // Exercise the real Fanfare entry instead of invoking the private cost matcher.
    let mut initial = setup(1, json!([{"id":"drawn","card":"unit-follower"}]));
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"s","card":"unit-follower"},{"id":"flex","card":"unit-spell"},{"id":"a0","card":"unit-follower"},{"id":"a1","card":"unit-follower"},{"id":"c","card":"unit-amulet"}]);
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    let mut game = Game::new(
        grouped_catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "joint",
    )
    .unwrap();
    game.load_internal_card("unit-follower", &raw_optional())
        .unwrap();
    game.load_internal_card("unit-spell", &card(vec![]))
        .unwrap();
    game.load_internal_card("unit-amulet", &card(vec![]))
        .unwrap();
    game.play_internal_card("s", PlayerId { seat: 1 }).unwrap();
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let CardPlay::Awaiting(q) = game
        .answer_internal_card(pending.actor, pending.token, Answer::Pending(1))
        .unwrap()
    else {
        panic!("optional");
    };
    (game, q)
}

#[test]
fn raw_optional_multiple_costs_preserve_large_minimum_and_cross_step_reservations() {
    let (mut game, q) = optional_start();
    let CardPlay::Awaiting(first) = game
        .answer_internal_card(q.actor, q.token, Answer::OptionalCost(true))
        .unwrap()
    else {
        panic!("broad");
    };
    let InputDomain::CostMaterials(domain) = &first.domain else {
        panic!("materials");
    };
    assert_eq!(domain.cardinality.minimum, Nat(2));
    let CardPlay::Awaiting(second) = game
        .answer_internal_card(first.actor, first.token, Answer::CostMaterials(vec![1, 2]))
        .unwrap()
    else {
        panic!("narrow");
    };
    let InputDomain::CostMaterials(domain) = &second.domain else {
        panic!("materials");
    };
    assert_eq!(domain.members.len(), 1);
    assert_eq!(domain.members[0].card.as_deref(), Some("unit-spell"));
    let CardPlay::Awaiting(third) = game
        .answer_internal_card(second.actor, second.token, Answer::CostMaterials(vec![0]))
        .unwrap()
    else {
        panic!("last");
    };
    let InputDomain::CostMaterials(domain) = &third.domain else {
        panic!("materials");
    };
    assert_eq!(domain.members.len(), 1);
    assert_eq!(domain.members[0].card.as_deref(), Some("unit-amulet"));
    resolved(
        game.answer_internal_card(third.actor, third.token, Answer::CostMaterials(vec![0]))
            .unwrap(),
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["cemetery"]
            .as_array()
            .unwrap()
            .len(),
        4
    );
}

#[test]
fn raw_optional_decline_and_starved_later_cost_do_not_commit_discard_or_partial_payment() {
    for accepted in [false, true] {
        let (mut game, q) = optional_start();
        let before = game.projection(View::Referee).unwrap();
        if accepted {
            game.advance_player_input();
            assert_eq!(
                answer(&mut game, Viewer::P1, Answer::OptionalCost(true)).kind,
                SubmissionKind::Applied
            );
            let first = question(&game, Viewer::P1);
            // A legal first answer may consume the sole b card; the whole ability then rolls back.
            assert_eq!(
                answer(&mut game, Viewer::P1, Answer::CostMaterials(vec![0, 1])).kind,
                SubmissionKind::Reverted
            );
            let after = game.projection(View::Referee).unwrap();
            assert_eq!(after["P1"], before["P1"]);
            assert_eq!(after["objects"], before["objects"]);
            if let Input::Question(next) = game.input(Viewer::P1) {
                assert_ne!(next.token, first.token);
            }
        } else {
            let result = resolved(
                game.answer_internal_card(q.actor, q.token, Answer::OptionalCost(false))
                    .unwrap(),
            );
            assert!(
                !result
                    .progress
                    .receipts
                    .values()
                    .any(|r| matches!(r, InstructionOutcome::Discard(_)))
            );
            assert!(
                !result
                    .progress
                    .facts
                    .iter()
                    .any(|f| matches!(f, Fact::Discard(_)))
            );
            assert_eq!(
                game.projection(View::Referee).unwrap()["P1"]["cemetery"],
                json!([])
            );
        }
    }
}

#[test]
fn moving_hand_source_is_reserved_before_another_hand_cost_is_asked() {
    let mut value = tree(&["a", "b"], false);
    value["cards"]["unit-follower"]["abilities"][0]["activate"] = json!({"zone":"hand","cost":{"steps":[
        {"self_to_ex":true},{"discard":{"sel":{"who":"self","zone":"hand"},"n":1}}
    ]},"do":[{"draw":{"n":1}}]});
    let program = lowerer()
        .lower(&value)
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    let mut initial = setup(1, json!([{"id":"drawn","card":"unit-follower"}]));
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    initial["players"]["P1"]["zones"]["hand"] =
        json!([{"id":"s","card":"unit-follower"},{"id":"other","card":"unit-spell"}]);
    let mut game = Game::new(
        grouped_catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "reserved-source",
    )
    .unwrap();
    game.load_internal_card("unit-follower", &program).unwrap();
    game.load_internal_card("unit-spell", &card(vec![]))
        .unwrap();
    let CardPlay::Awaiting(q) = game
        .activate_internal_card("s", AbilityId(1), PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("hand cost");
    };
    let InputDomain::CostMaterials(domain) = &q.domain else {
        panic!("materials");
    };
    assert_eq!(domain.members.len(), 1);
    assert_eq!(domain.members[0].card.as_deref(), Some("unit-spell"));
    resolved(
        game.answer_internal_card(q.actor, q.token, Answer::CostMaterials(vec![0]))
            .unwrap(),
    );
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["P1"]["ex"], json!(["s"]));
    assert_eq!(after["P1"]["cemetery"], json!(["other"]));
}

#[test]
fn selected_mode_hand_groups_are_optional_and_keep_the_head_payment_gate_closed() {
    let mut value = tree(&["a", "b", "c"], false);
    let cost =
        value["cards"]["unit-follower"]["abilities"][0]["activate"]["cost"]["steps"][2].clone();
    let ability = &mut value["cards"]["unit-follower"]["abilities"][0];
    ability["activate"] = json!({"cost":{"pp":0},"do":[{"choice":{"n":1,"options":[
        [{"pay":{"cost":cost,"do":[{"draw":{"n":1}}]}}], [{"draw":{"n":1}}]
    ]}}]});
    let program = lowerer()
        .lower(&value)
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    for accepted in [false, true] {
        let mut game = board(
            &program,
            json!([{"id":"flex","card":"unit-spell"},{"id":"a","card":"unit-follower"},{"id":"c","card":"unit-amulet"}]),
        );
        let CardPlay::Awaiting(modes) = game
            .activate_internal_card("s", AbilityId(1), PlayerId { seat: 1 })
            .unwrap()
        else {
            panic!("modes");
        };
        let CardPlay::Awaiting(fee) = game
            .answer_internal_card(modes.actor, modes.token, Answer::Modes(vec![ModeId(1)]))
            .unwrap()
        else {
            panic!("fee");
        };
        let play = game
            .answer_internal_card(fee.actor, fee.token, Answer::OptionalCost(accepted))
            .unwrap();
        let result = if accepted {
            let CardPlay::Awaiting(q) = play else {
                panic!("groups");
            };
            game.answer_internal_card(q.actor, q.token, Answer::CostMaterials(vec![0, 1, 2]))
                .unwrap()
        } else {
            play
        };
        resolved(result);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["cemetery"]
                .as_array()
                .unwrap()
                .len(),
            if accepted { 3 } else { 0 }
        );
    }
    // Head declarations cannot coexist with mode fees, regardless of this group's support.
    let ability = &mut value["cards"]["unit-follower"]["abilities"][0];
    let mut body = ability.as_object_mut().unwrap().remove("activate").unwrap();
    body.as_object_mut().unwrap().remove("cost");
    body["optional_cost"] = json!({"pp":1,"result":"head"});
    ability["fanfare"] = body;
    let raw = lowerer()
        .lower(&value)
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    let mut game = Game::new(
        grouped_catalog(),
        &setup(1, json!([])),
        &Value::Null,
        &Value::Null,
        "head-mode-gate",
    )
    .unwrap();
    let error = game.load_internal_card("unit-follower", &raw).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("optional declaration with exchange or activation")
    );
}

#[test]
fn hand_group_support_keeps_fusion_with_mode_fees_outside_the_lowered_range() {
    let value = evolution_linked_movement::tree(&[(
        "activate",
        json!({
            "fusion":{"sel":{"who":"self","zone":["hand","ex"],"kind":"spell"},"n":1},
            "do":[{"choice":{"n":1,"options":[
                [{"pay":{"cost":{"discard":{"sel":{"who":"self","zone":"hand","kind":"follower"},"n":1}},"do":[{"draw":{"n":0}}]}}],
                [{"draw":{"n":0}}]
            ]}}]
        }),
    )]);
    let error = lowerer().lower(&value).unwrap_err();
    assert_eq!(error.code, "unsupported_construct");
    assert_eq!(error.path, "/cards/unit-follower/abilities/0");
}

#[test]
fn multiple_hand_groups_keep_the_existing_filter_and_fixed_count_boundaries() {
    for replacement in [
        json!({"sel":{"who":"self","zone":"hand","name_contains":"unit"},"n":1}),
        json!({"sel":{"who":"self","zone":"hand"},"n":2}),
        json!({"sel":{"who":"self","zone":"hand"},"upto":1}),
        json!({"sel":{"who":"self","zone":"hand"},"any":true}),
        json!({"sel":{"who":"self","zone":"hand"},"n":1,"result":"material"}),
    ] {
        let mut value = tree(&["a", "b"], false);
        value["cards"]["unit-follower"]["abilities"][0]["activate"]["cost"]["steps"][2]["discard"]
            ["groups"][0] = replacement;
        lowerer().lower(&value).unwrap_err();
    }
}
