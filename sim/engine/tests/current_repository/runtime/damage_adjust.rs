use super::*;

const fn damage(recipients: RecipientSet, amount: i64) -> Action {
    Action::Damage {
        recipients,
        amount: DamageAmount::Uniform {
            amount: Expr::Literal(Int(amount)),
        },
        flags: DamageFlags::Ability,
        source: ObjectView::SourceInformation {},
        batch: BatchPolicy::Instruction {},
    }
}

fn allies() -> RecipientSet {
    let mut objects = subjects();
    if let Selector::Zone {
        players,
        binding,
        predicate,
        ..
    } = &mut objects
    {
        players.relation = PlayerRelation::SelfPlayer;
        players.cardinality = Cardinality {
            minimum: Nat(1),
            maximum: Some(Nat(1)),
        };
        **predicate = Expr::Compute(Box::new(Predicate::KindEquals {
            left: Expr::Live(Box::new(KindRead::Kind {
                object: ObjectView::Current {
                    object: Box::new(ObjectOperand::Bound { binding: *binding }),
                },
                view: CharacteristicView::Effective,
            })),
            right: Expr::Literal(CardKind::Follower),
        }));
    }
    RecipientSet::Objects {
        objects: Selector::Difference {
            input: Box::new(objects),
            except: Box::new(Selector::Objects {
                objects: vec![ObjectOperand::SourceHolder {}],
            }),
        },
    }
}

fn position(number: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"subject","card":number}]);
    initial
}

fn actual(result: &Activation) -> Vec<u64> {
    result
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
        .collect()
}

#[test]
fn bp01_134_reduces_each_damage_event_and_zero_damage_has_no_fact() {
    for (amount, expected) in [(1, vec![]), (3, vec![2])] {
        let mut game = with_producer(&position("BP01-134"), damage(allies(), amount));
        let result = resolved(
            game.activate_internal_card("producer", AbilityId(1), PlayerId { seat: 1 })
                .unwrap(),
        );
        assert_eq!(
            actual(&result),
            expected,
            "the live printed reduction determines each positive damage fact"
        );
    }
    let mut initial = position("BP01-134");
    initial["players"]["P1"]["zones"]["field"][0]["state"] = json!({"silenced":true});
    let mut game = with_producer(&initial, damage(allies(), 3));
    let result = resolved(
        game.activate_internal_card("producer", AbilityId(1), PlayerId { seat: 1 })
            .unwrap(),
    );
    assert_eq!(
        actual(&result),
        vec![3],
        "the live printed reduction determines each positive damage fact"
    );
}

#[test]
fn bp08_114_does_not_reduce_ability_damage_but_reduces_combat_by_two() {
    let mut ability_game = with_producer(&position("BP08-114"), damage(allies(), 3));
    let ability_result = resolved(
        ability_game
            .activate_internal_card("producer", AbilityId(1), PlayerId { seat: 1 })
            .unwrap(),
    );
    assert_eq!(
        actual(&ability_result),
        vec![3],
        "the printed reduction applies only to combat damage"
    );
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"attacker","card":"BP01-037"}]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"subject","card":"BP08-114","state":{"acted":true}}]);
    let mut game = game(&initial);
    game.internal_rule_action(&json!({"do":"attack","attacker":"attacker","target":"subject"}))
        .unwrap();
    let q = game.internal_quick_choice().unwrap().unwrap();
    let CardPlay::Continued(result) = game
        .answer_internal_card(q.actor, q.token, Answer::Quick(None))
        .unwrap()
    else {
        panic!("combat")
    };
    assert!(
        result.events.iter().any(|event| event["kind"] == "ダメージ"
            && event["target"] == "subject"
            && event["amount"] == 3_u8),
        "the printed reduction applies only to combat damage"
    );
}

#[test]
fn bp10_092_all_printings_cap_only_damage_at_or_above_four() {
    for number in ["BP10-092", "BP10-SL22", "BP10-U06"] {
        for (amount, expected) in [(3, 3), (4, 3), (9, 3)] {
            let mut game = with_producer(&position(number), damage(allies(), amount));
            let result = resolved(
                game.activate_internal_card("producer", AbilityId(1), PlayerId { seat: 1 })
                    .unwrap(),
            );
            assert_eq!(
                actual(&result),
                vec![expected],
                "only damage at or above four is replaced by three"
            );
        }
    }
}

