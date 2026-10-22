use super::*;
use crate::ir::lowering::{ExpandedLowerer, validator};
use alloc::collections::BTreeMap;
use serde_json::json;

#[path = "../../../tests/support/expanded.rs"]
mod expanded;

#[test]
fn expanded_dynamic_scope_rejects_a_parameter_that_authored_accepts() {
    let expanded = validator("expanded").unwrap();
    let mut tree = expanded::tree().clone();
    assert!(expanded.is_valid(&tree));
    tree["cards"]["BP08-038"]["abilities"][0]["fanfare"]["define"]["X"]["minus"][1] =
        json!({"param":"offset"});
    assert!(validator("authored").unwrap().is_valid(&tree));
    assert!(!expanded.is_valid(&tree));
}

#[test]
fn bp08_candidate_matches_the_handwritten_card_program() {
    let cards = ExpandedLowerer::new()
        .unwrap()
        .lower(expanded::tree())
        .unwrap();
    let mut actual = cards["BP08-038"].card().clone();
    let expected = bp08().node;
    let [ability] = actual.abilities.as_mut_slice() else {
        panic!("one fanfare");
    };
    assert_eq!(ability.source.line, 2);
    assert_eq!(ability.source.scope, "ability_0");
    assert_eq!(ability.source.author_path, "/cards/BP08-038/abilities/0");
    ability.source = source("BP08-038");
    normalize_printed_origin(&mut ability.node.source);
    let body = ability.node.body.as_mut().unwrap();
    let [step] = body.steps.as_mut_slice() else {
        panic!("one draw");
    };
    assert_eq!(
        step.source.author_path,
        "/cards/BP08-038/abilities/0/fanfare/do/0"
    );
    step.source = source("BP08-038");
    let Node::Instruction { instruction } = &mut step.node else {
        panic!("draw instruction");
    };
    normalize_printed_origin(&mut instruction.source);
    assert_eq!(actual, expected);
}

fn normalize_printed_origin(source: &mut AbilitySource) {
    let AbilityOrigin::Keyword { origin, .. } = &mut source.origin else {
        panic!("fanfare origin");
    };
    let AbilityOrigin::Printed { face, line } = origin.as_mut() else {
        panic!("printed origin");
    };
    *face = FaceId("base".to_owned());
    *line = Nat(1);
}

#[test]
fn unsupported_step_rejects_the_entire_candidate() {
    let mut tree = expanded::tree().clone();
    tree["cards"]["BP08-038"]["abilities"][0]["fanfare"]["do"]
        .as_array_mut()
        .unwrap()
        .push(json!({"banish":"this"}));
    let failure = ExpandedLowerer::new().unwrap().lower(&tree).unwrap_err();
    assert_eq!(failure.code, "unsupported_construct");
    assert_eq!(failure.path, "/cards/BP08-038/abilities/0");
}

#[test]
fn bp01_118_is_one_uniform_instruction_for_all_leaders() {
    let cards = ExpandedLowerer::new()
        .unwrap()
        .lower(expanded::tree())
        .unwrap();
    let [ability] = cards["BP01-118"].card().abilities.as_slice() else {
        panic!("one fanfare");
    };
    let [step] = ability.node.body.as_ref().unwrap().steps.as_slice() else {
        panic!("one damage instruction");
    };
    let Node::Instruction { instruction } = &step.node else {
        panic!("damage instruction");
    };
    assert_eq!(instruction.id, InstructionId(1));
    assert_eq!(instruction.receipt, ReceiptId(1));
    assert_eq!(
        instruction.action,
        Action::Damage {
            recipients: RecipientSet::Leaders {
                players: PlayerSet {
                    relation: PlayerRelation::All,
                    relative_to: PlayerRef::Controller {},
                    cardinality: Cardinality {
                        minimum: Nat(0),
                        maximum: None
                    },
                    text_quantifier: TextQuantifier::All,
                }
            },
            amount: DamageAmount::Uniform {
                amount: Expr::Literal(Int(1))
            },
            flags: DamageFlags::Ability,
            source: ObjectView::SourceInformation {},
            batch: BatchPolicy::Instruction {},
        }
    );
}

