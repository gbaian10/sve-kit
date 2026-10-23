use super::*;
use std::sync::OnceLock;
use sve_engine::game::internal::InputDomain;
use sve_engine::ir::lowering::ExpandedLowerer;

pub(super) fn lowerer() -> &'static ExpandedLowerer {
    static LOWERER: OnceLock<ExpandedLowerer> = OnceLock::new();
    LOWERER.get_or_init(|| ExpandedLowerer::new().unwrap())
}

fn lowered(header: &str, payload: Value) -> Located<Ability> {
    lowered_lines(&[(header, payload)]).remove(0)
}

/// Lowered together, the synthetic BP08-038 lines keep distinct scopes on one card.
fn lowered_lines(lines: &[(&str, Value)]) -> Vec<Located<Ability>> {
    let mut tree = fanfare::expanded::tree().clone();
    let mut candidate = tree["cards"]["BP08-038"].take();
    let mut template = candidate["abilities"][0].take();
    template.as_object_mut().unwrap().remove("fanfare");
    candidate["abilities"] = json!(
        lines
            .iter()
            .zip(1_u64..)
            .map(|((header, payload), line)| {
                let mut entry = template.clone();
                entry[*header] = payload.clone();
                entry["line"] = json!(line);
                entry
            })
            .collect::<Vec<_>>()
    );
    tree["cards"] = json!({"BP08-038":candidate});
    let cards = lowerer().lower(&tree).unwrap();
    cards["BP08-038"]
        .card()
        .abilities
        .iter()
        .cloned()
        .zip(1_u64..)
        .map(|(mut ability, line)| {
            ability.source = source("unit-follower");
            ability.source.fragment = None;
            ability.source.line = u32::try_from(line).unwrap();
            let printed = AbilityOrigin::Printed {
                face: FaceId("0".into()),
                line: Nat(line),
            };
            match &mut ability.node.source.origin {
                AbilityOrigin::Keyword { origin, .. } => **origin = printed,
                origin @ (AbilityOrigin::Printed { .. } | AbilityOrigin::Granted { .. }) => {
                    *origin = printed;
                }
            }
            locate_steps(
                &mut ability.node.body.as_mut().unwrap().steps,
                &ability.source,
                &ability.node.source,
            );
            ability
        })
        .collect()
}

fn next(game: &mut Game, handle: u64) -> Activation {
    let question = game.internal_pending_choice().unwrap().unwrap();
    resolved(
        game.answer_internal_card(question.actor, question.token, Answer::Pending(handle))
            .unwrap(),
    )
}

#[test]
fn main_begin_blocks_actions_once_for_first_and_normal_turns() {
    let trigger = lowered(
        "auto",
        json!({"on":{"phase":{"whose":"self","which":"main"}},"do":[{"draw":{"who":"self","n":1}}]}),
    );
    let program = card(vec![trigger, ability(2, vec![draw(20, 0)])]);
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([]);
    for seat in ["P1", "P2"] {
        initial["players"][seat]["zones"]["deck"] = json!(
            (1..=8)
                .map(|n| json!({"id":format!("{seat}-d{n}"),"card":"unit-spell"}))
                .collect::<Vec<_>>()
        );
    }
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["turn"]["active"] = json!("P2");
    for first in [true, false] {
        let mut game = Game::new(catalog(), &initial, &Value::Null, &Value::Null, "main").unwrap();
        if first {
            // A synthetic public checkpoint at the last mulligan isolates the first Main entry.
            let mut checkpoint = serde_json::to_value(&game).unwrap();
            checkpoint["state"]["flow"] =
                json!({"kind":"pregame","stage":"mulligan","mulligan_by":"P2"});
            checkpoint["state"]["turn"]["first_player"] = json!("P1");
            checkpoint["state"]["turn"]["active"] = json!("P1");
            game = serde_json::from_value(checkpoint).unwrap();
        }
        game.load_internal_card("unit-follower", &program).unwrap();
        game.load_internal_card("unit-spell", &card(vec![]))
            .unwrap();
        if first {
            game.internal_rule_action(&json!({"do":"mulligan","redo":false}))
                .unwrap();
        } else {
            game.internal_rule_action(&json!({"do":"end-phase"}))
                .unwrap();
            quick::pass(&mut game);
        }
        let question = game.internal_pending_choice().unwrap().unwrap();
        assert_eq!(question.actor, PlayerId { seat: 1 });
        assert!(
            matches!(&question.domain, InputDomain::Pending(options) if options.len()==1 && options[0].event.contains("Main Begin"))
        );
        let before = exact_authority(&game);
        game.activate_internal_card("s", AbilityId(2), PlayerId { seat: 1 })
            .unwrap_err();
        assert_eq!(before, exact_authority(&game));
        next(&mut game, 1);
        game.resume_internal_card().unwrap();
        game.activate_internal_card("s", AbilityId(2), PlayerId { seat: 1 })
            .unwrap();
        game.resume_internal_card().unwrap();
        assert!(game.internal_pending_choice().unwrap().is_none());
    }
}

