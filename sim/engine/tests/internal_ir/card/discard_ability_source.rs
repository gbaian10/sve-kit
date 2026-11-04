#![expect(
    clippy::panic,
    reason = "Unexpected decision kinds invalidate the discard authority scenarios."
)]
use super::*;
use sve_engine::game::internal::{DecisionView, InputDomain};

fn observer(filtered: bool, steps: Value) -> Value {
    let mut head = json!({
        "zone":"hand",
        "on":{"discarded":{"ref":"this","who":"self","from":"hand"}},
    });
    head["do"] = steps;
    if filtered {
        head["on"]["discarded"]["by"] = json!({"ability_of":{"who":"self","trait":"ウマ娘"}});
    }
    head
}

fn producer(cost: Value, steps: Value) -> Value {
    let mut ability = json!({"activate":{}});
    ability["activate"]["cost"] = cost;
    ability["activate"]["do"] = steps;
    ability
}

fn initial() -> Value {
    let mut state = setup(1, json!([]));
    for seat in ["P1", "P2"] {
        state["players"][seat]["pp"] = json!({"current":10_u8,"max":10_u8});
        state["players"][seat]["zones"]["field"] = json!([]);
        state["players"][seat]["zones"]["hand"] = json!([]);
        state["players"][seat]["zones"]["deck"] = json!([
            {"id":format!("{seat}-deck0"),"card":"unit-spell"},
            {"id":format!("{seat}-deck1"),"card":"unit-spell"}
        ]);
    }
    state["players"]["P1"]["zones"]["field"] = json!([{"id":"p","card":"unit-amulet"}]);
    state["players"]["P1"]["zones"]["hand"] = json!([{"id":"h","card":"unit-follower"}]);
    state
}

fn tree(head: &Value, source: &Value) -> Value {
    let mut input = equipment_played::tree(&json!({"auto":head}), "unit-follower");
    let emitter = equipment_played::tree(source, "unit-amulet");
    input["cards"]["unit-amulet"] = emitter["cards"]["unit-amulet"].clone();
    input
}

fn fixture(head: &Value, source: &Value, traits: Option<&str>, state: &Value) -> Game {
    let programs = lowerer().lower(&tree(head, source)).unwrap();
    let (snapshot, registry, document) = catalog_documents(&json!("1"));
    let snapshot = snapshot
        .lines()
        .map(|line| {
            let mut row: Value = serde_json::from_str(line).unwrap();
            if row["number"] == "unit-amulet" {
                if let Some(trait_id) = traits {
                    row["faces"][0]["traits"] = json!([trait_id]);
                } else {
                    row["faces"][0].as_object_mut().unwrap().remove("traits");
                }
            }
            row.to_string()
        })
        .collect::<Vec<_>>()
        .join("\n");
    let catalog = Arc::new(
        Catalog::from_documents(&snapshot, &registry, &[("unit.yaml".into(), document)]).unwrap(),
    );
    let mut game = Game::new(
        catalog,
        state,
        &Value::Null,
        &Value::Null,
        "direct-discard-source",
    )
    .unwrap();
    for (number, program) in programs {
        game.load_internal_card(&number, &program).unwrap();
    }
    game.load_internal_card("unit-spell", &card(vec![]))
        .unwrap();
    game
}

fn finish_checks(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}

fn take_pending(game: &mut Game) -> CardPlay {
    finish_checks(game);
    let q = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &q.domain else {
        panic!("discard occurrence order");
    };
    assert!(
        options.iter().all(|option| option.event == "Discard"),
        "only committed discard occurrences remain"
    );
    game.answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
        .unwrap()
}

fn drain(game: &mut Game) -> u8 {
    let mut count = 0_u8;
    loop {
        finish_checks(game);
        if game.internal_pending_choice().unwrap().is_none() {
            return count;
        }
        resolved(take_pending(game));
        count = count.checked_add(1).unwrap();
    }
}

fn question(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(q) = play else {
        panic!("a printed decision must be offered");
    };
    q
}

fn activate(game: &mut Game, actor: u32) -> CardPlay {
    game.activate_internal_card("p", AbilityId(1), PlayerId { seat: actor })
        .unwrap()
}

