use super::*;
use core::mem::swap;

fn input(effect: &Value) -> Value {
    equipment_played::tree(&json!({"static":{"effects":[effect]}}), "unit-follower")
}
fn lower_and_load(effect: &Value) {
    let programs = lowerer().lower(&input(effect)).unwrap();
    let mut game = Game::new(
        catalog(),
        &setup(1, json!([])),
        &Value::Null,
        &Value::Null,
        "prevention-controller-preflight",
    )
    .unwrap();
    game.load_internal_card("unit-follower", &programs["unit-follower"])
        .unwrap();
}

fn amulet(mut abilities: Vec<Located<Ability>>) -> WellFormedCardProgram {
    for ability in &mut abilities {
        ability.source.card_id = "unit-amulet".into();
        ability.source.card_no = "unit-amulet".into();
        if let Some(body) = &mut ability.node.body {
            locate_steps(&mut body.steps, &ability.source, &ability.node.source);
        }
    }
    card(abilities)
}

#[test]
fn ordinary_self_ability_damage_prevention_is_a_valid_lower_load_baseline() {
    lower_and_load(&json!({"prevent":{"kind":"ability"}}));
}
#[test]
fn ordinary_own_followers_ability_damage_reduction_is_a_valid_lower_load_baseline() {
    lower_and_load(
        &json!({"damage_mod":{"on":{"who":"self","zone":"field","kind":"follower"},"kind":"ability","delta":-1}}),
    );
}
#[test]
fn existing_field_source_filter_is_a_valid_but_narrower_lower_load_baseline() {
    lower_and_load(&json!({"prevent":{"kind":"ability","source":{"who":"opp","zone":"field"}}}));
}
#[test]
fn opponent_only_ability_damage_prevention_requires_a_controller_only_source_query() {
    lower_and_load(&json!({"prevent":{"kind":"ability","source":{"who":"opp"}}}));
}
#[test]
fn opponent_only_ability_damage_reduction_requires_a_controller_only_source_query() {
    lower_and_load(
        &json!({"damage_mod":{"on":{"who":"self","zone":"field","kind":"follower"},"kind":"ability","source":{"who":"opp"},"delta":-1}}),
    );
}

fn controller_program() -> WellFormedCardProgram {
    lowerer()
        .lower(&input(
            &json!({"prevent":{"kind":"ability","source":{"who":"opp"}}}),
        ))
        .unwrap()
        .remove("unit-follower")
        .unwrap()
}

#[test]
fn controller_only_source_keeps_other_author_shapes_closed() {
    for shape in [
        json!({"prevent":{"kind":"ability","source":{"who":"self"}}}),
        json!({"prevent":{"kind":"combat","source":{"who":"opp"}}}),
        json!({"prevent":{"kind":"any","source":{"who":"opp"}}}),
        json!({"prevent":{"kind":"ability","source":{"who":"opp","kind":"follower"}}}),
        json!({"prevent":{"kind":"ability","on":{"leader":"self"},"source":{"who":"opp"}}}),
    ] {
        lowerer().lower(&input(&shape)).unwrap_err();
    }
    let mut equipment = input(&json!({"prevent":{"kind":"ability","source":{"who":"opp"}}}));
    equipment["cards"]["unit-follower"]["abilities"][0]["static"]["zone"] = json!("equipment");
    lowerer().lower(&equipment).unwrap_err();
    let applied = equipment_played::tree(
        &json!({"activate":{"cost":{"pp":0},"do":[{"apply":{"on":"this","effect":{"prevent":{"kind":"ability","source":{"who":"opp"}}},"until":"turn"}}]}}),
        "unit-follower",
    );
    lowerer().lower(&applied).unwrap_err();
}

#[test]
fn raw_source_predicates_cannot_reverse_or_replace_the_canonical_players() {
    for reverse in [false, true] {
        let mut raw = controller_program().card().clone();
        let Effect::Replacement {
            pattern:
                ProposedEventPattern::Damage {
                    source: Some(source),
                    ..
                },
            ..
        } = &mut raw.abilities[0].node.declarations[0]
        else {
            panic!("expected the tested IR shape")
        };
        let Expr::Compute(predicate) = &mut source.condition else {
            panic!("expected the tested IR shape")
        };
        let Predicate::PlayerEquals { left, right } = predicate.as_mut() else {
            panic!("expected the tested IR shape")
        };
        if reverse {
            swap(left, right);
        } else {
            *right = Expr::Literal(PlayerRef::Controller {});
        }
        WellFormedCardProgram::check(raw).unwrap_err();
    }
}

