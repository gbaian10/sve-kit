use super::*;

fn observer(filter: &Value, own_turn: bool) -> Value {
    let mut auto = json!({"per_turn":1,"on":{"played":{"who":"self","sel":filter}},"do":[{"draw":{"who":"self","n":1}},{"discard":{"who":"self","n":1}}]});
    if own_turn {
        auto["on"]["played"]["during"] = json!({"turn":"self"});
    }
    equipment_played::tree(&json!({"auto":auto}), "unit-follower")
}

fn load(filter: &Value, own_turn: bool) {
    let program = lowerer().lower(&observer(filter, own_turn)).unwrap();
    let mut board = Game::new(
        catalog(),
        &setup(1, json!([])),
        &Value::Null,
        &Value::Null,
        "played-original-cost-preflight",
    )
    .unwrap();
    board
        .load_internal_card("unit-follower", &program["unit-follower"])
        .unwrap();
}

#[test]
fn ordinary_played_spell_is_a_valid_lower_load_baseline() {
    load(&json!({"kind":"spell"}), false);
}
#[test]
fn own_turn_played_spell_is_a_valid_lower_load_baseline() {
    load(&json!({"kind":"spell"}), true);
}
#[test]
fn played_single_trait_is_a_valid_lower_load_baseline() {
    load(&json!({"trait":"魔法使い"}), false);
}
fn probe(
    filter: &Value,
    own_turn: bool,
    cost: Option<&Value>,
    traits: &[&str],
    copies: u8,
) -> Game {
    probe_at(filter, own_turn, cost, traits, copies, 1)
}

fn probe_at(
    filter: &Value,
    own_turn: bool,
    cost: Option<&Value>,
    traits: &[&str],
    copies: u8,
    active: u32,
) -> Game {
    let mut tree = observer(filter, own_turn);
    tree["cards"]["unit-follower"]["abilities"][0]["auto"]["do"] =
        json!([{"life":{"who":"self","delta":1}}]);
    let program = lowerer().lower(&tree).unwrap();
    let (snapshot, registry, document) = catalog_documents(&json!("1"));
    let snapshot = snapshot
        .lines()
        .map(|line| {
            let mut row: Value = serde_json::from_str(line).unwrap();
            if row["number"] == "unit-spell" {
                if let Some(cost) = cost {
                    row["faces"][0]["cost"] = cost.clone();
                } else {
                    row["faces"][0].as_object_mut().unwrap().remove("cost");
                }
                row["faces"][0]["traits"] = json!(traits);
            }
            row.to_string()
        })
        .collect::<Vec<_>>()
        .join("\n");
    let catalog = Arc::new(
        Catalog::from_documents(&snapshot, &registry, &[("unit.yaml".into(), document)]).unwrap(),
    );
    let mut initial = setup(1, json!([]));
    initial["turn"]["active"] = json!(format!("P{active}"));
    initial["players"]["P1"]["pp"] = json!({"current":10,"max":10});
    initial["players"]["P1"]["zones"]["field"] = json!(
        (0_u8..copies)
            .map(|i| json!({"id":format!("observer{i}"),"card":"unit-follower"}))
            .collect::<Vec<_>>()
    );
    initial["players"]["P1"]["zones"]["hand"] =
        json!([{"id":"first","card":"unit-spell"},{"id":"second","card":"unit-spell"}]);
    let mut board = Game::new(
        catalog,
        &initial,
        &Value::Null,
        &Value::Null,
        "played-original-cost-runtime",
    )
    .unwrap();
    board
        .load_internal_card("unit-follower", &program["unit-follower"])
        .unwrap();
    let amount = cost.unwrap().as_str().unwrap().parse::<i64>().unwrap();
    let mut spell = play::spell(vec![]);
    let costs = &mut spell.node.play.as_mut().unwrap().costs;
    assert!(
        matches!(costs, CostSpec::Ordered { .. }),
        "the synthetic Spell has one declared intrinsic payment"
    );
    if let CostSpec::Ordered { steps } = costs {
        steps[0].instruction.action = payment::pp(amount);
    }
    board
        .load_internal_card("unit-spell", &card(vec![spell, quick::permit()]))
        .unwrap();
    board
}

fn play_and_count(board: &mut Game, id: &str) -> usize {
    resolved(board.play_internal_card(id, PlayerId { seat: 1 }).unwrap());
    let before = board.projection(View::Referee).unwrap();
    assert!(
        before["P1"]["cemetery"]
            .as_array()
            .unwrap()
            .contains(&json!(id)),
        "the Spell has left its committed Play generation before the pending ability resolves"
    );
    drain_observers(board)
}