fn discard_all() -> Value {
    producer(json!({"pp":0_u8}), json!([{"discard":{"all":true}}]))
}

#[test]
fn direct_trait_and_ability_controller_filter_each_committed_card_independently_of_ownership() {
    for (actor, owner, trait_id, expected) in [
        (1_u32, "P2", "ウマ娘", 2_u8),
        (1, "P1", "学院", 0),
        (2, "P1", "ウマ娘", 0),
    ] {
        let mut state = initial();
        state["players"]["P1"]["zones"]["hand"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"h2","card":"unit-follower"}));
        let mut source = discard_all();
        if actor == 2 {
            state["turn"]["active"] = json!("P2");
            state["players"]["P1"]["zones"]["field"] = json!([]);
            source["activate"]["do"][0]["discard"]["who"] = json!("opp");
        }
        state["players"][format!("P{actor}")]["zones"]["field"] =
            json!([{"id":"p","card":"unit-amulet","owner":owner}]);
        let mut game = fixture(
            &observer(true, json!([{"life":{"who":"self","delta":1_u8}}])),
            &source,
            Some(trait_id),
            &state,
        );
        resolved(activate(&mut game, actor));
        assert_eq!(drain(&mut game), expected);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
            20_u8 + expected
        );
    }
}

#[test]
fn direct_ability_cost_discard_is_observed_before_its_body_without_confusing_plain_movement() {
    let head = observer(true, json!([{"life":{"who":"self","delta":1_u8}}]));
    let source = producer(
        json!({"discard":{"sel":{"who":"self","zone":"hand"},"n":1_u8}}),
        json!([]),
    );
    let mut game = fixture(&head, &source, Some("ウマ娘"), &initial());
    let q = question(activate(&mut game, 1));
    let InputDomain::CostMaterials(domain) = &q.domain else {
        panic!("formal hand cost selection");
    };
    resolved(
        game.answer_internal_card(
            q.actor,
            q.token,
            Answer::CostMaterials(vec![domain.members[0].handle]),
        )
        .unwrap(),
    );
    assert_eq!(drain(&mut game), 1);
    let source = producer(
        json!({"pp":0_u8}),
        json!([
            {"choose":{"bind":"picked","from":{"who":"self","zone":"hand"},"n":1_u8}},
            {"banish":{"ref":"picked"}}
        ]),
    );
    let mut plain = fixture(&head, &source, Some("ウマ娘"), &initial());
    let q = question(activate(&mut plain, 1));
    let InputDomain::Targets(domain) = &q.domain else {
        panic!("plain movement choice");
    };
    resolved(
        plain
            .answer_internal_card(
                q.actor,
                q.token,
                Answer::Targets(vec![domain.members[0].handle]),
            )
            .unwrap(),
    );
    assert_eq!(drain(&mut plain), 0);
}

#[test]
fn departed_direct_source_uses_its_authority_snapshot_and_unfiltered_sources_need_no_traits() {
    let source = producer(
        json!({"pp":0_u8}),
        json!([
            {"move":{"ref":"this","to":"cemetery"}}, {"discard":{"all":true}}
        ]),
    );
    let mut game = fixture(
        &observer(true, json!([{"life":{"who":"self","delta":1_u8}}])),
        &source,
        Some("ウマ娘"),
        &initial(),
    );
    resolved(activate(&mut game, 1));
    assert_eq!(drain(&mut game), 1);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["field"],
        json!([])
    );
    let mut plain = fixture(
        &observer(false, json!([{"life":{"who":"self","delta":1_u8}}])),
        &discard_all(),
        None,
        &initial(),
    );
    resolved(activate(&mut plain, 1));
    assert_eq!(drain(&mut plain), 1);
}

fn guarded_return() -> Value {
    json!([{"if":{
        "cond":{"cmp":{"left":{"resource":{"who":"self","kind":"pp_max"}},"op":"ge","right":7_u8}},
        "then":[{"pay":{"cost":{"pp":3_u8},"do":[{"move":{"ref":"this","to":"field"}}]}}]
    }}])
}