fn bounce_source() -> Located<Node> {
    movement::put(
        2,
        Selector::Objects {
            objects: vec![ObjectOperand::SourceHolder {}],
        },
        OwnerZone::Hand,
    )
}

/// The bounce stays closed to after-following: counting its hidden hand generation is rejected.
fn reject_hidden_after_follow() {
    let follow = Expr::Compute(Box::new(IntOperation::FromNat {
        value: Expr::Compute(Box::new(NatOperation::Count {
            objects: Selector::MoveAfter {
                receipt: MovementReceipt::Move {
                    receipt: ReceiptReference::new(ReceiptId(2)),
                },
                permission: FollowPermission::SameEffectExplicitReference {},
            },
        })),
    }));
    let mut game = position_with_card(
        1,
        &card(vec![ability(
            2,
            vec![bounce_source(), draw_value(3, follow)],
        )]),
    );
    let before = exact_authority(&game);
    let error = game
        .activate_internal_card("s", AbilityId(2), PlayerId { seat: 1 })
        .unwrap_err();
    assert!(
        format!("{error:?}").contains("hidden hand after follow"),
        "the hidden after follow must be rejected"
    );
    assert_eq!(
        before,
        exact_authority(&game),
        "a rejected attempt leaves the authority unchanged"
    );
}

/// Without a departure subscription, a destination outside the typed producers stays a plain move.
fn keep_unsubscribed_prototype_destination() {
    let snapshot = [("unit-follower", "フォロワー"), ("unit-spell", "スペル")]
        .map(|(number, kind)| json!({"number":number,"faces":[{"name":number,"card_class":"ニュートラル","card_type":kind,"cost":"1","power":"2","hp":"3","traits":[],"text":null,"sections":[]}]}).to_string())
        .join("\n");
    let registry = json!({"format":1,"kind":"keyword_registry","keywords":{}}).to_string();
    let document = json!({"format":1,"kind":"effect_set","cards":{
        "unit-follower":{"status":"complete","abilities":[{"line":1,"kind":"activated","body":{"op":"move","subjects":"self","to":"evolve_deck"}}]},
        "unit-spell":{"status":"complete","abilities":[]}
    }})
    .to_string();
    let prototype = Arc::new(
        Catalog::from_documents(&snapshot, &registry, &[("prototype.yaml".into(), document)])
            .unwrap(),
    );
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
    let mut game = Game::new(prototype, &initial, &Value::Null, &Value::Null, "prototype").unwrap();
    let activate = game
        .legal()
        .unwrap()
        .into_iter()
        .find(|choice| choice["do"] == "activate")
        .unwrap();
    assert_eq!(
        game.submit(&activate).unwrap().outcome,
        "resolved",
        "the prototype move still resolves"
    );
    assert_eq!(
        game.query(View::Referee, "P1.evolve_deck")
            .unwrap()
            .unwrap(),
        json!(["s"]),
        "an unsubscribed departure keeps its destination"
    );
}

#[test]
fn self_leave_bounce_uses_public_before_and_destroy_prevention_emits_nothing() {
    let trigger = lowered(
        "auto",
        json!({"on":{"left_field":{"ref":"this"}},"do":[{"draw":{"who":"self","n":1}}]}),
    );
    let mut invalid = card(vec![trigger.clone()]).card().clone();
    invalid.abilities[0].node.trigger.as_mut().unwrap().view = EventSnapshotPolicy::AfterEntry;
    WellFormedCardProgram::check(invalid).unwrap_err();
    let program = card(vec![trigger, ability(2, vec![bounce_source()])]);
    let mut game = position_with_card(1, &program);
    game.activate_internal_card("s", AbilityId(2), PlayerId { seat: 1 })
        .unwrap();
    game.resume_internal_card().unwrap();
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("pending");
    };
    assert_eq!(options.len(), 1);
    assert_eq!(options[0].event, "LeaveField from field slot 1");
    // The opponent knew the field card, but the projection must not locate its new hand slot.
    assert_eq!(
        game.projection(View::P2).unwrap()["P1"]["hand"],
        json!([{"filler":1}])
    );
    next(&mut game, 1);
    game.resume_internal_card().unwrap();
    assert!(game.internal_pending_choice().unwrap().is_none());

    reject_hidden_after_follow();

    let trigger = lowered(
        "auto",
        json!({"on":{"left_field":{"ref":"this","during":{"turn":"self"}}},"do":[]}),
    );
    let program = card(vec![
        trigger,
        ability(
            2,
            vec![
                movement::protection(
                    2,
                    Selector::Objects {
                        objects: vec![ObjectOperand::SourceHolder {}],
                    },
                    CausePattern::Effect {
                        source: EffectSource::Any {},
                    },
                ),
                instruction(
                    3,
                    Action::Destroy {
                        objects: Selector::Objects {
                            objects: vec![ObjectOperand::SourceHolder {}],
                        },
                    },
                ),
            ],
        ),
    ]);
    let mut game = position_with_card(1, &program);
    game.activate_internal_card("s", AbilityId(2), PlayerId { seat: 1 })
        .unwrap();
    game.resume_internal_card().unwrap();
    assert!(game.internal_pending_choice().unwrap().is_none());
    assert_eq!(
        game.query(View::Referee, "P1.field").unwrap().unwrap(),
        json!(["s"])
    );
    keep_unsubscribed_prototype_destination();
}

