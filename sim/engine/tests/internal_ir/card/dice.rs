#![expect(
    clippy::wildcard_enum_match_arm,
    reason = "Dice scenarios pick their own facts out of the full progress."
)]
use super::*;
use sve_engine::game::internal::{DecisionView, InputDomain};

fn reroll() -> Located<Ability> {
    let mut entry = ability(2, vec![]);
    entry.node.kind = AbilityKind::Static;
    entry.node.play = None;
    entry.node.body = None;
    entry.node.declarations = vec![Effect::Permission {
        subject: PermissionSubject::Players {
            players: PlayerSet {
                relative_to: PlayerRef::Controller {},
                relation: PlayerRelation::SelfPlayer,
                cardinality: Cardinality {
                    minimum: Nat(1),
                    maximum: Some(Nat(1)),
                },
                text_quantifier: TextQuantifier::Implicit,
            },
        },
        action: ActionPattern::Reroll {
            player: PlayerRef::Controller {},
            times: Nat(1),
        },
        polarity: Polarity::Allow,
        condition: Expr::Literal(Bool(true)),
        duration: Duration {
            end: DurationEnd::NoTemporalExpiry {},
            anchor: TimeAnchor::Installation {},
            usage: None,
            intervals: vec![],
        },
        rule: RuleId(1),
    }];
    entry
}

fn roll(scope: u64) -> Located<Node> {
    instruction(
        10,
        Action::Dice {
            operation: DiceOperation::Roll {},
            result: Binding::new(20, ScopeId(scope)),
        },
    )
}

fn repeated() -> Located<Ability> {
    ability(
        1,
        vec![node(Node::Control {
            control: Control::Repeat {
                body: RepeatBody::Program {
                    body: body(10, vec![roll(10)]),
                },
                termination: Termination::Count {
                    count: Expr::Literal(Nat(2)),
                },
                check: IterationCheck::BeforeIteration,
                scope: ScopeId(9),
                receipts: vec![],
            },
        })],
    )
}

#[expect(
    clippy::panic,
    reason = "A reroll scenario must pause at its dice choice."
)]
fn waiting(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(question) = play else {
        panic!("expected reroll");
    };
    question
}

fn answer(game: &mut Game, question: &DecisionView, value: u64) -> CardPlay {
    game.answer_internal_card(question.actor, question.token, Answer::Integer(Nat(value)))
        .unwrap()
}