#[test]
fn conditional_payment_rechecks_maximum_and_never_moves_or_spends_on_decline_or_insufficient_pp() {
    for (maximum, current, accept, enters) in [
        (7_u8, 7_u8, true, true),
        (7, 7, false, false),
        (7, 2, true, false),
        (6, 6, true, false),
    ] {
        let mut state = initial();
        state["players"]["P1"]["pp"] = json!({"max":maximum,"current":current});
        let mut game = fixture(
            &observer(true, guarded_return()),
            &discard_all(),
            Some("ウマ娘"),
            &state,
        );
        resolved(activate(&mut game, 1));
        let result = take_pending(&mut game);
        if maximum >= 7 {
            let q = question(result);
            assert_eq!(q.domain, InputDomain::OptionalCost);
            resolved(
                game.answer_internal_card(q.actor, q.token, Answer::OptionalCost(accept))
                    .unwrap(),
            );
        } else {
            resolved(result);
        }
        finish_checks(&mut game);
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(
            view["P1"]["pp"]["current"],
            current - if enters { 3 } else { 0 }
        );
        assert_eq!(
            view["P1"]["field"],
            if enters {
                json!(["p", "h"])
            } else {
                json!(["p"])
            }
        );
    }
}

#[test]
fn optional_ex_movement_follows_only_the_triggering_successful_generation() {
    for accept in [true, false] {
        let head = observer(
            true,
            json!([{"optional":{"do":[{"move":{"ref":"this","to":"ex"}}]}}]),
        );
        let mut game = fixture(&head, &discard_all(), Some("ウマ娘"), &initial());
        resolved(activate(&mut game, 1));
        let q = question(take_pending(&mut game));
        assert_eq!(
            q.domain,
            InputDomain::Integer(Cardinality {
                minimum: Nat(0),
                maximum: Some(Nat(1))
            })
        );
        resolved(
            game.answer_internal_card(q.actor, q.token, Answer::Integer(Nat(u64::from(accept))))
                .unwrap(),
        );
        finish_checks(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["ex"],
            if accept { json!(["h"]) } else { json!([]) }
        );
    }
    let source = producer(
        json!({"pp":0_u8}),
        json!([
            {"discard":{"all":true}},
            {"choose":{"bind":"retrieved","from":{"who":"self","zone":"cemetery","kind":"follower"},"n":1_u8}},
            {"move":{"ref":"retrieved","to":"hand"}}
        ]),
    );
    let mut game = fixture(
        &observer(true, json!([{"move":{"ref":"this","to":"ex"}}])),
        &source,
        Some("ウマ娘"),
        &initial(),
    );
    let q = question(activate(&mut game, 1));
    let InputDomain::Targets(domain) = &q.domain else {
        panic!("a second movement before the pending trigger");
    };
    resolved(
        game.answer_internal_card(
            q.actor,
            q.token,
            Answer::Targets(vec![domain.members[0].handle]),
        )
        .unwrap(),
    );
    assert_eq!(drain(&mut game), 1);
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["ex"], json!([]));
    assert_eq!(view["P1"]["hand"], json!(["h"]));
}

#[test]
fn source_filters_reject_other_players_kinds_and_missing_self_hand_authority() {
    let valid = observer(true, json!([{"life":{"who":"self","delta":1_u8}}]));
    lowerer().lower(&tree(&valid, &discard_all())).unwrap();
    for case in 0_u8..12 {
        let mut head = valid.clone();
        match case {
            0 => {
                head["on"]["discarded"]
                    .as_object_mut()
                    .unwrap()
                    .remove("ref");
            }
            1 => {
                head["on"]["discarded"]["who"] = json!("opp");
            }
            2 => {
                head["on"]["discarded"]
                    .as_object_mut()
                    .unwrap()
                    .remove("who");
            }
            3 => {
                head["on"]["discarded"]["during"] = json!({"turn":"self"});
            }
            4 => {
                head["on"]["discarded"]["by"]["ability_of"]["trait"] = json!("");
            }
            5 => {
                head["on"]["discarded"]["by"]["ability_of"]["who"] = json!("opp");
            }
            6 => {
                head["on"]["discarded"]["by"]["ability_of"]
                    .as_object_mut()
                    .unwrap()
                    .remove("who");
            }
            7 => {
                head["on"]["discarded"]["by"]["ability_of"]["name"] = json!("unit-amulet");
            }
            8 => {
                head["on"]["discarded"]["by"]["ability_of"]["kind"] = json!("amulet");
            }
            9 => {
                head["on"]["discarded"]["by"]["ability_of"]["zone"] = json!("field");
            }
            10 => {
                head["on"]["discarded"]["by"]["ability_of"]["traits"] = json!(["ウマ娘"]);
            }
            _ => {
                head["on"]["discarded"]["by"]["ability_of"]["keyword"] = json!("ward");
            }
        }
        lowerer().lower(&tree(&head, &discard_all())).unwrap_err();
    }
}

