use super::*;
use crate::battle_costs::object;
use std::sync::OnceLock;
use sve_battle_contract::{
    CardRef, MAX_SAFE_INTEGER, Nat as WireNat, SourceRef, observation as wire,
};
use sve_engine::battle::{
    BattleReferences, PrintingFaceIdentity, ProjectionError, ReferenceInput, project_pending,
};
use sve_engine::catalog::EngineIdentityInput;
use sve_engine::game::player::{ObjectHandle, Viewer, observation as native};

pub(super) struct Fixture {
    pub(super) catalog: Arc<Catalog>,
    pub(super) references: BattleReferences,
    pub(super) snapshot: String,
}

pub(super) fn catalog_for(snapshot: &str) -> Catalog {
    let (_, keywords, document) = catalog_documents(&json!("1"));
    let rules = rule_fixtures::rules(
        snapshot,
        json!([rule_fixtures::title(
            "synthetic",
            "unit-follower",
            json!([])
        )]),
        json!([]),
    );
    Catalog::from_documents_with_rules(
        snapshot,
        &keywords,
        &[("synthetic".into(), document)],
        &rules.to_string(),
        &EngineIdentityInput::LegacyJp,
    )
    .unwrap()
}

pub(super) fn fixture() -> &'static Fixture {
    static FIXTURE: OnceLock<Fixture> = OnceLock::new();
    FIXTURE.get_or_init(|| {
        let (snapshot, _, _) = catalog_documents(&json!("1"));
        let snapshot = snapshot
            .lines()
            .map(|line| {
                let mut row: Value = serde_json::from_str(line).unwrap();
                if row["number"] == "unit-follower" {
                    row["faces"][0]["title"] = json!("synthetic-title");
                    let face = row["faces"][0].clone();
                    row["faces"].as_array_mut().unwrap().push(face);
                }
                row.to_string()
            })
            .collect::<Vec<_>>()
            .join("\n");
        let catalog = Arc::new(catalog_for(&snapshot));
        let faces = [
            ("unit-follower", 0_u64, "printing-follower", 1_u32),
            ("unit-follower", 1, "printing-follower", 0),
            ("unit-spell", 0, "printing-spell", 0),
            ("unit-amulet", 0, "printing-amulet", 0),
        ]
        .into_iter()
        .map(
            |(number, native_face, printing, face)| PrintingFaceIdentity {
                card_no: number.into(),
                native_face,
                card: CardRef {
                    printing_id: printing.into(),
                    face: WireNat::from(face),
                },
            },
        )
        .collect();
        let references = BattleReferences::new(
            &catalog,
            ReferenceInput {
                region: "jp".into(),
                snapshot_sha256: rule_fixtures::hash(snapshot.as_bytes()),
                faces,
            },
        )
        .unwrap();
        Fixture {
            catalog,
            references,
            snapshot,
        }
    })
}

pub(super) fn board(actor: Viewer, secret: &str) -> Game {
    board_with_program(actor, secret, &card(vec![]))
}

pub(super) fn board_with_program(
    actor: Viewer,
    secret: &str,
    follower: &WellFormedCardProgram,
) -> Game {
    let mut initial = setup(actor.player().seat, json!([]));
    let (own, opponent) = if actor == Viewer::P1 {
        ("P1", "P2")
    } else {
        ("P2", "P1")
    };
    initial["turn"]["active"] = json!(own);
    initial["players"][own]["zones"]["hand"] = json!([]);
    initial["players"][own]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
    initial["players"][opponent]["zones"]["hand"] = json!([{"id":"secret","card":secret}]);
    let mut game = Game::new(
        Arc::clone(&fixture().catalog),
        &initial,
        &Value::Null,
        &Value::Null,
        "wire-pending",
    )
    .unwrap();
    game.load_internal_card("unit-follower", follower).unwrap();
    let empty = card(vec![]);
    for number in ["unit-spell", "unit-amulet"] {
        game.load_internal_card(number, &empty).unwrap();
    }
    game
}

fn occurrence(holder: Option<ObjectHandle>) -> native::PendingTrigger {
    native::PendingTrigger {
        source: native::EffectSource {
            card: "unit-follower".into(),
            face: Some(1),
            line: Some(3),
            wording: None,
            holder,
        },
        controller: Viewer::P1,
        timing: native::TriggerTiming::LeaveField,
        admission_remaining: Some(2),
    }
}

fn project(observed: &native::Observation) -> Result<Vec<wire::PendingTrigger>, ProjectionError> {
    project_pending(&fixture().catalog, &fixture().references, observed)
}

