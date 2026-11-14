#![expect(
    clippy::panic,
    reason = "Unexpected player shapes fail the synthetic scenarios."
)]

use super::*;
use crate::player_pipeline::{answer, question};
use sve_engine::game::player::events::{
    ActionKind, EventError, EventKind, MovementMethod, PlayerEventBatch,
};
use sve_engine::game::player::{
    AttackTarget, Command, DecisionShape, MainActionAnswer, MainIntentKind, QuestionShape,
    SubmissionKind, Viewer,
};

fn batches(game: &Game, viewer: Viewer) -> Vec<PlayerEventBatch> {
    let batches = game
        .event_batches(viewer, None)
        .unwrap()
        .cloned()
        .collect::<Vec<_>>();
    assert!(batches.iter().all(|batch| !batch.events.is_empty()));
    assert_eq!(
        batches
            .iter()
            .flat_map(|batch| &batch.events)
            .collect::<Vec<_>>(),
        game.events(viewer, None).unwrap().collect::<Vec<_>>(),
        "every committed occurrence belongs to exactly one complete batch"
    );
    batches
}

fn actions(batch: &PlayerEventBatch, kind: ActionKind) -> bool {
    batch
        .events
        .iter()
        .all(|event| matches!(&event.kind, EventKind::Action(action) if action.kind == kind))
}

fn aoe_targets(scope: u64) -> RecipientSet {
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
                    binding: Binding::new(91, ScopeId(scope)),
                    predicate: Box::new(Expr::Literal(Bool(true))),
                },
            },
        ],
    }
}

fn aoe() -> Game {
    let mut initial = setup(1, json!([]));
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"f","card":"unit-follower"},
        {"id":"g","card":"unit-follower"}
    ]);
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "event-batches",
    )
    .unwrap();
    game.advance_player_input();
    game
}

#[test]
fn event_batches_aoe_and_consecutive_damage_freeze_each_hits_life_then_destroy_together() {
    let mut game = aoe();
    game.execute_internal(
        &checked(vec![
            damage(1, aoe_targets(98), 1),
            damage(2, aoe_targets(99), 2),
        ]),
        &source("synthetic"),
        context(1),
    )
    .unwrap();
    game.advance_player_input();
    for viewer in [Viewer::P1, Viewer::P2] {
        let history = batches(&game, viewer);
        assert_eq!(history.len(), 4);
        for (batch, before, after, amount) in [
            (&history[0], [20, 3, 3], [19, 2, 2], 1),
            (&history[1], [19, 2, 2], [17, 0, 0], 2),
        ] {
            assert!(actions(batch, ActionKind::Damage));
            assert_eq!(batch.events.len(), 3, "the entire AoE is sealed together");
            for ((event, before), after) in batch.events.iter().zip(before).zip(after) {
                let EventKind::Action(action) = &event.kind else {
                    panic!("damage");
                };
                assert_eq!(action.numbers.len(), 1);
                assert_eq!(action.numbers[0].amount, amount);
                assert_eq!(action.numbers[0].before, Some(before));
                assert_eq!(action.numbers[0].after, Some(after));
            }
        }
        assert!(actions(&history[2], ActionKind::Destroy));
        assert_eq!(
            history[2].events.len(),
            2,
            "simultaneous destruction attempts"
        );
        assert_eq!(
            history[3].events.len(),
            2,
            "successful deaths form their own later batch"
        );
        assert!(history[3].events.iter().all(|event| matches!(&event.kind,
            EventKind::Movement { method: MovementMethod::RuleDestroy, before, after }
            if before.zone == Some(sve_engine::game::player::observation::ObservedZone::Field)
            && after.zone == Some(sve_engine::game::player::observation::ObservedZone::Cemetery))));
        let first = &history[0].events;
        let other = if viewer == Viewer::P1 {
            Viewer::P2
        } else {
            Viewer::P1
        };
        assert_eq!(
            game.event_batches(other, Some(&first[2].id)).err(),
            Some(EventError::InvalidCursor)
        );
        assert_eq!(
            game.event_batches(viewer, Some(&first[0].id)).err(),
            Some(EventError::InvalidCursor),
            "a cursor cannot request a partial simultaneous batch"
        );
        assert_eq!(
            game.event_batches(viewer, Some(&first[2].id))
                .unwrap()
                .count(),
            3
        );
        assert_eq!(
            batches(&game, viewer),
            history,
            "reads never append members to sealed batches"
        );
    }
}