#[test]
fn recipient_orders_reduction_and_cap_before_simultaneous_damage_commits() {
    for cap_first in [false, true] {
        let mut initial = position("BP01-134");
        initial["players"]["P1"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"provider","card":"BP10-092"}));
        let mut game = with_producer(&initial, damage(allies(), 4));
        let CardPlay::Awaiting(q) = game
            .activate_internal_card("producer", AbilityId(1), PlayerId { seat: 1 })
            .unwrap()
        else {
            panic!("replacement question")
        };
        assert_eq!(
            q.actor,
            PlayerId { seat: 1 },
            "the recipient orders real replacements before any simultaneous hit commits"
        );
        let InputDomain::DamageReplacements(options) = &q.domain else {
            panic!("replacement domain")
        };
        assert_eq!(
            options.len(),
            2,
            "the recipient orders real replacements before any simultaneous hit commits"
        );
        let cap = options
            .iter()
            .find(|option| option.card == "BP10-092")
            .unwrap()
            .handle;
        let reduction = options
            .iter()
            .find(|option| option.card == "BP01-134")
            .unwrap()
            .handle;
        let before = game.projection(View::Referee).unwrap();
        game.answer_internal_card(
            PlayerId { seat: 2 },
            q.token,
            Answer::Order(vec![cap, reduction]),
        )
        .unwrap_err();
        game.answer_internal_card(q.actor, q.token, Answer::Order(vec![cap, cap]))
            .unwrap_err();
        assert_eq!(
            game.projection(View::Referee).unwrap(),
            before,
            "the recipient orders real replacements before any simultaneous hit commits"
        );
        let order = if cap_first {
            vec![cap, reduction]
        } else {
            vec![reduction, cap]
        };
        let result = resolved(
            game.answer_internal_card(q.actor, q.token, Answer::Order(order))
                .unwrap(),
        );
        let amounts = actual(&result);
        assert_eq!(
            amounts,
            vec![if cap_first { 2 } else { 3 }, 3],
            "the recipient orders real replacements before any simultaneous hit commits"
        );
    }
}

fn different_cap_selection() -> &'static SelectedEffects {
    static SELECTED: OnceLock<SelectedEffects> = OnceLock::new();
    SELECTED.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = producer_inputs();
        snapshot = snapshot.lines().map(|line| {
            let mut card: Value = serde_json::from_str(line).unwrap();
            if card["number"] == "producer" { card["faces"][0]["card_type"] = json!("アミュレット・トークン"); card["faces"][0]["power"] = json!("-"); card["faces"][0]["hp"] = json!("-"); }
            let mut encoded = card.to_string();
            encoded.push('\n');
            encoded
        }).collect();
        let mut face = snapshot.lines().map(|line|serde_json::from_str::<Value>(line).unwrap()).find(|card|card["number"]=="BP01-037").unwrap()["faces"][0].clone();
        face["name"]=json!("Synthetic cap provider");
        face["text"]=json!("Synthetic damage cap.");
        face["sections"]=json!([]);
        snapshot.push_str(&json!({"number":"cap-provider","faces":[face]}).to_string());
        snapshot.push('\n');
        input["documents"].as_array_mut().unwrap().push(json!(["cap-provider.yaml","format: 1\nkind: effect_set\ncards:\n  cap-provider:\n    status: complete\n    abilities: []\n"]));
        repository::select(&snapshot,&keywords,&input).unwrap()
    })
}

fn modifier_program(rewrite: EventRewrite) -> WellFormedCardProgram {
    let template = producer(damage(allies(), 1));
    let mut span = template.card().abilities[0].source.clone();
    span.card_id = "cap-provider".into();
    span.card_no = "cap-provider".into();
    let mut ability = template.card().abilities[0].node.clone();
    ability.kind = AbilityKind::Static;
    ability.play = None;
    ability.body = None;
    let event = Binding::new(1, ScopeId(9));
    ability.declarations = vec![Effect::Replacement {
        id: EffectDefinitionId(1),
        source: ability.source.clone(),
        pattern: ProposedEventPattern::Damage {
            recipient: Some(RecipientPredicate {
                binding: event,
                condition: Expr::Compute(Box::new(Predicate::RecipientIsFollower {
                    recipient: DamageRecipient::Bound { binding: event },
                    objects: Selector::Objects {
                        objects: vec![ObjectOperand::SourceHolder {}],
                    },
                })),
            }),
            source: None,
            kind: DamageKind::Any,
        },
        condition: Expr::Literal(Bool(true)),
        rewrite,
        scope: ReplacementScope::General,
        affected_player: PlayerRef::ProposedDamageRecipient {},
        once_per_event: OncePerEvent::Enforced,
        usage: None,
        prospective_chars: None,
        duration: Duration {
            end: DurationEnd::NoTemporalExpiry {},
            anchor: TimeAnchor::Installation {},
            usage: None,
            intervals: vec![],
        },
    }];
    WellFormedCardProgram::check(CardProgram {
        abilities: vec![Located {
            source: span,
            node: ability,
        }],
        card_play_costs: vec![],
    })
    .unwrap()
}

