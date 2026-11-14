#![expect(
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected questions or receipts invalidate the replacement scenarios."
)]
use super::*;
use sve_engine::game::internal::{InstructionOutcome, ResumeStage};

fn initial(silenced: bool, target: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"replacement","card":"unit-provider","state":{"silenced":silenced}}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"departing","card":target}
    ]);
    initial
}

fn finish(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}

#[test]
fn field_banish_replacement_redirects_effect_movement_and_destruction_without_lastword() {
    for destroy in [false, true] {
        let mut initial = initial(false, "unit-lastword");
        initial["players"]["P1"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"friendly","card":"unit-body"}));
        let action = if destroy {
            Action::Destroy {
                objects: field_name("subject"),
            }
        } else {
            move_named(ZoneKind::Field, OwnerZone::Cemetery, "subject")
        };
        let mut game = with_producer(&initial, action);
        let result = activate(&mut game, "P1");
        assert!(result.progress.receipts.values().any(|outcome| {
            let moves = match outcome {
                InstructionOutcome::Move(outcome) => &outcome.moved,
                InstructionOutcome::Destroy(outcome) => {
                    assert_eq!(
                        outcome.successful.len(),
                        1,
                        "the same-name producer on the provider's side is destroyed normally"
                    );
                    assert!(outcome.successful.iter().all(|member| {
                        let before = member.before.as_ref().unwrap().object;
                        outcome.moves.iter().any(|movement| {
                            movement.before == before
                                && movement
                                    .actual_to
                                    .is_some_and(|to| to.kind == ZoneKind::Cemetery)
                        })
                    }));
                    &outcome.moves
                }
                _ => return false,
            };
            moves.iter().any(|movement| {
                movement.requested_to.kind == ZoneKind::Cemetery
                    && movement
                        .actual_to
                        .is_some_and(|to| to.kind == ZoneKind::Banish)
            })
        }));
        finish(&mut game);
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["P2"]["banish"], json!(["departing"]));
        assert!(game.internal_pending_choice().unwrap().is_none());
        assert_eq!(view["P1"]["leader"]["life"], 20_u8);
        assert!(
            view["P1"]["field"]
                .as_array()
                .unwrap()
                .contains(&json!("friendly"))
        );
    }
}

#[test]
fn field_banish_replacement_stops_providing_after_silence_and_respects_banish_prohibition() {
    for (target, name, silenced) in [
        ("unit-body", "body", true),
        ("unit-immune", "immune", false),
    ] {
        let mut game = with_producer(
            &initial(silenced, target),
            move_named(ZoneKind::Field, OwnerZone::Cemetery, name),
        );
        activate(&mut game, "P1");
        finish(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P2"]["cemetery"],
            json!(["departing"])
        );
    }
}

#[test]
fn field_banish_replacement_replacement_uses_the_before_batch_field_even_when_its_source_leaves() {
    let mut game = with_producer(
        &initial(false, "unit-lastword"),
        movement(OwnerZone::Cemetery),
    );
    activate(&mut game, "P1");
    finish(&mut game);
    let view = game.projection(View::Referee).unwrap();
    assert!(
        view["P1"]["cemetery"]
            .as_array()
            .unwrap()
            .contains(&json!("replacement"))
    );
    assert_eq!(view["P2"]["banish"], json!(["departing"]));
    assert!(game.internal_pending_choice().unwrap().is_none());
}

#[test]
fn field_banish_replacement_relative_opponent_uses_control_and_returns_to_the_cards_owner() {
    let mut initial = draw_position();
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"replacement","card":"unit-provider"}
    ]);
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"departing","card":"unit-body","owner":"P2"}
    ]);
    let mut game = with_producer(
        &initial,
        move_named(ZoneKind::Field, OwnerZone::Cemetery, "body"),
    );
    activate(&mut game, "P1");
    finish(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["banish"],
        json!(["departing"])
    );
}

#[test]
fn field_banish_replacement_redirects_lethal_damage_rule_departures() {
    let action = Action::Damage {
        recipients: RecipientSet::Objects {
            objects: field_name("subject"),
        },
        amount: DamageAmount::Uniform {
            amount: Expr::Literal(Int(8)),
        },
        flags: DamageFlags::Ability,
        source: ObjectView::SourceInformation {},
        batch: BatchPolicy::Instruction {},
    };
    let mut game = with_producer(&initial(false, "unit-lastword"), action);
    activate(&mut game, "P1");
    finish(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["banish"],
        json!(["departing"])
    );
    assert!(game.internal_pending_choice().unwrap().is_none());
}

#[test]
fn field_banish_replacement_redirected_source_movement_still_pays_its_cost() {
    let mut initial = initial(false, "unit-body");
    initial["turn"]["active"] = json!("P2");
    let mut raw = producer(Action::Life {
        player: PlayerRef::Controller {},
        change: NumericChange::Add {
            amount: Expr::Literal(Int(1)),
        },
    })
    .card()
    .clone();
    let ability = &mut raw.abilities[0].node;
    ability.play.as_mut().unwrap().costs = CostSpec::Ordered {
        steps: vec![CostStep {
            materials: None,
            keyword: None,
            instruction: Box::new(Instruction {
                id: InstructionId(9),
                receipt: ReceiptId(9),
                source: ability.source.clone(),
                actor: PlayerRef::Controller {},
                context: InstructionContext::Cost,
                precondition: None,
                action: Action::Move {
                    objects: Selector::Objects {
                        objects: vec![ObjectOperand::SourceHolder {}],
                    },
                    movement: MoveSpec {
                        to: MoveDestination::Owners {
                            zone: OwnerZone::Cemetery,
                        },
                        position: None,
                        order: None,
                        initial_posture: None,
                        entry_effects: vec![],
                    },
                    reason: MoveReason::Put,
                },
            }),
        }],
    };
    let program = WellFormedCardProgram::check(raw).unwrap();
    let mut game = with_program(&initial, &program);
    activate(&mut game, "P2");
    finish(&mut game);
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P2"]["banish"], json!(["producer"]));
    assert_eq!(view["P2"]["leader"]["life"], 21_u8);
}

