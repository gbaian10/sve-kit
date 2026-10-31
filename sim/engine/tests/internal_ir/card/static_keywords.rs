use super::*;
use sve_engine::game::internal::InputDomain;

fn conditional_keyword() -> Located<Ability> {
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

fn keyword_catalog() -> Arc<Catalog> {
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
