//! Authority-only regressions. Printed ability fragments carry no production source admission.

#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::default_numeric_fallback,
    clippy::shadow_unrelated,
    reason = "Synthetic fixtures are literal JSON whose errors fail tests; repeated authority snapshots are independently compared."
)]

extern crate alloc;
use alloc::sync::Arc;
use serde_json::{Value, json};
use sve_engine::catalog::Catalog;
use sve_engine::game::internal::{Answer, Context, Fact, InstructionOutcome, Progress};
use sve_engine::game::{Game, View};
use sve_engine::ir::*;

#[path = "internal_ir/allocation.rs"]
mod allocation;
#[path = "internal_ir/card.rs"]
mod card;
#[path = "internal_ir/cost.rs"]
mod cost;
#[path = "internal_ir/damage.rs"]
mod damage;
#[path = "internal_ir/life.rs"]
mod life;
#[path = "internal_ir/numeric.rs"]
mod numeric;
#[path = "internal_ir/ordering.rs"]
mod ordering;
#[path = "internal_ir/projections.rs"]
mod projections;
#[path = "internal_ir/transactions.rs"]
mod transactions;

#[path = "internal_ir/terminal.rs"]
mod terminal;

#[path = "internal_ir/search.rs"]
mod search;

fn source(card: &str) -> SourceSpan {
    SourceSpan {
        card_id: card.into(),
        card_no: card.into(),
        scope: "fragment".into(),
        owner: "card".into(),
        face: "0".into(),
        field: "text".into(),
        section: None,
        line: 1,
        fragment: Some("D1".into()),
        author_path: "/body".into(),
    }
}
fn node(node: Node) -> Located<Node> {
    Located {
        node,
        source: source("synthetic"),
    }
}
const fn body(scope: u64, steps: Vec<Located<Node>>) -> Program {
    Program {
        scope: ScopeId(scope),
        steps,
    }
}
fn checked(steps: Vec<Located<Node>>) -> WellFormedProgram {
    WellFormedProgram::check(body(1, steps)).unwrap()
}
fn context(seat: u32) -> Context {
    Context {
        source: "s".into(),
        generation: Generation(0),
        controller: PlayerId { seat },
    }
}
fn instruction(id: u64, action: Action) -> Located<Node> {
    node(Node::Instruction {
        instruction: Box::new(Instruction {
            id: InstructionId(id),
            actor: PlayerRef::Controller {},
            source: AbilitySource {
                holder: ObjectOperand::SourceHolder {},
                origin: AbilityOrigin::Printed {
                    face: FaceId("0".into()),
                    line: Nat(1),
                },
                controller: PlayerRef::Controller {},
            },
            precondition: None,
            context: InstructionContext::Effect,
            receipt: ReceiptId(id),
            action,
        }),
    })
}
fn draw(id: u64, count: i64) -> Located<Node> {
    draw_value(id, Expr::Literal(Int(count)))
}
fn draw_value(id: u64, count: Expr<Int>) -> Located<Node> {
    instruction(
        id,
        Action::Draw {
            player: PlayerRef::Controller {},
            count,
            mode: StepwiseMode::Exact,
        },
    )
}
fn up_to(id: u64, count: i64) -> Located<Node> {
    instruction(
        id,
        Action::Draw {
            player: PlayerRef::Controller {},
            count: Expr::Literal(Int(count)),
            mode: StepwiseMode::UpToStepwise {
                decision: Box::new(StepwiseDecision {
                    id: DecisionId(id),
                    actor: PlayerRef::Controller {},
                    visibility: Visibility::Only {
                        players: PlayerSet {
                            cardinality: Cardinality {
                                minimum: Nat(1),
                                maximum: Some(Nat(1)),
                            },
                            ..players(PlayerRelation::SelfPlayer)
                        },
                    },
                    timing: StepwiseTiming::BeforeDraw,
                }),
            },
        },
    )
}
const fn players(relation: PlayerRelation) -> PlayerSet {
    PlayerSet {
        relation,
        relative_to: PlayerRef::Controller {},
        cardinality: Cardinality {
            minimum: Nat(0),
            maximum: None,
        },
        text_quantifier: TextQuantifier::All,
    }
}
fn zone(kind: ZoneKind, scope: u64) -> Selector {
    Selector::Zone {
        players: players(PlayerRelation::All),
        zone: kind,
        view: SelectionView::Current {},
        binding: Binding::new(9, ScopeId(scope)),
        predicate: Box::new(Expr::Literal(Bool(true))),
    }
}
fn recipients() -> RecipientSet {
    RecipientSet::Union {
        sets: vec![
            RecipientSet::Leaders {
                players: players(PlayerRelation::Opponents),
            },
            RecipientSet::Objects {
                objects: zone(ZoneKind::Field, 99),
            },
        ],
    }
}
fn damage(id: u64, recipients: RecipientSet, amount: i64) -> Located<Node> {
    instruction(
        id,
        Action::Damage {
            recipients,
            amount: DamageAmount::Uniform {
                amount: Expr::Literal(Int(amount)),
            },
            flags: DamageFlags::Ability,
            source: ObjectView::Current {
                object: Box::new(ObjectOperand::SourceHolder {}),
            },
            batch: BatchPolicy::Instruction {},
        },
    )
}
fn setup(seat: u32, deck: Value) -> Value {
    let mut value = json!({"history":"explicit","turn":{"active":"P1","first_player":"P1","phase":"main","elapsed_turns":{"P1":3,"P2":2}},"players":{
        "P1":{"leader":{"class":"ニュートラル","life":20},"pp":{"current":2,"max":2},"ep":0,"sep":0,"construction":"class","zones":{"deck":[]}},
        "P2":{"leader":{"class":"ニュートラル","life":20},"pp":{"current":2,"max":2},"ep":0,"sep":0,"construction":"class","zones":{"deck":[]}}
    }});
    let actor = if seat == 1 { "P1" } else { "P2" };
    let opponent = if seat == 1 { "P2" } else { "P1" };
    value["players"][actor]["zones"]["hand"] = json!([{"id":"s","card":"unit-spell"}]);
    value["players"][actor]["zones"]["deck"] = deck;
    value["players"][opponent]["zones"]["field"] = json!([{"id":"f","card":"unit-follower"}]);
    value
}
fn catalog() -> Arc<Catalog> {
    catalog_with_cost(&json!("1"))
}
fn catalog_with_cost(cost: &Value) -> Arc<Catalog> {
    let snapshot = [("unit-spell", "スペル"), ("unit-follower", "フォロワー"), ("unit-amulet", "アミュレット")].iter().map(|(number, kind)| {
        json!({"number":number,"faces":[{"name":number,"card_class":"ニュートラル","card_type":kind,"cost":cost,"power":if *kind=="アミュレット" {"-"}else{"2"},"hp":if *kind=="アミュレット" {"-"}else{"3"},"traits":[],"text":null,"sections":[]}]}).to_string()
    }).collect::<Vec<_>>().join("\n");
    let registry = json!({"format":1,"kind":"keyword_registry","keywords":{}}).to_string();
    let document = json!({"format":1,"kind":"effect_set","cards":{
        "unit-spell":{"status":"complete","review":"synthetic","abilities":[]},
        "unit-follower":{"status":"complete","review":"synthetic","abilities":[]},
        "unit-amulet":{"status":"complete","review":"synthetic","abilities":[]}
    }})
    .to_string();
    Arc::new(
        Catalog::from_documents(&snapshot, &registry, &[("unit.yaml".into(), document)]).unwrap(),
    )
}
fn game(seat: u32, count: usize) -> Game {
    let deck = (0..count)
        .map(|index| json!({"id":format!("d{index}"),"card":"unit-follower"}))
        .collect::<Vec<_>>();
    Game::new(
        catalog(),
        &setup(seat, json!(deck)),
        &Value::Null,
        &Value::Null,
        "D1-seed",
    )
    .unwrap()
}
fn cards(outcome: &Progress, id: u64, requested: i64, actual: u64) {
    assert_eq!(
        outcome.receipts.get(&ReceiptId(id)),
        Some(&InstructionOutcome::Cards(CardQuantityOutcome {
            requested: Int(requested),
            actual: Nat(actual)
        })),
        "draw receipt {id}"
    );
}
fn exact_authority(game: &Game) -> String {
    format!("{game:?}")
}
fn capture_int(slot: u64, value: Expr<Int>) -> Located<Node> {
    node(Node::Capture {
        capture: Capture::Integer {
            at: CaptureTime::AtResolution { scope: ScopeId(1) },
            value,
            into: CaptureRef {
                binding: Binding::new(slot, ScopeId(1)),
            },
        },
    })
}