fn draw_position() -> Value {
    let mut initial = setup(1, json!([]));
    for player in ["P1", "P2"] {
        initial["players"][player]["pp"] = json!({"current":10_u8,"max":10_u8});
        initial["players"][player]["zones"]["field"] = json!([]);
        initial["players"][player]["zones"]["hand"] = json!([]);
    }
    initial
}
fn field_name(name: &str) -> Selector {
    Selector::Zone {
        players: players(PlayerRelation::All),
        zone: ZoneKind::Field,
        view: SelectionView::Current {},
        binding: Binding::new(45, ScopeId(1)),
        predicate: Box::new(Expr::Compute(Box::new(Predicate::NamesIntersect {
            left: Expr::Live(Box::new(NamesRead::Names {
                object: ObjectView::Current {
                    object: Box::new(ObjectOperand::Bound {
                        binding: Binding::new(45, ScopeId(1)),
                    }),
                },
            })),
            right: Expr::Literal(vec![CardName(name.into())]),
        }))),
    }
}
fn move_named(_from: ZoneKind, to: OwnerZone, name: &str) -> Action {
    Action::Move {
        objects: field_name(name),
        movement: MoveSpec {
            to: MoveDestination::Owners { zone: to },
            position: None,
            order: None,
            initial_posture: None,
            entry_effects: vec![],
        },
        reason: MoveReason::Put,
    }
}
fn movement(to: OwnerZone) -> Action {
    Action::Move {
        objects: zone(ZoneKind::Field, 71),
        movement: MoveSpec {
            to: MoveDestination::Owners { zone: to },
            position: None,
            order: None,
            initial_posture: None,
            entry_effects: vec![],
        },
        reason: MoveReason::Put,
    }
}
fn producer(action: Action) -> WellFormedCardProgram {
    let mut located = ability(1, vec![instruction(1, action)]);
    located.source.card_id = "producer".into();
    located.source.card_no = "producer".into();
    let span = located.source.clone();
    let origin = located.node.source.clone();
    locate_steps(
        &mut located.node.body.as_mut().unwrap().steps,
        &span,
        &origin,
    );
    card(vec![located])
}
fn activate(game: &mut Game, seat: &str) -> Activation {
    resolved(
        game.activate_internal_card(
            "producer",
            AbilityId(1),
            PlayerId {
                seat: if seat == "P1" { 1 } else { 2 },
            },
        )
        .unwrap(),
    )
}
fn with_producer(initial: &Value, action: Action) -> Game {
    with_program(initial, &producer(action))
}
fn with_program(initial: &Value, producer: &WellFormedCardProgram) -> Game {
    let rows = [
        ("unit-provider", "provider"),
        ("unit-lastword", "subject"),
        ("unit-body", "body"),
        ("unit-immune", "immune"),
        ("producer", "subject"),
    ];
    let snapshot=rows.iter().map(|(number,name)|json!({"number":number,"faces":[{
        "name":name,"card_class":"ニュートラル","card_type":"フォロワー","cost":"1","power":"2","hp":"3","traits":[],"text":null,"sections":[]
    }]}).to_string()).collect::<Vec<_>>().join("\n");
    let definitions = rows
        .iter()
        .map(|(number, _)| {
            (
                (*number).to_owned(),
                json!({"status":"complete","abilities":[]}),
            )
        })
        .collect::<serde_json::Map<_, _>>();
    let document = json!({"format":1,"kind":"effect_set","cards":definitions}).to_string();
    let registry = json!({"format":1,"kind":"keyword_registry","keywords":{}}).to_string();
    let catalog = Arc::new(
        Catalog::from_documents(&snapshot, &registry, &[("synthetic.yaml".into(), document)])
            .unwrap(),
    );
    let mut initial = initial.clone();
    let actor = initial["turn"]["active"].as_str().unwrap().to_owned();
    initial["players"][&actor]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"producer","card":"producer"}));
    let mut game = Game::new(catalog, &initial, &Value::Null, &Value::Null, "replacement").unwrap();
    for (number, head) in [
        (
            "unit-provider",
            json!({"static":{"effects":[{"banish_instead":{"from":"field","to":"cemetery","on":{"who":"opp","zone":"field","kind":"follower"}}}]}}),
        ),
        (
            "unit-lastword",
            json!({"lastword":{"do":[{"life":{"who":"self","delta":-1_i8}}]}}),
        ),
        (
            "unit-immune",
            json!({"static":{"effects":[{"cannot":{"on":"this","what":"banish"}}]}}),
        ),
    ] {
        let program = lowerer()
            .lower(&equipment_played::tree(&head, number))
            .unwrap()
            .remove(number)
            .unwrap();
        game.load_internal_card(number, &program).unwrap();
    }
    game.load_internal_card("unit-body", &card(vec![])).unwrap();
    game.load_internal_card("producer", producer).unwrap();
    game
}