#[test]
fn self_discard_movement_and_resolution_pp_remain_closed_to_the_needed_actions() {
    for destination in ["hand", "deck", "cemetery"] {
        let head = observer(false, json!([{"move":{"ref":"this","to":destination}}]));
        lowerer().lower(&tree(&head, &discard_all())).unwrap_err();
    }
    for case in 0_u8..6 {
        let mut head = observer(false, guarded_return());
        match case {
            0 => {
                head["do"] = head["do"][0]["if"]["then"].clone();
            }
            1 => {
                head["do"][0]["if"]["cond"] = json!({"cmp":{"left":{"resource":{"who":"self","kind":"pp"}},"op":"ge","right":7_u8}});
            }
            2 => {
                head["do"][0]["if"]["then"][0]["pay"]["cost"]["pp"] = json!(0_u8);
            }
            3 => {
                head["do"][0]["if"]["then"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!({"draw":{"n":1_u8}}));
            }
            4 => {
                head["do"][0]["if"]["else"] = json!([]);
            }
            _ => {
                head["do"][0]["if"]["then"][0]["pay"]["cost"] =
                    json!({"steps":[{"pp":3_u8},{"act_self":true}]});
            }
        }
        lowerer().lower(&tree(&head, &discard_all())).unwrap_err();
    }
}

fn lowered_observer(head: &Value) -> CardProgram {
    lowerer().lower(&tree(head, &discard_all())).unwrap()["unit-follower"]
        .card()
        .clone()
}

fn reject_raw(program: CardProgram) {
    if let Ok(checked) = WellFormedCardProgram::check(program) {
        let mut game = Game::new(
            catalog(),
            &initial(),
            &Value::Null,
            &Value::Null,
            "raw-discard-boundary",
        )
        .unwrap();
        let before = format!("{game:?}");
        game.load_internal_card("unit-follower", &checked)
            .unwrap_err();
        assert_eq!(format!("{game:?}"), before, "native rejection is atomic");
    }
}

#[test]
fn raw_follow_rejects_field_batch_foreign_event_and_other_destination_authority() {
    let valid = lowered_observer(&observer(false, json!([{"move":{"ref":"this","to":"ex"}}])));
    for case in 0_u8..6 {
        let mut program = valid.clone();
        let ability = &mut program.abilities[0].node;
        match case {
            0 => {
                ability.active_zones = vec![ZoneKind::Field];
                ability.trigger.as_mut().unwrap().active_zones = vec![ZoneKind::Field];
            }
            1 => {
                ability.trigger.as_mut().unwrap().aggregation = TriggerAggregation::PerBatch {
                    key: BatchPolicy::Instruction {},
                }
            }
            _ => {
                let Node::Instruction { instruction } =
                    &mut ability.body.as_mut().unwrap().steps[0].node
                else {
                    panic!("self movement");
                };
                let Action::Move {
                    objects, movement, ..
                } = &mut instruction.action
                else {
                    panic!("self movement");
                };
                if case == 2 {
                    *objects = Selector::Objects {
                        objects: vec![ObjectOperand::FollowEventMove {
                            event: MovementEvent::Discard {
                                event: Binding::new(999, ability.scope),
                            },
                        }],
                    };
                } else if case == 3 {
                    movement.to = MoveDestination::Owners {
                        zone: OwnerZone::Hand,
                    };
                } else if case == 4 {
                    *objects = Selector::Objects {
                        objects: vec![ObjectOperand::SourceHolder {}],
                    };
                } else {
                    ability.trigger = None;
                    ability.kind = AbilityKind::Spell;
                    ability.active_zones = vec![ZoneKind::Resolution];
                }
            }
        }
        reject_raw(program);
    }
}

