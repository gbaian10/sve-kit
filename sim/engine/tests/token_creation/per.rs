#![expect(
    clippy::indexing_slicing,
    reason = "Synthetic fixture coordinates are fixed; a missing field must fail the test."
)]

use super::*;
use sve_engine::EngineFailure;
use sve_engine::game::internal::{Context, Progress};
use sve_engine::ir::lowering::ExpandedLowerer;
use sve_engine::ir::{
    Action, Arithmetic, Binding, Bool, Cardinality, Comparison, Control, Expr, Generation, Int,
    IntOperation, Nat, NatOperation, NatRead, Node, ObjectRef, PlayerRef, PlayerRelation,
    PlayerSet, Predicate, Program, ScopeId, SelectionView, Selector, TextQuantifier,
    WellFormedProgram, ZoneKind,
};

fn input(count: Value) -> (String, String, Value) {
    let (snapshot, keywords, mut input) = selection(1);
    input["authors"][0]["input"]["tree"]["cards"]["creator"]["faces"]["f:11111111111111111111111111111111"]
        ["abilities"][0]["spell"]["do"][0]["create"]["n"] = count;
    (snapshot, keywords, input)
}

fn position(graves: usize, occupied: usize) -> Value {
    let mut position = initial(occupied);
    position["players"]["P1"]["zones"]["cemetery"] = json!(
        (0..graves)
            .map(|index| json!({"id":format!("grave-{index}"),"card":"creator"}))
            .collect::<Vec<_>>()
    );
    position
}

fn author_game(count: Value, graves: usize, occupied: usize) -> sve_engine::Result<Game> {
    let (snapshot, keywords, input) = input(count);
    let selected = Catalog::from_current_selection(&snapshot, &keywords, &input)?;
    Game::from_selected_effects(
        selected,
        &position(graves, occupied),
        &Value::Null,
        &Value::Null,
        "per-count",
    )
}

#[expect(
    clippy::panic,
    reason = "The synthetic single-template creation must resolve."
)]
fn created(game: &mut Game) -> usize {
    let CardPlay::Resolved(result) = game
        .play_internal_card("creator", PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("one token template has no capacity choice");
    };
    result
        .progress
        .receipts
        .values()
        .filter_map(|outcome| {
            if let InstructionOutcome::Token(outcome) = outcome {
                Some(outcome.created.len())
            } else {
                None
            }
        })
        .sum()
}

fn count_query() -> Value {
    json!({"count":{"who":"self","zone":"cemetery"}})
}

#[test]
fn bp15_pr15_quantity_per_keeps_unit_boundaries_and_capacity() {
    // BP15-PR15 supplies the five-card unit; this fixture isolates its quantity.
    for (graves, expected) in [(0, 0), (1, 0), (4, 0), (5, 1), (6, 1), (10, 2)] {
        let mut game =
            author_game(json!({"per":{"of":count_query(),"unit":5_u8}}), graves, 0).unwrap();
        assert_eq!(created(&mut game), expected);
    }
    let mut unit_one = author_game(json!({"per":{"of":count_query(),"unit":1_u8}}), 3, 0).unwrap();
    assert_eq!(created(&mut unit_one), 3);
    let mut limited = author_game(json!({"per":{"of":count_query(),"unit":5_u8}}), 15, 4).unwrap();
    assert_eq!(created(&mut limited), 1);
    assert_eq!(
        limited.projection(View::Referee).unwrap()["P1"]["ex"]
            .as_array()
            .unwrap()
            .len(),
        5
    );
}