#[test]
fn rerolls_replace_the_result_and_fixed_repeats_reset_the_permission() {
    let program = card(vec![repeated(), reroll()]);
    let mut game = position_with_card(1, &program);
    let q1 = waiting(
        game.activate_internal_card("s", AbilityId(1), PlayerId { seat: 1 })
            .unwrap(),
    );
    assert!(matches!(
        q1.domain,
        InputDomain::DiceReroll {
            remaining: Nat(1),
            ..
        }
    ));
    assert_eq!(
        game.internal_decision(View::P2),
        Some(q1.clone()),
        "the visible die result is public"
    );
    game.answer_internal_card(PlayerId { seat: 2 }, q1.token, Answer::Integer(Nat(1)))
        .unwrap_err();
    assert_eq!(game.internal_decision(View::P1), Some(q1.clone()));
    let q2 = waiting(answer(&mut game, &q1, 1));
    assert!(matches!(
        q2.domain,
        InputDomain::DiceReroll {
            remaining: Nat(1),
            ..
        }
    ));
    game.answer_internal_card(q1.actor, q1.token, Answer::Integer(Nat(0)))
        .unwrap_err();
    assert_eq!(game.internal_decision(View::P1), Some(q2.clone()));
    let done = resolved(answer(&mut game, &q2, 0));
    let dice = done
        .progress
        .facts
        .iter()
        .filter_map(|fact| match fact {
            Fact::Dice(event) => Some(event),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(dice.len(), 2, "two rolls rather than three attempts");
    assert_eq!(dice[0].attempts.len(), 2);
    assert_eq!(dice[0].accepted_attempt, Nat(1));
    assert_eq!(dice[0].attempts.last(), Some(&dice[0].value));
    assert_eq!(dice[1].attempts.len(), 1);
    assert_ne!(dice[0].roll, dice[1].roll);
    assert_eq!(dice[0].attempts, vec![DieFace::One, DieFace::Two]);
    assert_eq!(dice[0].value, DieFace::Two);
    assert_eq!(dice[1].value, DieFace::Five);
}

#[test]
fn player_pipeline_reroll_waiting_summary_preserves_the_public_die() {
    use crate::player_pipeline::{answer as player_answer, question as player_question};
    use sve_engine::game::player::{Input, QuestionShape, SubmissionKind, Viewer};

    let program = card(vec![repeated(), reroll()]);
    let mut game = position_with_card(1, &program);
    game.activate_internal_card("s", AbilityId(1), PlayerId { seat: 1 })
        .unwrap();
    for _ in 0..2 {
        let question = player_question(&game, Viewer::P1);
        let QuestionShape::Decision(InputDomain::DiceReroll { value, remaining }) = question.shape
        else {
            panic!("reroll question");
        };
        let before = exact_authority(&game);
        for _ in 0..4 {
            assert!(matches!(game.input(Viewer::P2), Input::Waiting(summary)
                if summary.dice == Some((value, remaining))));
        }
        assert_eq!(exact_authority(&game), before);
        assert_eq!(
            player_answer(&mut game, Viewer::P1, Answer::Integer(Nat(0))).kind,
            SubmissionKind::Applied
        );
    }
    assert!(matches!(
        player_question(&game, Viewer::P1).shape,
        QuestionShape::ActionWindow(_)
    ));
}

#[test]
fn multiple_reroll_sources_stack_and_invalid_answers_preserve_the_choice() {
    let program = card(vec![ability(1, vec![roll(1)]), reroll()]);
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"s","card":"unit-follower"},{"id":"aura","card":"unit-follower"}]);
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "dice-stack",
    )
    .unwrap();
    game.load_internal_card("unit-follower", &program).unwrap();
    game.load_internal_card("unit-spell", &card(vec![]))
        .unwrap();
    let first = waiting(
        game.activate_internal_card("s", AbilityId(1), PlayerId { seat: 1 })
            .unwrap(),
    );
    assert!(matches!(
        first.domain,
        InputDomain::DiceReroll {
            remaining: Nat(2),
            ..
        }
    ));
    game.answer_internal_card(first.actor, first.token, Answer::Integer(Nat(2)))
        .unwrap_err();
    assert_eq!(game.internal_decision(View::P1), Some(first.clone()));
    let second = waiting(answer(&mut game, &first, 1));
    assert!(matches!(
        second.domain,
        InputDomain::DiceReroll {
            remaining: Nat(1),
            ..
        }
    ));
    let done = resolved(answer(&mut game, &second, 1));
    assert!(done.progress.facts.iter().any(|fact| matches!(fact,Fact::Dice(event) if event.attempts.len()==3 && event.accepted_attempt==Nat(2))));
}

#[test]
fn dice_stream_can_be_replayed_without_scripts() {
    let program = card(vec![ability(1, vec![roll(1)])]);
    let mut first = position_with_card(1, &program);
    let mut second = position_with_card(1, &program);
    let left = activate(&mut first, 1);
    let right = activate(&mut second, 1);
    assert_eq!(left.progress.facts, right.progress.facts);
    assert!(
        left.progress
            .facts
            .iter()
            .any(|fact| matches!(fact, Fact::Dice(_)))
    );
}

