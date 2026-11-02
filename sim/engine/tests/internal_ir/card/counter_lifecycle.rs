use super::*;
use sve_engine::EngineFailure;
use sve_engine::game::internal::{Answer, InputDomain};

fn information_count() -> Expr<Int> {
    Expr::Compute(Box::new(IntOperation::FromNat {
        value: Expr::Frozen(Box::new(NatFrozen::SnapshotCounter {
            object: Expr::Frozen(Box::new(SnapshotFields::SourceInformation {})),
            kind: CounterKind("test".into()),
        })),
    }))
}

#[test]
fn counter_source_information_tracks_live_changes_then_seals_at_body_or_cost_departure() {
    for paid_departure in [false, true] {
        let mut body = vec![];
        if !paid_departure {
            body.push(instruction(
                1,
                Action::Counter {
                    holder: Holder::Object {
                        object: ObjectOperand::SourceHolder {},
                    },
                    kind: CounterKind("test".into()),
                    operation: CounterOperation::Remove {},
                    amount: Expr::Literal(Nat(1)),
                },
            ));
            body.push(movement::put(
                2,
                Selector::Objects {
                    objects: vec![ObjectOperand::SourceHolder {}],
                },
                OwnerZone::Cemetery,
            ));
        }
        body.push(source_information::source_hit(3, information_count()));
        let costs = if paid_departure {
            vec![payment::self_to_cemetery()]
        } else {
            vec![payment::pp(0)]
        };
        let a = payment::paid_ability(costs, body);
        let mut game = position_with_state(1, &card(vec![a]), &json!({"counters":{"test":3}}));
        let result = activate(&mut game, 1);
        let hits = result
            .progress
            .facts
            .iter()
            .filter_map(|fact| {
                if let Fact::Damage(event) = fact {
                    Some(event.amount.0.get())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        assert_eq!(hits, [if paid_departure { 3 } else { 2 }]);
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["P1"]["cemetery"], json!(["s"]));
        assert_eq!(
            view["objects"]["s"]["counters"]["test"]
                .as_u64()
                .unwrap_or(0),
            0
        );
    }
}

fn after_move(counter_on: &Value, destination: &str) -> Value {
    let mut tree = fanfare::expanded::tree().clone();
    let mut seed = tree["cards"]["BP01-041"].clone();
    seed["abilities"][0]["activate"] = json!({
        "cost":{"pp":0},
        "targets":[{"bind":"chosen","from":{"who":"self","zone":"cemetery"},"n":1}],
        "do":[{"move":{"ref":"chosen","to":destination,"result":"moved"}},
            {"counter":{"on":counter_on,"name":"test","delta":1}}]
    });
    tree["cards"] = json!({"unit-follower":seed});
    tree
}

#[test]
fn implicit_old_selection_cannot_follow_a_new_generation_and_rolls_back_the_move() {
    let programs = lowerer()
        .lower(&after_move(&json!("chosen"), "ex"))
        .unwrap();
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
    initial["players"]["P1"]["zones"]["cemetery"] = json!([{"id":"material","card":"unit-amulet"}]);
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "old-counter-binding",
    )
    .unwrap();
    game.load_internal_card("unit-follower", &programs["unit-follower"])
        .unwrap();
    game.load_internal_card("unit-amulet", &card(vec![]))
        .unwrap();
    let origin = game.clone();
    let CardPlay::Awaiting(question) = game
        .activate_internal_card("s", AbilityId(1), PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("target")
    };
    game.answer_internal_card(question.actor, question.token, Answer::Targets(vec![0]))
        .unwrap_err();
    transactions::assert_restored(&game, &origin);
}

#[test]
fn counter_follow_requires_an_explicit_successful_ex_movement_producer() {
    lowerer()
        .lower(&after_move(&json!({"follow":"missing"}), "ex"))
        .unwrap_err();
    lowerer()
        .lower(&after_move(&json!({"follow":"moved"}), "hand"))
        .unwrap_err();
    lowerer()
        .lower(&after_move(&json!({"follow":"moved"}), "ex"))
        .unwrap();
}

fn union_target(zones: &Value) -> Value {
    let mut tree = fanfare::expanded::tree().clone();
    let mut seed = tree["cards"]["BP01-041"].clone();
    seed["abilities"][0]["activate"] = json!({
        "cost":{"pp":0},
        "targets":[{"bind":"chosen","from":{"who":"self","zone":zones,"kind":"follower"},"n":1}],
        "do":[{"counter":{"on":"chosen","name":"test","delta":1}}]
    });
    tree["cards"] = json!({"unit-follower":seed});
    tree
}

#[test]
fn one_formal_counter_target_can_be_in_field_or_ex_without_changing_player_or_kind_filters() {
    let programs = lowerer()
        .lower(&union_target(&json!(["field", "ex"])))
        .unwrap();
    let mut saw_ex = false;
    for choice in 0_usize..3 {
        let mut initial = setup(1, json!([]));
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] = json!([
            {"id":"s","card":"unit-follower"}, {"id":"ally","card":"unit-follower"},
            {"id":"wrong-kind","card":"unit-amulet"}
        ]);
        initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"extra","card":"unit-follower"}]);
        initial["players"]["P2"]["zones"]["field"] = json!([]);
        initial["players"]["P2"]["zones"]["ex"] = json!([{"id":"opponent","card":"unit-follower"}]);
        let mut game = Game::new(
            catalog(),
            &initial,
            &Value::Null,
            &Value::Null,
            "counter-public-pair",
        )
        .unwrap();
        game.load_internal_card("unit-follower", &programs["unit-follower"])
            .unwrap();
        game.load_internal_card("unit-amulet", &card(vec![]))
            .unwrap();
        let CardPlay::Awaiting(question) = game
            .activate_internal_card("s", AbilityId(1), PlayerId { seat: 1 })
            .unwrap()
        else {
            panic!("formal target")
        };
        let InputDomain::Targets(domain) = &question.domain else {
            panic!("public targets")
        };
        assert_eq!(domain.members.len(), 3);
        assert_eq!(domain.cardinality.minimum, Nat(1));
        assert!(
            domain
                .members
                .iter()
                .all(|member| member.player == (PlayerId { seat: 1 })),
            "the union retains the printed controller filter"
        );
        resolved(
            game.answer_internal_card(
                question.actor,
                question.token,
                Answer::Targets(vec![domain.members[choice].handle]),
            )
            .unwrap(),
        );
        let view = game.projection(View::Referee).unwrap();
        let receivers = ["s", "ally", "extra"]
            .into_iter()
            .filter(|id| view["objects"][*id]["counters"]["test"] == 1)
            .collect::<Vec<_>>();
        assert_eq!(receivers.len(), 1);
        saw_ex |= receivers.contains(&"extra");
        for excluded in ["wrong-kind", "opponent"] {
            assert_eq!(
                view["objects"][excluded]["counters"]["test"]
                    .as_u64()
                    .unwrap_or(0),
                0
            );
        }
    }
    assert!(saw_ex, "the same formal choice reaches an actual EX holder");
}

