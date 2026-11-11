use super::*;

fn expanded(optional: &Value) -> Value {
    let mut tree = fanfare::expanded::tree().clone();
    let mut seed = tree["cards"]["BP01-041"].clone();
    let mut entry = seed["abilities"][0].clone();
    entry.as_object_mut().unwrap().remove("activate");
    entry["play"] = json!({"optional_cost":optional,"optional_cost_delta":-9});
    seed["abilities"] = json!([entry]);
    tree["cards"] = json!({"unit-follower":seed});
    tree
}

fn optional() -> Value {
    json!({"one_of":[{"to_cemetery":{"sel":{"who":"self","zone":"field","class":"ナイトメア","acted":false},"n":4_u8}},{"banish":{"sel":{"who":"self","zone":"ex","class":"ナイトメア"},"n":4_u8}}]})
}

fn program() -> WellFormedCardProgram {
    lowerer()
        .lower(&expanded(&optional()))
        .unwrap()
        .remove("unit-follower")
        .unwrap()
}

#[test]
fn exchange_routes_keep_materials_and_costs_paired_and_reject_cross_route_ir() {
    let typed = program();
    let definition = &typed.card().card_play_costs[0]
        .node
        .exchange
        .as_ref()
        .unwrap()
        .node;
    assert_eq!(
        definition.materials.len(),
        2,
        "each route owns one material decision"
    );
    assert!(
        definition.route.is_some(),
        "route input has its own identity"
    );
    assert!(
        matches!(&definition.cost,CostSpec::Alternative {alternatives} if alternatives.len()==2),
        "both routes retain their complete cost"
    );
    for variant in [
        "no-route",
        "cross-binding",
        "union",
        "ex-put",
        "foreign",
        "count",
    ] {
        let mut raw = typed.card().clone();
        let exchange = &mut raw.card_play_costs[0].node.exchange.as_mut().unwrap().node;
        match variant {
            "no-route" => exchange.route = None,
            "cross-binding" => {
                let ChoiceDomain::Objects { binding, .. } = &exchange.materials[0].domain else {
                    panic!("first material binding")
                };
                let first = *binding;
                let CostSpec::Alternative { alternatives } = &mut exchange.cost else {
                    panic!("routes")
                };
                let CostSpec::Ordered { steps } = &mut alternatives[1] else {
                    panic!("route step")
                };
                let Action::Banish { objects } = &mut steps[0].instruction.action else {
                    panic!("EX banish")
                };
                *objects = Selector::Bound { reference: first };
            }
            other => {
                let ChoiceDomain::Objects { constraint, .. } = &mut exchange.materials[0].domain
                else {
                    panic!("field material constraint")
                };
                if other == "count" {
                    constraint.cardinality.minimum = Nat(3);
                    constraint.cardinality.maximum = Some(Nat(3));
                    constraint.groups[0].cardinality = constraint.cardinality;
                } else if other == "union" {
                    let original = constraint.groups[0].selector.clone();
                    constraint.groups[0].selector = Selector::Union {
                        sets: vec![original.clone(), original],
                    };
                } else {
                    let Selector::Zone { players, zone, .. } = &mut constraint.groups[0].selector
                    else {
                        panic!("field zone")
                    };
                    if other == "ex-put" {
                        *zone = ZoneKind::Ex;
                    } else {
                        players.relation = PlayerRelation::Opponents;
                    }
                }
            }
        }
        let error = WellFormedCardProgram::check(raw).unwrap_err();
        assert_eq!(
            error.reason,
            WellFormedReason::UnsupportedShape,
            "the exchange boundary rejects {variant}: {error:?}"
        );
    }
}

