use super::*;

fn tree(steps: Value) -> Value {
    let mut tree = fanfare::expanded::tree().clone();
    let ability = &mut tree["cards"]["BP08-038"]["abilities"][0];
    ability.as_object_mut().unwrap().remove("fanfare");
    ability["activate"] = json!({"cost":{"pp":1}});
    ability["activate"]["do"] = steps;
    tree
}

#[test]
fn unbounded_public_choice_rejects_ambiguous_cardinality_and_foreign_private_look() {
    let choice = json!({"choose":{"bind":"picked","from":{"who":"opp","zone":"field","kind":"follower"},"any":true,"by":"opp"}});
    lowerer().lower(&tree(json!([choice]))).unwrap();
    let mut conflicting = choice;
    conflicting["choose"]["n"] = json!(1);
    lowerer().lower(&tree(json!([conflicting]))).unwrap_err();
    let private = json!([
        {"look":{"top":{"who":"self","n":2},"bind":"seen"}},
        {"choose":{"bind":"picked","from":{"in":"seen"},"upto":1,"by":"opp"}}
    ]);
    lowerer().lower(&tree(private)).unwrap_err();
}

#[test]
fn extrema_require_the_same_public_population_and_matching_numeric_property() {
    let query = json!({"who":"opp","zone":"field","kind":"follower","where":{"cmp":{
        "left":{"cost_of":"item"},"op":"eq","right":{"agg":{"of":{"who":"opp","zone":"field","kind":"follower"},"attr":"cost","fn":"min"}}
    }}});
    lowerer()
        .lower(&tree(json!([{"destroy":{"ref":query}}])))
        .unwrap();
    let mut wrong_population = query.clone();
    wrong_population["where"]["cmp"]["right"]["agg"]["of"]["who"] = json!("self");
    lowerer()
        .lower(&tree(json!([{"destroy":{"ref":wrong_population}}])))
        .unwrap_err();
    let mut wrong_property = query;
    wrong_property["where"]["cmp"]["right"]["agg"]["attr"] = json!("orig_cost");
    lowerer()
        .lower(&tree(json!([{"destroy":{"ref":wrong_property}}])))
        .unwrap_err();
}

#[test]
fn aggregate_queries_reject_hidden_populations_and_preserve_defined_extrema() {
    let mut valid = tree(
        json!([{"destroy":{"ref":{"who":"opp","zone":"field","kind":"follower","where":{"cmp":{
            "left":{"cost_of":"item"},"op":"eq","right":{"bound":"lowest"}
        }}}}}]),
    );
    valid["cards"]["BP08-038"]["abilities"][0]["activate"]["define"] = json!({"lowest":{"agg":{"of":{"who":"opp","zone":"field","kind":"follower"},"attr":"cost","fn":"min"}}});
    lowerer().lower(&valid).unwrap();
    lowerer().lower(&tree(json!([{"draw":{"who":"self","n":{"agg":{"of":{"who":"self","zone":"hand"},"attr":"orig_cost","fn":"sum"}}}}]))).unwrap_err();
}

#[test]
fn extremum_choice_rejects_other_populations_and_private_or_recipient_domains() {
    let clause = json!({"choose":{"bind":"chosen","by":"opp","n":1,"from":{
        "who":"opp","zone":"field","kind":"follower","where":{"cmp":{
            "left":{"power_of":"item"},"op":"eq","right":{"agg":{"of":{"who":"opp","zone":"field","kind":"follower"},"attr":"power","fn":"max"}}
        }}
    }}});
    lowerer().lower(&tree(json!([clause]))).unwrap();
    let mut wrong = clause.clone();
    wrong["choose"]["from"]["where"]["cmp"]["right"]["agg"]["of"]["who"] = json!("self");
    lowerer().lower(&tree(json!([wrong]))).unwrap_err();
    let condition = clause["choose"]["from"]["where"].clone();
    let private = json!([
        {"look":{"top":{"who":"self","n":2},"bind":"seen"}},
        {"choose":{"bind":"chosen","n":1,"from":{"in":"seen","where":condition}}}
    ]);
    lowerer().lower(&tree(private)).unwrap_err();
    lowerer()
        .lower(&tree(
            json!([{"choose":{"bind":"chosen","n":1,"from":{"leader":"opp","where":condition}}}]),
        ))
        .unwrap_err();
}

