use super::*;

fn neptune_position(number: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"provider","card":number},
        {"id":"orca","card":"BP02-T05"},
        {"id":"other","card":"BP01-037"}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"enemy","card":"BP02-T05"}
    ]);
    initial
}

#[test]
fn neptune_grants_only_friendly_orcas_and_loses_its_aura_on_departure() {
    for (number, keyword) in [("BP02-055", "突進"), ("BP02-056", "疾走")] {
        let initial = neptune_position(number);
        let mut game = with_producer(
            &initial,
            Action::Move {
                objects: field_name("ネプチューン"),
                movement: MoveSpec {
                    to: MoveDestination::Owners {
                        zone: OwnerZone::Hand,
                    },
                    initial_posture: None,
                    entry_effects: vec![],
                    position: None,
                    order: None,
                },
                reason: MoveReason::Put,
            },
        );
        let before = game.projection(View::Referee).unwrap();
        assert_eq!(before["objects"]["orca"]["keywords"], json!([keyword]));
        assert_eq!(before["objects"]["enemy"]["keywords"], json!([]));
        assert_eq!(before["objects"]["other"]["keywords"], json!([]));
        resolved(
            game.activate_internal_card("producer", AbilityId(1), PlayerId { seat: 1 })
                .unwrap(),
        );
        let after = game.projection(View::Referee).unwrap();
        assert_eq!(after["objects"]["orca"]["keywords"], json!([]));
    }
}

#[test]
fn awakened_keyword_recomputes_when_maximum_pp_changes_and_keeps_printed_rush() {
    let mut initial = draw_position();
    initial["players"]["P1"]["pp"] = json!({"current":6_u8,"max":6_u8});
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"dragon","card":"BP16-073"}]);
    let mut game = with_producer(
        &initial,
        Action::Resource {
            player: PlayerRef::Controller {},
            resource: Resource::PpMaximum,
            operation: ResourceOperation::Add,
            amount: Expr::Literal(Int(1)),
        },
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["dragon"]["keywords"],
        json!(["突進"])
    );
    resolved(
        game.activate_internal_card("producer", AbilityId(1), PlayerId { seat: 1 })
            .unwrap(),
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["dragon"]["keywords"],
        json!(["突進", "疾走"])
    );
}

#[test]
fn silenced_neptune_does_not_grant_keywords() {
    let mut initial = neptune_position("BP02-055");
    initial["players"]["P1"]["zones"]["field"][0]["state"] = json!({"silenced":true});
    let game = game(&initial);
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["orca"]["keywords"],
        json!([])
    );
}

fn field_name(name: &str) -> Selector {
    let binding = Binding::new(50, ScopeId(1));
    let mut selector = subjects();
    if let Selector::Zone { predicate, .. } = &mut selector {
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
fn neptune_applies_to_orcas_created_after_it_and_enables_an_attack() {
    let mut initial = neptune_position("BP02-056");
    initial["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .remove(1);
    let mut game = with_producer(
        &initial,
        Action::Token {
            tokens: vec![TokenSpec {
                template: TokenTemplateId("オルカ".into()),
                name: Expr::Literal(CardName("オルカ".into())),
                count: Expr::Literal(Nat(1)),
            }],
            destination: TokenDestination::Single {
                zone: Expr::Literal(ZoneRef {
                    player: PlayerRef::Controller {},
                    kind: ZoneKind::Field,
                }),
            },
            output: Binding::new(51, ScopeId(1)),
            initial_state: None,
            initial_posture: Posture::Standing,
            entry_effects: vec![],
        },
    );
    resolved(
        game.activate_internal_card("producer", AbilityId(1), PlayerId { seat: 1 })
            .unwrap(),
    );
    game.resume_internal_card().unwrap();
    let view = game.projection(View::Referee).unwrap();
    let token = view["P1"]["field"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .find(|id| view["objects"][*id]["name"] == "オルカ")
        .unwrap();
    assert_eq!(view["objects"][token]["keywords"], json!(["疾走"]));
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":token,"target":"P2.leader"}))
            .unwrap()
            .outcome,
        "resolved"
    );
}

#[test]
fn static_grants_reject_keywords_without_a_runtime_consumer() {
    let input = &inputs().2;
    let source = input["authors"]
        .as_array()
        .unwrap()
        .iter()
        .find(|author| author["input"]["card_no"] == "BP16-073")
        .unwrap();
    let mut candidate = source["input"].clone();
    let card = &mut candidate["tree"]["cards"]["BP16-073"];
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
    abilities[1]["static"]["effects"][0]["grant"]["keywords"] = json!(["drain"]);
    match lowerer().lower_current(&candidate) {
        Err(error) => assert_eq!(error.code, "unsupported_construct"),
        Ok(_) => panic!("an unsupported static keyword was admitted"),
    }
}
