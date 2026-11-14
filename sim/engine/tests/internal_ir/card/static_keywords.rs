use super::*;
use alloc::collections::BTreeMap;
use sve_engine::game::internal::{InputDomain, ResumeStage};

pub(super) fn conditional_keyword() -> Located<Ability> {
    let mut located = ability(2, vec![]);
    let value = &mut located.node;
    value.kind = AbilityKind::Static;
    value.play = None;
    value.body = None;
    value.declarations = vec![Effect::Continuous {
        id: EffectDefinitionId(1),
        source: value.source.clone(),
        target: EffectTarget::Dynamic {
            selector: Selector::Objects {
                objects: vec![ObjectOperand::SourceHolder {}],
            },
        },
        condition: Some(Expr::Compute(Box::new(Predicate::CompareInt {
            left: Expr::Live(Box::new(IntRead::PlayerResource {
                player: PlayerRef::Controller {},
                property: PlayerResource::Life,
            })),
            comparison: Comparison::LessEqual,
            right: Expr::Literal(Int(19)),
        }))),
        patch: CharacteristicPatch::GrantAbility {
            abilities: vec![GrantedAbility::Keyword {
                keyword: KeywordId::Storm,
            }],
        },
        layer: ContinuousLayer::Ability,
        timestamp: TimestampBasis::ZoneEntry {
            object: ObjectOperand::SourceHolder {},
        },
        active_zones: vec![ZoneKind::Field],
        duration: Duration {
            end: DurationEnd::NoTemporalExpiry {},
            anchor: TimeAnchor::Installation {},
            usage: None,
            intervals: vec![],
        },
    }];
    located
}

#[test]
fn live_keyword_is_gained_and_lost_without_leaving_the_field() {
    for seat in [1, 2] {
        let change = |id, delta| {
            ability(
                id,
                vec![instruction(
                    id,
                    Action::Life {
                        player: PlayerRef::Controller {},
                        change: NumericChange::Add {
                            amount: Expr::Literal(Int(delta)),
                        },
                    },
                )],
            )
        };
        let mut game = keyword_position(
            seat,
            &card(vec![change(1, -1), conditional_keyword(), change(3, 1)]),
        );
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["s"]["keywords"],
            json!([])
        );
        activate(&mut game, seat);
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["s"]["keywords"],
            json!(["疾走"])
        );
        game.resume_internal_card().unwrap();
        resolved(
            game.activate_internal_card("s", AbilityId(3), PlayerId { seat })
                .unwrap(),
        );
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["s"]["keywords"],
            json!([])
        );
    }
}

#[test]
fn static_keyword_dependencies_are_rejected_before_they_can_recurse() {
    let mut located = conditional_keyword();
    let binding = Binding::new(50, located.node.scope);
    if let Effect::Continuous { target, .. } = &mut located.node.declarations[0] {
        *target = EffectTarget::Dynamic {
            selector: Selector::Zone {
                players: players(PlayerRelation::SelfPlayer),
                zone: ZoneKind::Field,
                view: SelectionView::Current {},
                binding,
                predicate: Box::new(Expr::Compute(Box::new(Predicate::HasAbility {
                    abilities: Expr::Live(Box::new(AbilitiesRead::Abilities {
                        object: ObjectView::Current {
                            object: Box::new(ObjectOperand::Bound { binding }),
                        },
                        view: CharacteristicView::Effective,
                    })),
                    predicate: Box::new(AbilityPredicate {
                        kind: None,
                        keyword: Some(KeywordId::Storm),
                        except: vec![],
                        holder: None,
                        origin: None,
                    }),
                }))),
            },
        };
    }
    let program = card(vec![located]);
    let mut game = Game::new(
        catalog(),
        &setup(1, json!([])),
        &Value::Null,
        &Value::Null,
        "recursive",
    )
    .unwrap();
    let error = game
        .load_internal_card("unit-follower", &program)
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("recursive static keyword dependency")
    );
}

#[test]
fn ward_aura_offers_optional_act_to_a_new_field_entrant() {
    let mut located = conditional_keyword();
    if let Effect::Continuous {
        target,
        condition,
        patch,
        ..
    } = &mut located.node.declarations[0]
    {
        *condition = None;
        *target = EffectTarget::Dynamic {
            selector: Selector::Zone {
                players: players(PlayerRelation::SelfPlayer),
                zone: ZoneKind::Field,
                view: SelectionView::Current {},
                binding: Binding::new(50, located.node.scope),
                predicate: Box::new(Expr::Literal(Bool(true))),
            },
        };
        *patch = CharacteristicPatch::GrantAbility {
            abilities: vec![GrantedAbility::Keyword {
                keyword: KeywordId::Ward,
            }],
        };
    }
    let program = card(vec![located]);
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"incoming","card":"unit-follower"}]);
    let mut game = Game::new(
        keyword_catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "ward-aura",
    )
    .unwrap();
    game.load_internal_card("unit-follower", &program).unwrap();
    let play = game
        .play_internal_card("incoming", PlayerId { seat: 1 })
        .unwrap();
    let CardPlay::Awaiting(question) = play else {
        panic!("granted Ward must offer entry posture");
    };
    assert!(matches!(question.domain, InputDomain::WardEntry { .. }));
    assert!(matches!(
        game.answer_internal_card(question.actor, question.token, Answer::WardEntry(true))
            .unwrap(),
        CardPlay::Entered(_)
    ));
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["incoming"]["acted"], true);
    assert_eq!(state["objects"]["incoming"]["keywords"], json!(["守護"]));
}