#[test]
fn wire_pending_preserves_repeated_occurrences_and_adopted_face_mapping() {
    let mut observed = board(Viewer::P1, "unit-spell").observe(Viewer::P1).unwrap();
    let holder = object(&observed, Viewer::P1, native::ObservedZone::Field);
    let pending = occurrence(Some(holder.clone()));
    observed.pending = vec![pending.clone(), pending];
    let result = project(&observed).unwrap();
    assert_eq!(
        serde_json::to_value(&result).unwrap(),
        json!([
            {"source":{"kind":"card","printing_id":"printing-follower","face":0,"line":3,"holder":holder.as_str()},"controller":"p1","timing":"leave_field","admission_remaining":2},
            {"source":{"kind":"card","printing_id":"printing-follower","face":0,"line":3,"holder":holder.as_str()},"controller":"p1","timing":"leave_field","admission_remaining":2}
        ])
    );
    observed.pending[0].source.face = Some(0);
    let SourceRef::Card { face, .. } = project(&observed).unwrap().remove(0).source else {
        panic!("card source")
    };
    assert_eq!(face.unwrap().get(), 1);
    observed.pending[0].source.face = None;
    assert_eq!(
        project(&observed),
        Err(ProjectionError::Unavailable),
        "a printed line cannot be located without its face"
    );
    observed.pending[0].source.line = None;
    observed.pending[0].source.holder = None;
    let SourceRef::Card {
        printing_id,
        face,
        line,
        holder,
    } = project(&observed).unwrap().remove(0).source
    else {
        panic!("card source")
    };
    assert_eq!(printing_id, "printing-follower");
    assert_eq!(face, None);
    assert_eq!(line, None);
    assert_eq!(holder, None);
}

#[test]
fn wire_pending_refuses_unknown_printings_faces_handles_and_mismatched_catalogs() {
    let mut observed = board(Viewer::P1, "unit-spell").observe(Viewer::P1).unwrap();
    assert_eq!(
        project(&observed).unwrap(),
        Vec::<wire::PendingTrigger>::new()
    );
    let holder = object(&observed, Viewer::P1, native::ObservedZone::Field);
    observed.pending = vec![occurrence(Some(holder.clone()))];
    let wrong = catalog_for(&format!("{}\n", fixture().snapshot));
    assert_eq!(
        project_pending(&wrong, &fixture().references, &observed),
        Err(ProjectionError::Unavailable)
    );
    observed.pending.clear();
    assert_eq!(
        project_pending(&wrong, &fixture().references, &observed),
        Err(ProjectionError::Unavailable)
    );
    let mut retired = observed.clone();
    for player in &mut retired.players {
        for zone in &mut player.zones {
            zone.cards.retain(|member| member.card.handle != holder);
        }
    }
    retired.pending = vec![occurrence(Some(holder.clone()))];
    assert_eq!(project(&retired), Err(ProjectionError::Unavailable));
    for broken in [
        native::EffectSource {
            card: "missing".into(),
            face: None,
            line: None,
            wording: None,
            holder: None,
        },
        native::EffectSource {
            card: "unit-follower".into(),
            face: Some(2),
            line: None,
            wording: None,
            holder: None,
        },
    ] {
        let mut pending = occurrence(None);
        pending.source = broken;
        observed.pending = vec![pending];
        assert_eq!(project(&observed), Err(ProjectionError::Unavailable));
    }
    observed.pending = vec![occurrence(Some(holder))];
    let duplicate = observed.players[0]
        .zones
        .iter()
        .find(|zone| zone.kind == native::ObservedZone::Field)
        .unwrap()
        .cards[0]
        .clone();
    observed.players[0]
        .zones
        .iter_mut()
        .find(|zone| zone.kind == native::ObservedZone::Field)
        .unwrap()
        .cards
        .push(duplicate);
    assert_eq!(project(&observed), Err(ProjectionError::Unavailable));
}

