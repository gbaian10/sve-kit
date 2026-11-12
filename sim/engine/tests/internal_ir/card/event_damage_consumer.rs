#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate the event recipient scenarios."
)]
use super::*;
use sve_engine::game::internal::{Answer, InputDomain};

fn observer(who: &str) -> WellFormedCardProgram {
    lowerer()
        .lower(&equipment_played::tree(
            &json!({"auto":{
                "on":{"enter_field":{"who":who,"sel":{"kind":"follower"}}},
                "do":[{"damage":{"to":"it","n":1}}]
            }}),
            "unit-amulet",
        ))
        .unwrap()
        .remove("unit-amulet")
        .unwrap()
}

fn initial() -> Value {
    let mut state = setup(1, json!([{"id":"draw","card":"unit-follower"}]));
    state["players"]["P1"]["zones"]["field"] = json!([{"id":"observer","card":"unit-amulet"}]);
    state["players"]["P1"]["zones"]["hand"] = json!([{"id":"incoming","card":"unit-follower"}]);
    state
}

fn board(
    state: &Value,
    observer: &WellFormedCardProgram,
    spell: Option<&WellFormedCardProgram>,
) -> Game {
    let mut game = Game::new(
        static_keywords::keyword_catalog(),
        state,
        &Value::Null,
        &Value::Null,
        "event-damage-recipient",
    )
    .unwrap();
    game.load_internal_card("unit-amulet", observer).unwrap();
    let mut aura = static_keywords::conditional_keyword();
    let Effect::Continuous {
        condition, patch, ..
    } = &mut aura.node.declarations[0]
    else {
        panic!("static keyword")
    };
    *condition = None;
    *patch = CharacteristicPatch::GrantAbility {
        abilities: vec![GrantedAbility::Keyword {
            keyword: KeywordId::Aura,
        }],
    };
    game.load_internal_card("unit-follower", &card(vec![aura]))
        .unwrap();
    let empty = card(vec![]);
    game.load_internal_card("unit-spell", spell.unwrap_or(&empty))
        .unwrap();
    game
}

fn finish_checks(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}

fn resolve_all(game: &mut Game) -> usize {
    finish_checks(game);
    let mut count = 0_usize;
    while let Some(question) = game.internal_pending_choice().unwrap() {
        let InputDomain::Pending(options) = &question.domain else {
            panic!("event pending without a formal target question")
        };
        resolved(
            game.answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap(),
        );
        finish_checks(game);
        count = count.checked_add(1).unwrap();
        assert!(
            count <= 2,
            "the fixture produces at most two entry occurrences"
        );
    }
    count
}

#[test]
fn author_damage_and_raw_union_read_the_event_subject_without_formal_targeting() {
    for union in [false, true] {
        let mut raw = observer("opp").card().clone();
        if union {
            let Node::Instruction { instruction } =
                &mut raw.abilities[0].node.body.as_mut().unwrap().steps[0].node
            else {
                panic!("damage")
            };
            let Action::Damage { recipients, .. } = &mut instruction.action else {
                panic!("damage recipients")
            };
            *recipients = RecipientSet::Union {
                sets: vec![
                    recipients.clone(),
                    RecipientSet::Leaders {
                        players: players(PlayerRelation::Opponents),
                    },
                ],
            };
        }
        let program = WellFormedCardProgram::check(raw).unwrap();
        let mut state = initial();
        state["turn"]["active"] = json!("P2");
        state["players"]["P1"]["zones"]["hand"] = json!([]);
        state["players"]["P2"]["zones"]["hand"] = json!([{"id":"incoming","card":"unit-follower"}]);
        state["players"]["P2"]["zones"]["deck"] =
            json!([{"id":"opponent-draw","card":"unit-follower"}]);
        let mut game = board(&state, &program, None);
        assert!(matches!(
            game.play_internal_card("incoming", PlayerId { seat: 2 })
                .unwrap(),
            CardPlay::Entered(_)
        ));
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["incoming"]["keywords"],
            json!(["オーラ"])
        );
        assert_eq!(resolve_all(&mut game), 1);
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["objects"]["incoming"]["damage"], 1);
        assert_eq!(view["objects"]["f"]["damage"], 0);
        assert_eq!(view["P1"]["leader"]["life"], 20);
        assert_eq!(view["P2"]["leader"]["life"], if union { 19 } else { 20 });
    }
}

