#![expect(
    clippy::panic,
    reason = "Synthetic attachment fixtures must preserve their declared body shape."
)]
use super::*;
use sve_engine::game::internal::{Answer, InputDomain};

fn active_source_program() -> WellFormedCardProgram {
    let tree = equipment_played::tree(
        &json!({"auto":{
            "zone":"equipment", "on":{"equipped":{"equipment":"this"}},
            "do":[{"buff":{"on":{"event":"holder"},"power":1}},
                {"life":{"who":"self","delta":{"power_of":"this"}}}]
        }}),
        "unit-spell",
    );
    let program = lowerer()
        .lower(&tree)
        .unwrap()
        .remove("unit-spell")
        .unwrap();
    let mut body = program.card().clone();
    let Node::Instruction { instruction } =
        &mut body.abilities[0].node.body.as_mut().unwrap().steps[0].node
    else {
        panic!("synthetic first instruction");
    };
    let Action::Modify { subjects, .. } = &instruction.action else {
        panic!("synthetic holder modification");
    };
    instruction.action = Action::Move {
        objects: subjects.clone(),
        movement: MoveSpec {
            to: MoveDestination::Owners {
                zone: OwnerZone::Hand,
            },
            initial_posture: None,
            entry_effects: vec![],
            position: None,
            order: None,
        },
        reason: MoveReason::Put,
    };
    WellFormedCardProgram::check(body).unwrap()
}

#[test]
fn an_equipment_execution_retains_source_information_when_its_body_moves_its_host() {
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([]);
    let mut board = Game::new(
        equipment::equipment_catalog("イクイップメント・トークン"),
        &initial,
        &Value::Null,
        &Value::Null,
        "equipment-source-departure",
    )
    .unwrap();
    board
        .load_internal_card("unit-spell", &active_source_program())
        .unwrap();
    board
        .load_internal_card(
            "unit-follower",
            &card(vec![ability(
                1,
                vec![instruction(
                    1,
                    Action::Equip {
                        holder: ObjectOperand::SourceHolder {},
                        template: TokenTemplateId("unit-spell".into()),
                        count: Nat(1),
                    },
                )],
            )]),
        )
        .unwrap();
    activate(&mut board, 1);
    board.resume_internal_card().unwrap();
    let question = board.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("equipment occurrence");
    };
    assert_eq!(options.len(), 1);
    resolved(
        board
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap(),
    );
    board.resume_internal_card().unwrap();
    let after = board.projection(View::Referee).unwrap();
    assert_eq!(after["P1"]["hand"], json!(["s"]));
    assert_eq!(after["P1"]["equipment"], json!([]));
    assert_eq!(after["P1"]["leader"]["life"], 10_u8);
    assert!(board.internal_pending_choice().unwrap().is_none());
}

fn automatic() -> Value {
    json!({"zone":"equipment","on":{"equipped":{"equipment":"this"}},
        "do":[{"buff":{"on":{"event":"holder"},"power":1}}]})
}

fn tree(auto: &Value) -> Value {
    equipment_played::tree(&json!({"auto":auto}), "unit-spell")
}

fn plain_program() -> CardProgram {
    lowerer()
        .lower(&tree(&automatic()))
        .unwrap()
        .remove("unit-spell")
        .unwrap()
        .card()
        .clone()
}