#[test]
fn wire_pending_checks_numeric_bounds_and_controller_only_budgets() {
    for viewer in [Viewer::P1, Viewer::P2] {
        let mut observed = board(viewer, "unit-spell").observe(viewer).unwrap();
        let mut pending = occurrence(None);
        pending.controller = viewer;
        pending.source.line = Some(MAX_SAFE_INTEGER);
        pending.admission_remaining = Some(MAX_SAFE_INTEGER);
        observed.pending = vec![pending];
        let result = project(&observed).unwrap();
        assert_eq!(
            result[0].admission_remaining.unwrap().get(),
            MAX_SAFE_INTEGER
        );
        let SourceRef::Card { line, .. } = &result[0].source else {
            panic!("card source")
        };
        assert_eq!(line.unwrap().get(), MAX_SAFE_INTEGER);
        observed.pending[0].source.line = Some(MAX_SAFE_INTEGER + 1);
        assert_eq!(project(&observed), Err(ProjectionError::Range));
        observed.pending[0].source.line = Some(0);
        observed.pending[0].admission_remaining = Some(MAX_SAFE_INTEGER + 1);
        assert_eq!(project(&observed), Err(ProjectionError::Range));
        observed.pending[0].admission_remaining = Some(0);
        observed.pending[0].controller = if viewer == Viewer::P1 {
            Viewer::P2
        } else {
            Viewer::P1
        };
        assert_eq!(project(&observed), Err(ProjectionError::Unavailable));
        observed.pending[0].admission_remaining = None;
        let result = project(&observed).unwrap();
        assert_eq!(result[0].admission_remaining, None);
        let SourceRef::Card { line, .. } = &result[0].source else {
            panic!("card source")
        };
        assert_eq!(line.unwrap().get(), 0);
    }
}

#[test]
fn wire_pending_uses_authoritative_lastwords_without_relinking_after_departure() {
    for actor in [Viewer::P1, Viewer::P2] {
        let movement = movement::put(
            2,
            Selector::Objects {
                objects: vec![ObjectOperand::SourceHolder {}],
            },
            OwnerZone::Cemetery,
        );
        let program = card(vec![
            triggers::lastword(ability(1, vec![draw(1, 0)])),
            ability(2, vec![movement]),
        ]);
        let mut first = board_with_program(actor, "unit-spell", &program);
        let mut second = board_with_program(actor, "unit-amulet", &program);
        for game in [&mut first, &mut second] {
            resolved(
                game.activate_internal_card("s", AbilityId(2), actor.player())
                    .unwrap(),
            );
            game.resume_internal_card().unwrap();
        }
        let before = exact_authority(&first);
        for viewer in [Viewer::P1, Viewer::P2] {
            let observed = first.observe(viewer).unwrap();
            let projected = project(&observed).unwrap();
            assert_eq!(
                projected,
                project(&second.observe(viewer).unwrap()).unwrap()
            );
            assert_eq!(projected.len(), 1);
            let SourceRef::Card {
                printing_id,
                face,
                line,
                holder,
            } = &projected[0].source
            else {
                panic!("card source")
            };
            assert_eq!(printing_id, "printing-follower");
            assert_eq!(face.unwrap().get(), 1);
            assert_eq!(line.unwrap().get(), 1);
            assert_eq!(holder, &None);
        }
        assert_eq!(exact_authority(&first), before);
    }
}

#[test]
fn wire_pending_preserves_each_trigger_timing() {
    let mut observed = board(Viewer::P1, "unit-spell").observe(Viewer::P1).unwrap();
    observed.pending = vec![occurrence(None)];
    for (timing, expected) in [
        (native::TriggerTiming::EnterField, "enter_field"),
        (native::TriggerTiming::Targeted, "targeted"),
        (native::TriggerTiming::Resolve, "resolve"),
        (native::TriggerTiming::Payment, "payment"),
        (native::TriggerTiming::CounterChange, "counter_change"),
        (native::TriggerTiming::EnterEx, "enter_ex"),
        (native::TriggerTiming::LeaveField, "leave_field"),
        (native::TriggerTiming::Attack, "attack"),
        (native::TriggerTiming::Evolve, "evolve"),
        (native::TriggerTiming::Phase, "phase"),
        (native::TriggerTiming::StatChange, "stat_change"),
        (native::TriggerTiming::Posture, "posture"),
        (native::TriggerTiming::Destroy, "destroy"),
        (native::TriggerTiming::UnionBurst, "union_burst"),
        (native::TriggerTiming::Fusion, "fusion"),
        (native::TriggerTiming::Play, "play"),
        (native::TriggerTiming::Discard, "discard"),
        (native::TriggerTiming::Banish, "banish"),
        (native::TriggerTiming::Draw, "draw"),
        (native::TriggerTiming::DriveGained, "drive_gained"),
        (native::TriggerTiming::DriveChecked, "drive_checked"),
        (native::TriggerTiming::DriveTrigger, "drive_trigger"),
        (native::TriggerTiming::Dice, "dice"),
        (native::TriggerTiming::Equip, "equip"),
        (native::TriggerTiming::Move, "move"),
        (native::TriggerTiming::Race, "race"),
        (native::TriggerTiming::LifeChange, "life_change"),
        (native::TriggerTiming::Damage, "damage"),
    ] {
        observed.pending[0].timing = timing;
        assert_eq!(
            serde_json::to_value(project(&observed).unwrap()).unwrap()[0]["timing"],
            expected
        );
    }
}