#[test]
fn raw_source_predicates_reject_changes_to_actor_view_policy_and_canonical_shape() {
    let valid = lowered_observer(&observer(
        true,
        json!([{"life":{"who":"self","delta":1_u8}}]),
    ));
    for case in 0_u8..7 {
        let mut program = valid.clone();
        let TriggerCondition::Event {
            pattern: EventPattern::Discard { predicate, .. },
        } = &mut program.abilities[0].node.trigger.as_mut().unwrap().on
        else {
            panic!("discard header");
        };
        let Expr::Compute(outer) = predicate else {
            panic!("canonical conjunction");
        };
        let Predicate::And { predicates } = outer.as_mut() else {
            panic!("canonical conjunction");
        };
        if case == 6 {
            predicates.push(predicates[2].clone());
        } else {
            let Expr::Compute(source) = &mut predicates[2] else {
                panic!("source clause");
            };
            let Predicate::And { predicates } = source.as_mut() else {
                panic!("source conjunction");
            };
            if case == 0 {
                let Expr::Compute(actor) = &mut predicates[0] else {
                    panic!("actor clause");
                };
                let Predicate::PlayerEquals { right, .. } = actor.as_mut() else {
                    panic!("actor clause");
                };
                *right = Expr::Literal(PlayerRef::Owner {});
            } else {
                let Expr::Compute(cause) = &mut predicates[1] else {
                    panic!("source cause");
                };
                let Predicate::EventCause {
                    event,
                    pattern:
                        CausePattern::Effect {
                            source: EffectSource::Matching { predicate },
                        },
                } = cause.as_mut()
                else {
                    panic!("source predicate");
                };
                if case == 5 {
                    *event = EventContext::Discard {
                        event: Binding::new(999, ScopeId(1)),
                    };
                } else {
                    let Expr::Compute(condition) = &mut predicate.condition else {
                        panic!("trait condition");
                    };
                    let Predicate::HasTrait { traits, trait_id } = condition.as_mut() else {
                        panic!("trait condition");
                    };
                    if case == 1 {
                        trait_id.0.clear();
                    } else {
                        let Expr::Live(read) = traits else {
                            panic!("effective traits");
                        };
                        let TraitsRead::Traits { object, view } = read.as_mut();
                        if case == 2 {
                            *view = CharacteristicView::Original;
                        } else {
                            let ObjectView::EventSnapshot {
                                object: source,
                                policy,
                                ..
                            } = object
                            else {
                                panic!("source snapshot");
                            };
                            if case == 3 {
                                *policy = EventSnapshotPolicy::AfterEntry;
                            } else {
                                *object = ObjectView::Current {
                                    object: source.clone(),
                                };
                            }
                        }
                    }
                }
            }
        }
        reject_raw(program);
    }
}

#[test]
fn rule_hand_limit_has_no_ability_source_but_unfiltered_self_movement_uses_its_commit() {
    for filtered in [true, false] {
        let mut state = initial();
        for index in 0_u8..7 {
            state["players"]["P1"]["zones"]["hand"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":format!("f{index}"),"card":"unit-spell"}));
        }
        let mut game = fixture(
            &observer(filtered, json!([{"move":{"ref":"this","to":"ex"}}])),
            &discard_all(),
            Some("ウマ娘"),
            &state,
        );
        game.internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap();
        quick::pass(&mut game);
        game.internal_rule_action(&json!({"do":"end-discard","select":["h"]}))
            .unwrap();
        assert_eq!(drain(&mut game), u8::from(!filtered));
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["ex"],
            if filtered { json!([]) } else { json!(["h"]) }
        );
    }
}

