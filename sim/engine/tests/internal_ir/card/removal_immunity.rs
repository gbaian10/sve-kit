use super::*;

fn restriction() -> Located<Ability> {
    let mut located = ability(2, vec![]);
    let ability = &mut located.node;
    ability.kind = AbilityKind::Static;
    ability.play = None;
    ability.body = None;
    let objects = Selector::Objects {
        objects: vec![ObjectOperand::SourceHolder {}],
    };
    ability.declarations = vec![Effect::Permission {
        subject: PermissionSubject::Objects {
            objects: objects.clone(),
        },
        action: ActionPattern::Destroy {
            objects,
            cause: CausePattern::Effect {
                source: EffectSource::Any {},
            },
        },
        polarity: Polarity::Forbid,
        condition: Expr::Literal(Bool(true)),
        duration: Duration {
            end: DurationEnd::NoTemporalExpiry {},
            anchor: TimeAnchor::Installation {},
            usage: None,
            intervals: vec![],
        },
        rule: RuleId(133),
    }];
    located
}

#[test]
fn printed_destruction_prohibition_returns_an_empty_success_set() {
    let action = Action::Destroy {
        objects: Selector::Objects {
            objects: vec![ObjectOperand::SourceHolder {}],
        },
    };
    let mut game = position_with_card(
        1,
        &card(vec![
            ability(1, vec![instruction(1, action)]),
            restriction(),
        ]),
    );
    let result = activate(&mut game, 1);
    assert!(result.progress.receipts.values().any(|outcome| matches!(outcome, InstructionOutcome::Destroy(outcome) if outcome.successful.is_empty())), "immunity records an attempted destruction without a successful member");
    game.resume_internal_card().unwrap();
    assert!(
        game.projection(View::Referee).unwrap()["P1"]["field"]
            .as_array()
            .unwrap()
            .contains(&json!("s")),
        "the protected holder stays on the field"
    );
}

#[test]
fn holder_immunity_rejects_unimplemented_permission_variants() {
    let base = restriction();
    let mut variants = Vec::new();
    let mut unrelated = base.clone();
    if let Effect::Permission {
        action: ActionPattern::Destroy { cause, .. },
        ..
    } = &mut unrelated.node.declarations[0]
    {
        *cause = CausePattern::Any {};
    }
    variants.push(unrelated);
    let mut conditional = base.clone();
    if let Effect::Permission { condition, .. } = &mut conditional.node.declarations[0] {
        *condition = Expr::Literal(Bool(false));
    }
    variants.push(conditional);
    let mut mismatched = base;
    if let Effect::Permission {
        subject: PermissionSubject::Objects { objects },
        ..
    } = &mut mismatched.node.declarations[0]
    {
        *objects = Selector::Objects { objects: vec![] };
    }
    variants.push(mismatched);
    for located in variants {
        let mut game = Game::new(
            catalog(),
            &setup(1, json!([])),
            &Value::Null,
            &Value::Null,
            "unsupported-immunity",
        )
        .unwrap();
        assert!(
            game.load_internal_card("unit-follower", &card(vec![located]))
                .is_err(),
            "only the exact printed holder permission is admitted"
        );
    }
}

fn lowering_tree(head: &Value) -> Value {
    let mut tree = fanfare::expanded::tree().clone();
    let mut entry = tree["cards"]["BP08-038"]["abilities"][0].clone();
    entry.as_object_mut().unwrap().remove("fanfare");
    entry
        .as_object_mut()
        .unwrap()
        .extend(head.as_object().unwrap().clone());
    let meta = tree["cards"]["BP08-038"]["meta"].clone();
    tree["cards"] = json!({"unit-follower":{"meta":meta,"abilities":[entry]}});
    tree
}

fn lowered(head: &Value) -> Option<WellFormedCardProgram> {
    lowerer()
        .lower(&lowering_tree(head))
        .ok()?
        .remove("unit-follower")
}

fn lowers(head: &Value) -> bool {
    lowered(head).is_some()
}