#[test]
fn event_batches_combat_seals_both_sides_before_simultaneous_deaths() {
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"attacker","card":"unit-follower","state":{"power":3,"entered_this_turn":false}}
    ]);
    initial["players"]["P2"]["zones"]["field"][0]["state"] = json!({"power":3,"acted":true});
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "combat-batches",
    )
    .unwrap();
    game.load_internal_card("unit-follower", &card(vec![]))
        .unwrap();
    game.advance_player_input();
    let main = question(&game, Viewer::P1);
    let QuestionShape::ActionWindow(window) = &main.shape else {
        panic!("main");
    };
    let (intent, target) = window
        .intents
        .iter()
        .find_map(|intent| {
            let MainIntentKind::Attack { targets, .. } = &intent.kind else {
                return None;
            };
            targets
                .iter()
                .find(|target| matches!(target, AttackTarget::Card(_)))
                .map(|target| (intent, target))
        })
        .unwrap();
    assert_eq!(
        game.submit_player(
            Viewer::P1,
            Command::MainAction {
                token: main.token,
                intent: intent.id.clone(),
                answer: MainActionAnswer::Attack(target.clone()),
            }
        )
        .kind,
        SubmissionKind::Applied
    );
    assert!(matches!(
        question(&game, Viewer::P2).shape,
        QuestionShape::Decision(DecisionShape::Quick { .. })
    ));
    assert_eq!(
        answer(&mut game, Viewer::P2, Answer::Quick(None)).kind,
        SubmissionKind::Applied
    );
    for viewer in [Viewer::P1, Viewer::P2] {
        let history = batches(&game, viewer);
        let damage = history
            .iter()
            .position(|batch| actions(batch, ActionKind::Damage))
            .unwrap();
        assert_eq!(
            history[damage].events.len(),
            2,
            "attack and counter damage are simultaneous"
        );
        assert!(history[damage].events.iter().all(|event| matches!(&event.kind,
            EventKind::Action(action) if action.numbers[0].before == Some(3) && action.numbers[0].after == Some(0))));
        assert!(actions(&history[damage + 1], ActionKind::Destroy));
        assert_eq!(history[damage + 1].events.len(), 2);
        assert_eq!(history[damage + 2].events.len(), 2);
        assert!(history[damage + 2].events.iter().all(|event| matches!(
            &event.kind,
            EventKind::Movement {
                method: MovementMethod::RuleDestroy,
                ..
            }
        )));
    }
}

#[test]
fn event_batches_suspended_and_reverted_effects_never_publish_partial_aoe() {
    let mut game = aoe();
    game.execute_internal(
        &checked(vec![damage(1, recipients(), 1)]),
        &source("synthetic"),
        context(1),
    )
    .unwrap();
    game.advance_player_input();
    let prefix = [Viewer::P1, Viewer::P2].map(|viewer| batches(&game, viewer));
    game.execute_internal(
        &checked(vec![
            damage(1, recipients(), 1),
            up_to(2, 1),
            draw(3, 10_001),
        ]),
        &source("synthetic"),
        context(1),
    )
    .unwrap();
    game.advance_player_input();
    for (viewer, prefix) in [Viewer::P1, Viewer::P2].into_iter().zip(&prefix) {
        assert_eq!(&batches(&game, viewer), prefix);
    }
    assert_eq!(
        answer(
            &mut game,
            Viewer::P1,
            Answer::Stepwise(StepwiseAnswer::Stop)
        )
        .kind,
        SubmissionKind::EngineFault
    );
    for (viewer, prefix) in [Viewer::P1, Viewer::P2].into_iter().zip(&prefix) {
        assert_eq!(
            &batches(&game, viewer),
            prefix,
            "origin rollback removes every new member and keeps the prefix"
        );
    }
}