#[test]
fn numeric_duration_and_continuous_preserve_distinct_semantics() {
    let mut tree = expanded::tree().clone();
    let steps = &mut tree["cards"]["BP08-038"]["abilities"][0]["fanfare"]["do"];
    *steps = json!([
        {"set_stat":{"on":"this","hp":1_i64,"until":"next_opp_turn"}},
        {"buff":{"on":"this","power":-1_i64,"until":"this_and_next_opp_turn"}},
        {"apply":{"on":"this","effect":{"stat":{"hp":2_i64}},"until":"turn"}},
        {"apply":{"on":{"who":"self","zone":"field"},"effect":{"stat":{"power":1_i64}}}}
    ]);
    let second = tree["cards"]["BP08-038"]["abilities"][0].clone();
    tree["cards"]["BP08-038"]["abilities"]
        .as_array_mut()
        .unwrap()
        .push(second);
    let cards = ExpandedLowerer::new().unwrap().lower(&tree).unwrap();
    let body = cards["BP08-038"].card().abilities[0]
        .node
        .body
        .as_ref()
        .unwrap();
    let actions = body
        .steps
        .iter()
        .map(|step| match &step.node {
            Node::Instruction { instruction } => &instruction.action,
            Node::Control { .. } | Node::Capture { .. } | Node::Decision { .. } => {
                panic!("instruction")
            }
        })
        .collect::<Vec<_>>();
    assert!(matches!(
        actions[0],
        Action::Modify {
            patch: CharacteristicPatch::Numeric {
                change: NumericChange::Set { .. },
                ..
            },
            expires: Duration {
                end: DurationEnd::NextTurnEnd { .. },
                ..
            },
            ..
        }
    ));
    assert!(
        matches!(actions[1], Action::Modify { expires: Duration { end: DurationEnd::CurrentTurnEnd {}, intervals, .. }, .. } if matches!(intervals.as_slice(), [Interval::DuringNextTurn { .. }]))
    );
    assert!(
        matches!(actions[2], Action::InstallEffect { effect, .. } if matches!(effect.as_ref(), Effect::Continuous { layer: ContinuousLayer::Numeric, .. }))
    );
    tree["cards"]["BP08-038"]["abilities"][0]["fanfare"]["do"][0]["set_stat"]["hp"] = json!("X");
    assert_eq!(
        ExpandedLowerer::new()
            .unwrap()
            .lower(&tree)
            .unwrap_err()
            .code,
        "unsupported_construct"
    );
}

#[test]
fn search_groups_partition_and_successful_ex_discount_are_scoped() {
    let mut tree = expanded::tree().clone();
    tree["cards"]["BP03-002"]["abilities"][0]["fanfare"]["do"] = json!([
        {"search":{"groups":[
            {"sel":{"who":"self","zone":"deck","kind":"follower"},"n":1_u64},
            {"sel":{"who":"self","zone":"deck","kind":"spell"},"upto":1_u64}
        ],"distinct_by":"name","to":{"order":"printed","partition":[{"zone":"hand","n":1_u64},{"zone":"cemetery","n":1_u64}]}}},
        {"search":{"sel":{"who":"self","zone":"deck","name":"コスモスファング"},"n":1_u64,"to":"ex","result":"searched"}},
        {"apply":{"on":{"follow":"searched"},"effect":{"cost_mod":{"when_playing":{"in":"affected"},"delta":-2_i64}},"until":"turn"}},
        {"look":{"top":{"who":"self","n":3_u64},"bind":"seen"}},
        {"move":{"ref":{"in":"seen"},"to":"ex","result":"moved"}},
        {"move":{"ref":{"in":"seen","except":{"in":{"objects_of":"moved","view":"before"}}},"to":"deck","position":"bottom","order":"chooser"}}
    ]);
    let lowerer = ExpandedLowerer::new().unwrap();
    let cards = lowerer.lower(&tree).unwrap();
    let body = cards["BP03-002"].card().abilities[0]
        .node
        .body
        .as_ref()
        .unwrap();
    let Node::Instruction { instruction: first } = &body.steps[0].node else {
        panic!("search");
    };
    let Action::Search {
        groups: Some(groups),
        destination: Some(SearchDestination::Partition { destinations }),
        ..
    } = &first.action
    else {
        panic!("partition");
    };
    assert_eq!(
        groups.cardinality,
        Cardinality {
            minimum: Nat(1),
            maximum: Some(Nat(2))
        }
    );
    assert_eq!(destinations.len(), 2);
    let Node::Instruction {
        instruction: searched,
    } = &body.steps[1].node
    else {
        panic!("search");
    };
    let Node::Instruction {
        instruction: installed,
    } = &body.steps[2].node
    else {
        panic!("installation");
    };
    assert!(
        matches!(&installed.action, Action::InstallEffect { subjects: InstallationSubjects::Objects { objects: Selector::MoveAfter { receipt: MovementReceipt::Search { receipt }, .. }, .. }, .. } if *receipt == ReceiptReference::new(searched.receipt))
    );
    let Node::Instruction {
        instruction: ordered,
    } = &body.steps[5].node
    else {
        panic!("order");
    };
    assert!(
        matches!(&ordered.action, Action::Order { cards: Selector::Difference { except, .. }, .. } if matches!(except.as_ref(), Selector::ReceiptMembers { receipt: ObjectReceipt::Movement { receipt: MovementReceipt::Move { .. } }, projection: MemberProjection::Before }))
    );
    tree["cards"]["BP03-002"]["abilities"][0]["fanfare"]["do"][1]["search"]["to"] = json!("hand");
    assert_eq!(
        lowerer.lower(&tree).unwrap_err().code,
        "unsupported_construct"
    );
}

