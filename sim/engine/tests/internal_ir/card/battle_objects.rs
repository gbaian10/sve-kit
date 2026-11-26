#[path = "battle_objects/ex_stats.rs"]
mod ex_stats;

use super::battle_pending::{board, board_with_program, catalog_for, fixture};
use super::*;
use crate::battle_costs::object;
use std::sync::OnceLock;
use sve_battle_contract::{MAX_SAFE_INTEGER, ObjectHandle, Seat, observation as wire};
use sve_engine::battle::ProjectionError;
use sve_engine::game::player::{Viewer, observation as native};

fn handle(game: &Game, viewer: Viewer, owner: Viewer, zone: native::ObservedZone) -> ObjectHandle {
    ObjectHandle::new(
        object(&game.observe(viewer).unwrap(), owner, zone)
            .as_str()
            .into(),
    )
}

fn position(
    catalog: Arc<Catalog>,
    initial: &Value,
    program: Option<&WellFormedCardProgram>,
) -> Game {
    let mut game = Game::new(catalog, initial, &Value::Null, &Value::Null, "wire-objects").unwrap();
    if let Some(program) = program {
        game.load_internal_card("unit-follower", program).unwrap();
        for number in ["unit-spell", "unit-amulet"] {
            game.load_internal_card(number, &card(vec![])).unwrap();
        }
    }
    game
}

fn initial(state: &Value) -> Value {
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"raw-source-first","card":"unit-follower","state":state},
        {"id":"raw-source-second","card":"unit-follower"}
    ]);
    initial
}

fn counter_catalog() -> Arc<Catalog> {
    static CATALOG: OnceLock<Arc<Catalog>> = OnceLock::new();
    Arc::clone(CATALOG.get_or_init(|| {
        let (_, _, document) = catalog_documents(&json!("1"));
        let registry = json!({"format":1,"kind":"keyword_registry","rule_names":{
            "known_counter":{"ja":"Known counter","role":"counter"}
        }});
        let rules = rule_fixtures::rules(
            &fixture().snapshot,
            json!([rule_fixtures::title(
                "synthetic",
                "unit-follower",
                json!([])
            )]),
            json!([]),
        );
        Arc::new(
            rule_fixtures::catalog(
                &fixture().snapshot,
                &registry.to_string(),
                &[("synthetic".into(), document)],
                &rules.to_string(),
            )
            .unwrap(),
        )
    }))
}

fn limited(maximum: u64) -> WellFormedCardProgram {
    let mut entry = ability(1, vec![draw(1, 0)]);
    entry.node.play.as_mut().unwrap().limit = Some(LimitSpec {
        maximum: Nat(maximum),
        window: LimitWindow::Turn,
        scope: LimitScope::AbilityInstance,
        consume_at: ConsumptionPoint::PlayCommitted,
    });
    card(vec![entry])
}

#[test]
fn wire_object_table_is_viewer_authorized_and_independent_of_opponent_secrets() {
    for viewer in [Viewer::P1, Viewer::P2] {
        let first = board(viewer, "unit-spell");
        let second = board(viewer, "unit-amulet");
        let before = exact_authority(&first);
        let table = first.battle_objects(viewer, &fixture().references).unwrap();
        assert_eq!(
            table,
            second
                .battle_objects(viewer, &fixture().references)
                .unwrap()
        );
        assert_eq!(table.len(), 2, "hidden hand has no object or placeholder");
        for value in table.values() {
            assert_eq!(value.card.printing_id, "printing-follower");
            assert_eq!(
                value.card.face.get(),
                1,
                "adopted face differs from native face0"
            );
            assert_eq!(value.card, value.effective_face);
            assert_eq!(value.knowledge, wire::Knowledge::Public);
            assert_eq!(value.facing, wire::Facing::Up);
        }
        let opponent = if viewer == Viewer::P1 {
            Viewer::P2
        } else {
            Viewer::P1
        };
        assert_ne!(
            first
                .battle_objects(opponent, &fixture().references)
                .unwrap(),
            second
                .battle_objects(opponent, &fixture().references)
                .unwrap()
        );
        assert_eq!(
            first.battle_objects(viewer, &fixture().references).unwrap(),
            table
        );
        assert_eq!(exact_authority(&first), before);
    }
}