#[test]
fn different_damage_caps_require_the_affected_players_order() {
    for small_first in [false, true] {
        let mut initial = position("cap-provider");
        initial["players"]["P1"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"provider","card":"BP10-092"}));
        let mut game =
            with_selected_producer(&initial, damage(allies(), 6), different_cap_selection());
        game.load_internal_card(
            "cap-provider",
            &modifier_program(EventRewrite::CapDamage {
                threshold: Expr::Literal(Nat(5)),
                replacement: Expr::Literal(Nat(1)),
            }),
        )
        .unwrap();
        let CardPlay::Awaiting(q) = game
            .activate_internal_card("producer", AbilityId(1), PlayerId { seat: 1 })
            .unwrap()
        else {
            panic!("different caps require an order")
        };
        let InputDomain::DamageReplacements(options) = &q.domain else {
            panic!("cap order")
        };
        let small = options
            .iter()
            .find(|option| option.card == "cap-provider")
            .unwrap()
            .handle;
        let other = options
            .iter()
            .find(|option| option.card == "BP10-092")
            .unwrap()
            .handle;
        let order = if small_first {
            vec![small, other]
        } else {
            vec![other, small]
        };
        let result = resolved(
            game.answer_internal_card(q.actor, q.token, Answer::Order(order))
                .unwrap(),
        );
        assert_eq!(
            actual(&result),
            vec![if small_first { 1 } else { 3 }, 3],
            "different damage caps preserve the affected players ordering choice"
        );
    }
}

#[test]
fn combat_reduction_still_applies_after_a_typed_life_buff() {
    let mut initial = position("BP01-134");
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-037","state":{"acted":true}}]);
    let RecipientSet::Objects { objects } = allies() else {
        panic!("object recipients")
    };
    let mut game = with_producer(
        &initial,
        Action::Modify {
            subjects: objects,
            patch: CharacteristicPatch::Numeric {
                stat: Stat::Life,
                change: NumericChange::Add {
                    amount: Expr::Literal(Int(10)),
                },
            },
            expires: Duration {
                end: DurationEnd::NoTemporalExpiry {},
                anchor: TimeAnchor::Installation {},
                usage: None,
                intervals: vec![],
            },
            source: EffectReference::CurrentInstallation {},
        },
    );
    resolved(
        game.activate_internal_card("producer", AbilityId(1), PlayerId { seat: 1 })
            .unwrap(),
    );
    game.resume_internal_card().unwrap();
    game.internal_rule_action(&json!({"do":"attack","attacker":"subject","target":"enemy"}))
        .unwrap();
    let q = game.internal_quick_choice().unwrap().unwrap();
    let CardPlay::Continued(result) = game
        .answer_internal_card(q.actor, q.token, Answer::Quick(None))
        .unwrap()
    else {
        panic!("buffed combat")
    };
    assert!(
        result.events.iter().any(|event| event["kind"] == "ダメージ"
            && event["target"] == "subject"
            && event["amount"] == 4_u8),
        "a typed numeric layer preserves the checked combat reduction"
    );
}