#[test]
fn public_per_reads_after_the_preceding_instruction_moves_a_card() {
    let (snapshot, keywords, mut input) = input(json!({"per":{"of":count_query(),"unit":2_u8}}));
    let (material, definition) = author(
        "material",
        "33333333333333333333333333333333",
        "フォロワー",
        vec![],
    );
    let snapshot = format!("{snapshot}{material}\n");
    input["authors"].as_array_mut().unwrap().push(definition);
    let steps = input["authors"][0]["input"]["tree"]["cards"]["creator"]["faces"]
        ["f:11111111111111111111111111111111"]["abilities"][0]["spell"]["do"]
        .as_array_mut().unwrap();
    steps.insert(
        0,
        json!({"destroy":{"ref":{"who":"self","zone":"field","kind":"follower"}}}),
    );
    let mut position = position(1, 0);
    position["players"]["P1"]["zones"]["field"] = json!([{"id":"material","card":"material"}]);
    let selected = Catalog::from_current_selection(&snapshot, &keywords, &input).unwrap();
    let mut game =
        Game::from_selected_effects(selected, &position, &Value::Null, &Value::Null, "live-per")
            .unwrap();
    assert_eq!(
        created(&mut game),
        1,
        "resolution sees the newly arrived cemetery card"
    );
}

#[test]
fn public_per_retains_the_existing_public_filter_policy() {
    let filtered = json!({"count":{"who":"self","zone":"cemetery","kind":"spell"}});
    for count in [filtered.clone(), json!({"per":{"of":filtered,"unit":3_u8}})] {
        assert!(
            matches!(author_game(count, 3, 0), Err(EngineFailure::Unsupported(_))),
            "a formula does not grant a new identity-reading selector spine"
        );
    }
}

#[test]
fn author_arithmetic_preserves_nonnegative_compositions() {
    for (count, expected) in [
        (json!({"plus":[count_query(),1_u8]}), 3),
        (json!({"times":[count_query(),2_u8]}), 4),
        (json!({"minus":[count_query(),0_u8]}), 2),
        (json!({"minus":[3_u8,{"min":[count_query(),3_u8]}]}), 1),
        (json!({"half_up":count_query()}), 1),
        (json!({"min":[{"plus":[count_query(),1_u8]},2_u8]}), 2),
    ] {
        let mut game = author_game(count, 2, 0).unwrap();
        assert_eq!(created(&mut game), expected);
    }
}

#[test]
fn unproved_negative_and_constant_overflow_counts_fail_at_load_with_location() {
    let expected = raw_body().steps[0].source.clone();
    for count in [
        json!({"minus":[count_query(),3_u8]}),
        json!({"times":[4_000_000_000_u64,4_000_000_000_u64]}),
    ] {
        let error = author_game(count, 4, 0).unwrap_err();
        assert!(
            matches!(&error, EngineFailure::Unsupported(reason)
            if reason.contains(&expected.card_no) && reason.contains(&expected.author_path)),
            "the rejection must retain the instruction's source: {error}"
        );
    }
}

#[test]
fn number_fallback_keeps_other_leaf_sources_closed() {
    for count in [
        json!({"plus":[{"count":{"who":"opp","zone":"hand"}},1_u8]}),
        json!({"plus":[{"resource":{"who":"self","kind":"pp"}},1_u8]}),
        json!({"plus":[{"turn_number":{"who":"self"}},1_u8]}),
    ] {
        author_game(count, 2, 0).unwrap_err();
    }
}

fn raw_body() -> Program {
    let (_, _, input) = input(json!(1_u8));
    ExpandedLowerer::new()
        .unwrap()
        .lower_current(&input["authors"][0]["input"])
        .unwrap()
        .program
        .card()
        .abilities[0]
        .node
        .body
        .as_ref()
        .unwrap()
        .clone()
}

#[expect(
    clippy::panic,
    reason = "The fixture must retain its token instruction."
)]
fn token_count(program: &mut Program) -> &mut Expr<Nat> {
    let Node::Instruction { instruction } = &mut program.steps[0].node else {
        panic!("token instruction");
    };
    let Action::Token { tokens, .. } = &mut instruction.action else {
        panic!("token action");
    };
    &mut tokens[0].count
}

fn raw_game(graves: usize) -> Game {
    raw_game_at(&position(graves, 0))
}

fn raw_game_at(position: &Value) -> Game {
    let (snapshot, keywords, mut input) = input(json!(1_u8));
    let (_, empty) = author(
        "creator",
        "11111111111111111111111111111111",
        "スペル",
        vec![],
    );
    input["authors"][0] = empty;
    let selected = Catalog::from_current_selection(&snapshot, &keywords, &input).unwrap();
    Game::from_selected_effects(selected, position, &Value::Null, &Value::Null, "raw-per").unwrap()
}