#[test]
fn wire_object_table_keeps_copies_private_facing_and_authoritative_state() {
    let mut initial = initial(&json!({"acted":true,"power":9,"hp":1,"max_hp":3}));
    initial["players"]["P1"]["zones"]["field"][0]["owner"] = json!("P2");
    initial["players"]["P1"]["zones"]["hand"] =
        json!([{"id":"raw-private-card","card":"unit-spell"}]);
    let game = position(
        Arc::clone(&fixture().catalog),
        &initial,
        Some(&card(vec![])),
    );
    let before = exact_authority(&game);
    for viewer in [Viewer::P1, Viewer::P2] {
        let observed = game.observe(viewer).unwrap();
        let table = game.battle_objects(viewer, &fixture().references).unwrap();
        let field = &observed.players[0]
            .zones
            .iter()
            .find(|zone| zone.kind == native::ObservedZone::Field)
            .unwrap()
            .cards;
        let first = ObjectHandle::new(field[0].card.handle.as_str().into());
        let second = ObjectHandle::new(field[1].card.handle.as_str().into());
        assert_ne!(first, second);
        assert_eq!(table[&first].card, table[&second].card);
        let value = &table[&first];
        assert_eq!(value.owner, Seat::P2);
        assert_eq!(value.controller, Seat::P1);
        assert_eq!(value.characteristics.rules_names, ["unit-follower"]);
        assert_eq!(value.characteristics.card_type, "フォロワー");
        assert_eq!(value.characteristics.classes, None);
        assert_eq!(
            value.characteristics.cost, None,
            "printed cost is not a current quote"
        );
        let state = value.state.as_ref().unwrap();
        assert_eq!(state.posture, wire::Posture::Acted);
        assert_eq!(state.evolution, wire::EvolutionState::Base);
        assert_eq!(state.stats.as_ref().unwrap().attack.get(), 9);
        assert_eq!(state.stats.as_ref().unwrap().life.get(), 1);
        assert_eq!(state.quotas, Some(vec![]));
        let json = serde_json::to_string(&table).unwrap();
        for private in [
            "raw-source-first",
            "raw-source-second",
            "raw-private-card",
            "generation",
        ] {
            assert!(!json.contains(private));
        }
        if viewer == Viewer::P1 {
            let own = &table[&handle(&game, viewer, Viewer::P1, native::ObservedZone::Hand)];
            assert_eq!(own.knowledge, wire::Knowledge::Private);
            assert_eq!(own.facing, wire::Facing::Down);
            assert_eq!(own.state.as_ref().unwrap().stats, None);
        } else {
            assert_eq!(table.len(), 3);
        }
    }
    assert_eq!(exact_authority(&game), before);
}

#[test]
fn wire_object_table_distinguishes_physical_and_effective_faces() {
    let mut initial = initial(&json!({"evolved_with":"raw-evolution","super_evolved":true}));
    initial["players"]["P1"]["zones"]["evolution"] = json!([
        {"id":"raw-evolution","card":"unit-follower","state":{"face":1}}
    ]);
    let game = position(Arc::clone(&fixture().catalog), &initial, None);
    for viewer in [Viewer::P1, Viewer::P2] {
        assert!(
            game.observe(viewer).is_ok(),
            "the synthetic evolved position must be observable"
        );
        let table = game.battle_objects(viewer, &fixture().references).unwrap();
        let base = &table[&handle(&game, viewer, Viewer::P1, native::ObservedZone::Field)];
        let evolution = &table[&handle(&game, viewer, Viewer::P1, native::ObservedZone::Evolution)];
        assert_eq!(base.card.face.get(), 1);
        assert_eq!(base.effective_face.face.get(), 0);
        assert_eq!(base.effective_face, evolution.card);
        assert_eq!(
            base.state.as_ref().unwrap().evolution,
            wire::EvolutionState::SuperEvolved
        );
    }
}

#[test]
fn wire_object_table_checks_numbers_and_requires_known_counter_names() {
    for amount in [0, MAX_SAFE_INTEGER, MAX_SAFE_INTEGER + 1, u64::MAX] {
        let initial = initial(&json!({"counters":{"known_counter":amount}}));
        let game = position(counter_catalog(), &initial, Some(&card(vec![])));
        let projected = game.battle_objects(Viewer::P1, &fixture().references);
        if amount > MAX_SAFE_INTEGER {
            assert_eq!(projected, Err(ProjectionError::Range));
        } else {
            let table = projected.unwrap();
            let state = table[&handle(&game, Viewer::P1, Viewer::P1, native::ObservedZone::Field)]
                .state
                .as_ref()
                .unwrap();
            assert_eq!(state.counters[0].name, "Known counter");
            assert_eq!(state.counters[0].amount.get(), amount);
        }
    }
    let unknown = position(
        Arc::clone(&fixture().catalog),
        &initial(&json!({"counters":{"unknown_internal_counter":1}})),
        Some(&card(vec![])),
    );
    unknown.observe(Viewer::P1).unwrap();
    assert_eq!(
        unknown.battle_objects(Viewer::P1, &fixture().references),
        Err(ProjectionError::Unavailable)
    );
    let maximum = i64::try_from(MAX_SAFE_INTEGER).unwrap();
    for amount in [-maximum - 1, maximum + 1, i64::MIN, i64::MAX] {
        let game = position(
            Arc::clone(&fixture().catalog),
            &initial(&json!({"power":amount})),
            Some(&card(vec![])),
        );
        assert_eq!(
            game.battle_objects(Viewer::P1, &fixture().references),
            Err(ProjectionError::Range)
        );
    }
}

