use super::*;

fn own_attack(scope: ScopeId, id: u64) -> Expr<Bool> {
    let binding = Binding::new(id, scope);
    Expr::Compute(Box::new(Predicate::Exists {
        objects: Selector::Zone {
            players: players(PlayerRelation::SelfPlayer),
            zone: ZoneKind::Field,
            view: SelectionView::Current {},
            binding,
            predicate: Box::new(Expr::Compute(Box::new(Predicate::CompareInt {
                left: source_information::source_read(
                    &ObjectView::Current {
                        object: Box::new(ObjectOperand::Bound { binding }),
                    },
                    CharacteristicView::Effective,
                    Some(Stat::Attack),
                ),
                comparison: Comparison::Equal,
                right: Expr::Literal(Int(2)),
            }))),
        },
    }))
}

fn finish(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}

#[test]
fn all_providers_read_the_sources_live_attack_without_reentering_keywords() {
    let mut tree = input(&effects(), false);
    let query_tree = equipment_played::tree(
        &json!({"activate":{
            "cost":{"pp":0},
            "targets":[{"bind":"chosen","from":{"who":"self","zone":"field","kind":"follower","power_ge":2,"power_le":2},"n":1}],
            "do":[{"buff":{"on":"chosen","power":0}}]
        }}),
        "unit-follower",
    );
    let mut query = query_tree["cards"]["unit-follower"]["abilities"][0].clone();
    query["line"] = json!(98);
    tree["cards"]["unit-follower"]["abilities"]
        .as_array_mut()
        .unwrap()
        .push(query);
    let mut raw = lowerer()
        .lower(&tree)
        .unwrap()
        .remove("unit-follower")
        .unwrap()
        .card()
        .clone();
    let query = raw.abilities[1].node.id;
    let scope = raw.abilities[0].node.scope;
    for (effect, id) in raw.abilities[0].node.declarations.iter_mut().zip(900_u64..) {
        match effect {
            Effect::Continuous { condition, .. } => *condition = Some(own_attack(scope, id)),
            Effect::Replacement { condition, .. } => *condition = own_attack(scope, id),
            Effect::CostModifier { duration, .. } => {
                duration.end = DurationEnd::While {
                    predicate: own_attack(scope, id),
                };
            }
            Effect::Permission { .. } | Effect::RuleModifier { .. } | Effect::DeckRule { .. } => {
                panic!("three providers")
            }
        }
    }
    raw.abilities
        .push(ability(99, vec![numeric::buff(99, 1, 0)]));
    raw.abilities
        .push(ability(100, vec![numeric::buff(100, -1, 0)]));
    let program = WellFormedCardProgram::check(raw).unwrap();
    let mut game = board(&program, 7);
    for active in [true, false, true] {
        let before = game.projection(View::Referee).unwrap();
        assert_eq!(
            before["objects"]["provider"]["power"],
            if active { 2 } else { 3 }
        );
        let mut legality = game.clone();
        let target = legality.activate_internal_card("provider", query, PlayerId { seat: 1 });
        if active {
            let CardPlay::Awaiting(question) = target.unwrap() else {
                panic!("legal public target")
            };
            let InputDomain::Targets(domain) = question.domain else {
                panic!("target domain")
            };
            assert_eq!(domain.members.len(), 1);
        } else {
            target.unwrap_err();
        }
        assert_effects(&game, active, false);
        assert_effects(&game, active, false);
        assert_eq!(
            game.projection(View::Referee).unwrap(),
            before,
            "read-only projection and quotes"
        );
        if active {
            resolved(
                game.activate_internal_card("provider", AbilityId(99), PlayerId { seat: 1 })
                    .unwrap(),
            );
        } else {
            resolved(
                game.activate_internal_card("provider", AbilityId(100), PlayerId { seat: 1 })
                    .unwrap(),
            );
        }
        finish(&mut game);
    }
}