pub(super) fn keyword_catalog() -> Arc<Catalog> {
    let (snapshot, _, document) = catalog_documents(&json!("1"));
    Arc::new(
        Catalog::from_documents(
            &snapshot,
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../authored/rules/keywords.yaml"
            )),
            &[("unit.yaml".into(), document)],
        )
        .unwrap(),
    )
}

fn keyword_position(seat: u32, program: &WellFormedCardProgram) -> Game {
    let actor = if seat == 1 { "P1" } else { "P2" };
    let mut initial = setup(seat, json!([]));
    initial["turn"]["active"] = json!(actor);
    initial["players"][actor]["zones"]["hand"] = json!([]);
    initial["players"][actor]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
    let mut game = Game::new(
        keyword_catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "live-keyword",
    )
    .unwrap();
    game.load_internal_card("unit-follower", program).unwrap();
    game.load_internal_card("unit-spell", &card(vec![]))
        .unwrap();
    game
}

fn cross_zone_tree(zones: &Value) -> Value {
    equipment_played::tree(
        &json!({"static":{"zone":zones,"effects":[{"grant":{
            "on":{"who":"self","zone":"field","kind":"follower"},"keywords":["rush"]
        }}]}}),
        "unit-amulet",
    )
}

fn cross_zone_position(zone: &str) -> Value {
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["pp"] = json!({"current":10,"max":10});
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"recipient","card":"unit-follower"}]);
    initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"ex-recipient","card":"unit-follower"}]);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"remove","card":"unit-spell"}]);
    if initial["players"]["P1"]["zones"][zone].is_null() {
        initial["players"]["P1"]["zones"][zone] = json!([]);
    }
    initial["players"]["P1"]["zones"][zone]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"provider","card":"unit-amulet"}));
    initial
}

#[test]
fn cross_zone_keyword_sources_work_in_field_and_ex_and_stop_when_the_source_leaves() {
    let provider = lowerer()
        .lower_with_printed_costs(
            &cross_zone_tree(&json!(["ex", "field"])),
            &BTreeMap::from([("unit-amulet".to_owned(), Nat(1))]),
        )
        .unwrap()
        .remove("unit-amulet")
        .unwrap();
    let removal = equipment_played::tree(
        &json!({"spell":{
            "targets":[{"bind":"provider","from":{"who":"self","zone":"field","kind":"amulet","name":"unit-amulet"},"n":1}],
            "do":[{"destroy":{"ref":"provider"}}]
        }}),
        "unit-spell",
    );
    let remover = lowerer()
        .lower_with_printed_costs(
            &removal,
            &BTreeMap::from([("unit-spell".to_owned(), Nat(1))]),
        )
        .unwrap()
        .remove("unit-spell")
        .unwrap();
    for zone in ["field", "ex", "cemetery", "hand", "deck"] {
        let initial = cross_zone_position(zone);
        let mut game = Game::new(
            keyword_catalog(),
            &initial,
            &Value::Null,
            &Value::Null,
            "cross-zone-keywords",
        )
        .unwrap();
        game.load_internal_card("unit-amulet", &provider).unwrap();
        game.load_internal_card("unit-follower", &card(vec![]))
            .unwrap();
        game.load_internal_card("unit-spell", &remover).unwrap();
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(
            view["objects"]["recipient"]["keywords"],
            if ["field", "ex"].contains(&zone) {
                json!(["突進"])
            } else {
                json!([])
            }
        );
        assert_eq!(view["objects"]["f"]["keywords"], json!([]));
        assert_eq!(view["objects"]["ex-recipient"]["keywords"], json!([]));
        if !["field", "ex"].contains(&zone) {
            continue;
        }
        if zone == "ex" {
            assert!(matches!(
                game.play_internal_card("provider", PlayerId { seat: 1 })
                    .unwrap(),
                CardPlay::Entered(_)
            ));
            while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
                game.resume_internal_card().unwrap();
            }
            assert_eq!(
                game.projection(View::Referee).unwrap()["objects"]["recipient"]["keywords"],
                json!(["突進"])
            );
        }
        let CardPlay::Awaiting(q) = game
            .play_internal_card("remove", PlayerId { seat: 1 })
            .unwrap()
        else {
            panic!("provider target")
        };
        let InputDomain::Targets(domain) = q.domain else {
            panic!("provider target domain")
        };
        assert_eq!(domain.members.len(), 1);
        resolved(
            game.answer_internal_card(
                q.actor,
                q.token,
                Answer::Targets(vec![domain.members[0].handle]),
            )
            .unwrap(),
        );
        while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["recipient"]["keywords"],
            json!([])
        );
    }
}