#[test]
fn contextual_damage_controller_is_rejected_in_ordinary_values_and_conditions() {
    let contextual = PlayerRef::ProposedDamageController {};
    let comparison = Expr::Compute(Box::new(Predicate::PlayerEquals {
        left: Expr::Literal(contextual.clone()),
        right: Expr::Literal(PlayerRef::Controller {}),
    }));
    for action in [
        Action::Draw {
            player: contextual.clone(),
            count: Expr::Literal(Int(1)),
            mode: StepwiseMode::Exact,
        },
        Action::Life {
            player: contextual,
            change: NumericChange::Add {
                amount: Expr::Literal(Int(1)),
            },
        },
    ] {
        WellFormedProgram::check(body(1, vec![instruction(1, action)])).unwrap_err();
    }
    let mut step = instruction(
        1,
        Action::Draw {
            player: PlayerRef::Controller {},
            count: Expr::Literal(Int(0)),
            mode: StepwiseMode::Exact,
        },
    );
    let Node::Instruction { instruction } = &mut step.node else {
        panic!("expected the tested IR shape")
    };
    instruction.precondition = Some(Precondition {
        kind: PreconditionKind::CardTextPrerequisite,
        predicate: comparison.clone(),
        protects: InstructionId(1),
        reason: PreconditionReason::CardText {},
    });
    WellFormedProgram::check(body(1, vec![step])).unwrap_err();
    let mut raw = controller_program().card().clone();
    let Effect::Replacement { condition, .. } = &mut raw.abilities[0].node.declarations[0] else {
        panic!("expected the tested IR shape")
    };
    *condition = comparison;
    WellFormedCardProgram::check(raw).unwrap_err();
}

#[test]
fn native_damage_uses_the_ability_controller_even_when_the_object_has_another_owner() {
    for actor in [1, 2] {
        for owner in ["P1", "P2"] {
            let mut position = setup(1, json!([]));
            position["players"]["P1"]["zones"]["hand"][0]["owner"] = json!(owner);
            if actor == 2 {
                position["players"]["P2"]["zones"]["hand"] =
                    position["players"]["P1"]["zones"]["hand"].take();
            }
            let mut board = Game::new(
                catalog(),
                &position,
                &Value::Null,
                &Value::Null,
                "controller-owner",
            )
            .unwrap();
            board
                .load_internal_card("unit-follower", &controller_program())
                .unwrap();
            board
                .load_internal_card("unit-spell", &card(vec![]))
                .unwrap();
            let result = board
                .execute_internal(
                    &checked(vec![damage(
                        1,
                        RecipientSet::Objects {
                            objects: zone(ZoneKind::Field, 99),
                        },
                        2,
                    )]),
                    &source("synthetic"),
                    context(actor),
                )
                .unwrap();
            assert_eq!(
                result
                    .facts
                    .iter()
                    .filter(|f| matches!(f, Fact::Damage(_)))
                    .count(),
                usize::from(actor == 2)
            );
            assert_eq!(
                board.projection(View::Referee).unwrap()["objects"]["f"]["damage"],
                if actor == 2 { 2 } else { 0 }
            );
        }
    }
}

#[test]
fn a_departed_stolen_source_keeps_its_ability_controller_and_old_generation() {
    for (destination, zone_name) in [(OwnerZone::Cemetery, "cemetery"), (OwnerZone::Hand, "hand")] {
        let mut position = setup(1, json!([]));
        position["players"]["P1"]["zones"]["hand"] = json!([]);
        position["players"]["P1"]["zones"]["field"] =
            json!([{"id":"s","card":"unit-amulet","owner":"P2"}]);
        let mut board = Game::new(
            catalog(),
            &position,
            &Value::Null,
            &Value::Null,
            "departed-stolen-source",
        )
        .unwrap();
        board
            .load_internal_card("unit-follower", &controller_program())
            .unwrap();
        let departure = movement::put(
            1,
            Selector::Objects {
                objects: vec![ObjectOperand::SourceHolder {}],
            },
            destination,
        );
        let mut hit = damage(
            2,
            RecipientSet::Objects {
                objects: zone(ZoneKind::Field, 99),
            },
            2,
        );
        let Node::Instruction { instruction } = &mut hit.node else {
            panic!("expected the tested IR shape")
        };
        let Action::Damage {
            source: damage_source,
            ..
        } = &mut instruction.action
        else {
            panic!("expected the tested IR shape")
        };
        *damage_source = ObjectView::SourceInformation {};
        board
            .load_internal_card(
                "unit-amulet",
                &amulet(vec![ability(1, vec![departure, hit])]),
            )
            .unwrap();
        let result = activate(&mut board, 1).progress;
        assert!(!result.facts.iter().any(|f| matches!(f, Fact::Damage(_))));
        let state = board.projection(View::Referee).unwrap();
        assert!(
            state["P2"][zone_name]
                .as_array()
                .unwrap()
                .contains(&json!("s"))
        );
        assert_eq!(state["objects"]["f"]["damage"], 0);
    }
}