#[test]
fn formal_counter_target_union_does_not_admit_hidden_or_duplicate_zones() {
    for zones in [
        json!(["field", "hand"]),
        json!(["field", "field"]),
        json!(["field", "cemetery"]),
    ] {
        lowerer().lower(&union_target(&zones)).unwrap_err();
    }
}

#[test]
fn counter_target_union_does_not_acquire_a_field_only_extremum_authority() {
    let mut tree = union_target(&json!("field"));
    tree["cards"]["unit-follower"]["abilities"][0]["activate"]["targets"][0]["from"]["where"] = json!({
        "cmp": {"left":{"power_of":"item"},"op":"eq", "right":{"agg":{
            "of":{"who":"self","zone":"field","kind":"follower"},
            "attr":"power","fn":"max"
        }}}
    });
    lowerer().lower(&tree).unwrap();
    tree["cards"]["unit-follower"]["abilities"][0]["activate"]["targets"][0]["from"]["zone"] =
        json!(["field", "ex"]);
    assert_eq!(
        lowerer().lower(&tree).unwrap_err().code,
        "unsupported_construct",
        "a mixed public pool cannot borrow the Field-only extremum guard"
    );
}

#[test]
fn trigger_counter_head_reuses_the_same_selected_material_payment_and_may_be_declined() {
    let mut tree = fanfare::expanded::tree().clone();
    let mut entry = tree["cards"]["BP08-038"]["abilities"][0].clone();
    entry["fanfare"] = json!({
        "cost":{"counter":{"on":{"who":"self","zone":"field","name":"unit-amulet"},"holders":1,"name":"test","n":2}},
        "do":[{"draw":{"who":"self","n":1}}]
    });
    let meta = tree["cards"]["BP08-038"]["meta"].clone();
    tree["cards"] = json!({"unit-follower":{"meta":meta,"abilities":[entry]}});
    let program = lowerer()
        .lower(&tree)
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    for accept in [false, true] {
        let mut initial = setup(1, json!([{"id":"d","card":"unit-spell"}]));
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"s","card":"unit-follower"}]);
        initial["players"]["P1"]["zones"]["field"] = json!([
            {"id":"pool","card":"unit-amulet","state":{"counters":{"test":3}}},
            {"id":"poor","card":"unit-amulet","state":{"counters":{"test":1}}}
        ]);
        initial["players"]["P1"]["zones"]["ex"] =
            json!([{"id":"ex","card":"unit-amulet","state":{"counters":{"test":99}}}]);
        initial["players"]["P2"]["zones"]["field"] =
            json!([{"id":"foreign","card":"unit-amulet","state":{"counters":{"test":99}}}]);
        let mut game = Game::new(
            catalog(),
            &initial,
            &Value::Null,
            &Value::Null,
            "counter-head-material",
        )
        .unwrap();
        game.load_internal_card("unit-follower", &program).unwrap();
        for number in ["unit-amulet", "unit-spell"] {
            game.load_internal_card(number, &card(vec![])).unwrap();
        }
        assert!(
            matches!(
                game.play_internal_card("s", PlayerId { seat: 1 }).unwrap(),
                CardPlay::Entered(_)
            ),
            "normal play reaches the printed fanfare"
        );
        let pending = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = &pending.domain else {
            panic!("fanfare")
        };
        assert_eq!(options.len(), 1);
        let CardPlay::Awaiting(declaration) = game
            .answer_internal_card(
                pending.actor,
                pending.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap()
        else {
            panic!("head declaration")
        };
        assert_eq!(declaration.domain, InputDomain::OptionalCost);
        let play = game
            .answer_internal_card(
                declaration.actor,
                declaration.token,
                Answer::OptionalCost(accept),
            )
            .unwrap();
        if accept {
            let CardPlay::Awaiting(material) = play else {
                panic!("counter pool")
            };
            let InputDomain::CostMaterials(domain) = &material.domain else {
                panic!("materials")
            };
            assert_eq!(domain.members.len(), 1);
            assert_eq!(domain.members[0].player, (PlayerId { seat: 1 }));
            resolved(
                game.answer_internal_card(
                    material.actor,
                    material.token,
                    Answer::CostMaterials(vec![domain.members[0].handle]),
                )
                .unwrap(),
            );
        } else {
            assert_eq!(play, CardPlay::Cancelled);
        }
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(
            view["objects"]["pool"]["counters"]["test"],
            if accept { 1 } else { 3 }
        );
        assert_eq!(view["objects"]["poor"]["counters"]["test"], 1);
        assert_eq!(view["objects"]["ex"]["counters"]["test"], 99);
        assert_eq!(view["objects"]["foreign"]["counters"]["test"], 99);
        assert_eq!(view["P1"]["pp"]["current"], 1);
        assert_eq!(
            view["P1"]["hand"].as_array().unwrap().len(),
            usize::from(accept)
        );
    }
}