#[test]
fn cross_zone_keyword_sources_reject_unneeded_zones_and_conditional_raw_ir() {
    for zones in [
        json!("ex"),
        json!(["field", "field"]),
        json!(["field", "cemetery"]),
        json!(["field", "ex", "cemetery"]),
    ] {
        lowerer().lower(&cross_zone_tree(&zones)).unwrap_err();
    }
    let mut conditional = conditional_keyword();
    conditional.node.active_zones = vec![ZoneKind::Field, ZoneKind::Ex];
    let Effect::Continuous {
        active_zones,
        patch,
        ..
    } = &mut conditional.node.declarations[0]
    else {
        panic!("continuous keyword")
    };
    *active_zones = vec![ZoneKind::Field, ZoneKind::Ex];
    *patch = CharacteristicPatch::GrantAbility {
        abilities: vec![GrantedAbility::Keyword {
            keyword: KeywordId::Rush,
        }],
    };
    let program = card(vec![conditional]);
    let mut game = Game::new(
        keyword_catalog(),
        &setup(1, json!([])),
        &Value::Null,
        &Value::Null,
        "raw-cross-zone",
    )
    .unwrap();
    game.load_internal_card("unit-follower", &program)
        .unwrap_err();
}

#[test]
fn cross_zone_keyword_sources_keep_other_keywords_and_raw_zone_orders_closed() {
    for keywords in [json!(["ward"]), json!(["rush", "storm"])] {
        let mut tree = cross_zone_tree(&json!(["field", "ex"]));
        tree["cards"]["unit-amulet"]["abilities"][0]["static"]["effects"][0]["grant"]["keywords"] =
            keywords;
        lowerer().lower(&tree).unwrap_err();
    }
    let seed = lowerer()
        .lower(&cross_zone_tree(&json!(["field", "ex"])))
        .unwrap()
        .remove("unit-amulet")
        .unwrap();
    for reverse in [false, true] {
        let mut raw = seed.card().clone();
        let ability = &mut raw.abilities[0].node;
        let Effect::Continuous {
            active_zones,
            patch,
            ..
        } = &mut ability.declarations[0]
        else {
            panic!("The seed has one keyword declaration.")
        };
        if reverse {
            ability.active_zones = vec![ZoneKind::Ex, ZoneKind::Field];
            *active_zones = ability.active_zones.clone();
        } else {
            *patch = CharacteristicPatch::GrantAbility {
                abilities: vec![GrantedAbility::Keyword {
                    keyword: KeywordId::Ward,
                }],
            };
        }
        let checked = WellFormedCardProgram::check(raw).unwrap();
        let mut game = Game::new(
            keyword_catalog(),
            &setup(1, json!([])),
            &Value::Null,
            &Value::Null,
            "raw-cross-zone-variant",
        )
        .unwrap();
        game.load_internal_card("unit-amulet", &checked)
            .unwrap_err();
    }
}

#[path = "static_keywords/conditions.rs"]
mod conditions;

#[test]
fn static_keyword_power_equality_rechecks_both_sides_of_the_boundary() {
    let provider = lowerer().lower(&equipment_played::tree(&json!({"static":{"effects":[{"grant":{
        "on":{"who":"self","zone":"field","kind":"follower","where":{"cmp":{"left":{"power_of":"item"},"op":"eq","right":1}}},
        "keywords":["aura"]
    }}]}}),"unit-amulet")).unwrap().remove("unit-amulet").unwrap();
    let receiver = lowerer()
        .lower(&equipment_played::tree(
            &json!({"activate":{"cost":{"pp":0},"do":[{"buff":{"on":"this","power":1,"hp":0}}]}}),
            "unit-follower",
        ))
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    let id = receiver.card().abilities[0].node.id;
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"provider","card":"unit-amulet"},
        {"id":"below","card":"unit-follower","state":{"power":0_u8}},
        {"id":"equal","card":"unit-follower","state":{"power":1_u8}},
        {"id":"above","card":"unit-follower","state":{"power":2_u8}}
    ]);
    initial["players"]["P2"]["zones"]["field"][0]["state"] = json!({"power":1_u8});
    let mut game = Game::new(
        keyword_catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "power-equality",
    )
    .unwrap();
    game.load_internal_card("unit-amulet", &provider).unwrap();
    game.load_internal_card("unit-follower", &receiver).unwrap();
    game.load_internal_card("unit-spell", &card(vec![]))
        .unwrap();
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["equal"]["keywords"], json!(["オーラ"]));
    for object in ["below", "above", "f"] {
        assert_eq!(state["objects"][object]["keywords"], json!([]));
    }
    for object in ["below", "equal"] {
        resolved(
            game.activate_internal_card(object, id, PlayerId { seat: 1 })
                .unwrap(),
        );
        while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
    }
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["below"]["keywords"], json!(["オーラ"]));
    assert_eq!(state["objects"]["equal"]["keywords"], json!([]));
}