#[test]
fn a_stolen_sources_real_lastword_uses_its_pre_departure_ability_controller() {
    use sve_engine::game::internal::{Answer, InputDomain};
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"s","card":"unit-amulet","owner":"P2"}]);
    let destroy = instruction(
        1,
        Action::Destroy {
            objects: Selector::Objects {
                objects: vec![ObjectOperand::SourceHolder {}],
            },
        },
    );
    let mut hit = damage(
        2,
        RecipientSet::Objects {
            objects: zone(ZoneKind::Field, 99),
        },
        2,
    );
    let Node::Instruction { instruction } = &mut hit.node else {
        panic!("expected the tested IR shape")
    };
    let Action::Damage {
        source: damage_source,
        ..
    } = &mut instruction.action
    else {
        panic!("expected the tested IR shape")
    };
    *damage_source = ObjectView::SourceInformation {};
    let program = amulet(vec![
        ability(1, vec![destroy]),
        triggers::lastword(ability(2, vec![hit])),
    ]);
    let mut board = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "stolen-lastword-controller",
    )
    .unwrap();
    board.load_internal_card("unit-amulet", &program).unwrap();
    board
        .load_internal_card("unit-follower", &controller_program())
        .unwrap();
    activate(&mut board, 1);
    while board.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        board.resume_internal_card().unwrap();
    }
    assert_eq!(
        board.projection(View::Referee).unwrap()["P2"]["cemetery"],
        json!(["s"])
    );
    let question = board.internal_pending_choice().unwrap().unwrap();
    assert_eq!(question.actor, PlayerId { seat: 1 });
    let InputDomain::Pending(options) = question.domain else {
        panic!("expected the tested IR shape")
    };
    assert_eq!(options.len(), 1);
    let result = resolved(
        board
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap(),
    );
    assert!(
        !result
            .progress
            .facts
            .iter()
            .any(|fact| matches!(fact, Fact::Damage(_)))
    );
    assert_eq!(
        board.projection(View::Referee).unwrap()["objects"]["f"]["damage"],
        0
    );
}

#[test]
fn native_hit_uses_the_instruction_source_controller_instead_of_the_object_role() {
    let mut board = Game::new(
        catalog(),
        &setup(1, json!([])),
        &Value::Null,
        &Value::Null,
        "instruction-source-controller",
    )
    .unwrap();
    board
        .load_internal_card("unit-spell", &card(vec![]))
        .unwrap();
    board
        .load_internal_card("unit-follower", &controller_program())
        .unwrap();
    let mut hit = damage(
        1,
        RecipientSet::Objects {
            objects: zone(ZoneKind::Field, 99),
        },
        2,
    );
    let Node::Instruction { instruction } = &mut hit.node else {
        panic!("expected the tested IR shape")
    };
    instruction.source.controller = PlayerRef::RequireOne {
        players: Box::new(PlayerSet {
            relation: PlayerRelation::Opponents,
            relative_to: PlayerRef::Controller {},
            cardinality: Cardinality {
                minimum: Nat(1),
                maximum: Some(Nat(1)),
            },
            text_quantifier: TextQuantifier::One,
        }),
    };
    let result = board
        .execute_internal(&checked(vec![hit]), &source("synthetic"), context(1))
        .unwrap();
    let hits = result
        .facts
        .iter()
        .filter_map(|fact| {
            if let Fact::Damage(hit) = fact {
                Some(hit)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].controller, PlayerId { seat: 2 });
    assert_eq!(
        board.projection(View::Referee).unwrap()["objects"]["f"]["damage"],
        2
    );
}

#[test]
fn raw_controller_prevention_cannot_target_leaders_or_install_as_an_applied_effect() {
    let original = controller_program().card().clone();
    let mut leader = original.clone();
    let Effect::Replacement {
        pattern:
            ProposedEventPattern::Damage {
                recipient: Some(recipient),
                ..
            },
        ..
    } = &mut leader.abilities[0].node.declarations[0]
    else {
        panic!("expected the tested IR shape")
    };
    recipient.condition = Expr::Compute(Box::new(Predicate::RecipientIsLeader {
        recipient: DamageRecipient::Bound {
            binding: recipient.binding,
        },
        players: players(PlayerRelation::SelfPlayer),
    }));
    WellFormedCardProgram::check(leader).unwrap_err();
    let mut installed=lowerer().lower(&equipment_played::tree(&json!({"activate":{"cost":{"pp":0},"do":[{"apply":{"on":"this","effect":{"prevent":{"kind":"ability"}},"until":"turn"}}]}}),"unit-follower")).unwrap().remove("unit-follower").unwrap().card().clone();
    let Node::Instruction { instruction } =
        &mut installed.abilities[0].node.body.as_mut().unwrap().steps[0].node
    else {
        panic!("expected the tested IR shape")
    };
    let Action::InstallEffect { effect, .. } = &mut instruction.action else {
        panic!("expected the tested IR shape")
    };
    let Effect::Replacement {
        pattern: ProposedEventPattern::Damage { source, .. },
        ..
    } = effect.as_mut()
    else {
        panic!("expected the tested IR shape")
    };
    let Effect::Replacement {
        pattern: ProposedEventPattern::Damage {
            source: canonical, ..
        },
        ..
    } = &original.abilities[0].node.declarations[0]
    else {
        panic!("expected the tested IR shape")
    };
    *source = canonical.clone();
    WellFormedCardProgram::check(installed).unwrap_err();
}
