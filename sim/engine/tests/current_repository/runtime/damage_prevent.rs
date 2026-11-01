#![expect(
    clippy::panic,
    reason = "Unexpected questions invalidate these real-card scenarios."
)]
use super::*;
use sve_engine::game::internal::{InstructionOutcome, Progress};

fn rules_game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "damage-prevention",
    )
    .unwrap()
}

fn pass_quick(game: &mut Game) {
    for _ in 0_u8..4 {
        let Some(question) = game.internal_quick_choice().unwrap() else {
            return;
        };
        game.answer_internal_card(question.actor, question.token, Answer::Quick(None))
            .unwrap();
    }
    panic!("combat did not leave its Quick window");
}

#[test]
fn bp01_024_quick_curse_preserves_the_attack_but_prevents_its_damage() {
    for cast in [false, true] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"attacker","card":"BP01-037"}]);
        initial["players"]["P2"]["zones"]["hand"] = json!([{"id":"curse","card":"BP01-024"}]);
        let mut game = rules_game(&initial);
        game.internal_rule_action(
            &json!({"do":"attack","attacker":"attacker","target":"P2.leader"}),
        )
        .unwrap();
        let question = game.internal_quick_choice().unwrap().unwrap();
        assert_eq!(
            question.actor,
            PlayerId { seat: 2 },
            "the defender owns this Quick window"
        );
        let InputDomain::Quick { options, .. } = &question.domain else {
            panic!("Quick window");
        };
        assert_eq!(options.len(), 1);
        assert_eq!(options[0].card, "BP01-024");
        if cast {
            let CardPlay::Awaiting(targets) = game
                .answer_internal_card(
                    question.actor,
                    question.token,
                    Answer::Quick(Some(options[0].handle)),
                )
                .unwrap()
            else {
                panic!("curse target");
            };
            let InputDomain::Targets(domain) = &targets.domain else {
                panic!("targets");
            };
            assert_eq!(
                domain.members.len(),
                1,
                "only the opposing attacker qualifies"
            );
            resolved(
                game.answer_internal_card(
                    targets.actor,
                    targets.token,
                    Answer::Targets(vec![domain.members[0].handle]),
                )
                .unwrap(),
            );
            game.resume_internal_card().unwrap();
            pass_quick(&mut game);
        } else {
            game.answer_internal_card(question.actor, question.token, Answer::Quick(None))
                .unwrap();
        }
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["objects"]["attacker"]["acted"], true,
            "the attack was still declared"
        );
        assert_eq!(
            state["P2"]["leader"]["life"],
            if cast { 20_u8 } else { 15_u8 },
            "the printed five-power attacker deals no damage while cursed"
        );
        assert_eq!(
            state["P2"]["pp"]["current"],
            if cast { 8_u8 } else { 10_u8 }
        );
        assert_eq!(
            state["P2"]["cemetery"],
            if cast { json!(["curse"]) } else { json!([]) }
        );
    }
}

const fn all_leaders_damage() -> Action {
    Action::Damage {
        recipients: RecipientSet::Leaders {
            players: PlayerSet {
                relation: PlayerRelation::All,
                relative_to: PlayerRef::Controller {},
                cardinality: Cardinality {
                    minimum: Nat(2),
                    maximum: Some(Nat(2)),
                },
                text_quantifier: TextQuantifier::All,
            },
        },
        amount: DamageAmount::Uniform {
            amount: Expr::Literal(Int(2)),
        },
        flags: DamageFlags::Ability,
        source: ObjectView::SourceInformation {},
        batch: BatchPolicy::Instruction {},
    }
}

fn invoke_producer(game: &mut Game) -> Progress {
    resolved(
        game.activate_internal_card("producer", AbilityId(1), PlayerId { seat: 1 })
            .unwrap(),
    )
    .progress
}