fn damaged_observer() -> Located<Ability> {
    let mut observer = ability(
        1,
        vec![instruction(
            1,
            Action::Life {
                player: PlayerRef::Controller {},
                change: NumericChange::Add {
                    amount: Expr::Literal(Int(1)),
                },
            },
        )],
    );
    let event = Binding::new(90, ScopeId(1));
    observer.node.kind = AbilityKind::Triggered;
    observer.node.play = None;
    observer.node.trigger = Some(TriggerSpec {
        on: TriggerCondition::Event {
            pattern: EventPattern::Damage {
                binding: event,
                predicate: Expr::Compute(Box::new(Predicate::And {
                    predicates: vec![Expr::Compute(Box::new(Predicate::RecipientIsFollower {
                        recipient: DamageRecipient::FromExpression {
                            value: Box::new(Expr::Frozen(Box::new(
                                RecipientFields::DamageRecipient { event },
                            ))),
                        },
                        objects: Selector::Objects {
                            objects: vec![ObjectOperand::SourceHolder {}],
                        },
                    }))],
                })),
            },
        },
        active_zones: vec![ZoneKind::Field],
        view: EventSnapshotPolicy::AfterEntry,
        limit: None,
        aggregation: TriggerAggregation::PerFact {},
    });
    observer
}

#[test]
fn event_batches_after_damage_triggers_run_after_the_whole_batch() {
    let recipients = RecipientSet::Objects {
        objects: Selector::Zone {
            players: players(PlayerRelation::Opponents),
            zone: ZoneKind::Field,
            view: SelectionView::Current {},
            binding: Binding::new(91, ScopeId(99)),
            predicate: Box::new(Expr::Literal(Bool(true))),
        },
    };
    let producer = ability(2, vec![damage(2, recipients, 1)]);
    let program = card(vec![damaged_observer(), producer]);
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"producer","card":"unit-follower"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"f","card":"unit-follower"}, {"id":"g","card":"unit-follower"}
    ]);
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "trigger-batches",
    )
    .unwrap();
    game.load_internal_card("unit-follower", &program).unwrap();
    game.advance_player_input();
    resolved(
        game.activate_internal_card("producer", AbilityId(2), PlayerId { seat: 1 })
            .unwrap(),
    );
    game.advance_player_input();
    let before = batches(&game, Viewer::P1);
    let damage_batches = before
        .iter()
        .filter(|batch| actions(batch, ActionKind::Damage))
        .collect::<Vec<_>>();
    assert_eq!(damage_batches.len(), 1);
    assert_eq!(damage_batches[0].events.len(), 2);
    assert!(!before.iter().any(|batch| actions(batch, ActionKind::Life)));
    let pending = question(&game, Viewer::P2);
    let QuestionShape::Decision(DecisionShape::Pending(options)) = &pending.shape else {
        panic!("pending");
    };
    assert_eq!(
        options.len(),
        2,
        "each damaged follower creates its own after-view occurrence"
    );
    assert_eq!(
        answer(&mut game, Viewer::P2, Answer::Pending(options[0].handle)).kind,
        SubmissionKind::Applied
    );
    let after = batches(&game, Viewer::P1);
    assert_eq!(&after[..before.len()], before);
    assert!(
        after[before.len()..]
            .iter()
            .any(|batch| actions(batch, ActionKind::Life))
    );
}

#[test]
fn event_batches_put_in_cemetery_does_not_claim_destruction() {
    let mut game = aoe();
    game.execute_internal(
        &checked(vec![movement::put(
            1,
            zone(ZoneKind::Field, 99),
            OwnerZone::Cemetery,
        )]),
        &source("synthetic"),
        context(1),
    )
    .unwrap();
    game.advance_player_input();
    let history = batches(&game, Viewer::P1);
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].events.len(), 2);
    assert!(history[0].events.iter().all(|event| matches!(
        &event.kind,
        EventKind::Movement {
            method: MovementMethod::Effect(MoveMethod::Put),
            ..
        }
    )));
}