#[test]
fn event_cause_in_a_body_does_not_gain_the_header_exception() {
    let mut program = lowered_observer(&observer(
        false,
        json!([{"if":{
            "cond":{"cmp":{"left":{"resource":{"who":"self","kind":"pp_max"}},"op":"ge","right":7_u8}},
            "then":[{"life":{"who":"self","delta":1_u8}}]
        }}]),
    ));
    let ability = &mut program.abilities[0].node;
    let TriggerCondition::Event {
        pattern: EventPattern::Discard { binding, .. },
    } = &ability.trigger.as_ref().unwrap().on
    else {
        panic!("discard event");
    };
    let event = *binding;
    let Node::Control {
        control: Control::If { condition, .. },
    } = &mut ability.body.as_mut().unwrap().steps[0].node
    else {
        panic!("body condition");
    };
    *condition = Expr::Compute(Box::new(Predicate::EventCause {
        event: EventContext::Discard { event },
        pattern: CausePattern::Effect {
            source: EffectSource::Any {},
        },
    }));
    let checked = WellFormedCardProgram::check(program).unwrap();
    let mut game = Game::new(
        catalog(),
        &initial(),
        &Value::Null,
        &Value::Null,
        "body-cause-rejection",
    )
    .unwrap();
    game.load_internal_card("unit-follower", &checked)
        .unwrap_err();
}

#[test]
fn raw_field_and_non_discard_resolution_pp_do_not_gain_the_hand_exception() {
    let valid = lowered_observer(&observer(false, guarded_return()));
    for phase in [false, true] {
        let mut program = valid.clone();
        let ability = &mut program.abilities[0].node;
        ability.active_zones = vec![ZoneKind::Field];
        let head = if phase {
            json!({"on":{"phase":{"which":"end","whose":"self"}},"do":[{"life":{"who":"self","delta":1_u8}}]})
        } else {
            json!({"on":{"discarded":{"who":"self"}},"do":[{"life":{"who":"self","delta":1_u8}}]})
        };
        let field = lowered_observer(&head);
        ability.trigger = field.abilities[0].node.trigger.clone();
        let Node::Control {
            control: Control::If { then_body, .. },
        } = &mut ability.body.as_mut().unwrap().steps[0].node
        else {
            panic!("guarded pay");
        };
        let Node::Control {
            control: Control::Pay { on_paid, .. },
        } = &mut then_body.steps[0].node
        else {
            panic!("resolution pay");
        };
        let Node::Instruction { instruction } = &mut on_paid.steps[0].node else {
            panic!("success instruction");
        };
        instruction.action = Action::Life {
            player: PlayerRef::Controller {},
            change: NumericChange::Add {
                amount: Expr::Literal(Int(1)),
            },
        };
        let checked = WellFormedCardProgram::check(program).unwrap();
        let mut game = Game::new(
            catalog(),
            &initial(),
            &Value::Null,
            &Value::Null,
            "field-pp-rejection",
        )
        .unwrap();
        game.load_internal_card("unit-follower", &checked)
            .unwrap_err();
    }
}

#[test]
fn a_granted_discard_ability_uses_its_recipient_holder_instead_of_the_provider() {
    let grant = producer(
        json!({"pp":0_u8}),
        json!([
            {"grant":{"on":{"who":"self","zone":"field","kind":"follower"},"abilities":[
                {"activate":{"cost":{"pp":0_u8},"do":[{"discard":{"all":true}}]}}
            ]}}
        ]),
    );
    let mut state = initial();
    state["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"receiver","card":"unit-follower"}));
    let mut game = fixture(
        &observer(true, json!([{"life":{"who":"self","delta":1_u8}}])),
        &grant,
        Some("ウマ娘"),
        &state,
    );
    resolved(activate(&mut game, 1));
    finish_checks(&mut game);
    let source = game
        .internal_activated_sources("receiver")
        .unwrap()
        .pop()
        .unwrap();
    resolved(
        game.activate_internal_card_instance("receiver", source.instance, PlayerId { seat: 1 })
            .unwrap(),
    );
    assert_eq!(
        drain(&mut game),
        0,
        "the Uma provider is not the direct holder of the granted ability"
    );
}

#[test]
fn a_discarded_card_follows_its_owners_cemetery_to_its_owners_ex() {
    let mut state = initial();
    state["players"]["P1"]["zones"]["hand"][0]["owner"] = json!("P2");
    let mut game = fixture(
        &observer(true, json!([{"move":{"ref":"this","to":"ex"}}])),
        &discard_all(),
        Some("ウマ娘"),
        &state,
    );
    resolved(activate(&mut game, 1));
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["cemetery"],
        json!(["h"])
    );
    assert_eq!(drain(&mut game), 1);
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P2"]["ex"], json!(["h"]));
    assert_eq!(view["P1"]["ex"], json!([]));
}