#[expect(
    clippy::panic,
    reason = "Only committed Play occurrences can be pending in this fixture."
)]
fn drain_observers(board: &mut Game) -> usize {
    use sve_engine::game::internal::InputDomain;
    let mut count = 0_usize;
    loop {
        while board.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            board.resume_internal_card().unwrap();
        }
        let Some(q) = board.internal_pending_choice().unwrap() else {
            break;
        };
        let InputDomain::Pending(options) = q.domain else {
            panic!("printed Play pending");
        };
        resolved(
            board
                .answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
                .unwrap(),
        );
        count = count.checked_add(1).unwrap();
        assert!(
            count <= 2,
            "only the declared observer instances can trigger"
        );
    }
    count
}

#[test]
fn original_cost_at_least_five_uses_printed_information_and_each_instances_own_limit() {
    for (n, expected) in [(4_u8, 0_usize), (5, 2), (6, 2)] {
        let mut board = probe(
            &json!({"orig_cost_ge":5}),
            false,
            Some(&json!(n.to_string())),
            &[],
            2,
        );
        assert_eq!(play_and_count(&mut board, "first"), expected);
        assert_eq!(
            board.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
            20_u8 + u8::try_from(expected).unwrap()
        );
        if n <= 5 {
            assert_eq!(
                play_and_count(&mut board, "second"),
                0_usize,
                "a second fully paid spell produces no additional matching occurrence this turn"
            );
        }
    }
}
#[test]
fn own_turn_spell_original_cost_at_most_one_accepts_zero_and_one_but_not_two() {
    for (n, expected) in [(0_u8, 1_usize), (1, 1), (2, 0)] {
        let mut board = probe(
            &json!({"kind":"spell","orig_cost_le":1}),
            true,
            Some(&json!(n.to_string())),
            &[],
            1,
        );
        assert_eq!(play_and_count(&mut board, "first"), expected);
    }
}
#[test]
fn single_trait_original_cost_boundary_is_checked_at_committed_play() {
    for (n, expected) in [(3_u8, 1_usize), (4, 0)] {
        let mut board = probe(
            &json!({"trait":"魔法使い","orig_cost_le":3}),
            false,
            Some(&json!(n.to_string())),
            &["魔法使い"],
            1,
        );
        assert_eq!(play_and_count(&mut board, "first"), expected);
    }
}
#[test]
fn trait_union_is_one_trigger_for_either_or_both_traits_and_requires_the_cost_bound() {
    for (traits, n, expected) in [
        (&["宴楽"][..], 3_u8, 1_usize),
        (&["魔法使い"][..], 3, 1),
        (&["宴楽", "魔法使い"][..], 3, 1),
        (&["他"][..], 3, 0),
        (&["宴楽", "魔法使い"][..], 4, 0),
    ] {
        let mut board = probe(
            &json!({"traits_any":["宴楽","魔法使い"],"orig_cost_le":3}),
            false,
            Some(&json!(n.to_string())),
            traits,
            1,
        );
        assert_eq!(play_and_count(&mut board, "first"), expected);
    }
}
#[test]
fn equality_accepts_known_zero_and_two_but_rejects_neighboring_costs() {
    for bound in [0_u8, 2] {
        for n in [0_u8, 1, 2, 3] {
            let mut board = probe(
                &json!({"orig_cost_eq":bound}),
                false,
                Some(&json!(n.to_string())),
                &[],
                1,
            );
            assert_eq!(play_and_count(&mut board, "first"), usize::from(n == bound));
        }
    }
}