#[test]
fn empty_program_and_exact_signed_draws_both_seats() {
    for seat in [1, 2] {
        for (count, actual) in [(-2, 0), (0, 0), (2, 2), (4, 3)] {
            let mut game = game(seat, 3);
            let empty = game
                .execute_internal(&checked(vec![]), &source("synthetic"), context(seat))
                .unwrap();
            assert!(empty.complete && empty.receipts.is_empty() && empty.facts.is_empty());
            let result = game
                .execute_internal(
                    &checked(vec![draw(1, count)]),
                    &source("synthetic"),
                    context(seat),
                )
                .unwrap();
            cards(&result, 1, count, actual);
            assert!(result.complete);
            assert_eq!(result.facts.len(), usize::try_from(actual).unwrap());
            for (ordinal, fact) in result.facts.iter().enumerate() {
                let Fact::Draw(fact) = fact else {
                    panic!("draw fact");
                };
                assert_eq!(fact.player.seat, seat);
                assert_eq!(fact.ordinal.0, u64::try_from(ordinal).unwrap() + 1);
                assert_eq!(fact.object.generation, Generation(1));
            }
            let projection = game.projection(View::Referee).unwrap();
            assert_eq!(projection["game"]["ended"] == true, count > 3);
        }
    }
}

#[test]
fn new_private_hand_slots_have_no_opponent_identity() {
    for seat in [1, 2] {
        let mut game = game(seat, 2);
        game.execute_internal(
            &checked(vec![draw(1, 2)]),
            &source("synthetic"),
            context(seat),
        )
        .unwrap();
        let (own, opponent, actor) = if seat == 1 {
            (View::P1, View::P2, "P1")
        } else {
            (View::P2, View::P1, "P2")
        };
        let own = game.projection(own).unwrap();
        let other = game.projection(opponent).unwrap();
        assert_eq!(own[actor]["hand_count"], 3);
        for id in ["d0", "d1"] {
            assert!(own["objects"].get(id).is_some());
            assert!(other["objects"].get(id).is_none());
            assert!(
                !other[actor]["hand"]
                    .as_array()
                    .unwrap()
                    .contains(&json!(id))
            );
        }
    }
}