#[test]
fn aggregate_stat_admission_requires_a_public_field_follower_population() {
    let mut document = tree(json!([{"draw":{"who":"self","n":{"agg":{
        "of":{"who":"self","zone":"field","kind":"follower"},"attr":"power","fn":"sum"
    }}}}]));
    let entry = document["cards"]["BP08-038"].take();
    document["cards"] = json!({"unit-follower":entry});
    let program = lowerer()
        .lower(&document)
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    let initial = setup(1, json!([]));
    let mut allowed = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "aggregate-positive",
    )
    .unwrap();
    allowed
        .load_internal_card("unit-follower", &program)
        .unwrap();
    for zone in [ZoneKind::Field, ZoneKind::Cemetery, ZoneKind::Ex] {
        let mut raw = program.card().clone();
        let body = raw.abilities[0].node.body.as_mut().unwrap();
        let instruction = body
            .steps
            .iter_mut()
            .find_map(|step| {
                if let Node::Instruction { instruction } = &mut step.node
                    && matches!(instruction.action, Action::Draw { .. })
                {
                    return Some(instruction);
                }
                None
            })
            .unwrap();
        let Action::Draw {
            count: Expr::Compute(operation),
            ..
        } = &mut instruction.action
        else {
            panic!("aggregate draw")
        };
        let IntOperation::Sum {
            objects:
                Selector::Zone {
                    zone: input_zone,
                    predicate,
                    ..
                },
            ..
        } = operation.as_mut()
        else {
            panic!("zone aggregate")
        };
        *input_zone = zone;
        if zone == ZoneKind::Field {
            **predicate = Expr::Literal(Bool(true));
        }
        let checked = WellFormedCardProgram::check(raw).unwrap();
        let mut game = Game::new(
            catalog(),
            &initial,
            &Value::Null,
            &Value::Null,
            "aggregate-negative",
        )
        .unwrap();
        game.load_internal_card("unit-follower", &checked)
            .unwrap_err();
    }
}

#[test]
fn opponent_discard_requires_the_instruction_actor_to_match_the_player() {
    let mut document = tree(json!([{"discard":{"who":"opp","n":1}}]));
    let entry = document["cards"]["BP08-038"].take();
    document["cards"] = json!({"unit-follower":entry});
    let program = lowerer()
        .lower(&document)
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    let initial = setup(1, json!([]));
    let mut allowed = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "discard-actor-positive",
    )
    .unwrap();
    allowed
        .load_internal_card("unit-follower", &program)
        .unwrap();
    let mut raw = program.card().clone();
    let body = raw.abilities[0].node.body.as_mut().unwrap();
    let instruction = body
        .steps
        .iter_mut()
        .find_map(|step| {
            if let Node::Instruction { instruction } = &mut step.node
                && matches!(instruction.action, Action::Discard { .. })
            {
                return Some(instruction);
            }
            None
        })
        .unwrap();
    instruction.actor = PlayerRef::Controller {};
    WellFormedCardProgram::check(raw).unwrap_err();
}

#[test]
fn attack_target_aggregates_preserve_stats_without_admitting_cost_reads() {
    let mut document = fanfare::expanded::tree().clone();
    let ability = &mut document["cards"]["BP08-038"]["abilities"][0];
    ability.as_object_mut().unwrap().remove("fanfare");
    ability["auto"] = json!({"on":{"attacked":{"who":"self","sel":{"kind":"follower"}}},"do":[
        {"if":{"cond":{"cmp":{"left":{"hp_of":"attack_target"},"op":"le","right":5}},"then":[{"draw":{"who":"self","n":1}}]}}
    ]});
    let entry = document["cards"]["BP08-038"].take();
    document["cards"] = json!({"unit-follower":entry});
    let program = lowerer()
        .lower(&document)
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    let initial = setup(1, json!([]));
    for requested in [
        NumericProperty::Attack,
        NumericProperty::Life,
        NumericProperty::Cost,
        NumericProperty::OriginalCost,
    ] {
        let mut raw = program.card().clone();
        let body = raw.abilities[0].node.body.as_mut().unwrap();
        let Node::Control {
            control:
                Control::If {
                    condition: Expr::Compute(predicate),
                    ..
                },
        } = &mut body.steps[0].node
        else {
            panic!("attack target comparison")
        };
        let Predicate::CompareInt {
            left: Expr::Compute(operation),
            ..
        } = predicate.as_mut()
        else {
            panic!("attack target numeric comparison")
        };
        let IntOperation::Sum { property, .. } = operation.as_mut() else {
            panic!("attack target aggregate")
        };
        *property = requested;
        let checked = WellFormedCardProgram::check(raw).unwrap();
        let mut game = Game::new(
            catalog(),
            &initial,
            &Value::Null,
            &Value::Null,
            "attack-aggregate-admission",
        )
        .unwrap();
        let result = game.load_internal_card("unit-follower", &checked);
        if matches!(requested, NumericProperty::Attack | NumericProperty::Life) {
            result.unwrap();
        } else {
            result.unwrap_err();
        }
    }
}