#[test]
fn event_cost_ranges_compose_for_entry_but_keep_history_and_other_events_closed() {
    let mut entry = equipment_played::tree(
        &json!({"auto":{"on":{"enter_field":{"who":"self","sel":{"traits_any":["宴楽","魔法使い"]}}},"do":[{"life":{"who":"self","delta":1}}]}}),
        "unit-follower",
    );
    lowerer().lower(&entry).unwrap();
    entry["cards"]["unit-follower"]["abilities"][0]["auto"]["on"]["enter_field"]["sel"]["orig_cost_le"] =
        json!(3);
    lowerer().lower(&entry).unwrap();
    for header in ["enter_field", "left_field", "evolved"] {
        let baseline = equipment_played::tree(
            &json!({"auto":{"on":{header:{"who":"self","sel":{"kind":"follower"}}},"do":[{"life":{"who":"self","delta":1}}]}}),
            "unit-follower",
        );
        lowerer().lower(&baseline).unwrap();
        for filter in [
            json!({"orig_cost_le":1}),
            json!({"orig_cost_ge":5}),
            json!({"orig_cost_eq":0}),
            json!({"traits_any":["宴楽","魔法使い"]}),
        ] {
            if header == "enter_field" && filter.get("traits_any").is_some() {
                continue;
            }
            let mut tree = baseline.clone();
            let accepts_event_cost = header == "enter_field"
                || (matches!(header, "left_field" | "evolved")
                    && filter.get("traits_any").is_none());
            tree["cards"]["unit-follower"]["abilities"][0]["auto"]["on"][header]["sel"] = filter;
            if accepts_event_cost {
                lowerer().lower(&tree).unwrap();
            } else {
                assert!(matches!(
                    lowerer().lower(&tree).unwrap_err().code,
                    "unsupported_construct" | "invalid_expanded"
                ));
            }
        }
    }
    for filter in [
        json!({"orig_cost_le":1}),
        json!({"orig_cost_ge":5}),
        json!({"orig_cost_eq":0}),
        json!({"traits_any":["宴楽","魔法使い"],"orig_cost_le":3}),
    ] {
        let tree = equipment_played::tree(
            &json!({"activate":{"cost":{"pp":0},"do":[{"life":{"who":"self","delta":{"history_count":{"event":"played","window":"this_turn","who":"self","sel":filter}}}}]}}),
            "unit-follower",
        );
        assert!(matches!(
            lowerer().lower(&tree).unwrap_err().code,
            "unsupported_construct" | "invalid_expanded"
        ));
    }
}
#[test]
fn played_filters_reject_ambiguous_bounds_and_noncanonical_unions() {
    for filter in [
        json!({"orig_cost_eq":1,"orig_cost_le":3}),
        json!({"orig_cost_ge":1,"orig_cost_le":3}),
        json!({"traits_any":[]}),
        json!({"traits_any":["宴楽"]}),
        json!({"traits_any":["宴楽","宴楽"]}),
        json!({"traits_any":["","魔法使い"]}),
        json!({"traits_any":["宴楽","魔法使い"],"trait":"宴楽"}),
        json!({"traits_any":["宴楽","魔法使い"],"class":"ウィッチ"}),
        json!({"traits_any":["宴楽","魔法使い"],"other":false}),
        json!({"traits_any":["宴楽","魔法使い"],"other":true}),
        json!({"cost_le":3}),
    ] {
        assert!(matches!(
            lowerer().lower(&observer(&filter, false)).unwrap_err().code,
            "unsupported_construct" | "invalid_expanded"
        ));
    }
}

#[expect(
    clippy::panic,
    reason = "The accepted Played header must retain its declared canonical shape."
)]
fn conditions(raw: &mut CardProgram) -> &mut Vec<Expr<Bool>> {
    let TriggerCondition::Event {
        pattern:
            EventPattern::Play {
                predicate: Expr::Compute(operation),
                ..
            },
    } = &mut raw.abilities[0].node.trigger.as_mut().unwrap().on
    else {
        panic!("Played header");
    };
    let Predicate::PlayedCard {
        predicate: Expr::Compute(operation),
        ..
    } = operation.as_mut()
    else {
        panic!("Played subject binder");
    };
    let Predicate::And { predicates } = operation.as_mut() else {
        panic!("complete branch");
    };
    predicates
}

#[test]
fn raw_cost_headers_require_the_guard_original_view_reveal_policy_and_canonical_bound() {
    let program = lowerer()
        .lower(&observer(&json!({"orig_cost_le":3}), false))
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    for case in 0_u8..6 {
        let mut raw = program.card().clone();
        let predicates = conditions(&mut raw);
        let len = predicates.len();
        if case == 0 {
            predicates.remove(len - 2);
        } else if case == 1 {
            predicates.pop();
        } else {
            if (case == 2 || case == 3)
                && let Expr::Compute(operation) = &mut predicates[len - 2]
                && let Predicate::HasInfo {
                    information: NumberRead::Cost { object, view },
                } = operation.as_mut()
            {
                if case == 2 {
                    *view = CharacteristicView::Effective;
                } else if let ObjectView::EventSnapshot { policy, .. } = object {
                    *policy = EventSnapshotPolicy::BeforeExit;
                } else {
                    assert!(
                        matches!(object, ObjectView::EventSnapshot { .. }),
                        "the canonical cost guard reads an event snapshot"
                    );
                }
            }
            if let Expr::Compute(operation) = predicates.last_mut().unwrap()
                && let Predicate::CompareInt {
                    left: Expr::Compute(number),
                    comparison,
                    right,
                } = operation.as_mut()
            {
                if case == 4 {
                    *comparison = Comparison::Less;
                } else if case == 5 {
                    *right = Expr::Literal(Int(-1));
                } else {
                    assert!(
                        case == 2 || case == 3,
                        "the remaining mutation changes the cost view"
                    );
                }
                if let IntOperation::InformationOrZero {
                    information: NumberRead::Cost { object, view },
                } = number.as_mut()
                {
                    if case == 2 {
                        *view = CharacteristicView::Effective;
                    } else if case == 3
                        && let ObjectView::EventSnapshot { policy, .. } = object
                    {
                        *policy = EventSnapshotPolicy::BeforeExit;
                    } else {
                        assert!(
                            case == 4 || case == 5,
                            "only the operator or literal changes here"
                        );
                    }
                }
            }
        }
        assert!(
            WellFormedCardProgram::check(raw).is_err(),
            "a noncanonical cost predicate must fail before runtime"
        );
    }
}