#[test]
fn before_draw_occurrences_stop_and_reject_stale_actor_and_payload_atomically() {
    for seat in [1, 2] {
        let mut game = game(seat, 3);
        let first = game
            .execute_internal(
                &checked(vec![up_to(1, 3), draw(2, 0)]),
                &source("synthetic"),
                context(seat),
            )
            .unwrap();
        assert!(!first.complete && first.facts.is_empty());
        let actor = PlayerId { seat };
        let view = if seat == 1 { View::P1 } else { View::P2 };
        let other_view = if seat == 1 { View::P2 } else { View::P1 };
        let first = game.internal_decision(view).unwrap();
        assert!(game.internal_decision(other_view).is_none());
        assert!(game.projection(other_view).unwrap()["awaiting"].is_null());
        assert_eq!(game.legal().unwrap(), Vec::<Value>::new());
        serde_json::to_value(&game).unwrap_err();
        game.digest().unwrap_err();
        for answer in [
            Answer::Integer(Nat(0)),
            Answer::Stepwise(StepwiseAnswer::Continue),
        ] {
            let before = exact_authority(&game);
            let wrong = if matches!(answer, Answer::Integer(_)) {
                actor
            } else {
                PlayerId {
                    seat: if seat == 1 { 2 } else { 1 },
                }
            };
            game.answer_internal(wrong, first.token, answer)
                .unwrap_err();
            assert_eq!(exact_authority(&game), before);
        }
        let before = exact_authority(&game);
        game.decide(&json!({"do":"end"}), "wrong-channel")
            .unwrap_err();
        assert_eq!(exact_authority(&game), before);
        let next = game
            .answer_internal(
                actor,
                first.token,
                Answer::Stepwise(StepwiseAnswer::Continue),
            )
            .unwrap();
        assert_eq!(next.facts.len(), 1);
        let second = game.internal_decision(view).unwrap();
        assert_ne!(first.token, second.token);
        let before = exact_authority(&game);
        game.answer_internal(
            actor,
            first.token,
            Answer::Stepwise(StepwiseAnswer::Continue),
        )
        .unwrap_err();
        assert_eq!(exact_authority(&game), before);
        let final_result = game
            .answer_internal(actor, second.token, Answer::Stepwise(StepwiseAnswer::Stop))
            .unwrap();
        cards(&final_result, 1, 3, 1);
        cards(&final_result, 2, 0, 0);
        assert!(final_result.complete);
        assert!(game.internal_decision(view).is_none());
        let before = exact_authority(&game);
        game.answer_internal(actor, second.token, Answer::Stepwise(StepwiseAnswer::Stop))
            .unwrap_err();
        assert_eq!(exact_authority(&game), before);
    }
}

#[test]
fn stepwise_zero_negative_stop_first_and_empty_attempts() {
    for count in [-3, 0, 1, 3] {
        let mut game = game(1, 0);
        let result = game
            .execute_internal(
                &checked(vec![up_to(1, count)]),
                &source("synthetic"),
                context(1),
            )
            .unwrap();
        if count <= 0 {
            cards(&result, 1, count, 0);
            assert!(result.complete);
            continue;
        }
        for _ in 0..count {
            assert_ne!(
                game.projection(View::Referee).unwrap()["game"]["ended"],
                true
            );
            let question = game.internal_decision(View::P1).unwrap();
            game.answer_internal(
                PlayerId { seat: 1 },
                question.token,
                Answer::Stepwise(StepwiseAnswer::Continue),
            )
            .unwrap();
        }
        assert_eq!(
            game.projection(View::Referee).unwrap()["game"]["ended"],
            true
        );
    }
    let mut game = game(1, 3);
    game.execute_internal(
        &checked(vec![up_to(1, 3)]),
        &source("synthetic"),
        context(1),
    )
    .unwrap();
    let question = game.internal_decision(View::P1).unwrap();
    let result = game
        .answer_internal(
            PlayerId { seat: 1 },
            question.token,
            Answer::Stepwise(StepwiseAnswer::Stop),
        )
        .unwrap();
    cards(&result, 1, 3, 0);
    assert_eq!(result.facts, Vec::<Fact>::new());
}