#[test]
fn bp05_108_all_printings_protect_only_their_own_leader_and_preserve_quick_activation() {
    for printing in ["BP05-108", "PR-163", "SP01-050", "SP01-SL50"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"angel","card":printing}]);
        let mut game = with_producer(&initial, all_leaders_damage());
        let progress = invoke_producer(&mut game);
        assert_eq!(
            progress
                .facts
                .iter()
                .filter(|fact| matches!(fact, Fact::Damage(_)))
                .count(),
            1,
            "prevention produces no zero-damage event"
        );
        assert!(
            progress.receipts.values().any(|outcome| matches!(outcome,
                InstructionOutcome::Damage(quantity)
                    if quantity.requested == Int(4) && quantity.actual == Nat(2)
            )),
            "the receipt counts only the one actual two-damage hit"
        );
        game.resume_internal_card().unwrap();
        let protected = game.projection(View::Referee).unwrap();
        assert_eq!(protected["P1"]["leader"]["life"], 20_u8);
        assert_eq!(protected["P2"]["leader"]["life"], 18_u8);
        resolved(
            game.activate_internal_card("angel", AbilityId(4), PlayerId { seat: 1 })
                .unwrap(),
        );
        game.resume_internal_card().unwrap();
        let activated = game.projection(View::Referee).unwrap();
        assert_eq!(
            activated["objects"]["angel"]["acted"], true,
            "the real activated ability still pays act"
        );
        assert_eq!(activated["P2"]["leader"]["life"], 16_u8);
    }
}

#[test]
fn bp05_108_silence_and_actual_departure_remove_the_leader_shield() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"angel","card":"BP05-108","state":{"silenced":true}}]);
    let mut silenced_game = with_producer(&initial, all_leaders_damage());
    invoke_producer(&mut silenced_game);
    assert_eq!(
        silenced_game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        18_u8
    );

    initial["players"]["P1"]["zones"]["field"][0]["state"] = json!({});
    initial["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"producer","card":"producer"}));
    for destroy in [false, true] {
        let first = if destroy {
            Action::Destroy {
                objects: field_name("神弓の座天使・リリエル"),
            }
        } else {
            Action::Damage {
                recipients: RecipientSet::Objects {
                    objects: field_name("神弓の座天使・リリエル"),
                },
                amount: DamageAmount::Uniform {
                    amount: Expr::Literal(Int(5)),
                },
                flags: DamageFlags::Ability,
                source: ObjectView::SourceInformation {},
                batch: BatchPolicy::Instruction {},
            }
        };
        let mut game = Game::from_selected_effects(
            producer_selection().clone(),
            &initial,
            &Value::Null,
            &Value::Null,
            "angel-departure-timing",
        )
        .unwrap();
        game.load_internal_card(
            "producer",
            &producer_steps(vec![first, all_leaders_damage()]),
        )
        .unwrap();
        invoke_producer(&mut game);
        game.resume_internal_card().unwrap();
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["cemetery"], json!(["angel"]));
        // Lethal damage waits for the checkpoint; explicit destruction changes the field immediately.
        assert_eq!(
            state["P1"]["leader"]["life"],
            if destroy { 18_u8 } else { 20_u8 }
        );
        assert_eq!(state["P2"]["leader"]["life"], 18_u8);
    }
}

#[test]
fn bp06_021_all_printings_buff_and_gain_rush_before_preventing_combat_damage() {
    for printing in ["BP06-021", "BP06-P06"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"samurai","card":printing}]);
        initial["players"]["P2"]["zones"]["field"] =
            json!([{"id":"enemy","card":"BP01-037","state":{"acted":true}}]);
        let mut game = rules_game(&initial);
        resolved(
            game.activate_internal_card("samurai", AbilityId(4), PlayerId { seat: 1 })
                .unwrap(),
        );
        game.resume_internal_card().unwrap();
        let before = game.projection(View::Referee).unwrap();
        assert_eq!(before["objects"]["samurai"]["power"], 2_u8);
        assert_eq!(before["objects"]["samurai"]["hp"], 2_u8);
        assert_eq!(before["objects"]["samurai"]["keywords"], json!(["突進"]));
        assert_eq!(before["P1"]["pp"]["current"], 8_u8);
        game.internal_rule_action(&json!({"do":"attack","attacker":"samurai","target":"enemy"}))
            .unwrap();
        pass_quick(&mut game);
        let after = game.projection(View::Referee).unwrap();
        assert_eq!(
            after["objects"]["samurai"]["damage"], 0_u8,
            "five combat damage was prevented even with a typed numeric layer"
        );
        assert_eq!(after["objects"]["samurai"]["hp"], 2_u8);
        assert_eq!(
            after["objects"]["enemy"]["damage"], 2_u8,
            "prevention does not suppress outgoing damage"
        );
    }
}

