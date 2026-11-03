use super::*;
use sve_engine::game::internal::InputDomain;

fn tree(cost: &Value, amount: &Value) -> Value {
    let mut tree = fanfare::expanded::tree().clone();
    let mut seed = tree["cards"]["BP01-041"].clone();
    seed["abilities"][0]["activate"] =
        json!({"cost":cost,"do":[{"damage":{"to":{"leaders":"opponents"},"n":amount}}]});
    tree["cards"] = json!({"unit-follower":seed});
    tree
}

fn read() -> Value {
    json!({"cost_of":{"objects_of":"fee","view":"before"}})
}

fn fee() -> Value {
    json!({"discard":{"sel":{"who":"self","zone":"hand"},"n":1,"result":"fee"}})
}

#[test]
fn discarded_cost_reads_the_selected_before_cost_after_movement_for_both_seats() {
    let program = lowerer().lower(&tree(&fee(), &read())).unwrap()["unit-follower"].clone();
    for seat in [1, 2] {
        let mut initial = setup(seat, json!([]));
        let actor = if seat == 1 { "P1" } else { "P2" };
        let opponent = if seat == 1 { "P2" } else { "P1" };
        initial["turn"]["active"] = json!(actor);
        initial["players"][actor]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
        initial["players"][actor]["zones"]["hand"] =
            json!([{"id":"fuel","card":"unit-spell","state":{"cost":9}}]);
        let mut board = Game::new(
            catalog(),
            &initial,
            &Value::Null,
            &Value::Null,
            "before-cost",
        )
        .unwrap();
        board.load_internal_card("unit-follower", &program).unwrap();
        board
            .load_internal_card("unit-spell", &card(vec![]))
            .unwrap();
        let CardPlay::Awaiting(question) = board
            .activate_internal_card("s", AbilityId(1), PlayerId { seat })
            .unwrap()
        else {
            panic!("private cost")
        };
        assert!(
            matches!(question.domain, InputDomain::CostMaterials(_)),
            "payment asks privately"
        );
        assert!(
            board
                .internal_decision(if seat == 1 { View::P2 } else { View::P1 })
                .is_none(),
            "opponent cannot inspect hand candidates"
        );
        resolved(
            board
                .answer_internal_card(
                    question.actor,
                    question.token,
                    Answer::CostMaterials(vec![0]),
                )
                .unwrap(),
        );
        let state = board.projection(View::Referee).unwrap();
        assert_eq!(
            state[actor]["cemetery"],
            json!(["fuel"]),
            "the material has actually moved"
        );
        assert_eq!(
            state[opponent]["leader"]["life"], 11,
            "damage uses cost nine from hand rather than printed one in cemetery"
        );
    }
}

#[test]
fn discarded_cost_rejects_ambiguous_missing_and_future_receipts() {
    for cost in [
        json!({"discard":{"sel":{"who":"self","zone":"hand"},"n":2,"result":"fee"}}),
        json!({"discard":{"sel":{"who":"self","zone":"hand"},"upto":1,"result":"fee"}}),
        json!({"discard":{"sel":{"who":"self","zone":"hand"},"any":true,"result":"fee"}}),
        json!({"pp":1}),
    ] {
        assert!(
            lowerer().lower(&tree(&cost, &read())).is_err(),
            "only an exact one-card discard supplies a cost"
        );
    }
    for amount in [
        json!({"cost_of":{"objects_of":"fee","view":"after"}}),
        json!({"cost_of":{"objects_of":"unknown","view":"before"}}),
    ] {
        assert!(
            lowerer().lower(&tree(&fee(), &amount)).is_err(),
            "after and unknown reads stay unsupported"
        );
    }
    let mut optional = tree(&fee(), &read());
    let ability = optional["cards"]["unit-follower"]["abilities"][0]
        .as_object_mut()
        .unwrap();
    let mut body = ability.remove("activate").unwrap();
    body.as_object_mut().unwrap().remove("cost");
    body["optional_cost"] = json!({"discard":{"sel":{"who":"self","zone":"hand"},"n":1,"result":"fee"},"result":"paid"});
    ability.insert("fanfare".into(), body);
    assert!(
        lowerer().lower(&optional).is_err(),
        "an optional unpaid branch cannot expose a scalar receipt"
    );
    let mut early = tree(&fee(), &read());
    early["cards"]["unit-follower"]["abilities"][0]["activate"]["targets"] = json!([{"bind":"enemy","from":{"who":"opp","zone":"field","kind":"follower","cost_le":read()},"n":1}]);
    assert!(
        lowerer().lower(&early).is_err(),
        "formal targets cannot inspect future payments"
    );
}

