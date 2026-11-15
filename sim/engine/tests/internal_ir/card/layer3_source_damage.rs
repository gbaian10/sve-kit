use super::*;

fn filter(kind: &str, trait_id: &str) -> Value {
    json!({"who":"self","kind":kind,"trait":trait_id})
}

fn aura(source: &Value) -> WellFormedCardProgram {
    let mut tree = equipment_played::tree(
        &json!({"static":{"effects":[{
            "damage_mod":{"source":source,"kind":"ability","delta":1}
        }]}}),
        "unit-amulet",
    );
    tree["cards"]["unit-amulet"]["abilities"][0]["line"] = json!(1);
    lowerer()
        .lower(&tree)
        .unwrap()
        .remove("unit-amulet")
        .unwrap()
}

fn school_catalog(trait_name: &str) -> Arc<Catalog> {
    let (snapshot, keywords, documents) = catalog_documents(&json!("1"));
    let snapshot = snapshot
        .lines()
        .map(|line| {
            let mut row: Value = serde_json::from_str(line).unwrap();
            row["faces"][0]["traits"] = json!([trait_name]);
            row.to_string()
        })
        .collect::<Vec<_>>()
        .join("\n");
    Arc::new(
        Catalog::from_documents(&snapshot, &keywords, &[("unit.yaml".into(), documents)]).unwrap(),
    )
}

fn recipients() -> RecipientSet {
    let binding = Binding::new(90, ScopeId(90));
    RecipientSet::Union {
        sets: vec![
            RecipientSet::Leaders {
                players: players(PlayerRelation::Opponents),
            },
            RecipientSet::Objects {
                objects: Selector::Zone {
                    players: players(PlayerRelation::Opponents),
                    zone: ZoneKind::Field,
                    view: SelectionView::Current {},
                    binding,
                    predicate: Box::new(Expr::Compute(Box::new(Predicate::KindEquals {
                        left: Expr::Live(Box::new(KindRead::Kind {
                            object: ObjectView::Current {
                                object: Box::new(ObjectOperand::Bound { binding }),
                            },
                            view: CharacteristicView::Effective,
                        })),
                        right: Expr::Literal(CardKind::Follower),
                    }))),
                },
            },
        ],
    }
}

fn board(
    seat: u32,
    source_card: &str,
    trait_name: &str,
    copies: usize,
    hostile: bool,
    source_filter: &Value,
    steps: Vec<Located<Node>>,
) -> Game {
    let own = if seat == 1 { "P1" } else { "P2" };
    let other = if seat == 1 { "P2" } else { "P1" };
    let provider = if hostile { other } else { own };
    let mut initial = setup(seat, json!([]));
    initial["turn"]["active"] = json!(own);
    initial["players"][own]["zones"]["hand"] = json!([]);
    initial["players"][own]["zones"]["field"] = json!([]);
    initial["players"][other]["zones"]["field"] =
        json!([{"id":"f","card":"unit-follower","state":{"hp":40_i32}}]);
    initial["players"][own]["zones"][if source_card == "unit-spell" {
        "hand"
    } else {
        "field"
    }]
    .as_array_mut()
    .unwrap()
    .push(json!({"id":"s","card":source_card}));
    for index in 0..copies {
        initial["players"][provider]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":format!("provider-{index}"),"card":"unit-amulet"}));
    }
    let mut game = Game::new(
        school_catalog(trait_name),
        &initial,
        &Value::Null,
        &Value::Null,
        "source-damage",
    )
    .unwrap();
    game.load_internal_card("unit-amulet", &aura(source_filter))
        .unwrap();
    game.load_internal_card("unit-follower", &card(vec![ability(1, steps.clone())]))
        .unwrap();
    game.load_internal_card("unit-spell", &card(vec![play::spell(steps)]))
        .unwrap();
    game
}