#[test]
fn activated_payment_order_and_trigger_limit_remain_separate() {
    let mut tree = expanded::tree().clone();
    tree["cards"]["BP08-067"]["abilities"][0]["auto"]["per_turn"] = json!(1_u64);
    tree["cards"]["BP01-041"]["abilities"][0]["activate"]["cost"]["steps"]
        .as_array_mut()
        .unwrap()
        .push(json!({"self_to_cemetery":true}));
    let lowerer = ExpandedLowerer::new().unwrap();
    let cards = lowerer.lower(&tree).unwrap();
    let ability = &cards["BP01-041"].card().abilities[0].node;
    assert_eq!(ability.kind, AbilityKind::Activated);
    assert!(matches!(
        ability.source.origin,
        AbilityOrigin::Printed { .. }
    ));
    assert!(ability.trigger.is_none());
    let CostSpec::Ordered { steps } = &ability.play.as_ref().unwrap().costs else {
        panic!("ordered payment");
    };
    assert!(matches!(
        steps[0].instruction.action,
        Action::Resource {
            resource: Resource::PpCurrent,
            operation: ResourceOperation::Pay,
            amount: Expr::Literal(Int(3)),
            ..
        }
    ));
    assert!(matches!(
        steps[1].instruction.action,
        Action::Posture {
            posture: Posture::Acted,
            ..
        }
    ));
    assert!(matches!(&steps[2].instruction.action, Action::Move {
        objects: Selector::Objects { objects },
        movement: MoveSpec { to: MoveDestination::Owners { zone: OwnerZone::Cemetery }, position: None, order: None, initial_posture: None, entry_effects },
        reason: MoveReason::Put,
    } if objects == &[ObjectOperand::SourceHolder {}] && entry_effects.is_empty()));
    assert_eq!(
        cards["BP08-067"].card().abilities[0]
            .node
            .trigger
            .as_ref()
            .unwrap()
            .limit,
        Some(LimitSpec::once_per_turn())
    );
    tree["cards"]["BP01-041"]["abilities"][0]["activate"]["per_turn"] = json!(1_u64);
    assert_eq!(
        lowerer.lower(&tree).unwrap_err().code,
        "unsupported_construct"
    );
}