#[test]
fn an_equipment_event_accepts_only_its_own_equipment_zone_and_explicit_holder_role() {
    let mut initial = setup(1, json!([]));
    for player in ["P1", "P2"] {
        initial["players"][player]["zones"]["field"] = json!([]);
    }
    let mut board = Game::new(
        equipment::equipment_catalog("イクイップメント・トークン"),
        &initial,
        &Value::Null,
        &Value::Null,
        "closed-equipment-event",
    )
    .unwrap();
    board
        .load_internal_card(
            "unit-spell",
            &WellFormedCardProgram::check(plain_program()).unwrap(),
        )
        .unwrap();
    for case in 0_u8..7 {
        let mut changed = automatic();
        match case {
            0 => changed["on"]["equipped"]["equipment"] = json!("other"),
            1 => changed["zone"] = json!("field"),
            2 => {
                changed.as_object_mut().unwrap().remove("zone");
            }
            3 => changed["zone"] = json!(["equipment", "field"]),
            4 => changed["on"]["equipped"]["who"] = json!("self"),
            5 => changed["aggregation"] = json!("batch"),
            _ => changed["do"][0]["buff"]["on"] = json!("it"),
        }
        lowerer().lower(&tree(&changed)).unwrap_err();
    }
    let misplaced = equipment_played::tree(
        &json!({"fanfare":{"do":[{"buff":{"on":{"event":"holder"},"power":1}}]}}),
        "unit-follower",
    );
    lowerer().lower(&misplaced).unwrap_err();
    let mut other_event = automatic();
    other_event["on"] = json!({"phase":{"whose":"self","which":"end"}});
    other_event["do"] = json!([]);
    lowerer().lower(&tree(&other_event)).unwrap_err();
}

#[test]
fn equipment_headers_keep_other_views_zones_qualifications_and_event_captures_closed() {
    for case in 0_u8..8 {
        let mut candidate = plain_program();
        let ability = &mut candidate.abilities[0].node;
        match case {
            0 => ability.active_zones = vec![ZoneKind::Field],
            1 => ability.trigger.as_mut().unwrap().active_zones = vec![ZoneKind::Field],
            2 => ability.trigger.as_mut().unwrap().view = EventSnapshotPolicy::BeforeExit,
            3 => {
                ability.trigger.as_mut().unwrap().aggregation = TriggerAggregation::PerBatch {
                    key: BatchPolicy::Instruction {},
                }
            }
            4 => ability.qualification = AbilityQualification::UnionBurst,
            5 => {
                let TriggerCondition::Event {
                    pattern: EventPattern::Equip { predicate, .. },
                } = &mut ability.trigger.as_mut().unwrap().on
                else {
                    panic!("equipment header");
                };
                *predicate = Expr::Literal(Bool(true));
            }
            6 => ability.event_captures.push(Capture::Boolean {
                at: CaptureTime::AtEvent {
                    event: EventContext::Trigger {},
                    policy: EventSnapshotPolicy::AfterEntry,
                },
                value: Expr::Literal(Bool(true)),
                into: CaptureRef {
                    binding: Binding::new(999, ability.scope),
                },
            }),
            _ => {
                ability.source.origin = AbilityOrigin::Keyword {
                    keyword: KeywordId::OnAttack,
                    origin: Box::new(ability.source.origin.clone()),
                }
            }
        }
        WellFormedCardProgram::check(candidate).unwrap_err();
    }
}

#[test]
fn an_equipment_body_cannot_read_the_equipment_role_or_a_foreign_event_binding_as_its_host() {
    let base = plain_program();
    let ability = &base.abilities[0].node;
    let TriggerCondition::Event {
        pattern: EventPattern::Equip { binding, .. },
    } = &ability.trigger.as_ref().unwrap().on
    else {
        panic!("equipment header");
    };
    for role in [
        ObjectFrozen::EquipEquipment { event: *binding },
        ObjectFrozen::EquipHolder {
            event: Binding::new(999, ability.scope),
        },
    ] {
        let mut candidate = base.clone();
        let Node::Instruction { instruction } =
            &mut candidate.abilities[0].node.body.as_mut().unwrap().steps[0].node
        else {
            panic!("holder body");
        };
        let Action::Modify { subjects, .. } = &mut instruction.action else {
            panic!("holder modification");
        };
        *subjects = Selector::Objects {
            objects: vec![ObjectOperand::FromExpression {
                value: Box::new(Expr::Frozen(Box::new(role))),
            }],
        };
        let error = WellFormedCardProgram::check(candidate).unwrap_err();
        assert!(
            error.ir_path.contains("/body"),
            "only the body role is changed: {error}"
        );
    }
}