#[test]
fn damage_prevention_rejects_unresolved_next_only_usage() {
    let input = &inputs().2;
    let source = input["authors"]
        .as_array()
        .unwrap()
        .iter()
        .find(|author| author["input"]["card_no"] == "BP06-021")
        .unwrap();
    let mut candidate = source["input"].clone();
    let card = &mut candidate["tree"]["cards"]["BP06-021"];
    let abilities = if card["abilities"].is_array() {
        &mut card["abilities"]
    } else {
        &mut card["faces"]
            .as_object_mut()
            .unwrap()
            .values_mut()
            .next()
            .unwrap()["abilities"]
    };
    abilities[0]["static"]["effects"][0]["prevent"]["next_only"] = json!(true);
    let error = lowerer().lower_current(&candidate).unwrap_err();
    assert_eq!(error.code, "unsupported_construct");
}

pub(super) fn field_name(name: &str) -> Selector {
    named_field(name, 50)
}

pub(super) fn named_field(name: &str, number: u64) -> Selector {
    let binding = Binding::new(number, ScopeId(1));
    let mut selector = subjects();
    if let Selector::Zone {
        predicate,
        binding: declared,
        ..
    } = &mut selector
    {
        *declared = binding;
        **predicate = Expr::Compute(Box::new(Predicate::NamesIntersect {
            left: Expr::Live(Box::new(NamesRead::Names {
                object: ObjectView::Current {
                    object: Box::new(ObjectOperand::Bound { binding }),
                },
            })),
            right: Expr::Literal(vec![CardName(name.into())]),
        }));
    }
    selector
}

#[test]
fn proposed_damage_recipient_is_not_a_general_player_read() {
    let mut candidate = producer(Action::Life {
        player: PlayerRef::Controller {},
        change: NumericChange::Add {
            amount: Expr::Literal(Int(1)),
        },
    })
    .card()
    .clone();
    let body = candidate.abilities[0].node.body.as_mut().unwrap();
    let Node::Instruction { instruction } = &mut body.steps[0].node else {
        panic!("instruction");
    };
    let Action::Life { player, .. } = &mut instruction.action else {
        panic!("life change");
    };
    *player = PlayerRef::ProposedDamageRecipient {};
    let error = WellFormedCardProgram::check(candidate).unwrap_err();
    assert_eq!(error.reason, WellFormedReason::UnsupportedShape);
}

#[test]
fn bp05_108_does_not_prevent_an_attack_on_its_leader() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"attacker","card":"BP01-037"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"angel","card":"BP05-108"}]);
    let mut game = rules_game(&initial);
    game.internal_rule_action(&json!({"do":"attack","attacker":"attacker","target":"P2.leader"}))
        .unwrap();
    pass_quick(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
        15_u8,
        "an ability-only shield leaves attack damage intact"
    );
}

#[test]
fn bp06_021_does_not_prevent_ability_damage() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"samurai","card":"BP06-021"}]);
    let mut game = with_producer(
        &initial,
        Action::Damage {
            recipients: RecipientSet::Objects {
                objects: field_name("ペインレスサムライ"),
            },
            amount: DamageAmount::Uniform {
                amount: Expr::Literal(Int(1)),
            },
            flags: DamageFlags::Ability,
            source: ObjectView::SourceInformation {},
            batch: BatchPolicy::Instruction {},
        },
    );
    let progress = invoke_producer(&mut game);
    assert_eq!(
        progress
            .facts
            .iter()
            .filter(|fact| matches!(fact, Fact::Damage(_)))
            .count(),
        1
    );
    game.resume_internal_card().unwrap();
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["cemetery"],
        json!(["samurai"]),
        "combat immunity does not prevent one ability damage"
    );
}