#[test]
fn typed_discarded_cost_cannot_bypass_exact_cardinality_or_dominance() {
    let original = lowerer().lower(&tree(&fee(), &read())).unwrap()["unit-follower"]
        .card()
        .clone();
    for minimum in [0, 2] {
        let mut program = original.clone();
        let CostSpec::Ordered { steps } =
            &mut program.abilities[0].node.play.as_mut().unwrap().costs
        else {
            panic!("ordered cost")
        };
        let Action::Discard {
            mode: DiscardMode::Chosen { constraint },
            ..
        } = &mut steps[0].instruction.action
        else {
            panic!("discard")
        };
        constraint.cardinality.minimum = Nat(minimum);
        constraint.cardinality.maximum = Some(Nat(if minimum == 0 { 1 } else { 2 }));
        constraint.groups[0].cardinality = constraint.cardinality;
        assert!(
            WellFormedCardProgram::check(program).is_err(),
            "typed reads require one guaranteed member"
        );
    }
    let mut program = original;
    program.abilities[0].node.play.as_mut().unwrap().costs = CostSpec::Ordered { steps: vec![] };
    assert!(
        WellFormedCardProgram::check(program).is_err(),
        "a scalar receipt cannot outlive its producer"
    );
}

#[test]
fn half_up_uses_ceiling_across_the_signed_range_without_overflow() {
    for (amount, expected) in [
        (0, 0),
        (1, 1),
        (2, 1),
        (9, 5),
        (-1, 0),
        (-3, -1),
        (i64::MIN, -0x4000_0000_0000_0000),
        (i64::MAX, 0x4000_0000_0000_0000),
    ] {
        let mut value = tree(&json!({"pp":0}), &json!(0));
        value["cards"]["unit-follower"]["abilities"][0]["activate"]["do"] = json!([{"if":{"cond":{"cmp":{"left":{"half_up":0},"op":"eq","right":0}},"then":[{"life":{"who":"self","delta":1}}]}}]);
        let mut program = lowerer().lower(&value).unwrap()["unit-follower"]
            .card()
            .clone();
        let Node::Control {
            control: Control::If { condition, .. },
        } = &mut program.abilities[0].node.body.as_mut().unwrap().steps[0].node
        else {
            panic!("comparison")
        };
        *condition = Expr::Compute(Box::new(Predicate::CompareInt {
            left: Expr::Compute(Box::new(IntOperation::Arithmetic {
                operation: Arithmetic::HalfUp,
                arguments: vec![Expr::Literal(Int(amount))],
            })),
            comparison: Comparison::Equal,
            right: Expr::Literal(Int(expected)),
        }));
        let program = WellFormedCardProgram::check(program).unwrap();
        let mut board = position_with_card(1, &program);
        activate(&mut board, 1);
        assert_eq!(
            board.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
            21,
            "the rounded value satisfies the comparison"
        );
    }
}

#[test]
fn class_filtered_play_discount_stays_scoped_to_own_cemetery_counts() {
    for (who, zone, admitted) in [
        ("self", "cemetery", true),
        ("opp", "cemetery", false),
        ("self", "field", false),
        ("self", "ex", false),
    ] {
        let mut value = tree(&json!({"pp":0}), &json!(0));
        let ability = value["cards"]["unit-follower"]["abilities"][0]
            .as_object_mut()
            .unwrap();
        ability.remove("activate");
        ability.insert("play".into(), json!({"cost_delta":{"if":{"cmp":{"left":{"count":{"who":who,"zone":zone,"class":"ドラゴン","orig_cost_ge":7}},"op":"ge","right":4}},"delta":-2}}));
        let result = lowerer().lower(&value);
        if admitted {
            result
                .as_ref()
                .unwrap_or_else(|error| panic!("cemetery condition: {error:?}"));
        }
        assert_eq!(
            result.is_ok(),
            admitted,
            "class counts outside the required cemetery condition remain rejected"
        );
    }
}