#[test]
fn wire_object_table_preserves_unknown_quotas_and_projects_real_limits() {
    let unprepared = position(Arc::clone(&fixture().catalog), &initial(&Value::Null), None);
    let table = unprepared
        .battle_objects(Viewer::P1, &fixture().references)
        .unwrap();
    assert_eq!(
        table[&handle(
            &unprepared,
            Viewer::P1,
            Viewer::P1,
            native::ObservedZone::Field
        )]
            .state
            .as_ref()
            .unwrap()
            .quotas,
        None
    );
    for maximum in [1, MAX_SAFE_INTEGER, MAX_SAFE_INTEGER + 1] {
        let mut game = board_with_program(Viewer::P1, "unit-spell", &limited(maximum));
        let projected = game.battle_objects(Viewer::P1, &fixture().references);
        if maximum > MAX_SAFE_INTEGER {
            assert_eq!(projected, Err(ProjectionError::Range));
        } else {
            let table = projected.unwrap();
            let quota = &table[&handle(&game, Viewer::P1, Viewer::P1, native::ObservedZone::Field)]
                .state
                .as_ref()
                .unwrap()
                .quotas
                .as_ref()
                .unwrap()[0];
            assert_eq!(quota.ability, wire::AbilityKind::Activated);
            assert_eq!(quota.maximum.get(), maximum);
            assert_eq!(quota.remaining.get(), maximum);
            assert_eq!(quota.consume_at, wire::ConsumptionPoint::PlayCommitted);
            if maximum == 1 {
                resolved(
                    game.activate_internal_card("s", AbilityId(1), Viewer::P1.player())
                        .unwrap(),
                );
                game.resume_internal_card().unwrap();
                let before = exact_authority(&game);
                for viewer in [Viewer::P1, Viewer::P2] {
                    let current = game.battle_objects(viewer, &fixture().references).unwrap();
                    let source =
                        &current[&handle(&game, viewer, Viewer::P1, native::ObservedZone::Field)];
                    assert_eq!(
                        source.state.as_ref().unwrap().quotas.as_ref().unwrap()[0]
                            .remaining
                            .get(),
                        0
                    );
                }
                assert_eq!(exact_authority(&game), before);
            }
        }
    }
    let wrong = position(
        Arc::new(catalog_for(&format!("{}\n", fixture().snapshot))),
        &initial(&Value::Null),
        Some(&card(vec![])),
    );
    assert_eq!(
        wrong.battle_objects(Viewer::P1, &fixture().references),
        Err(ProjectionError::Unavailable)
    );
}

#[test]
fn wire_object_table_retires_identity_after_the_source_returns_to_a_shuffled_deck() {
    let mut departure = movement::put(
        1,
        Selector::Objects {
            objects: vec![ObjectOperand::SourceHolder {}],
        },
        OwnerZone::Deck,
    );
    let Node::Instruction { instruction: moved } = &mut departure.node else {
        panic!("movement instruction")
    };
    let Action::Move { movement, .. } = &mut moved.action else {
        panic!("movement")
    };
    movement.position = Some(Position::Top {});
    let program = card(vec![ability(
        1,
        vec![
            departure,
            instruction(
                2,
                Action::Shuffle {
                    deck: PlayerRef::Controller {},
                    scope: ShuffleScope::FullZone {},
                    randomness: RandomnessPolicy::FreshRequest,
                },
            ),
        ],
    )]);
    for actor in [Viewer::P1, Viewer::P2] {
        let mut game = board_with_program(actor, "unit-spell", &program);
        let old = [Viewer::P1, Viewer::P2]
            .map(|viewer| handle(&game, viewer, actor, native::ObservedZone::Field));
        resolved(
            game.activate_internal_card("s", AbilityId(1), actor.player())
                .unwrap(),
        );
        game.resume_internal_card().unwrap();
        let before = exact_authority(&game);
        for (viewer, retired) in [Viewer::P1, Viewer::P2].into_iter().zip(old) {
            let table = game.battle_objects(viewer, &fixture().references).unwrap();
            assert!(!table.contains_key(&retired));
            assert!(
                table
                    .values()
                    .any(|card| card.card.printing_id == "printing-follower"),
                "the other same-printing follower still exists"
            );
        }
        assert_eq!(exact_authority(&game), before);
    }
}