#[test]
fn damage_prevention_installation_does_not_grant_destroy_immunity() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"producer","card":"producer"},{"id":"follower","card":"BP01-037"}
    ]);
    let mut game = Game::from_selected_effects(
        producer_selection().clone(),
        &initial,
        &Value::Null,
        &Value::Null,
        "prevention-is-not-destroy-guard",
    )
    .unwrap();
    let objects = field_name("セージコマンダー");
    game.load_internal_card(
        "producer",
        &producer_steps(vec![
            recipient_prevention(objects),
            Action::Damage {
                recipients: RecipientSet::Objects {
                    objects: named_field("セージコマンダー", 51),
                },
                amount: DamageAmount::Uniform {
                    amount: Expr::Literal(Int(5)),
                },
                flags: DamageFlags::Ability,
                source: ObjectView::SourceInformation {},
                batch: BatchPolicy::Instruction {},
            },
            Action::Destroy {
                objects: named_field("セージコマンダー", 52),
            },
        ]),
    )
    .unwrap();
    let progress = invoke_producer(&mut game);
    assert!(progress.receipts.values().any(|outcome| matches!(outcome,
        InstructionOutcome::Damage(quantity) if quantity.requested == Int(5) && quantity.actual == Nat(0)
    )), "the installed recipient shield prevents the preceding five damage");
    assert!(
        !progress
            .facts
            .iter()
            .any(|fact| matches!(fact, Fact::Damage(_)))
    );
    game.resume_internal_card().unwrap();
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["cemetery"],
        json!(["follower"]),
        "prevention is not a destroy guard in the same resolution"
    );
}

pub(super) fn recipient_prevention(objects: Selector) -> Action {
    let affected = Binding::new(1, ScopeId(2));
    let recipient = Binding::new(1, ScopeId(3));
    let duration = Duration {
        end: DurationEnd::CurrentTurnEnd {},
        anchor: TimeAnchor::Installation {},
        usage: None,
        intervals: vec![],
    };
    Action::InstallEffect {
        subjects: InstallationSubjects::Objects { objects, affected },
        capture: InstallationTime::InstructionStart,
        effect: Box::new(Effect::Replacement {
            id: EffectDefinitionId(1),
            source: AbilitySource {
                holder: ObjectOperand::SourceHolder {},
                controller: PlayerRef::Controller {},
                origin: AbilityOrigin::Printed {
                    face: FaceId("0".into()),
                    line: Nat(1),
                },
            },
            pattern: ProposedEventPattern::Damage {
                recipient: Some(RecipientPredicate {
                    binding: recipient,
                    condition: Expr::Compute(Box::new(Predicate::RecipientIsFollower {
                        recipient: DamageRecipient::Bound { binding: recipient },
                        objects: Selector::Bound {
                            reference: affected,
                        },
                    })),
                }),
                source: None,
                kind: DamageKind::Any,
            },
            condition: Expr::Literal(Bool(true)),
            rewrite: EventRewrite::PreventDamage {
                recipient: None,
                source: None,
                kind: DamageKind::Any,
            },
            scope: ReplacementScope::General,
            affected_player: PlayerRef::ProposedDamageRecipient {},
            once_per_event: OncePerEvent::Enforced,
            usage: None,
            prospective_chars: None,
            duration: duration.clone(),
        }),
        duration,
    }
}

#[test]
fn bp01_024_source_restriction_expires_at_the_actual_turn_boundary() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"fairy","card":"BP01-T03"}]);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"curse","card":"BP01-024"}]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-037","state":{"acted":true}}]);
    initial["players"]["P2"]["zones"]["deck"] = json!([
        {"id":"next-draw","card":"BP01-037"},{"id":"later-draw","card":"BP01-037"}
    ]);
    let mut game = rules_game(&initial);
    let CardPlay::Awaiting(question) = game
        .play_internal_card("curse", PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("curse target");
    };
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("targets");
    };
    resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Targets(vec![domain.members[0].handle]),
        )
        .unwrap(),
    );
    game.resume_internal_card().unwrap();
    game.internal_rule_action(&json!({"do":"attack","attacker":"fairy","target":"enemy"}))
        .unwrap();
    pass_quick(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        state["P1"]["field"],
        json!(["fairy"]),
        "the cursed defender cannot retaliate against the one-health Fairy"
    );
    assert_eq!(state["objects"]["enemy"]["damage"], 1_u8);
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    pass_quick(&mut game);
    game.internal_rule_action(&json!({"do":"end-discard","select":[]}))
        .unwrap();
    assert_eq!(
        game.projection(View::Referee).unwrap()["turn"]["active"],
        "P2"
    );
    game.internal_rule_action(&json!({"do":"attack","attacker":"enemy","target":"P1.leader"}))
        .unwrap();
    pass_quick(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        15_u8,
        "the same source deals its five damage after the previous turn's restriction expires"
    );
}