#[test]
fn dead_arms_costs_hidden_reads_and_unsupported_requirements_reject_whole_body() {
    let unsupported = instruction(
        2,
        Action::Resource {
            player: PlayerRef::Controller {},
            resource: Resource::PpCurrent,
            operation: ResourceOperation::Add,
            amount: Expr::Literal(Int(1)),
        },
    );
    let programs = [
        checked(vec![draw(1, 1), unsupported.clone()]),
        checked(vec![
            draw(1, 1),
            node(Node::Control {
                control: Control::If {
                    condition: Expr::Literal(Bool(false)),
                    then_body: body(2, vec![unsupported]),
                    else_body: None,
                },
            }),
        ]),
        checked(vec![
            draw(1, 1),
            damage(
                2,
                RecipientSet::Objects {
                    objects: zone(ZoneKind::Deck, 90),
                },
                2,
            ),
        ]),
        checked(vec![
            draw(1, 1),
            node(Node::Control {
                control: Control::Repeat {
                    body: RepeatBody::Program {
                        body: body(3, vec![]),
                    },
                    termination: Termination::Count {
                        count: Expr::Literal(Nat(1)),
                    },
                    check: IterationCheck::BeforeIteration,
                    scope: ScopeId(2),
                    receipts: vec![],
                },
            }),
        ]),
        checked(vec![
            draw(1, 1),
            integer_choice_within(
                2,
                1,
                Cardinality {
                    minimum: Nat(0),
                    maximum: None,
                },
            ),
        ]),
    ];
    for program in programs {
        let mut game = game(1, 3);
        let before = exact_authority(&game);
        game.execute_internal(&program, &source("synthetic"), context(1))
            .unwrap_err();
        assert_eq!(exact_authority(&game), before);
    }
}

#[test]
fn uniform_damage_normalizes_union_and_checks_only_after_resolution() {
    for seat in [1, 2] {
        for amount in [-2, 0, 2, 4] {
            let mut game = game(seat, 2);
            let both = RecipientSet::Union {
                sets: vec![
                    recipients(),
                    RecipientSet::Objects {
                        objects: zone(ZoneKind::Field, 98),
                    },
                ],
            };
            let program = checked(vec![
                damage(1, both, amount),
                damage(
                    2,
                    RecipientSet::Objects {
                        objects: zone(ZoneKind::Cemetery, 97),
                    },
                    1,
                ),
            ]);
            let result = game
                .execute_internal(&program, &source("synthetic"), context(seat))
                .unwrap();
            let positive = u64::try_from(amount.max(0)).unwrap();
            assert_eq!(
                result.receipts.get(&ReceiptId(1)),
                Some(&InstructionOutcome::Damage(DamageQuantityOutcome {
                    requested: Int(i64::try_from(2 * positive).unwrap()),
                    actual: Nat(2 * positive)
                }))
            );
            assert_eq!(result.facts.len(), if positive == 0 { 0 } else { 2 });
            let projection = game.projection(View::Referee).unwrap();
            let other = if seat == 1 { "P2" } else { "P1" };
            assert_eq!(
                projection[other]["leader"]["life"],
                20 - i64::try_from(positive).unwrap()
            );
            assert_eq!(projection[other]["cemetery_count"], i32::from(amount >= 3));
            // Cemetery was still empty at the second instruction, before the checkpoint.
            assert_eq!(
                result.receipts.get(&ReceiptId(2)),
                Some(&InstructionOutcome::Damage(DamageQuantityOutcome {
                    requested: Int(0),
                    actual: Nat(0)
                }))
            );
        }
    }
}

#[test]
fn empty_recipients_have_no_damage_facts() {
    for recipients in [
        RecipientSet::Union { sets: vec![] },
        RecipientSet::Explicit { recipients: vec![] },
    ] {
        let mut game = game(1, 2);
        let result = game
            .execute_internal(
                &checked(vec![damage(1, recipients, 8)]),
                &source("synthetic"),
                context(1),
            )
            .unwrap();
        assert_eq!(result.facts, Vec::<Fact>::new());
    }
}

#[test]
fn capture_freezes_public_live_values_while_later_reads_observe_damage() {
    let live = Expr::Live(Box::new(IntRead::PlayerResource {
        player: PlayerRef::Controller {},
        property: PlayerResource::Life,
    }));
    let mut game = game(1, 5);
    let program = checked(vec![
        capture_int(1, live.clone()),
        damage(
            1,
            RecipientSet::Leaders {
                players: players(PlayerRelation::SelfPlayer),
            },
            18,
        ),
        draw_value(
            2,
            Expr::Captured(CaptureRef {
                binding: Binding::new(1, ScopeId(1)),
            }),
        ),
        draw_value(3, live),
    ]);
    let result = game
        .execute_internal(&program, &source("synthetic"), context(1))
        .unwrap();
    cards(&result, 2, 20, 5);
    cards(&result, 3, 2, 0);
}

fn integer_choice(id: u64, scope: u64) -> Located<Node> {
    integer_choice_within(
        id,
        scope,
        Cardinality {
            minimum: Nat(0),
            maximum: Some(Nat(2)),
        },
    )
}
fn integer_choice_within(id: u64, scope: u64, bounds: Cardinality) -> Located<Node> {
    node(Node::Decision {
        decision: Decision {
            id: DecisionId(id),
            captures: vec![],
            actor: PlayerRef::Controller {},
            domain: ChoiceDomain::Integer {
                cardinality: bounds,
                binding: Binding::new(1, ScopeId(scope)),
            },
            kind: DecisionKind::ResolutionChoice {
                may: Declinable::Required,
                cardinality: bounds,
                precondition: None,
            },
            visibility: Visibility::Public {},
        },
    })
}
fn signed_binding(scope: u64) -> Expr<Int> {
    Expr::Compute(Box::new(IntOperation::FromNat {
        value: Expr::Bound(Binding::new(1, ScopeId(scope))),
    }))
}