#[test]
fn removal_lowering_only_opens_the_holder_forms_printed_by_seed_cards() {
    for (shape, accepted) in [
        (json!({"what":"destroy","by":"ability"}), true),
        (json!({"what":"banish","on":"this"}), true),
        (json!({"what":"destroy"}), false),
        (json!({"what":"banish","by":"ability"}), false),
        (json!({"what":"destroy","by":"ability","on":"other"}), false),
    ] {
        assert_eq!(
            lowers(&json!({"static":{"effects":[{"cannot":shape}]}})),
            accepted
        );
    }
    assert!(
        !lowers(
            &json!({"static":{"while":{"my_turn":true},"effects":[{"cannot":{"what":"destroy","by":"ability"}}]}})
        ),
        "conditional holder removal immunity remains closed"
    );
    for (effect, accepted) in [
        (json!({"prevent":{"kind":"any"}}), true),
        (json!({"cannot":{"what":"destroy","by":"ability"}}), false),
    ] {
        assert_eq!(
            lowers(
                &json!({"activate":{"cost":{"pp":0},"do":[{"apply":{"on":"this","until":"turn","effect":effect}}]}})
            ),
            accepted
        );
    }
}

#[test]
fn mixed_immunity_keeps_one_printed_ability_and_rejects_numeric_prevention() {
    let defense = json!({"prevent":{"kind":"ability"}});
    let removal = json!({"cannot":{"what":"destroy","by":"ability"}});
    let guarded = json!({"if":{"cond":{"not":{"standing":"this"}},"then":[defense]}});
    for effects in [
        json!([defense, removal]),
        json!([guarded, removal]),
        json!([removal, defense]),
    ] {
        let program = lowered(&json!({"static":{"effects":effects}})).unwrap();
        assert_eq!(program.card().abilities.len(), 1);
        assert_eq!(program.card().abilities[0].node.declarations.len(), 2);
        let _game = position_with_card(1, &program);
        let mut private_zone = program.card().clone();
        private_zone.abilities[0].node.active_zones = vec![ZoneKind::Hand];
        assert!(
            WellFormedCardProgram::check(private_zone).is_err(),
            "mixed public-field declarations must reject private active zones before loading"
        );
        let mut changed = program.card().clone();
        let rewrite = changed.abilities[0]
            .node
            .declarations
            .iter_mut()
            .find_map(|effect| {
                if let Effect::Replacement { rewrite, .. } = effect {
                    Some(rewrite)
                } else {
                    None
                }
            })
            .unwrap();
        *rewrite = EventRewrite::AddDamage {
            amount: Expr::Literal(Int(-1)),
        };
        let mut game = Game::new(
            catalog(),
            &setup(1, json!([])),
            &Value::Null,
            &Value::Null,
            "mixed-numeric-rejected",
        )
        .unwrap();
        assert!(
            game.load_internal_card(
                "unit-follower",
                &WellFormedCardProgram::check(changed).unwrap()
            )
            .is_err(),
            "the mixed holder form only admits ability-damage prevention"
        );
    }
    for effects in [
        json!([guarded]),
        json!([defense, guarded]),
        json!([{"if":{"cond":{"my_turn":true},"then":[defense],"else":[defense]}},removal]),
        json!([{"if":{"cond":{"my_turn":true},"then":[defense,defense]}},removal]),
    ] {
        assert!(
            !lowers(&json!({"static":{"effects":effects}})),
            "conditional defense requires its one-branch mixed printed form"
        );
    }
    let foreign = lowered(&json!({"static":{"effects":[{"prevent":{"on":{"leader":"self"},"kind":"ability"}},removal]}})).unwrap();
    let mut game = Game::new(
        catalog(),
        &setup(1, json!([])),
        &Value::Null,
        &Value::Null,
        "foreign-prevention-rejected",
    )
    .unwrap();
    assert!(
        game.load_internal_card("unit-follower", &foreign).is_err(),
        "mixed immunity does not grant leader or other-object protection"
    );
}
