use super::*;

fn subjects() -> Value {
    json!({"any_of":[
        {"who":"self","zone":"field","kind":"follower","trait":"妖精","token":true},
        {"who":"self","zone":"ex","kind":"follower","trait":"妖精","token":true}
    ]})
}

fn tree(on: &Value, set: bool) -> Value {
    let action = if set { "set_stat" } else { "buff" };
    equipment_played::tree(
        &json!({"activate":{"cost":{"pp":0},"do":[
            {action:{"on":on,"power":1,"hp":1}}
        ]}}),
        "unit-amulet",
    )
}

#[test]
fn numeric_token_unions_accept_both_zone_orders_and_keep_set_stat_closed() {
    let programs = lowerer().lower(&tree(&subjects(), false)).unwrap();
    let body = programs["unit-amulet"].card().abilities[0]
        .node
        .body
        .as_ref()
        .unwrap();
    assert_eq!(
        body.steps.len(),
        1,
        "both zones receive one numeric instruction"
    );
    assert!(
        matches!(&body.steps[0].node, Node::Instruction { instruction }
        if matches!(&instruction.action, Action::Modify { subjects: Selector::Union { sets }, .. } if sets.len() == 2))
    );
    let mut reversed = subjects();
    reversed["any_of"].as_array_mut().unwrap().reverse();
    lowerer().lower(&tree(&reversed, false)).unwrap();
    lowerer().lower(&tree(&subjects(), true)).unwrap_err();
}

#[test]
fn numeric_unions_do_not_widen_the_shared_grant_object_boundary() {
    let mut input = tree(&subjects(), false);
    input["cards"]["unit-amulet"]["abilities"][0]["activate"]["do"][0] = json!({"grant":{
        "on":{"who":"self","zone":"field","kind":"follower","trait":"妖精","token":true},
        "keywords":["ward"]
    }});
    lowerer().lower(&input).unwrap();
    input["cards"]["unit-amulet"]["abilities"][0]["activate"]["do"][0]["grant"]["on"] = subjects();
    lowerer().lower(&input).unwrap_err();
}

#[test]
fn numeric_union_admission_keeps_hand_deck_and_opposing_union_reads_closed() {
    let program = lowerer()
        .lower(&tree(&subjects(), false))
        .unwrap()
        .remove("unit-amulet")
        .unwrap();
    let host = || {
        Game::new(
            catalog(),
            &setup(1, json!([])),
            &Value::Null,
            &Value::Null,
            "numeric-union-boundary",
        )
        .unwrap()
    };
    host().load_internal_card("unit-amulet", &program).unwrap();
    for (zone, relation) in [
        (ZoneKind::Hand, PlayerRelation::SelfPlayer),
        (ZoneKind::Deck, PlayerRelation::SelfPlayer),
        (ZoneKind::Ex, PlayerRelation::Opponents),
    ] {
        let mut invalid = program.card().clone();
        let Node::Instruction { instruction } =
            &mut invalid.abilities[0].node.body.as_mut().unwrap().steps[0].node
        else {
            panic!("numeric instruction")
        };
        let Action::Modify {
            subjects: Selector::Union { sets },
            ..
        } = &mut instruction.action
        else {
            panic!("numeric union")
        };
        let Selector::Zone {
            zone: actual,
            players,
            ..
        } = &mut sets[1]
        else {
            panic!("public leaf")
        };
        *actual = zone;
        players.relation = relation;
        let checked = WellFormedCardProgram::check(invalid).unwrap();
        host()
            .load_internal_card("unit-amulet", &checked)
            .unwrap_err();
    }
}

#[test]
fn numeric_unions_lower_each_existing_public_filter_and_reject_private_query_shapes() {
    lowerer().lower(&tree(&subjects(), false)).unwrap();
    for (key, value) in [
        ("who", json!("opp")),
        ("who", json!("all")),
        ("zone", json!("field")),
        ("kind", json!("amulet")),
        ("trait", json!("人形")),
        ("name", json!("フェアリー")),
        ("orig_cost_le", json!(2)),
        ("other", json!(true)),
        ("acted", json!(true)),
    ] {
        let mut query = subjects();
        query["any_of"][1][key] = value;
        lowerer().lower(&tree(&query, false)).unwrap();
    }
    for key in ["trait", "token", "kind"] {
        let mut query = subjects();
        query["any_of"][1].as_object_mut().unwrap().remove(key);
        lowerer().lower(&tree(&query, false)).unwrap();
    }
    let mut non_token = subjects();
    non_token["any_of"][1]["token"] = json!(false);
    lowerer().lower(&tree(&non_token, false)).unwrap_err();
    let mut private = subjects();
    private["any_of"][1]["zone"] = json!("hand");
    lowerer().lower(&tree(&private, false)).unwrap_err();
    let mut too_many = subjects();
    let extra = too_many["any_of"][0].clone();
    too_many["any_of"].as_array_mut().unwrap().push(extra);
    lowerer().lower(&tree(&too_many, false)).unwrap_err();
    let mut sibling = subjects();
    sibling["who"] = json!("opp");
    lowerer().lower(&tree(&sibling, false)).unwrap_err();
}