#[test]
fn equal_numeric_binding_slots_in_distinct_scopes_do_not_alias() {
    let mut game = game(1, 5);
    let program = checked(vec![
        integer_choice(1, 1),
        node(Node::Control {
            control: Control::Sequence {
                body: body(
                    2,
                    vec![integer_choice(2, 2), draw_value(1, signed_binding(2))],
                ),
            },
        }),
        draw_value(2, signed_binding(1)),
    ]);
    game.execute_internal(&program, &source("synthetic"), context(1))
        .unwrap();
    let first = game.internal_decision(View::P1).unwrap();
    let before = exact_authority(&game);
    game.answer_internal(PlayerId { seat: 1 }, first.token, Answer::Integer(Nat(3)))
        .unwrap_err();
    assert_eq!(exact_authority(&game), before);
    game.answer_internal(PlayerId { seat: 1 }, first.token, Answer::Integer(Nat(1)))
        .unwrap();
    let second = game.internal_decision(View::P1).unwrap();
    let result = game
        .answer_internal(PlayerId { seat: 1 }, second.token, Answer::Integer(Nat(2)))
        .unwrap();
    cards(&result, 1, 2, 2);
    cards(&result, 2, 1, 1);
}

#[test]
fn runtime_expression_failure_rolls_back_draw_events_random_and_all_authority_fields() {
    let overflow = Expr::Compute(Box::new(IntOperation::Arithmetic {
        operation: Arithmetic::Add,
        arguments: vec![Expr::Literal(Int(i64::MAX)), Expr::Literal(Int(1))],
    }));
    for with_pause in [false, true] {
        let mut game = game(1, 3);
        let mut steps = vec![draw(1, 1)];
        if with_pause {
            steps.push(integer_choice(1, 1));
        }
        steps.push(draw_value(2, overflow.clone()));
        let origin = game.clone();
        let before = exact_authority(&game);
        let result = game.execute_internal(&checked(steps), &source("synthetic"), context(1));
        if with_pause {
            result.unwrap();
            let question = game.internal_decision(View::P1).unwrap();
            game.answer_internal(
                PlayerId { seat: 1 },
                question.token,
                Answer::Integer(Nat(1)),
            )
            .unwrap_err();
            transactions::assert_restored(&game, &origin);
        } else {
            result.unwrap_err();
            assert_eq!(exact_authority(&game), before);
        }
    }
}

#[test]
fn quantity_receipts_feed_if_and_later_draws() {
    let actual = Expr::Frozen(Box::new(NatFrozen::ReceiptActual {
        receipt: MeasuredReceipt::Cards {
            receipt: ReceiptReference::new(ReceiptId(1)),
        },
    }));
    let condition = Expr::Compute(Box::new(Predicate::CompareNat {
        left: actual.clone(),
        comparison: Comparison::Equal,
        right: Expr::Literal(Nat(1)),
    }));
    let mut game = game(1, 3);
    let result = game
        .execute_internal(
            &checked(vec![
                draw(1, 1),
                node(Node::Control {
                    control: Control::If {
                        condition,
                        then_body: body(
                            2,
                            vec![draw_value(
                                2,
                                Expr::Compute(Box::new(IntOperation::FromNat { value: actual })),
                            )],
                        ),
                        else_body: Some(body(3, vec![draw(3, 0)])),
                    },
                }),
            ]),
            &source("synthetic"),
            context(1),
        )
        .unwrap();
    cards(&result, 2, 1, 1);
    assert!(!result.receipts.contains_key(&ReceiptId(3)));
}

#[test]
fn stale_source_wrong_controller_and_busy_entry_reject_before_mutation() {
    let mut game = game(1, 3);
    for context in [
        Context {
            generation: Generation(1),
            ..context(1)
        },
        context(2),
        Context {
            controller: PlayerId { seat: 3 },
            ..context(1)
        },
    ] {
        let before = exact_authority(&game);
        game.execute_internal(&checked(vec![draw(1, 1)]), &source("synthetic"), context)
            .unwrap_err();
        assert_eq!(exact_authority(&game), before);
    }
    game.execute_internal(
        &checked(vec![up_to(1, 2)]),
        &source("synthetic"),
        context(1),
    )
    .unwrap();
    let before = exact_authority(&game);
    game.execute_internal(&checked(vec![draw(1, 1)]), &source("synthetic"), context(1))
        .unwrap_err();
    assert_eq!(exact_authority(&game), before);
}

#[test]
fn typed_object_identities_survive_saves_and_are_never_reassigned() {
    let field = || RecipientSet::Objects {
        objects: zone(ZoneKind::Field, 99),
    };
    let mut game = game(1, 3);
    let first = game
        .execute_internal(
            &checked(vec![draw(1, 1), damage(2, field(), 1)]),
            &source("synthetic"),
            context(1),
        )
        .unwrap();
    let [Fact::Draw(drawn), Fact::Damage(hit)] = first.facts.as_slice() else {
        panic!("draw then damage facts");
    };
    let mut saved = serde_json::to_value(&game).unwrap();
    // A later prototype object, such as a token, may sort before every existing key.
    let mut created = saved["state"]["objects"]["f"].clone();
    created["id"] = json!("a");
    saved["state"]["objects"]["a"] = created;
    saved["state"]["players"]["P2"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!("a"));
    let mut game: Game = serde_json::from_value(saved).unwrap();
    let second = game
        .execute_internal(
            &checked(vec![damage(1, field(), 1)]),
            &source("synthetic"),
            context(1),
        )
        .unwrap();
    let [Fact::Damage(old), Fact::Damage(new)] = second.facts.as_slice() else {
        panic!("two damage facts");
    };
    assert_eq!(old.recipient, hit.recipient);
    let RuntimeDamageRecipient::Object { object: new } = new.recipient else {
        panic!("object recipient");
    };
    let RuntimeDamageRecipient::Object { object: old } = old.recipient else {
        panic!("object recipient");
    };
    assert!(new.id != drawn.object.id && new.id != old.id);
}