fn context() -> Context {
    Context {
        source: "creator".into(),
        generation: Generation(0),
        controller: PlayerId { seat: 1 },
    }
}

fn cemetery(scope: ScopeId, identity: u64) -> Expr<Nat> {
    Expr::Live(Box::new(NatRead::ZoneCount {
        selector: Selector::Zone {
            players: PlayerSet {
                relation: PlayerRelation::SelfPlayer,
                relative_to: PlayerRef::Controller {},
                cardinality: Cardinality {
                    minimum: Nat(1),
                    maximum: Some(Nat(1)),
                },
                text_quantifier: TextQuantifier::Implicit,
            },
            zone: ZoneKind::Cemetery,
            view: SelectionView::Current {},
            binding: Binding::<ObjectRef>::new(identity, scope),
            predicate: Box::new(Expr::Literal(Bool(true))),
        },
    }))
}

fn signed(value: Expr<Int>) -> Expr<Nat> {
    Expr::Compute(Box::new(NatOperation::CheckedInt { value }))
}

fn integer(value: Expr<Nat>) -> Expr<Int> {
    Expr::Compute(Box::new(IntOperation::FromNat { value }))
}

fn arithmetic(operation: Arithmetic, left: Expr<Int>, right: Expr<Int>) -> Expr<Int> {
    Expr::Compute(Box::new(IntOperation::Arithmetic {
        operation,
        arguments: vec![left, right],
    }))
}

const fn literal(value: i64) -> Expr<Int> {
    Expr::Literal(Int(value))
}

fn execute(game: &mut Game, program: Program) -> sve_engine::Result<Progress> {
    let source = program.steps[0].source.clone();
    let checked = WellFormedProgram::check(program)
        .map_err(|error| EngineFailure::Invalid(error.to_string()))?;
    game.execute_internal(&checked, &source, context())
}

#[test]
fn raw_conditional_and_maximum_keep_negative_intermediate_values_signed() {
    for (graves, expected) in [(0, 0), (1, 0), (3, 0), (4, 1)] {
        let mut program = raw_body();
        let count = integer(cemetery(program.scope, 100));
        *token_count(&mut program) = signed(arithmetic(
            Arithmetic::Maximum,
            arithmetic(Arithmetic::Subtract, count, literal(3)),
            literal(0),
        ));
        let mut game = raw_game(graves);
        assert!(execute(&mut game, program).unwrap().complete);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["ex"]
                .as_array()
                .unwrap()
                .len(),
            expected
        );
    }
}

#[test]
fn every_checked_int_is_proved_even_in_conditions_and_unselected_branches() {
    for hidden_in_condition in [false, true] {
        let mut program = raw_body();
        let bad = signed(arithmetic(
            Arithmetic::Subtract,
            integer(cemetery(program.scope, 100)),
            literal(3),
        ));
        *token_count(&mut program) = if hidden_in_condition {
            Expr::Compute(Box::new(NatOperation::Conditional {
                condition: Expr::Compute(Box::new(Predicate::CompareNat {
                    left: bad,
                    comparison: Comparison::Equal,
                    right: Expr::Literal(Nat(0)),
                })),
                then_value: Expr::Literal(Nat(1)),
                else_value: Expr::Literal(Nat(2)),
            }))
        } else {
            signed(arithmetic(Arithmetic::Add, integer(bad), literal(5)))
        };
        let mut game = raw_game(4);
        let before = format!("{game:?}");
        assert!(matches!(
            execute(&mut game, program),
            Err(EngineFailure::Unsupported(_))
        ));
        assert_eq!(format!("{game:?}"), before);
    }
}