#[test]
fn simultaneous_replacement_questions_start_with_the_active_player() {
    for active in ["P1", "P2"] {
        let mut initial = draw_position();
        initial["turn"]["active"] = json!(active);
        for seat in ["P1", "P2"] {
            initial["players"][seat]["zones"]["field"] = json!([
                {"id":format!("{seat}-church"),"card":"BP01-134"},
                {"id":format!("{seat}-sofia"),"card":"BP10-092"}
            ]);
        }
        let mut recipients = allies();
        if let RecipientSet::Objects {
            objects: Selector::Difference { input, .. },
        } = &mut recipients
            && let Selector::Zone { players, .. } = input.as_mut()
        {
            players.relation = PlayerRelation::All;
            players.cardinality = Cardinality {
                minimum: Nat(2),
                maximum: Some(Nat(2)),
            };
        }
        let mut game = with_producer(&initial, damage(recipients, 4));
        let before = game.projection(View::Referee).unwrap();
        let CardPlay::Awaiting(first) = game
            .activate_internal_card(
                "producer",
                AbilityId(1),
                PlayerId {
                    seat: if active == "P1" { 1 } else { 2 },
                },
            )
            .unwrap()
        else {
            panic!("first player order")
        };
        assert_eq!(
            first.actor,
            PlayerId {
                seat: if active == "P1" { 1 } else { 2 }
            },
            "the active player decides first for simultaneous damage"
        );
        let InputDomain::DamageReplacements(options) = &first.domain else {
            panic!("replacement options")
        };
        let CardPlay::Awaiting(second) = game
            .answer_internal_card(
                first.actor,
                first.token,
                Answer::Order(options.iter().map(|option| option.handle).collect()),
            )
            .unwrap()
        else {
            panic!("second player order")
        };
        assert_ne!(
            first.actor, second.actor,
            "the other recipient makes its own decision"
        );
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"],
            before["objects"],
            "neither player's damage commits before both decisions"
        );
        let InputDomain::DamageReplacements(second_options) = &second.domain else {
            panic!("replacement options")
        };
        let result = resolved(
            game.answer_internal_card(
                second.actor,
                second.token,
                Answer::Order(second_options.iter().map(|option| option.handle).collect()),
            )
            .unwrap(),
        );
        assert_eq!(
            actual(&result).len(),
            4,
            "the two choices commit one simultaneous damage batch"
        );
    }
}

#[test]
fn pure_prevention_needs_no_order_after_its_token_provider_disappears() {
    let mut initial = position("BP01-134");
    initial["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"producer","card":"producer"}));
    let mut game = Game::from_selected_effects(
        different_cap_selection().clone(),
        &initial,
        &Value::Null,
        &Value::Null,
        "erased-prevention-provider",
    )
    .unwrap();
    let target = damage_prevent::field_name("教会の護り手");
    let program = producer_steps(vec![
        damage_prevent::recipient_prevention(target),
        Action::Destroy {
            objects: Selector::Objects {
                objects: vec![ObjectOperand::SourceHolder {}],
            },
        },
        damage(
            RecipientSet::Objects {
                objects: damage_prevent::named_field("教会の護り手", 51),
            },
            3,
        ),
    ]);
    game.load_internal_card("producer", &program).unwrap();
    let result = resolved(
        game.activate_internal_card("producer", AbilityId(1), PlayerId { seat: 1 })
            .unwrap(),
    );
    assert!(
        game.projection(View::Referee).unwrap()["objects"]
            .get("producer")
            .is_none(),
        "the synthetic Token provider has disappeared"
    );
    assert!(
        actual(&result).is_empty(),
        "the surviving installed shield prevents all damage without an irrelevant order choice"
    );
}

#[test]
fn signed_additive_replacements_change_actual_damage_in_both_directions() {
    for (delta, expected) in [(-1_i64, vec![]), (1, vec![2])] {
        let mut game = with_selected_producer(
            &position("cap-provider"),
            damage(allies(), 1),
            different_cap_selection(),
        );
        game.load_internal_card(
            "cap-provider",
            &modifier_program(EventRewrite::AddDamage {
                amount: Expr::Literal(Int(delta)),
            }),
        )
        .unwrap();
        let result = resolved(
            game.activate_internal_card("producer", AbilityId(1), PlayerId { seat: 1 })
                .unwrap(),
        );
        assert_eq!(
            actual(&result),
            expected,
            "a checked signed modifier changes the public positive damage facts"
        );
    }
}

#[test]
fn printed_drain_recovers_the_damage_after_the_targets_reduction() {
    let mut initial = draw_position();
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"attacker","card":"BP13-T05"}]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"target","card":"BP01-134","state":{"acted":true}}]);
    let mut game = game(&initial);
    game.internal_rule_action(&json!({"do":"attack","attacker":"attacker","target":"target"}))
        .unwrap();
    let q = game.internal_quick_choice().unwrap().unwrap();
    game.answer_internal_card(q.actor, q.token, Answer::Quick(None))
        .unwrap();
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("printed Drain after combat")
    };
    assert_eq!(
        options.len(),
        1,
        "the modified hit queues one Drain occurrence"
    );
    assert_eq!(options[0].card, "BP13-T05", "the real token supplies Drain");
    resolved(
        game.answer_internal_card(
            pending.actor,
            pending.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap(),
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        12_u8,
        "three attack damage reduced by one recovers only two life"
    );
}