#[cfg(feature = "runner")]
#[test]
fn printed_bp01_065_and_bp01_006_fragments_use_the_generated_snapshot() {
    use std::env::var_os;
    use std::fs::read_to_string;
    use std::path::PathBuf;
    let snapshot = read_to_string(PathBuf::from(
        var_os("SVE_TEST_SNAPSHOT").expect("generate public card input first"),
    ))
    .unwrap();
    let registry = json!({"format":1,"kind":"keyword_registry","keywords":{}}).to_string();
    // The synthetic harness supplies no prototype abilities; the IR below covers fragments only.
    let fragments = json!({"format":1,"kind":"effect_set","cards":{
        "BP01-065":{"status":"complete","review":"synthetic fragment harness","abilities":[]},
        "BP01-006":{"status":"complete","review":"synthetic fragment harness","abilities":[]}
    }})
    .to_string();
    let catalog = Arc::new(
        Catalog::from_documents(
            &snapshot,
            &registry,
            &[("fragments.yaml".into(), fragments)],
        )
        .unwrap(),
    );
    for seat in [1, 2] {
        let actor = if seat == 1 { "P1" } else { "P2" };
        let opponent = if seat == 1 { "P2" } else { "P1" };
        let mut position = setup(
            seat,
            json!([{"id":"d0","card":"BP01-006"},{"id":"d1","card":"BP01-006"}]),
        );
        position["players"][actor]["zones"]["hand"][0]["card"] = json!("BP01-065");
        position["players"][opponent]["zones"]["field"][0]["card"] = json!("BP01-006");
        let mut game = Game::new(
            Arc::clone(&catalog),
            &position,
            &Value::Null,
            &Value::Null,
            "BP01-065-fragment",
        )
        .unwrap();
        let result = game
            .execute_internal(
                &checked(vec![draw(1, 2)]),
                &source("BP01-065"),
                context(seat),
            )
            .unwrap();
        cards(&result, 1, 2, 2);
        position["players"][actor]["zones"]["hand"][0]["card"] = json!("BP01-006");
        let mut game = Game::new(
            Arc::clone(&catalog),
            &position,
            &Value::Null,
            &Value::Null,
            "BP01-006-fragment",
        )
        .unwrap();
        let result = game
            .execute_internal(
                &checked(vec![damage(
                    1,
                    RecipientSet::Objects {
                        objects: zone(ZoneKind::Field, 99),
                    },
                    4,
                )]),
                &source("BP01-006"),
                context(seat),
            )
            .unwrap();
        assert_eq!(result.facts.len(), 1);
        assert_eq!(
            result.receipts.get(&ReceiptId(1)),
            Some(&InstructionOutcome::Damage(DamageQuantityOutcome {
                requested: Int(4),
                actual: Nat(4)
            }))
        );
    }
}

#[test]
fn admitted_arithmetic_natural_values_and_resource_reads_execute_both_seats() {
    for seat in [1, 2] {
        let mut expressions = Vec::new();
        for (operation, left, right) in [
            (Arithmetic::Add, 1, 1),
            (Arithmetic::Subtract, 5, 3),
            (Arithmetic::Multiply, 1, 2),
            (Arithmetic::Minimum, 2, 3),
            (Arithmetic::Maximum, 1, 2),
        ] {
            expressions.push((
                Expr::Compute(Box::new(IntOperation::Arithmetic {
                    operation,
                    arguments: vec![Expr::Literal(Int(left)), Expr::Literal(Int(right))],
                })),
                2,
            ));
        }
        for property in [
            PlayerResource::Life,
            PlayerResource::PpCurrent,
            PlayerResource::PpMaximum,
            PlayerResource::Ep,
            PlayerResource::Sep,
        ] {
            let expected = match property {
                PlayerResource::Life => 20,
                PlayerResource::Ep | PlayerResource::Sep => 0,
                PlayerResource::PpCurrent | PlayerResource::PpMaximum => 2,
            };
            expressions.push((
                Expr::Live(Box::new(IntRead::PlayerResource {
                    player: PlayerRef::Controller {},
                    property,
                })),
                expected,
            ));
        }
        for condition in [true, false] {
            expressions.push((
                Expr::Compute(Box::new(IntOperation::Conditional {
                    condition: Expr::Literal(Bool(condition)),
                    then_value: Expr::Literal(Int(2)),
                    else_value: Expr::Literal(Int(1)),
                })),
                if condition { 2 } else { 1 },
            ));
        }
        let naturals = [
            Expr::Compute(Box::new(NatOperation::CheckedInt {
                value: Expr::Literal(Int(1)),
            })),
            Expr::Compute(Box::new(NatOperation::Count {
                objects: zone(ZoneKind::Field, 99),
            })),
            Expr::Live(Box::new(NatRead::ZoneCount {
                selector: zone(ZoneKind::Field, 99),
            })),
            Expr::Compute(Box::new(NatOperation::Conditional {
                condition: Expr::Literal(Bool(true)),
                then_value: Expr::Literal(Nat(1)),
                else_value: Expr::Literal(Nat(2)),
            })),
            Expr::Compute(Box::new(NatOperation::Conditional {
                condition: Expr::Literal(Bool(false)),
                then_value: Expr::Literal(Nat(2)),
                else_value: Expr::Literal(Nat(1)),
            })),
        ];
        for value in naturals {
            expressions.push((Expr::Compute(Box::new(IntOperation::FromNat { value })), 1));
        }
        for (value, expected) in expressions {
            let mut game = game(seat, 25);
            let result = game
                .execute_internal(
                    &checked(vec![draw_value(1, value)]),
                    &source("synthetic"),
                    context(seat),
                )
                .unwrap();
            cards(&result, 1, expected, u64::try_from(expected).unwrap());
        }
    }
}