#[test]
fn combat_prevention_does_not_prevent_the_existing_bane_rule() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"samurai","card":"BP06-021"}]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"bane","card":"BP07-080","state":{"acted":true}}]);
    let mut game = rules_game(&initial);
    game.internal_rule_action(&json!({"do":"attack","attacker":"samurai","target":"bane"}))
        .unwrap();
    pass_quick(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        state["P1"]["cemetery"],
        json!(["samurai"]),
        "the existing Bane destruction rule still applies even when combat damage is prevented"
    );
    assert_eq!(state["objects"]["bane"]["damage"], 1_u8);
}

#[test]
fn prevention_pattern_membership_does_not_enable_an_ordinary_boolean_instruction() {
    let mut candidate = producer(Action::Life {
        player: PlayerRef::Controller {},
        change: NumericChange::Add {
            amount: Expr::Literal(Int(1)),
        },
    })
    .card()
    .clone();
    let located = &mut candidate.abilities[0];
    let source = located.source.clone();
    let mut body = located.node.body.take().unwrap();
    body.scope = ScopeId(2);
    located.node.body = Some(Program {
        scope: ScopeId(1),
        steps: vec![Located {
            source,
            node: Node::Control {
                control: Control::If {
                    condition: Expr::Compute(Box::new(Predicate::MemberOf {
                        object: ObjectOperand::SourceHolder {},
                        set: Selector::Objects {
                            objects: vec![ObjectOperand::SourceHolder {}],
                        },
                    })),
                    then_body: body,
                    else_body: None,
                },
            },
        }],
    });
    let checked = WellFormedCardProgram::check(candidate).unwrap();
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"producer","card":"producer"}]);
    let mut game = Game::from_selected_effects(
        producer_selection().clone(),
        &initial,
        &Value::Null,
        &Value::Null,
        "ordinary-membership-is-not-a-prevention-pattern",
    )
    .unwrap();
    let before = format!("{game:?}");
    let error = game.load_internal_card("producer", &checked).unwrap_err();
    assert!(
        format!("{error:?}").contains("predicate.member-of"),
        "ordinary membership remains outside the admitted capability slice"
    );
    assert_eq!(format!("{game:?}"), before);
}

#[test]
fn bp01_024_restricts_a_real_lastword_using_its_saved_source_generation() {
    for cursed in [false, true] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["field"] = json!([]);
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"curse","card":"BP01-024"}]);
        initial["players"]["P2"]["zones"]["field"] = json!([{"id":"rabbit","card":"BP01-P25"}]);
        let mut game = with_producer(
            &initial,
            Action::Destroy {
                objects: field_name("ラビットネクロマンサー"),
            },
        );
        if cursed {
            let CardPlay::Awaiting(question) = game
                .play_internal_card("curse", PlayerId { seat: 1 })
                .unwrap()
            else {
                panic!("curse target");
            };
            let InputDomain::Targets(domain) = &question.domain else {
                panic!("public target");
            };
            resolved(
                game.answer_internal_card(
                    question.actor,
                    question.token,
                    Answer::Targets(vec![domain.members[0].handle]),
                )
                .unwrap(),
            );
            game.resume_internal_card().unwrap();
        }
        invoke_producer(&mut game);
        game.resume_internal_card().unwrap();
        let question = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = &question.domain else {
            panic!("real Lastword");
        };
        assert_eq!(options.len(), 1);
        assert_eq!(options[0].card, "BP01-P25");
        assert_eq!(question.actor.seat, 2);
        resolved(
            game.answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap(),
        );
        game.resume_internal_card().unwrap();
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P2"]["cemetery"], json!(["rabbit"]));
        for seat in ["P1", "P2"] {
            assert_eq!(
                state[seat]["leader"]["life"],
                if cursed { 20_u8 } else { 18_u8 },
                "the actual Lastword resolves with its captured source generation"
            );
        }
    }
}