#[test]
fn lazy_conditionals_skip_dynamic_errors_but_boolean_and_or_are_eager() {
    for eager in [None, Some(false), Some(true)] {
        let mut program = raw_body();
        let overflow = arithmetic(
            Arithmetic::Add,
            integer(cemetery(program.scope, 100)),
            literal(i64::MAX),
        );
        let condition = match eager {
            None => Expr::Literal(Bool(true)),
            Some(shortcut) => {
                let predicates = vec![
                    Expr::Literal(Bool(shortcut)),
                    Expr::Compute(Box::new(Predicate::CompareInt {
                        left: arithmetic(
                            Arithmetic::Add,
                            integer(cemetery(program.scope, 101)),
                            literal(i64::MAX),
                        ),
                        comparison: Comparison::Equal,
                        right: literal(0),
                    })),
                ];
                Expr::Compute(Box::new(if shortcut {
                    Predicate::Or { predicates }
                } else {
                    Predicate::And { predicates }
                }))
            }
        };
        *token_count(&mut program) = Expr::Compute(Box::new(NatOperation::Conditional {
            condition,
            then_value: Expr::Literal(Nat(1)),
            else_value: signed(overflow),
        }));
        let mut game = raw_game(1);
        let before = format!("{game:?}");
        let result = execute(&mut game, program);
        if eager.is_none() {
            assert!(result.unwrap().complete);
            assert_eq!(
                game.projection(View::Referee).unwrap()["P1"]["ex"]
                    .as_array()
                    .unwrap()
                    .len(),
                1
            );
        } else {
            assert!(matches!(result, Err(EngineFailure::Invalid(_))));
            assert_eq!(format!("{game:?}"), before);
        }
    }
}

#[test]
fn each_player_checks_before_leaves_inside_a_condition() {
    let mut program = dynamic::raw_body();
    let Node::Control {
        control: Control::EachPlayer { stages, .. },
    } = &mut program.steps[1].node
    else {
        panic!("each player");
    };
    let count = token_count(&mut stages[0].body);
    let Expr::Compute(operation) = count else {
        panic!("Before count");
    };
    let NatOperation::Count {
        objects: Selector::Filter { predicate, .. },
    } = operation.as_mut()
    else {
        panic!("Before filter");
    };
    let Expr::Compute(predicate) = predicate.as_mut() else {
        panic!("controller predicate");
    };
    let Predicate::PlayerEquals { right, .. } = predicate.as_mut() else {
        panic!("player comparison");
    };
    *right = Expr::Literal(PlayerRef::Controller {});
    let changed = count.clone();
    *count = Expr::Compute(Box::new(NatOperation::Conditional {
        condition: Expr::Compute(Box::new(Predicate::CompareNat {
            left: changed,
            comparison: Comparison::Equal,
            right: Expr::Literal(Nat(0)),
        })),
        then_value: Expr::Literal(Nat(1)),
        else_value: Expr::Literal(Nat(0)),
    }));
    let mut game = raw_game_at(&destroyed_position());
    let source = program.steps[0].source.clone();
    let checked = WellFormedProgram::check(program).unwrap();
    assert!(matches!(
        game.prepare_internal_play(&checked, &source, context()),
        Err(EngineFailure::Unsupported(_))
    ));
}

fn destroyed_position() -> Value {
    let mut position = position(0, 0);
    position["players"]["P1"]["zones"]["field"] =
        json!([{"id":"mine","card":"token","owner":"P2"}]);
    position["players"]["P2"]["zones"]["field"] = json!([
        {"id":"theirs-1","card":"token","owner":"P1"},
        {"id":"theirs-2","card":"token","owner":"P1"}
    ]);
    position
}

#[test]
fn each_player_composes_before_counts_without_counting_current_owners() {
    let mut program = dynamic::raw_body();
    let Node::Control {
        control: Control::EachPlayer { stages, .. },
    } = &mut program.steps[1].node
    else {
        panic!("each player");
    };
    let count = token_count(&mut stages[0].body);
    *count = signed(arithmetic(
        Arithmetic::Add,
        integer(count.clone()),
        literal(1),
    ));
    let mut game = dynamic::protected_game(program);
    assert!(matches!(
        game.play_internal_card("creator", PlayerId { seat: 1 })
            .unwrap(),
        CardPlay::Resolved(_)
    ));
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["field"].as_array().unwrap().len(), 2);
    assert_eq!(view["P2"]["field"].as_array().unwrap().len(), 3);
}