#[test]
fn damage_source_requires_controller_kind_and_trait_and_stacks_for_each_recipient() {
    for seat in [1, 2] {
        for (source_card, trait_name, copies, hostile, amount) in [
            ("unit-spell", "academy", 1, false, 3),
            ("unit-spell", "other", 1, false, 2),
            ("unit-follower", "academy", 1, false, 2),
            ("unit-spell", "academy", 1, true, 2),
            ("unit-spell", "academy", 2, false, 4),
        ] {
            let mut game = board(
                seat,
                source_card,
                trait_name,
                copies,
                hostile,
                &filter("spell", "academy"),
                vec![damage(1, recipients(), 2)],
            );
            let result = if source_card == "unit-spell" {
                resolved(game.play_internal_card("s", PlayerId { seat }).unwrap())
            } else {
                activate(&mut game, seat)
            };
            let amounts = result
                .progress
                .facts
                .iter()
                .filter_map(|fact| {
                    if let Fact::Damage(event) = fact {
                        Some(event.amount.0.get())
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();
            assert_eq!(amounts, vec![amount, amount]);
            let other = if seat == 1 { "P2" } else { "P1" };
            let after = game.projection(View::Referee).unwrap();
            assert_eq!(after[other]["leader"]["life"], 20 - amount);
            assert_eq!(after["objects"]["f"]["hp"], 40 - amount);
        }
    }
}

#[test]
fn an_opposing_source_filter_uses_the_same_causal_controller() {
    let mut source = filter("spell", "academy");
    source["who"] = json!("opp");
    let mut game = board(
        1,
        "unit-spell",
        "academy",
        1,
        true,
        &source,
        vec![damage(1, recipients(), 2)],
    );
    resolved(game.play_internal_card("s", PlayerId { seat: 1 }).unwrap());
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
        17
    );
}

#[test]
fn a_departed_damage_source_keeps_its_granted_trait_and_old_generation() {
    let holder = Selector::Objects {
        objects: vec![ObjectOperand::SourceHolder {}],
    };
    let mut departed_hit = damage(3, recipients(), 2);
    let Node::Instruction { instruction } = &mut departed_hit.node else {
        panic!("damage instruction");
    };
    let Action::Damage { source, .. } = &mut instruction.action else {
        panic!("damage source");
    };
    *source = ObjectView::SourceInformation {};
    let steps = vec![
        numeric::modify(
            1,
            holder.clone(),
            CharacteristicPatch::AddTrait {
                trait_id: TraitId("bonus".into()),
            },
            DurationEnd::NoTemporalExpiry {},
        ),
        movement::put(2, holder, OwnerZone::Hand),
        departed_hit,
    ];
    let mut game = board(
        1,
        "unit-follower",
        "academy",
        1,
        false,
        &filter("follower", "bonus"),
        steps,
    );
    let result = activate(&mut game, 1);
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["P2"]["leader"]["life"], 17);
    assert_eq!(after["objects"]["f"]["hp"], 37);
    assert_eq!(after["objects"]["s"]["generation"], 1);
    assert_eq!(after["P1"]["hand"], json!(["s"]));
    assert_eq!(result.source.holder.generation, Generation(0));
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
    assert!(matches!(
        game.play_internal_card("s", PlayerId { seat: 1 }).unwrap(),
        CardPlay::Entered(_)
    ));
    let returned = game.projection(View::Referee).unwrap();
    assert_ne!(
        returned["objects"]["s"]["generation"],
        after["objects"]["s"]["generation"]
    );
    assert!(
        !returned["objects"]["s"]["traits"]
            .as_array()
            .unwrap()
            .contains(&json!("bonus"))
    );
}

#[test]
fn raw_source_characteristic_filters_keep_binding_and_ability_damage_boundaries() {
    let valid = aura(&filter("spell", "academy"));
    let mut raw = valid.card().clone();
    let Effect::Replacement {
        pattern:
            ProposedEventPattern::Damage {
                source: Some(source),
                ..
            },
        ..
    } = &mut raw.abilities[0].node.declarations[0]
    else {
        panic!("source filter");
    };
    source.binding = Binding::new(999, ScopeId(999));
    WellFormedCardProgram::check(raw).unwrap_err();
    let mut combat = valid.card().clone();
    let Effect::Replacement {
        pattern: ProposedEventPattern::Damage { kind, .. },
        ..
    } = &mut combat.abilities[0].node.declarations[0]
    else {
        panic!("damage");
    };
    *kind = DamageKind::Combat;
    let rejected = WellFormedCardProgram::check(combat).map_or(true, |program| {
        game(1, 0)
            .load_internal_card("unit-amulet", &program)
            .is_err()
    });
    assert!(rejected);
}