#[test]
fn raw_trait_unions_cannot_change_other_branch_conditions_or_repeat_a_trait() {
    let program = lowerer()
        .lower(&observer(
            &json!({"orig_cost_le":3,"traits_any":["宴楽","魔法使い"]}),
            false,
        ))
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    for case in 0_u8..3 {
        let mut raw = program.card().clone();
        if let TriggerCondition::Event {
            pattern:
                EventPattern::Play {
                    predicate: Expr::Compute(outer),
                    ..
                },
        } = &mut raw.abilities[0].node.trigger.as_mut().unwrap().on
            && let Predicate::PlayedCard {
                predicate: Expr::Compute(inner),
                ..
            } = outer.as_mut()
            && let Predicate::Or { predicates } = inner.as_mut()
        {
            match case {
                0 => {
                    predicates[1] = predicates[0].clone();
                }
                1 => {
                    predicates.pop();
                }
                _ => {
                    if let Expr::Compute(branch) = &mut predicates[1]
                        && let Predicate::And { predicates } = branch.as_mut()
                        && let Some(Expr::Compute(last)) = predicates.last_mut()
                        && let Predicate::CompareInt { right, .. } = last.as_mut()
                    {
                        *right = Expr::Literal(Int(4));
                    }
                }
            }
        }
        assert!(
            WellFormedCardProgram::check(raw).is_err(),
            "the union must be one shared condition with distinct traits"
        );
    }
}

#[test]
fn original_cost_observers_use_true_opponent_turn_quick_and_respect_own_turn_only() {
    use sve_engine::game::internal::InputDomain;
    for own_turn in [false, true] {
        let filter = if own_turn {
            json!({"orig_cost_le":1,"kind":"spell"})
        } else {
            json!({"orig_cost_ge":5})
        };
        let cost = if own_turn { "1" } else { "5" };
        let mut board = probe_at(&filter, own_turn, Some(&json!(cost)), &[], 1, 2);
        board
            .internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap();
        let q = board.internal_quick_choice().unwrap().unwrap();
        assert_eq!(q.actor, PlayerId { seat: 1 });
        if let InputDomain::Quick { options, .. } = q.domain {
            assert_eq!(options.len(), 2_usize);
            resolved(
                board
                    .answer_internal_card(q.actor, q.token, Answer::Quick(Some(options[0].handle)))
                    .unwrap(),
            );
        } else {
            panic!("the opponent turn exposes an actual Quick choice");
        }
        assert_eq!(drain_observers(&mut board), usize::from(!own_turn));
    }
}

#[test]
fn has_info_remains_unavailable_as_an_ordinary_body_condition() {
    let program = lowerer()
        .lower(&observer(&json!({"kind":"spell"}), false))
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    let mut raw = program.card().clone();
    let body_source = raw.abilities[0].source.clone();
    raw.abilities[0]
        .node
        .body
        .as_mut()
        .unwrap()
        .steps
        .push(Located {
            source: body_source,
            node: Node::Control {
                control: Control::If {
                    condition: Expr::Literal(Bool(false)),
                    then_body: Program {
                        scope: ScopeId(900),
                        steps: vec![],
                    },
                    else_body: None,
                },
            },
        });
    WellFormedCardProgram::check(raw.clone()).unwrap();
    if let Node::Control {
        control: Control::If { condition, .. },
    } = &mut raw.abilities[0]
        .node
        .body
        .as_mut()
        .unwrap()
        .steps
        .last_mut()
        .unwrap()
        .node
    {
        *condition = Expr::Compute(Box::new(Predicate::HasInfo {
            information: NumberRead::Cost {
                object: ObjectView::Current {
                    object: Box::new(ObjectOperand::SourceHolder {}),
                },
                view: CharacteristicView::Original,
            },
        }));
    }
    assert!(
        WellFormedCardProgram::check(raw).is_err(),
        "the guarded Played header must not open a general information predicate"
    );
}