#[test]
fn invalid_material_answer_does_not_commit_discard_or_pending_authority() {
    let mut game = fixture(
        &observer(true, json!([{"life":{"who":"self","delta":1_u8}}])),
        &producer(
            json!({"discard":{"n":1_u8,"sel":{"who":"self","zone":"hand"}}}),
            json!([]),
        ),
        Some("ウマ娘"),
        &initial(),
    );
    let q = question(activate(&mut game, 1));
    let InputDomain::CostMaterials(domain) = &q.domain else {
        panic!("actual discard cost");
    };
    let before = format!("{game:?}");
    game.answer_internal_card(q.actor, q.token, Answer::CostMaterials(vec![u64::MAX]))
        .unwrap_err();
    assert_eq!(format!("{game:?}"), before);
    resolved(
        game.answer_internal_card(
            q.actor,
            q.token,
            Answer::CostMaterials(vec![domain.members[0].handle]),
        )
        .unwrap(),
    );
    assert_eq!(
        drain(&mut game),
        1,
        "only the successful retry commits one occurrence"
    );
}

#[test]
fn nested_field_payment_cannot_hide_a_positive_pp_resolution_payment() {
    let mut program = lowered_observer(&json!({
        "on":{"phase":{"which":"end","whose":"self"}},
        "do":[{"pay":{"cost":{"act_self":true},"do":[
            {"pay":{"cost":{"act_self":true},"do":[{"life":{"who":"self","delta":1_u8}}]}}
        ]}}]
    }));
    let Node::Control {
        control: Control::Pay { on_paid, .. },
    } = &mut program.abilities[0].node.body.as_mut().unwrap().steps[0].node
    else {
        panic!("outer source payment");
    };
    let Node::Control {
        control: Control::Pay { costs, .. },
    } = &mut on_paid.steps[0].node
    else {
        panic!("nested payment");
    };
    let CostSpec::Ordered { steps } = costs else {
        panic!("one nested payment");
    };
    steps[0].instruction.action = Action::Resource {
        player: PlayerRef::Controller {},
        resource: Resource::PpCurrent,
        operation: ResourceOperation::Pay,
        amount: Expr::Literal(Int(3)),
    };
    let checked = WellFormedCardProgram::check(program).unwrap();
    let mut game = Game::new(
        catalog(),
        &initial(),
        &Value::Null,
        &Value::Null,
        "nested-resolution-pp-boundary",
    )
    .unwrap();
    let before = format!("{game:?}");
    game.load_internal_card("unit-follower", &checked)
        .unwrap_err();
    assert_eq!(format!("{game:?}"), before);
}

#[test]
fn conditional_pp_payment_cannot_move_to_ex_instead_of_returning_to_field() {
    let mut program = lowered_observer(&observer(false, guarded_return()));
    let Node::Control {
        control: Control::If { then_body, .. },
    } = &mut program.abilities[0].node.body.as_mut().unwrap().steps[0].node
    else {
        panic!("guarded return");
    };
    let Node::Control {
        control: Control::Pay { on_paid, .. },
    } = &mut then_body.steps[0].node
    else {
        panic!("return payment");
    };
    let Node::Instruction { instruction } = &mut on_paid.steps[0].node else {
        panic!("return movement");
    };
    let Action::Move { movement, .. } = &mut instruction.action else {
        panic!("return movement");
    };
    movement.to = MoveDestination::Owners {
        zone: OwnerZone::Ex,
    };
    let checked = WellFormedCardProgram::check(program).unwrap();
    let mut game = Game::new(
        catalog(),
        &initial(),
        &Value::Null,
        &Value::Null,
        "pp-return-destination-boundary",
    )
    .unwrap();
    game.load_internal_card("unit-follower", &checked)
        .unwrap_err();
}