#[test]
fn ex_phase_source_can_enter_field_with_zero_counters_but_nonzero_carry_is_rejected() {
    let mut tree = fanfare::expanded::tree().clone();
    let mut entry = tree["cards"]["BP08-038"]["abilities"][0].clone();
    entry.as_object_mut().unwrap().remove("fanfare");
    entry["auto"] = json!({"on":{"phase":{"which":"end","whose":"self"}},"zone":"ex",
        "do":[{"move":{"ref":"this","to":"field"}}]});
    let meta = tree["cards"]["BP08-038"]["meta"].clone();
    tree["cards"] = json!({"unit-follower":{"meta":meta,"abilities":[entry]}});
    for zone in ["field", "hand"] {
        let mut invalid = tree.clone();
        invalid["cards"]["unit-follower"]["abilities"][0]["auto"]["zone"] = json!(zone);
        lowerer().lower(&invalid).unwrap_err();
    }
    let program = lowerer()
        .lower(&tree)
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    for count in [0_u8, 1] {
        let mut initial = setup(1, json!([]));
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] = json!([]);
        initial["players"]["P1"]["zones"]["ex"] =
            json!([{"id":"s","card":"unit-follower","state":{"counters":{"test":count}}}]);
        initial["players"]["P2"]["zones"]["field"] = json!([]);
        let mut game = Game::new(
            catalog(),
            &initial,
            &Value::Null,
            &Value::Null,
            "ex-source-entry",
        )
        .unwrap();
        game.load_internal_card("unit-follower", &program).unwrap();
        assert_eq!(
            game.internal_rule_action(&json!({"do":"end-phase"}))
                .unwrap()
                .outcome,
            "resolved"
        );
        let question = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = &question.domain else {
            panic!("EX phase")
        };
        assert_eq!(options.len(), 1);
        let origin = game.clone();
        let result = game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        );
        if count == 0 {
            resolved(result.unwrap());
            let view = game.projection(View::Referee).unwrap();
            assert_eq!(view["P1"]["field"], json!(["s"]));
            assert_eq!(view["P1"]["ex"], json!([]));
            assert_eq!(view["objects"]["s"]["generation"], 1_u8);
        } else {
            assert!(
                matches!(result, Err(EngineFailure::Unsupported(message)) if message.contains("EX source counter carry")),
                "nonzero counters cannot be silently discarded on EX source entry"
            );
            transactions::assert_restored(&game, &origin);
        }
    }
}