#[test]
fn admitted_predicates_choose_the_correct_arm_both_seats() {
    for seat in [1, 2] {
        let mut predicates = vec![
            Expr::Compute(Box::new(Predicate::And {
                predicates: vec![Expr::Literal(Bool(true)), Expr::Literal(Bool(true))],
            })),
            Expr::Compute(Box::new(Predicate::Or {
                predicates: vec![Expr::Literal(Bool(false)), Expr::Literal(Bool(true))],
            })),
            Expr::Compute(Box::new(Predicate::Not {
                predicate: Expr::Literal(Bool(false)),
            })),
            Expr::Compute(Box::new(Predicate::Exists {
                objects: zone(ZoneKind::Field, 99),
            })),
        ];
        for (comparison, left, right) in [
            (Comparison::Equal, 2, 2),
            (Comparison::NotEqual, 2, 1),
            (Comparison::Less, 1, 2),
            (Comparison::LessEqual, 2, 2),
            (Comparison::Greater, 2, 1),
            (Comparison::GreaterEqual, 2, 2),
        ] {
            predicates.push(Expr::Compute(Box::new(Predicate::CompareInt {
                left: Expr::Literal(Int(left)),
                comparison,
                right: Expr::Literal(Int(right)),
            })));
            predicates.push(Expr::Compute(Box::new(Predicate::CompareNat {
                left: Expr::Literal(Nat(u64::try_from(left).unwrap())),
                comparison,
                right: Expr::Literal(Nat(u64::try_from(right).unwrap())),
            })));
        }
        for predicate in predicates {
            let mut game = game(seat, 3);
            let result = game
                .execute_internal(
                    &checked(vec![node(Node::Control {
                        control: Control::If {
                            condition: predicate,
                            then_body: body(2, vec![draw(1, 1)]),
                            else_body: Some(body(3, vec![draw(2, 2)])),
                        },
                    })]),
                    &source("synthetic"),
                    context(seat),
                )
                .unwrap();
            cards(&result, 1, 1, 1);
            assert!(!result.receipts.contains_key(&ReceiptId(2)));
        }
    }
}

#[test]
fn natural_boolean_and_object_captures_preserve_typed_values_both_seats() {
    for seat in [1, 2] {
        let natural = Binding::new(1, ScopeId(1));
        let boolean = Binding::new(2, ScopeId(1));
        let objects = Binding::new(3, ScopeId(1));
        let mut game = game(seat, 3);
        let steps = vec![
            node(Node::Capture {
                capture: Capture::Natural {
                    at: CaptureTime::AtResolution { scope: ScopeId(1) },
                    value: Expr::Literal(Nat(2)),
                    into: CaptureRef { binding: natural },
                },
            }),
            node(Node::Capture {
                capture: Capture::Boolean {
                    at: CaptureTime::AtResolution { scope: ScopeId(1) },
                    value: Expr::Literal(Bool(true)),
                    into: CaptureRef { binding: boolean },
                },
            }),
            node(Node::Capture {
                capture: Capture::Objects {
                    at: CaptureTime::AtResolution { scope: ScopeId(1) },
                    value: zone(ZoneKind::Field, 99),
                    into: CaptureRef { binding: objects },
                },
            }),
            node(Node::Control {
                control: Control::If {
                    condition: Expr::Captured(CaptureRef { binding: boolean }),
                    then_body: body(
                        2,
                        vec![draw_value(
                            1,
                            Expr::Compute(Box::new(IntOperation::FromNat {
                                value: Expr::Captured(CaptureRef { binding: natural }),
                            })),
                        )],
                    ),
                    else_body: None,
                },
            }),
            damage(
                2,
                RecipientSet::Objects {
                    objects: Selector::Captured {
                        reference: CaptureRef { binding: objects },
                    },
                },
                2,
            ),
        ];
        let result = game
            .execute_internal(&checked(steps), &source("synthetic"), context(seat))
            .unwrap();
        cards(&result, 1, 2, 2);
        assert_eq!(result.facts.len(), 3);
        assert_eq!(
            result.receipts.get(&ReceiptId(2)),
            Some(&InstructionOutcome::Damage(DamageQuantityOutcome {
                requested: Int(2),
                actual: Nat(2)
            }))
        );
    }
}