#[test]
fn event_damage_does_not_follow_a_subject_into_a_later_generation() {
    let movement = |id, from, destination| {
        let mut objects = zone(from, 1);
        let Selector::Zone {
            players,
            binding,
            predicate,
            ..
        } = &mut objects
        else {
            panic!("public movement selector")
        };
        players.relation = PlayerRelation::SelfPlayer;
        *binding = Binding::new(100_u64.checked_add(id).unwrap(), ScopeId(1));
        **predicate = Expr::Compute(Box::new(Predicate::KindEquals {
            left: Expr::Live(Box::new(KindRead::Kind {
                object: ObjectView::Current {
                    object: Box::new(ObjectOperand::Bound { binding: *binding }),
                },
                view: CharacteristicView::Effective,
            })),
            right: Expr::Literal(CardKind::Follower),
        }));
        instruction(
            id,
            Action::Move {
                objects,
                movement: MoveSpec {
                    to: destination,
                    initial_posture: None,
                    entry_effects: vec![],
                    position: None,
                    order: None,
                },
                reason: MoveReason::Put,
            },
        )
    };
    let entry = MoveDestination::Shared {
        zone: ZoneRef {
            player: PlayerRef::Controller {},
            kind: ZoneKind::Field,
        },
    };
    let producer = card(vec![play::spell(vec![
        movement(1, ZoneKind::Cemetery, entry.clone()),
        movement(
            2,
            ZoneKind::Field,
            MoveDestination::Owners {
                zone: OwnerZone::Cemetery,
            },
        ),
        movement(3, ZoneKind::Cemetery, entry),
    ])]);
    let mut state = initial();
    state["players"]["P1"]["zones"]["hand"] = json!([{"id":"s","card":"unit-spell"}]);
    state["players"]["P1"]["zones"]["cemetery"] = json!([{"id":"incoming","card":"unit-follower"}]);
    let mut game = board(&state, &observer("self"), Some(&producer));
    resolved(game.play_internal_card("s", PlayerId { seat: 1 }).unwrap());
    assert_eq!(resolve_all(&mut game), 2);
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["incoming"]["damage"],
        1
    );
}

#[test]
fn damage_subject_shorthand_needs_an_object_event() {
    for head in [
        json!({"auto":{"on":{"phase":{"whose":"self","which":"end"}},"do":[]}}),
        json!({"activate":{"cost":{"pp":0},"do":[]}}),
    ] {
        let mut tree = equipment_played::tree(&head, "unit-amulet");
        let entry = &mut tree["cards"]["unit-amulet"]["abilities"][0];
        let family = if head.get("auto").is_some() {
            "auto"
        } else {
            "activate"
        };
        entry[family]["do"] = json!([{"damage":{"to":{"leader":"opp"},"n":1}}]);
        lowerer().lower(&tree).unwrap();
        tree["cards"]["unit-amulet"]["abilities"][0][family]["do"][0]["damage"]["to"] = json!("it");
        assert_eq!(
            lowerer().lower(&tree).unwrap_err().code,
            "unsupported_construct"
        );
    }
}

#[test]
fn the_recipient_permission_does_not_authorize_event_objects_as_damage_sources() {
    let valid = observer("self");
    let mut raw = valid.card().clone();
    let body = raw.abilities[0].node.body.as_mut().unwrap();
    let Node::Instruction { instruction } = &mut body.steps[0].node else {
        panic!("damage")
    };
    let Action::Damage {
        recipients:
            RecipientSet::Objects {
                objects: Selector::Objects { objects },
            },
        source,
        ..
    } = &mut instruction.action
    else {
        panic!("event recipient")
    };
    *source = ObjectView::Current {
        object: Box::new(objects[0].clone()),
    };
    let invalid = WellFormedCardProgram::check(raw).unwrap();
    let mut game = Game::new(
        catalog(),
        &initial(),
        &Value::Null,
        &Value::Null,
        "source-boundary",
    )
    .unwrap();
    let error = game
        .load_internal_card("unit-amulet", &invalid)
        .unwrap_err();
    assert!(
        matches!(&error, sve_engine::EngineFailure::Unsupported(message)
            if message.contains("requirements") && message.contains("/action/source/object")),
        "the event recipient does not authorize the damage source: {error:?}"
    );
}

#[test]
fn a_foreign_event_cannot_supply_the_damage_recipient() {
    for frozen in [
        ObjectFrozen::AttackAttacker {
            event: Binding::new(999, ScopeId(999)),
        },
        ObjectFrozen::EnterFieldObject {
            event: Binding::new(999, ScopeId(999)),
        },
    ] {
        let mut raw = observer("self").card().clone();
        let Node::Instruction { instruction } =
            &mut raw.abilities[0].node.body.as_mut().unwrap().steps[0].node
        else {
            panic!("damage")
        };
        let Action::Damage {
            recipients:
                RecipientSet::Objects {
                    objects: Selector::Objects { objects },
                },
            ..
        } = &mut instruction.action
        else {
            panic!("event recipient")
        };
        objects[0] = ObjectOperand::FromExpression {
            value: Box::new(Expr::Frozen(Box::new(frozen))),
        };
        let error = WellFormedCardProgram::check(raw).unwrap_err();
        assert_eq!(error.reason, WellFormedReason::UnsupportedShape);
    }
}