#[test]
fn event_batches_simultaneous_token_deaths_erase_together_after_moving() {
    let (snapshot, registry, document) = catalog_documents(&json!("1"));
    let snapshot = snapshot
        .lines()
        .map(|line| {
            let mut row: Value = serde_json::from_str(line).unwrap();
            if row["number"] == "unit-follower" {
                row["faces"][0]["card_type"] = json!("フォロワー・トークン");
            }
            row.to_string()
        })
        .collect::<Vec<_>>()
        .join("\n");
    let catalog = Arc::new(
        Catalog::from_documents(&snapshot, &registry, &[("unit.yaml".into(), document)]).unwrap(),
    );
    let mut initial = setup(1, json!([]));
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"f","card":"unit-follower"},
        {"id":"g","card":"unit-follower"}
    ]);
    let mut game = Game::new(
        catalog,
        &initial,
        &Value::Null,
        &Value::Null,
        "token-batches",
    )
    .unwrap();
    game.advance_player_input();
    game.execute_internal(
        &checked(vec![damage(1, aoe_targets(99), 3)]),
        &source("synthetic"),
        context(1),
    )
    .unwrap();
    game.advance_player_input();
    for viewer in [Viewer::P1, Viewer::P2] {
        let history = batches(&game, viewer);
        assert_eq!(history.len(), 4);
        assert!(actions(&history[0], ActionKind::Damage));
        assert!(actions(&history[1], ActionKind::Destroy));
        assert_eq!(history[2].events.len(), 2);
        assert!(history[2].events.iter().all(|event| matches!(
            &event.kind,
            EventKind::Movement {
                method: MovementMethod::RuleDestroy,
                ..
            }
        )));
        assert!(actions(&history[3], ActionKind::Erase));
        assert_eq!(
            history[3].events.len(),
            2,
            "both tokens erase in one later batch"
        );
    }
}

#[test]
fn event_batches_stack_replacements_counters_and_successful_movement_stay_together() {
    let (snapshot, _, document) = catalog_documents(&json!("1"));
    let mut document: Value = serde_json::from_str(&document).unwrap();
    document["cards"]["unit-spell"]["abilities"] = json!([{
        "kind":"spell","line":1,
        "body":{"op":"move","subjects":{"zone":"field","side":"opponent"},"to":"cemetery"}
    }]);
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
    let mut initial = setup(1, json!([]));
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"f","card":"unit-amulet","state":{"keywords":["stack"],"counters":{"stack_counter":2}}},
        {"id":"g","card":"unit-amulet","state":{"keywords":["stack"],"counters":{"stack_counter":2}}},
        {"id":"h","card":"unit-follower"}
    ]);
    let mut game = Game::new(
        catalog,
        &initial,
        &Value::Null,
        &Value::Null,
        "stack-batches",
    )
    .unwrap();
    game.advance_player_input();
    game.decide(&json!({"do":"play","card":"s"}), "stack-move")
        .unwrap();
    game.advance_player_input();
    for viewer in [Viewer::P1, Viewer::P2] {
        let history = batches(&game, viewer);
        let replaced = history
            .iter()
            .filter(|batch| {
                batch.events.iter().any(|event| matches!(
            &event.kind, EventKind::Action(action) if action.kind == ActionKind::Replacement
        ))
            })
            .collect::<Vec<_>>();
        assert_eq!(
            replaced.len(),
            1,
            "replacements belong to this movement batch"
        );
        let batch = replaced[0];
        assert_eq!(batch.events.len(), 5);
        for kind in [ActionKind::Replacement, ActionKind::Counter] {
            assert_eq!(
                batch
                    .events
                    .iter()
                    .filter(|event| matches!(
                        &event.kind, EventKind::Action(action) if action.kind == kind
                    ))
                    .count(),
                2
            );
        }
        assert_eq!(
            batch
                .events
                .iter()
                .filter(|event| matches!(
                    &event.kind,
                    EventKind::Movement {
                        method: MovementMethod::Procedure,
                        ..
                    }
                ))
                .count(),
            1
        );
    }
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["field"], json!(["f", "g"]));
    assert_eq!(state["P2"]["cemetery"], json!(["h"]));
    for id in ["f", "g"] {
        assert_eq!(state["objects"][id]["counters"]["スタックカウンター"], 1);
    }
}