#[test]
fn self_damage_lethal_and_zero_then_combat_share_the_actual_hit_producer() {
    let mut trigger = lowered(
        "auto",
        json!({"on":{"damaged":{"recipient":"this"}},"do":[{"draw":{"who":"self","n":1}}]}),
    );
    let TriggerCondition::Event {
        pattern: EventPattern::Damage { binding, .. },
    } = &trigger.node.trigger.as_ref().unwrap().on
    else {
        panic!("damage");
    };
    let amount = Expr::Frozen(Box::new(IntFrozen::DamageAmount { event: *binding }));
    let Node::Instruction { instruction } = &mut trigger.node.body.as_mut().unwrap().steps[0].node
    else {
        panic!("draw");
    };
    let Action::Draw { count, .. } = &mut instruction.action else {
        panic!("draw");
    };
    *count = Expr::Compute(Box::new(IntOperation::Conditional {
        condition: Expr::Compute(Box::new(Predicate::CompareInt {
            comparison: Comparison::GreaterEqual,
            left: amount,
            right: Expr::Literal(Int(4)),
        })),
        then_value: Expr::Literal(Int(1)),
        else_value: Expr::Literal(Int(0)),
    }));
    let make = |amount| {
        card(vec![
            trigger.clone(),
            ability(
                2,
                vec![damage(
                    2,
                    RecipientSet::Objects {
                        objects: Selector::Objects {
                            objects: vec![ObjectOperand::SourceHolder {}],
                        },
                    },
                    amount,
                )],
            ),
        ])
    };
    let mut game = position_with_card(1, &make(4));
    game.activate_internal_card("s", AbilityId(2), PlayerId { seat: 1 })
        .unwrap();
    game.resume_internal_card().unwrap();
    assert_eq!(
        game.query(View::Referee, "P1.field").unwrap().unwrap(),
        json!([])
    );
    let q = game.internal_pending_choice().unwrap().unwrap();
    assert!(
        matches!(q.domain, InputDomain::Pending(options) if options.len()==1 && options[0].event.contains("Damage 4"))
    );
    next(&mut game, 1);
    game.resume_internal_card().unwrap();
    // The bound amount 4 meets the body condition, so the destroyed source's controller draws.
    assert_eq!(
        game.query(View::Referee, "P1.hand").unwrap().unwrap(),
        json!(["d"])
    );
    let mut game = position_with_card(1, &make(0));
    game.activate_internal_card("s", AbilityId(2), PlayerId { seat: 1 })
        .unwrap();
    game.resume_internal_card().unwrap();
    assert!(game.internal_pending_choice().unwrap().is_none());

    let mut initial = setup(1, json!([{"id":"d","card":"unit-spell"}]));
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"s","card":"unit-follower","state":{"entered_this_turn":false}}]);
    initial["players"]["P2"]["zones"]["field"][0]["state"] = json!({"acted":true});
    let mut game = Game::new(catalog(), &initial, &Value::Null, &Value::Null, "combat").unwrap();
    game.load_internal_card("unit-follower", &make(0)).unwrap();
    game.load_internal_card("unit-spell", &card(vec![]))
        .unwrap();
    game.internal_rule_action(&json!({"do":"attack","attacker":"s","target":"f"}))
        .unwrap();
    quick::pass(&mut game);
    let q = game.internal_pending_choice().unwrap().unwrap();
    assert!(
        matches!(q.domain, InputDomain::Pending(options) if options.len()==1 && options[0].event.contains("Damage 2"))
    );
    next(&mut game, 1);
    game.resume_internal_card().unwrap();
    assert_eq!(
        game.query(View::Referee, "P1.hand").unwrap().unwrap(),
        json!([])
    );
    let q = game.internal_pending_choice().unwrap().unwrap();
    assert_eq!(q.actor, PlayerId { seat: 2 });
    // The opponent's damaged source also waits; no deck is needed to observe its occurrence.
}

pub(super) fn evolve_triggers() -> Vec<Located<Ability>> {
    lowered_lines(&[
        ("evolve_time", json!({"do":[{"draw":{"who":"self","n":1}}]})),
        (
            "super_evolve_time",
            json!({"do":[{"draw":{"who":"self","n":2}}]}),
        ),
    ])
}