#[test]
fn exchange_routes_reject_extra_routes_nested_shapes_and_broader_posture_queries() {
    for variant in ["three", "nested", "result", "mixed-count", "acted-ex"] {
        let mut cost = optional();
        match variant {
            "three" => {
                let extra = cost["one_of"][0].clone();
                cost["one_of"].as_array_mut().unwrap().push(extra);
            }
            "nested" => cost["one_of"][0] = optional(),
            "result" => cost["result"] = json!("fee"),
            "mixed-count" => cost["one_of"][1]["banish"]["n"] = json!(3_u8),
            _ => cost["one_of"][1]["banish"]["sel"]["acted"] = json!(false),
        }
        let error = lowerer().lower(&expanded(&cost)).unwrap_err();
        if variant == "nested" {
            assert_eq!(
                error.code, "invalid_expanded",
                "nested optional cost is outside the schema"
            );
        } else {
            assert!(
                matches!(error.code, "unsupported_construct" | "ill_formed"),
                "the typed exchange shape rejects {variant}: {error:?}"
            );
        }
    }
    let mut tree = fanfare::expanded::tree().clone();
    let mut seed = tree["cards"]["BP01-041"].clone();
    seed["abilities"][0]["activate"] = json!({"cost":{"pp":0_u8},"do":[{"draw":{"n":{"count":{"who":"self","zone":"field","acted":false}}}}]});
    tree["cards"] = json!({"unit-follower":seed});
    let error = lowerer().lower(&tree).unwrap_err();
    assert_eq!(
        error.code, "unsupported_construct",
        "posture is not opened for ordinary counts"
    );
}

#[test]
fn player_question_exchange_preserves_the_only_affordable_route_index() {
    use crate::player_pipeline::{answer, question};
    use sve_engine::game::player::{Command, DecisionShape, QuestionShape, SubmissionKind, Viewer};
    let (snapshot, keywords, document) = catalog_documents(&json!("1"));
    let snapshot = snapshot
        .lines()
        .map(|line| {
            let mut row: Value = serde_json::from_str(line).unwrap();
            row["faces"][0]["card_class"] = json!("ナイトメア");
            row.to_string()
        })
        .collect::<Vec<_>>()
        .join("\n");
    let catalog = Arc::new(
        Catalog::from_documents(&snapshot, &keywords, &[("unit.yaml".into(), document)]).unwrap(),
    );
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"s","card":"unit-follower"}]);
    initial["players"]["P1"]["zones"]["ex"] = json!(
        (0..4)
            .map(|i| json!({"id":format!("ex-{i}"),"card":"unit-follower"}))
            .collect::<Vec<_>>()
    );
    let mut game = Game::new(
        catalog,
        &initial,
        &Value::Null,
        &Value::Null,
        "player-route-index",
    )
    .unwrap();
    game.load_internal_card("unit-follower", &program())
        .unwrap();
    game.play_internal_card("s", PlayerId { seat: 1 }).unwrap();
    game.advance_player_input();
    assert_eq!(
        answer(&mut game, Viewer::P1, Answer::OptionalCost(true)).kind,
        SubmissionKind::Applied
    );
    let route = question(&game, Viewer::P1);
    assert_eq!(
        route.shape,
        QuestionShape::Decision(DecisionShape::ExchangeRoutes(vec![Nat(1)]))
    );
    let before = exact_authority(&game);
    assert_eq!(
        game.submit_player(
            Viewer::P1,
            Command::Answer {
                token: route.token,
                answer: Answer::Integer(Nat(0))
            }
        )
        .kind,
        SubmissionKind::Rejected
    );
    assert_eq!(exact_authority(&game), before);
    assert_eq!(
        answer(&mut game, Viewer::P1, Answer::Integer(Nat(1))).kind,
        SubmissionKind::Applied
    );
    let material = question(&game, Viewer::P1);
    let QuestionShape::Decision(DecisionShape::CostMaterials(domain)) = material.shape else {
        panic!("route materials")
    };
    assert_eq!(domain.members.len(), 4);
    assert_eq!(
        answer(
            &mut game,
            Viewer::P1,
            Answer::CostMaterials(domain.members.iter().map(|member| member.handle).collect())
        )
        .kind,
        SubmissionKind::Applied
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["ex"], json!([]));
    assert_eq!(state["P1"]["field"], json!(["s"]));
    assert_eq!(state["P1"]["banish"].as_array().unwrap().len(), 4);
}