#[test]
fn full_u64_naturals_remain_admitted_but_signed_constant_errors_do_not() {
    let mut program = raw_body();
    *token_count(&mut program) = Expr::Literal(Nat(u64::MAX));
    let source = program.steps[0].source.clone();
    let checked = WellFormedProgram::check(program).unwrap();
    raw_game(0)
        .prepare_internal_play(&checked, &source, context())
        .unwrap();

    for expression in [
        signed(integer(Expr::Literal(Nat(u64::MAX)))),
        signed(arithmetic(
            Arithmetic::Multiply,
            literal(i64::MAX),
            literal(2),
        )),
    ] {
        let mut rejected = raw_body();
        *token_count(&mut rejected) = expression;
        let mut game = raw_game(0);
        assert!(matches!(
            execute(&mut game, rejected),
            Err(EngineFailure::Unsupported(_))
        ));
    }
}

#[test]
fn signed_multiplication_bounds_admit_two_nonpositive_operands() {
    let mut program = raw_body();
    let left = arithmetic(
        Arithmetic::Subtract,
        literal(0),
        integer(cemetery(program.scope, 100)),
    );
    let right = arithmetic(
        Arithmetic::Subtract,
        literal(0),
        integer(cemetery(program.scope, 101)),
    );
    *token_count(&mut program) = signed(arithmetic(Arithmetic::Multiply, left, right));
    let mut game = raw_game(2);
    assert!(execute(&mut game, program).unwrap().complete);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["ex"]
            .as_array()
            .unwrap()
            .len(),
        4
    );

    let mut rejected = raw_body();
    let negative = arithmetic(
        Arithmetic::Subtract,
        literal(0),
        integer(cemetery(rejected.scope, 100)),
    );
    *token_count(&mut rejected) = signed(arithmetic(Arithmetic::Multiply, negative, literal(2)));
    assert!(matches!(
        execute(&mut raw_game(0), rejected),
        Err(EngineFailure::Unsupported(_))
    ));
}

#[test]
fn integer_conditionals_and_boolean_negation_select_the_expected_count() {
    for (graves, expected) in [(0, 2), (1, 1)] {
        let mut program = raw_body();
        let condition = Expr::Compute(Box::new(Predicate::Not {
            predicate: Expr::Compute(Box::new(Predicate::CompareNat {
                left: cemetery(program.scope, 100),
                comparison: Comparison::Equal,
                right: Expr::Literal(Nat(0)),
            })),
        }));
        *token_count(&mut program) = signed(Expr::Compute(Box::new(IntOperation::Conditional {
            condition,
            then_value: literal(1),
            else_value: literal(2),
        })));
        let mut game = raw_game(graves);
        assert!(execute(&mut game, program).unwrap().complete);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["ex"]
                .as_array()
                .unwrap()
                .len(),
            expected
        );
    }
}

#[test]
fn constant_conditionals_cannot_hide_a_signed_conversion_overflow() {
    let mut program = raw_body();
    let selected = Expr::Compute(Box::new(NatOperation::Conditional {
        condition: Expr::Literal(Bool(true)),
        then_value: Expr::Literal(Nat(u64::MAX)),
        else_value: Expr::Literal(Nat(0)),
    }));
    *token_count(&mut program) = signed(integer(selected));
    let source = program.steps[0].source.clone();
    let checked = WellFormedProgram::check(program).unwrap();
    assert!(matches!(
        raw_game(0).prepare_internal_play(&checked, &source, context()),
        Err(EngineFailure::Unsupported(_))
    ));
}

#[test]
fn arithmetic_does_not_hide_an_unproved_conditional_branch() {
    let mut program = raw_body();
    let selected = Expr::Compute(Box::new(IntOperation::Conditional {
        condition: Expr::Literal(Bool(true)),
        then_value: literal(0),
        else_value: literal(-1),
    }));
    *token_count(&mut program) = signed(arithmetic(Arithmetic::Add, selected, literal(0)));
    let source = program.steps[0].source.clone();
    let checked = WellFormedProgram::check(program).unwrap();
    assert!(matches!(
        raw_game(0).prepare_internal_play(&checked, &source, context()),
        Err(EngineFailure::Unsupported(_))
    ));
}
