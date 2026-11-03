use super::*;
use sve_engine::game::internal::InputDomain;

fn tree(amount: &Value) -> Value {
    let mut tree = fanfare::expanded::tree().clone();
    let mut seed = tree["cards"]["BP01-041"].clone();
    seed["abilities"][0]["activate"] = json!({"cost":{"steps":[{"act_self":true},{"act":{"sel":{"who":"self","zone":"field","kind":"amulet"},"any":true,"result":"fuel"}}]},"do":[{"damage":{"to":{"leaders":"opponents"},"n":amount}}]});
    tree["cards"] = json!({"unit-follower":seed});
    tree
}

fn program() -> WellFormedCardProgram {
    lowerer().lower(&tree(&json!({"x_paid":"fuel"}))).unwrap()["unit-follower"].clone()
}

#[test]
fn named_material_acts_read_only_the_selected_cost_step_after_complete_payment() {
    let typed = program();
    for count in 0..=2_u64 {
        let mut initial = setup(1, json!([]));
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"unit-follower","state":{"acted":false}},{"id":"first","card":"unit-amulet","state":{"acted":false}},{"id":"second","card":"unit-amulet","state":{"acted":false}},{"id":"acted","card":"unit-amulet","state":{"acted":true}}]);
        initial["players"]["P2"]["zones"]["field"] =
            json!([{"id":"foreign","card":"unit-amulet","state":{"acted":false}}]);
        let mut game = Game::new(
            catalog(),
            &initial,
            &Value::Null,
            &Value::Null,
            "material-act-quantity",
        )
        .unwrap();
        game.load_internal_card("unit-follower", &typed).unwrap();
        game.load_internal_card("unit-amulet", &card(vec![]))
            .unwrap();
        let CardPlay::Awaiting(q) = game
            .activate_internal_card("source", AbilityId(1), PlayerId { seat: 1 })
            .unwrap()
        else {
            panic!("material selection")
        };
        let InputDomain::CostMaterials(domain) = &q.domain else {
            panic!("actual cost materials")
        };
        assert_eq!(
            domain.members.len(),
            2,
            "already acted and foreign objects cannot pay"
        );
        resolved(
            game.answer_internal_card(
                q.actor,
                q.token,
                Answer::CostMaterials((0..count).collect()),
            )
            .unwrap(),
        );
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P2"]["leader"]["life"],
            20 - count,
            "the independent source Act is not counted"
        );
        assert_eq!(state["objects"]["source"]["acted"], true);
        assert_eq!(state["objects"]["first"]["acted"], count > 0);
        assert_eq!(state["objects"]["second"]["acted"], count > 1);
        assert_eq!(state["objects"]["foreign"]["acted"], false);
    }
}

#[test]
fn material_act_reads_reject_an_opaque_source_act_missing_receipt_and_effect_producer() {
    let typed = program();
    let CostSpec::Ordered { steps } = &typed.card().abilities[0].node.play.as_ref().unwrap().costs
    else {
        panic!("ordered cost")
    };
    for (id, expected) in [
        (steps[0].instruction.receipt, WellFormedReason::ReceiptType),
        (ReceiptId(9000), WellFormedReason::UnavailableReceipt),
    ] {
        let mut raw = typed.card().clone();
        let Node::Instruction { instruction } =
            &mut raw.abilities[0].node.body.as_mut().unwrap().steps[0].node
        else {
            panic!("damage")
        };
        let Action::Damage {
            amount: DamageAmount::Uniform { amount },
            ..
        } = &mut instruction.action
        else {
            panic!("uniform damage")
        };
        *amount = Expr::Frozen(Box::new(IntFrozen::PaidMaterialActs {
            receipt: ReceiptReference::new(id),
        }));
        let error = WellFormedCardProgram::check(raw).unwrap_err();
        assert_eq!(error.reason, expected);
    }
    let mut raw = typed.card().clone();
    let play = raw.abilities[0].node.play.as_mut().unwrap();
    let CostSpec::Ordered { steps } = &mut play.costs else {
        panic!("ordered cost")
    };
    let mut effect = *steps.pop().unwrap().instruction;
    effect.context = InstructionContext::Effect;
    raw.abilities[0].node.body.as_mut().unwrap().steps.insert(
        0,
        Located {
            source: source("unit-follower"),
            node: Node::Instruction {
                instruction: Box::new(effect),
            },
        },
    );
    let error = WellFormedCardProgram::check(raw).unwrap_err();
    assert_eq!(
        error.reason,
        WellFormedReason::ReceiptType,
        "an effect Act is not a paid material receipt"
    );
}

#[test]
fn named_material_act_reads_reject_unknown_names_early_use_and_shadowing() {
    let missing = tree(&json!({"x_paid":"other"}));
    let error = lowerer().lower(&missing).unwrap_err();
    assert_eq!(error.code, "unsupported_construct");
    assert_eq!(error.path, "/cards/unit-follower/abilities/0/activate/do/0");
    let mut shadow = tree(&json!({"x_paid":"fuel"}));
    shadow["cards"]["unit-follower"]["abilities"][0]["activate"]["targets"] =
        json!([{"bind":"fuel","from":{"who":"opp","zone":"field","kind":"follower"},"n":1_u8}]);
    let error = lowerer().lower(&shadow).unwrap_err();
    assert_eq!(error.code, "unsupported_construct");
    assert_eq!(
        error.path, "/cards/unit-follower/abilities/0",
        "the existing activated-cost lowering reports its owning ability, before the body"
    );
    let mut early = program().card().clone();
    let play = early.abilities[0].node.play.as_mut().unwrap();
    let CostSpec::Ordered { steps } = &play.costs else {
        panic!("ordered cost")
    };
    let receipt = steps[1].instruction.receipt;
    let ChoiceDomain::Objects { constraint, .. } = &mut play.parameters[0].domain else {
        panic!("cost parameter")
    };
    let Selector::Zone { predicate, .. } = &mut constraint.groups[0].selector else {
        panic!("field selector")
    };
    **predicate = Expr::Compute(Box::new(Predicate::CompareInt {
        left: Expr::Frozen(Box::new(IntFrozen::PaidMaterialActs {
            receipt: ReceiptReference::new(receipt),
        })),
        comparison: Comparison::GreaterEqual,
        right: Expr::Literal(Int(0)),
    }));
    let error = WellFormedCardProgram::check(early).unwrap_err();
    assert_eq!(error.reason, WellFormedReason::UnavailableReceipt);
    assert!(
        error.ir_path.contains("/play/parameters/0/"),
        "the receipt cannot be read while choosing its material: {}",
        error.ir_path
    );
}