#[test]
fn printed_spell_cost_targets_distribution_and_choice_preserve_play_scopes() {
    let lowerer = ExpandedLowerer::new().unwrap();
    let mut tree = expanded::spell_tree().clone();
    let costs = BTreeMap::from([("SD03-005".to_owned(), Nat(1))]);
    assert_eq!(
        lowerer.lower(&tree).unwrap_err().code,
        "unsupported_construct"
    );
    let initial_cards = lowerer.lower_with_printed_costs(&tree, &costs).unwrap();
    let initial_ability = &initial_cards["SD03-005"].card().abilities[0].node;
    assert_eq!(initial_ability.kind, AbilityKind::Spell);
    assert_eq!(initial_ability.active_zones, vec![ZoneKind::Resolution]);
    assert!(matches!(
        initial_ability.source.origin,
        AbilityOrigin::Printed { .. }
    ));
    let body = "/cards/SD03-005/abilities/0/spell";
    *tree.pointer_mut(body).unwrap() = json!({
        "targets":[{"bind":"t","from":{"who":"opp","zone":"field"},"upto":2_u64}],
        "do":[{"damage":{"to":"t","n":3_u64,"distribute":true}}]
    });
    let cards = lowerer.lower_with_printed_costs(&tree, &costs).unwrap();
    let ability = &cards["SD03-005"].card().abilities[0].node;
    assert_play_distribution(ability);
    tree.pointer_mut(body).unwrap()["targets"][0]["from"]["kind"] = json!("follower");
    assert_eq!(
        lowerer
            .lower_with_printed_costs(&tree, &costs)
            .unwrap_err()
            .code,
        "unsupported_construct"
    );
    *tree.pointer_mut(body).unwrap() = json!({"targets":[{"bind":"mode_t","from":{"who":"opp","zone":"field"},"n":1_u64,"option":1_u64}], "do":[{"choice":{"n":1_u64,"options":[[{"damage":{"to":"mode_t","n":2_u64}}],[{"draw":{"n":1_u64}}]]}}]});
    let choices = lowerer.lower_with_printed_costs(&tree, &costs).unwrap();
    let program = choices["SD03-005"].card().abilities[0]
        .node
        .body
        .as_ref()
        .unwrap();
    let Node::Control {
        control:
            Control::Mode {
                modes,
                chosen,
                choice_kind,
                ..
            },
    } = &program.steps[0].node
    else {
        panic!("choice keyword");
    };
    assert_eq!(*choice_kind, ModeChoiceKind::ChoiceKeyword);
    assert!(matches!(
        chosen.kind,
        ModeDecisionKind::PlayParameter { .. }
    ));
    let ChoiceDomain::Objects { binding, .. } = modes[0].parameters[0].domain else {
        panic!("mode target");
    };
    assert_eq!(binding.key().0, modes[0].body.scope);
    assert_eq!(modes[1].parameters, []);
    assert_ne!(modes[0].body.scope, modes[1].body.scope);
    assert_ne!(modes[0].body.scope, program.scope);
    assert!(
        modes[0].body.steps[0]
            .source
            .author_path
            .ends_with("/spell/do/0/choice/options/0/0")
    );
    tree.pointer_mut(body).unwrap()["targets"][0]["option"] = json!(3_u64);
    assert_eq!(
        lowerer
            .lower_with_printed_costs(&tree, &costs)
            .unwrap_err()
            .code,
        "unsupported_construct"
    );
    tree.pointer_mut(body).unwrap()["targets"][0]["option"] = json!(1_u64);
    tree.pointer_mut(body).unwrap()["do"][0]["choice"]["exclude_chosen"] = json!("this_turn");
    assert_eq!(
        lowerer
            .lower_with_printed_costs(&tree, &costs)
            .unwrap_err()
            .code,
        "unsupported_construct"
    );
}

fn assert_play_distribution(ability: &Ability) {
    let parameters = &ability.play.as_ref().unwrap().parameters;
    assert_eq!(parameters.len(), 2);
    let ChoiceDomain::Objects {
        binding: target,
        constraint,
        ..
    } = &parameters[0].domain
    else {
        panic!("targets");
    };
    assert_eq!(
        constraint.cardinality,
        Cardinality {
            minimum: Nat(0),
            maximum: Some(Nat(2))
        }
    );
    let ChoiceDomain::Distribution {
        recipients,
        total,
        minimum,
        binding,
    } = &parameters[1].domain
    else {
        panic!("distribution");
    };
    assert_eq!(
        recipients,
        &RecipientSet::Objects {
            objects: Selector::Bound { reference: *target }
        }
    );
    assert_eq!(total, &Expr::Literal(Nat(3)));
    assert_eq!(minimum, &Nat(1));
    let Node::Instruction { instruction } = &ability.body.as_ref().unwrap().steps[0].node else {
        panic!("damage");
    };
    assert!(
        matches!(instruction.action, Action::Damage { amount: DamageAmount::Allocation { binding: actual }, .. } if actual == *binding)
    );
}
