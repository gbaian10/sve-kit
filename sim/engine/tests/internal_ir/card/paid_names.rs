use super::*;
use sve_engine::game::internal::InputDomain;

fn tree() -> Value {
    evolution_linked_movement::tree(&[(
        "activate",
        json!({
            "cost":{"reveal":{"sel":{"who":"self","zone":"hand","kind":"follower"},"n":1_u8,"result":"paid"}},
            "do":[{"search":{"sel":{"who":"self","zone":"deck","kind":"follower","name":{"same_as":{"objects_of":"paid","view":"before"}}},"n":1_u8,"to":"ex"}}]
        }),
    )])
}

#[test]
fn payment_names_reject_unknown_after_cross_ability_and_shadowed_references() {
    lowerer().lower(&tree()).unwrap();
    for case in 0_u8..4 {
        let mut input = tree();
        let ability = &mut input["cards"]["unit-follower"]["abilities"][0];
        if case == 0 {
            ability["activate"]["do"][0]["search"]["sel"]["name"]["same_as"]["objects_of"] =
                json!("unknown");
        } else if case == 1 {
            ability["activate"]["do"][0]["search"]["sel"]["name"]["same_as"]["view"] =
                json!("after");
        } else if case == 2 {
            let search = ability["activate"]["do"][0].clone();
            ability["activate"]["do"] = json!([
                {"look":{"bind":"paid","top":{"who":"self","n":1_u8}}},search
            ]);
        } else {
            let mut foreign = ability.clone();
            foreign["line"] = json!(2_u8);
            foreign["activate"]["cost"] = json!({"pp":0_u8});
            input["cards"]["unit-follower"]["abilities"]
                .as_array_mut()
                .unwrap()
                .push(foreign);
        }
        assert!(
            lowerer().lower(&input).is_err(),
            "only the current scope's paid-before names are readable, case {case}"
        );
    }
}

#[test]
fn raw_payment_name_reads_require_a_preceding_cost_and_before_snapshot() {
    let program = lowerer()
        .lower(&tree())
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    for after in [false, true] {
        let mut raw = program.card().clone();
        let ability = &mut raw.abilities[0].node;
        if after {
            let Node::Instruction { instruction } =
                &mut ability.body.as_mut().unwrap().steps[0].node
            else {
                panic!("search instruction")
            };
            let Action::Search { filter, .. } = &mut instruction.action else {
                panic!("search predicate")
            };
            let Expr::Compute(and) = filter else {
                panic!("kind and same-name")
            };
            let Predicate::And { predicates } = and.as_mut() else {
                panic!("same-name conjunction")
            };
            let Expr::Compute(any) = &mut predicates[1] else {
                panic!("snapshot member match")
            };
            let Predicate::ReceiptAny { projection, .. } = any.as_mut() else {
                panic!("payment before predicate")
            };
            *projection = MemberProjection::After;
        } else {
            let CostSpec::Ordered { steps } = &mut ability.play.as_mut().unwrap().costs else {
                panic!("ordered reveal payment")
            };
            steps.clear();
        }
        assert!(
            WellFormedCardProgram::check(raw).is_err(),
            "cost provenance and before-only reads remain enforced"
        );
    }
}

#[test]
fn paid_names_intersect_effective_aliases_on_both_materials_and_search_candidates() {
    let mut input = tree();
    input["cards"]["unit-follower"]["abilities"][0]["activate"]["cost"]["reveal"]["sel"]
        .as_object_mut()
        .unwrap()
        .remove("kind");
    input["cards"]["unit-follower"]["abilities"][0]["activate"]["do"][0]["search"]["sel"]
        .as_object_mut()
        .unwrap()
        .remove("kind");
    let mut alias = input["cards"]["unit-follower"]["abilities"][0].clone();
    alias.as_object_mut().unwrap().remove("activate");
    alias["line"] = json!(2_u8);
    alias["static"] = json!({"effects":[{"also_named":{"name":"unit-spell"}}]});
    input["cards"]["unit-follower"]["abilities"]
        .as_array_mut()
        .unwrap()
        .push(alias);
    let program = lowerer()
        .lower(&input)
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    let ability = program.card().abilities[0].node.id;
    for (material, found) in [
        ("unit-follower", "unit-spell"),
        ("unit-spell", "unit-follower"),
    ] {
        let mut initial = setup(1, json!([{"id":"found","card":found}]));
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"material","card":material}]);
        initial["players"]["P1"]["zones"]["field"] =
            json!([{"id":"source","card":"unit-follower"}]);
        initial["players"]["P2"]["zones"]["field"] = json!([]);
        let mut game = evolution_linked_movement::loaded(&initial, &program);
        game.load_internal_card("unit-spell", &card(vec![]))
            .unwrap();
        let CardPlay::Awaiting(payment) = game
            .activate_internal_card("source", ability, PlayerId { seat: 1 })
            .unwrap()
        else {
            panic!("named reveal payment")
        };
        let InputDomain::CostMaterials(domain) = &payment.domain else {
            panic!("one material")
        };
        let CardPlay::Awaiting(search) = game
            .answer_internal_card(
                payment.actor,
                payment.token,
                Answer::CostMaterials(vec![domain.members[0].handle]),
            )
            .unwrap()
        else {
            panic!("same-name search")
        };
        let InputDomain::Search(domain) = &search.domain else {
            panic!("name candidates")
        };
        assert_eq!(
            domain.cards.len(),
            1,
            "the live candidate and the frozen material share an effective name in either direction"
        );
        assert_eq!(
            domain.cards[0].card, found,
            "printed names may differ when an effective alias intersects"
        );
    }
}