fn lifetime_game() -> Game {
    let mut effects = effects();
    effects[0]["grant"]["on"] = json!({"who":"self","zone":"field","kind":"follower"});
    let mut raw = program(&effects, false).card().clone();
    raw.abilities.push(ability(
        99,
        vec![instruction(
            99,
            Action::Grant {
                subjects: zone(ZoneKind::Field, 199),
                abilities: vec![GrantedAbility::Keyword {
                    keyword: KeywordId::Bane,
                }],
                duration: Duration {
                    end: DurationEnd::CurrentTurnEnd {},
                    anchor: TimeAnchor::Installation {},
                    usage: None,
                    intervals: vec![],
                },
                provider: TextProvider {
                    provider: ObjectView::Current {
                        object: Box::new(ObjectOperand::SourceHolder {}),
                    },
                    face: FaceId("0".into()),
                    ability: AbilityId(99),
                },
            },
        )],
    ));
    raw.abilities.push(ability(
        100,
        vec![movement::put(
            100,
            Selector::Objects {
                objects: vec![ObjectOperand::SourceHolder {}],
            },
            OwnerZone::Hand,
        )],
    ));
    let program = WellFormedCardProgram::check(raw).unwrap();
    let mut position = initial(10);
    position["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"recipient","card":"unit-follower"}));
    position["players"]["P1"]["zones"]["field"][1]["card"] = json!("unit-other");
    position["players"]["P1"]["zones"]["hand"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"late","card":"unit-other"}));
    let (snapshot, _, document) = catalog_documents(&json!("3"));
    let mut row: Value = serde_json::from_str(
        snapshot
            .lines()
            .find(|line| line.contains("unit-follower"))
            .unwrap(),
    )
    .unwrap();
    row["number"] = json!("unit-other");
    let snapshot = format!("{snapshot}\n{row}");
    let mut document: Value = serde_json::from_str(&document).unwrap();
    document["cards"]["unit-other"] = json!({"status":"complete","abilities":[]});
    let catalog = Arc::new(
        Catalog::from_documents(
            &snapshot,
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../authored/rules/keywords.yaml"
            )),
            &[("unit.yaml".into(), document.to_string())],
        )
        .unwrap(),
    );
    let mut game = Game::new(
        catalog,
        &position,
        &Value::Null,
        &Value::Null,
        "provider-lifetimes",
    )
    .unwrap();
    game.load_internal_card("unit-follower", &program).unwrap();
    game.load_internal_card("unit-other", &card(vec![]))
        .unwrap();
    game.load_internal_card("unit-spell", &spell()).unwrap();
    game
}

#[test]
fn source_departure_stops_live_providers_but_keeps_the_installed_turn_grant() {
    let mut game = lifetime_game();
    assert_effects(&game, true, false);
    resolved(
        game.activate_internal_card("provider", AbilityId(99), PlayerId { seat: 1 })
            .unwrap(),
    );
    finish(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["recipient"]["keywords"],
        json!(["必殺", "疾走"])
    );
    assert!(matches!(
        game.play_internal_card("late", PlayerId { seat: 1 })
            .unwrap(),
        CardPlay::Entered(_)
    ));
    finish(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["late"]["keywords"],
        json!(["疾走"])
    );
    resolved(
        game.activate_internal_card("provider", AbilityId(100), PlayerId { seat: 1 })
            .unwrap(),
    );
    finish(&mut game);
    assert_effects(&game, false, false);
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["provider"]["generation"],
        1
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["late"]["keywords"],
        json!([])
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["recipient"]["keywords"],
        json!(["必殺"])
    );
    assert!(matches!(
        game.play_internal_card("provider", PlayerId { seat: 1 })
            .unwrap(),
        CardPlay::Entered(_)
    ));
    finish(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["provider"]["generation"],
        3
    );
    assert_effects(&game, true, false);
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["recipient"]["keywords"],
        json!(["必殺", "疾走"])
    );
    for _ in 0..2 {
        game.internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap();
        quick::pass(&mut game);
    }
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["recipient"]["keywords"],
        json!(["疾走"])
    );
}