#[test]
fn following_text_reads_the_replacement_value() {
    let hit = instruction(
        11,
        Action::Damage {
            recipients: RecipientSet::Leaders {
                players: players(PlayerRelation::Opponents),
            },
            amount: DamageAmount::Uniform {
                amount: Expr::Compute(Box::new(IntOperation::FromNat {
                    value: Expr::Bound(Binding::new(20, ScopeId(1))),
                })),
            },
            flags: DamageFlags::Ability,
            source: ObjectView::Current {
                object: Box::new(ObjectOperand::SourceHolder {}),
            },
            batch: BatchPolicy::Instruction {},
        },
    );
    let program = card(vec![ability(1, vec![roll(1), hit]), reroll()]);
    let mut game = position_with_card(1, &program);
    let question = waiting(
        game.activate_internal_card("s", AbilityId(1), PlayerId { seat: 1 })
            .unwrap(),
    );
    let done = resolved(answer(&mut game, &question, 1));
    let damage = done
        .progress
        .facts
        .iter()
        .filter_map(|fact| match fact {
            Fact::Damage(event) => Some(event.amount.0.get()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(damage, vec![2]);
}

#[test]
fn unadmitted_roller_is_rejected_before_any_random_draw() {
    let mut step = roll(1);
    if let Node::Instruction { instruction } = &mut step.node {
        instruction.actor = PlayerRef::Active {};
    }
    let program = card(vec![ability(1, vec![step])]);
    let mut game = Game::new(
        catalog(),
        &setup(1, json!([])),
        &Value::Null,
        &Value::Null,
        "dice-actor",
    )
    .unwrap();
    let before = format!("{game:?}");
    assert!(
        matches!(game.load_internal_card("unit-follower",&program),Err(sve_engine::EngineFailure::Unsupported(message)) if message.contains("dice requires the controller"))
    );
    assert_eq!(format!("{game:?}"), before);
}

#[test]
fn repeats_preserve_the_same_timestamp_numeric_restriction() {
    for count in [1, 2] {
        let set = numeric::modify(
            10,
            Selector::Objects {
                objects: vec![ObjectOperand::SourceHolder {}],
            },
            CharacteristicPatch::Numeric {
                stat: Stat::Life,
                change: NumericChange::Set {
                    amount: Expr::Literal(Int(3)),
                },
            },
            DurationEnd::NoTemporalExpiry {},
        );
        let repeated = node(Node::Control {
            control: Control::Repeat {
                body: RepeatBody::Program {
                    body: body(10, vec![set]),
                },
                termination: Termination::Count {
                    count: Expr::Literal(Nat(count)),
                },
                check: IterationCheck::BeforeIteration,
                scope: ScopeId(9),
                receipts: vec![],
            },
        });
        let program = card(vec![ability(1, vec![repeated])]);
        let mut game = Game::new(
            catalog(),
            &setup(1, json!([])),
            &Value::Null,
            &Value::Null,
            "repeat-numeric",
        )
        .unwrap();
        let result = game.load_internal_card("unit-follower", &program);
        if count == 1 {
            result.unwrap();
        } else {
            assert!(
                matches!(result,Err(sve_engine::EngineFailure::Unsupported(message)) if message.contains("same-timestamp SetLife"))
            );
        }
    }
}

#[test]
fn field_permission_also_applies_during_the_opponents_turn() {
    let mut initial = setup(1, json!([]));
    initial["turn"]["active"] = json!("P2");
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
    let mut game = Game::new(catalog(), &initial, &Value::Null, &Value::Null, "dice-3").unwrap();
    game.load_internal_card("unit-follower", &card(vec![reroll()]))
        .unwrap();
    game.load_internal_card("unit-spell", &card(vec![]))
        .unwrap();
    let progress = game
        .execute_internal(&checked(vec![roll(1)]), &source("synthetic"), context(1))
        .unwrap();
    assert!(!progress.complete);
    let question = game.internal_decision(View::P1).unwrap();
    assert!(matches!(
        question.domain,
        InputDomain::DiceReroll {
            value: Nat(6),
            remaining: Nat(1)
        }
    ));
    let progress = game
        .answer_internal(question.actor, question.token, Answer::Integer(Nat(1)))
        .unwrap();
    assert!(progress.complete);
    assert!(progress.facts.iter().any(|fact|matches!(fact,Fact::Dice(event) if event.roller.seat==1 && event.value==DieFace::Four)));
}

fn lower_activations(bodies: &[Value]) -> WellFormedCardProgram {
    let mut tree = fanfare::expanded::tree().clone();
    let entry = tree["cards"]["BP08-038"]["abilities"][0].clone();
    let entries = bodies
        .iter()
        .enumerate()
        .map(|(index, body)| {
            let mut entry = entry.clone();
            entry.as_object_mut().unwrap().remove("fanfare");
            entry["sources"][0]["ability_scope"] = json!(format!("dice_{index}"));
            entry["activate"] = body.clone();
            entry
        })
        .collect::<Vec<_>>();
    let meta = tree["cards"]["BP08-038"]["meta"].clone();
    tree["cards"] = json!({"unit-follower":{"meta":meta,"abilities":entries}});
    lowerer()
        .lower(&tree)
        .unwrap()
        .remove("unit-follower")
        .unwrap()
}

fn hand_size(game: &Game, player: &str) -> usize {
    game.projection(View::Referee).unwrap()[player]["hand"]
        .as_array()
        .unwrap()
        .len()
}

fn activate_on(game: &mut Game, holder: &str, ability: AbilityId, seat: u32) {
    resolved(
        game.activate_internal_card(holder, ability, PlayerId { seat })
            .unwrap(),
    );
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}

#[test]
fn dice_history_counts_only_the_controllers_rolls_in_the_current_turn() {
    let program = lower_activations(&[
        json!({"cost":{"pp":0},"do":[{"dice":{"bind":"die"}}]}),
        json!({"cost":{"pp":0},"do":[{"if":{
            "cond":{"history":{"event":"dice_rolled","who":"self","window":"this_turn","where":{"value":{"ge":1}}}},
            "then":[{"draw":{"n":1}}]
        }}]}),
    ]);
    let roller = program.card().abilities[0].node.id;
    let reader = program.card().abilities[1].node.id;
    let mut initial = setup(
        1,
        json!([{"id":"d0","card":"unit-spell"},{"id":"d1","card":"unit-spell"}]),
    );
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
    initial["players"]["P2"]["zones"]["deck"] = json!([
        {"id":"o0","card":"unit-spell"},{"id":"o1","card":"unit-spell"},{"id":"o2","card":"unit-spell"}
    ]);
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "dice-history",
    )
    .unwrap();
    game.load_internal_card("unit-spell", &card(vec![]))
        .unwrap();
    game.load_internal_card("unit-follower", &program).unwrap();
    let progress = game
        .execute_internal(
            &checked(vec![roll(1)]),
            &source("synthetic"),
            Context {
                source: "f".into(),
                generation: Generation(0),
                controller: PlayerId { seat: 2 },
            },
        )
        .unwrap();
    assert!(
        progress
            .facts
            .iter()
            .any(|fact| matches!(fact, Fact::Dice(event) if event.roller.seat == 2))
    );
    activate_on(&mut game, "s", reader, 1);
    assert_eq!(
        hand_size(&game, "P1"),
        0,
        "the opponent's roll this turn is not mine"
    );
    activate_on(&mut game, "s", roller, 1);
    activate_on(&mut game, "s", reader, 1);
    assert_eq!(
        hand_size(&game, "P1"),
        1,
        "my own accepted roll this turn counts"
    );
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    quick::pass(&mut game);
    let before = hand_size(&game, "P2");
    activate_on(&mut game, "f", reader, 2);
    assert_eq!(
        hand_size(&game, "P2"),
        before,
        "a roll from an earlier turn is not this turn"
    );
    activate_on(&mut game, "f", roller, 2);
    activate_on(&mut game, "f", reader, 2);
    assert_eq!(hand_size(&game, "P2"), before + 1);
}