#[test]
fn owner_active_singleton_and_all_leaders_resolve_their_actual_players() {
    for seat in [1, 2] {
        for player in [
            PlayerRef::Owner {},
            PlayerRef::Active {},
            PlayerRef::RequireOne {
                players: Box::new(PlayerSet {
                    cardinality: Cardinality {
                        minimum: Nat(1),
                        maximum: Some(Nat(1)),
                    },
                    ..players(PlayerRelation::Opponents)
                }),
            },
        ] {
            let actor = if seat == 1 { "P1" } else { "P2" };
            let other = if seat == 1 { "P2" } else { "P1" };
            let opponent = if seat == 1 { 2 } else { 1 };
            let mut position = setup(seat, json!([{"id":"d","card":"unit-follower"}]));
            position["players"][other]["zones"]["deck"] =
                json!([{"id":"other","card":"unit-follower"}]);
            position["players"][actor]["zones"]["hand"][0]["owner"] = json!(other);
            let expected = if matches!(player, PlayerRef::Active {}) {
                1
            } else {
                opponent
            };
            let mut step = instruction(
                1,
                Action::Draw {
                    player: player.clone(),
                    count: Expr::Literal(Int(1)),
                    mode: StepwiseMode::Exact,
                },
            );
            if let Node::Instruction { instruction } = &mut step.node {
                instruction.actor = player;
            }
            let mut game =
                Game::new(catalog(), &position, &Value::Null, &Value::Null, "roles").unwrap();
            let result = game
                .execute_internal(&checked(vec![step]), &source("synthetic"), context(seat))
                .unwrap();
            let [Fact::Draw(event)] = result.facts.as_slice() else {
                panic!("one successful draw");
            };
            assert_eq!(event.player, PlayerId { seat: expected });
        }
        let mut game = game(seat, 1);
        let result = game
            .execute_internal(
                &checked(vec![damage(
                    1,
                    RecipientSet::Leaders {
                        players: players(PlayerRelation::All),
                    },
                    1,
                )]),
                &source("synthetic"),
                context(seat),
            )
            .unwrap();
        assert_eq!(result.facts.len(), 2);
    }
}

#[test]
fn public_ex_banish_selectors_and_cards_requested_projection_execute() {
    for seat in [1, 2] {
        let mut position = setup(
            seat,
            json!([{"id":"d0","card":"unit-follower"},{"id":"d1","card":"unit-follower"},{"id":"d2","card":"unit-follower"}]),
        );
        position["players"]["P1"]["zones"]["ex"] = json!([{"id":"x","card":"unit-follower"}]);
        position["players"]["P2"]["zones"]["banish"] = json!([{"id":"b","card":"unit-follower"}]);
        let mut game = Game::new(
            catalog(),
            &position,
            &Value::Null,
            &Value::Null,
            "public-zones",
        )
        .unwrap();
        let steps = vec![
            draw_value(
                1,
                Expr::Compute(Box::new(IntOperation::FromNat {
                    value: Expr::Compute(Box::new(NatOperation::Count {
                        objects: zone(ZoneKind::Ex, 98),
                    })),
                })),
            ),
            draw_value(
                2,
                Expr::Compute(Box::new(IntOperation::FromNat {
                    value: Expr::Live(Box::new(NatRead::ZoneCount {
                        selector: zone(ZoneKind::Banish, 99),
                    })),
                })),
            ),
            draw_value(
                3,
                Expr::Frozen(Box::new(IntFrozen::ReceiptRequested {
                    receipt: MeasuredReceipt::Cards {
                        receipt: ReceiptReference::new(ReceiptId(1)),
                    },
                })),
            ),
        ];
        let result = game
            .execute_internal(&checked(steps), &source("synthetic"), context(seat))
            .unwrap();
        for id in [1, 2, 3] {
            cards(&result, id, 1, 1);
        }
    }
}

#[test]
fn explicit_object_and_leader_recipients_deduplicate_source_generations() {
    for seat in [1, 2] {
        let actor = if seat == 1 { "P1" } else { "P2" };
        let mut position = setup(seat, json!([]));
        position["players"][actor]["zones"]["hand"] = json!([]);
        position["players"][actor]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
        let recipients = RecipientSet::Union {
            sets: vec![
                RecipientSet::Objects {
                    objects: Selector::Objects {
                        objects: vec![
                            ObjectOperand::SourceHolder {},
                            ObjectOperand::SourceHolder {},
                        ],
                    },
                },
                RecipientSet::Explicit {
                    recipients: vec![
                        DamageRecipient::Object {
                            object: ObjectOperand::SourceHolder {},
                        },
                        DamageRecipient::Leader {
                            player: PlayerRef::Controller {},
                        },
                    ],
                },
            ],
        };
        let mut game =
            Game::new(catalog(), &position, &Value::Null, &Value::Null, "explicit").unwrap();
        let result = game
            .execute_internal(
                &checked(vec![damage(1, recipients, 1)]),
                &source("synthetic"),
                context(seat),
            )
            .unwrap();
        assert_eq!(result.facts.len(), 2);
        assert_eq!(
            result.receipts.get(&ReceiptId(1)),
            Some(&InstructionOutcome::Damage(DamageQuantityOutcome {
                requested: Int(2),
                actual: Nat(2)
            }))
        );
    }
}

#[path = "internal_ir/movement.rs"]
mod movement;
