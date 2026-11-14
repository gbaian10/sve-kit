use super::*;

fn ward() -> Located<Ability> {
    let mut located = ability(1, vec![]);
    located.node.kind = AbilityKind::Static;
    located.node.play = None;
    located.node.body = None;
    let holder = Selector::Objects {
        objects: vec![ObjectOperand::SourceHolder {}],
    };
    located.node.declarations = vec![Effect::Permission {
        subject: PermissionSubject::Objects {
            objects: holder.clone(),
        },
        action: ActionPattern::IgnoreWard { attacker: holder },
        polarity: Polarity::Allow,
        condition: Expr::Literal(Bool(true)),
        duration: Duration {
            end: DurationEnd::NoTemporalExpiry {},
            anchor: TimeAnchor::Installation {},
            usage: None,
            intervals: vec![],
        },
        rule: RuleId(128),
    }];
    located
}

#[test]
fn raw_ward_permissions_reject_noncanonical_shapes_before_loading() {
    let mut accepted = Game::new(
        catalog(),
        &setup(1, json!([])),
        &Value::Null,
        &Value::Null,
        "raw-ward-valid",
    )
    .unwrap();
    accepted
        .load_internal_card("unit-follower", &card(vec![ward()]))
        .unwrap();
    for variant in 0_u8..5 {
        let mut located = ward();
        let Effect::Permission {
            subject,
            action,
            polarity,
            duration,
            ..
        } = located.node.declarations.first_mut().unwrap()
        else {
            panic!("permission");
        };
        match variant {
            0 => *polarity = Polarity::Forbid,
            1 => {
                *subject = PermissionSubject::Objects {
                    objects: Selector::Objects { objects: vec![] },
                }
            }
            2 => {
                let duplicate = Selector::Objects {
                    objects: vec![
                        ObjectOperand::SourceHolder {},
                        ObjectOperand::SourceHolder {},
                    ],
                };
                *subject = PermissionSubject::Objects {
                    objects: duplicate.clone(),
                };
                *action = ActionPattern::IgnoreWard {
                    attacker: duplicate,
                };
            }
            3 => duration.end = DurationEnd::CurrentTurnEnd {},
            _ => located.node.active_zones = vec![ZoneKind::Hand],
        }
        let program = WellFormedCardProgram::check(CardProgram {
            abilities: vec![located],
            card_play_costs: vec![],
        });
        let mut game = Game::new(
            catalog(),
            &setup(1, json!([])),
            &Value::Null,
            &Value::Null,
            "raw-ward",
        )
        .unwrap();
        let before = format!("{game:?}");
        match program {
            Ok(program) => {
                game.load_internal_card("unit-follower", &program)
                    .unwrap_err();
            }
            Err(error) => assert_eq!(error.reason, WellFormedReason::UnsupportedShape),
        }
        assert!(
            format!("{game:?}") == before,
            "unsupported raw ward permission must not mutate the game"
        );
    }
}

#[test]
fn live_source_power_controls_ward_bypass_without_an_occurrence_snapshot() {
    let mut tree = equipment_played::tree(
        &json!({"static":{
            "while":{"cmp":{"left":{"power_of":"this"},"op":"ge","right":10}},
            "effects":[{"ignore_ward":{"on":"this"}}]
        }}),
        "unit-follower",
    );
    let mut ward = tree["cards"]["unit-follower"]["abilities"][0].clone();
    ward["static"] = json!({"effects":[{"grant":{"keywords":["ward"]}}]});
    ward["sources"][0]["ability_scope"] = json!("ward-provider");
    tree["cards"]["unit-follower"]["abilities"]
        .as_array_mut()
        .unwrap()
        .push(ward);
    let program = lowerer()
        .lower(&tree)
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    for power in [9_u8, 10_u8] {
        let mut initial = setup(1, json!([]));
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] = json!([{
            "id":"source","card":"unit-follower","state":{"power":power,"entered_this_turn":false,"acted":false}
        }]);
        initial["players"]["P2"]["zones"]["field"] = json!([{
            "id":"guard","card":"unit-follower","state":{"entered_this_turn":false,"acted":true}
        }]);
        let mut game = Game::new(
            static_keywords::keyword_catalog(),
            &initial,
            &Value::Null,
            &Value::Null,
            "live-source-ward",
        )
        .unwrap();
        game.load_internal_card("unit-follower", &program).unwrap();
        let result = game
            .internal_rule_action(&json!({"do":"attack","attacker":"source","target":"P2.leader"}))
            .unwrap();
        assert_eq!(
            result.outcome,
            if power == 10 {
                "resolved"
            } else {
                "cannot-attack"
            }
        );
        for _ in 0_u8..10_u8 {
            if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
                game.resume_internal_card().unwrap();
                continue;
            }
            assert!(game.internal_pending_choice().unwrap().is_none());
            let Some(q) = game.internal_quick_choice().unwrap() else {
                break;
            };
            game.answer_internal_card(q.actor, q.token, Answer::Quick(None))
                .unwrap();
        }
        assert_eq!(
            game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
            if power == 10 { 10_u8 } else { 20_u8 }
        );
    }
}
