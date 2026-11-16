use super::board_fixture::{Fixture, catalog_for, empty_board as board, fixture};
use super::board_fixtures::object;
use crate::battle::ProjectionError;
use crate::battle::pending::project_pending;
use serde_json::json;
use sve_battle_contract::{MAX_SAFE_INTEGER, SourceRef, observation as wire};
use sve_engine::game::player::{ObjectHandle, Viewer, observation as native};

fn project(
    binding: &Fixture,
    observed: &native::Observation,
) -> Result<Vec<wire::PendingTrigger>, ProjectionError> {
    project_pending(&binding.catalog, &binding.references, observed)
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

#[test]
fn wire_pending_preserves_repeated_occurrences_and_adopted_face_mapping() {
    let mut observed = board(Viewer::P1, "unit-spell")
        .unwrap()
        .observe(Viewer::P1)
        .unwrap();
    let holder = object(&observed, Viewer::P1, native::ObservedZone::Field).unwrap();
    let pending = occurrence(Some(holder.clone()));
    observed.pending = vec![pending.clone(), pending];
    let result = project(fixture().unwrap(), &observed).unwrap();
    assert_eq!(
        serde_json::to_value(&result).unwrap(),
        json!([
            {"source":{"kind":"card","printing_id":"printing-follower","face":0_i32,"line":3_i32,"holder":holder.as_str()},"controller":"p1","timing":"leave_field","admission_remaining":2_i32},
            {"source":{"kind":"card","printing_id":"printing-follower","face":0_i32,"line":3_i32,"holder":holder.as_str()},"controller":"p1","timing":"leave_field","admission_remaining":2_i32}
        ])
    );
    observed.pending[0].source.face = Some(0);
    let SourceRef::Card {
        face: adopted_face, ..
    } = project(fixture().unwrap(), &observed)
        .unwrap()
        .remove(0)
        .source
    else {
        panic!("card source")
    };
    assert_eq!(adopted_face.unwrap().get(), 1);
    observed.pending[0].source.face = None;
    assert_eq!(
        project(fixture().unwrap(), &observed),
        Err(ProjectionError::Unavailable),
        "a printed line cannot be located without its face"
    );
    observed.pending[0].source.line = None;
    observed.pending[0].source.holder = None;
    let SourceRef::Card {
        printing_id,
        face: unknown_face,
        line,
        holder: retired_holder,
    } = project(fixture().unwrap(), &observed)
        .unwrap()
        .remove(0)
        .source
    else {
        panic!("card source")
    };
    assert_eq!(printing_id, "printing-follower");
    assert_eq!(unknown_face, None);
    assert_eq!(line, None);
    assert_eq!(retired_holder, None);
}

#[test]
fn wire_pending_refuses_unknown_printings_faces_handles_and_mismatched_catalogs() {
    let mut observed = board(Viewer::P1, "unit-spell")
        .unwrap()
        .observe(Viewer::P1)
        .unwrap();
    assert_eq!(
        project(fixture().unwrap(), &observed).unwrap(),
        Vec::<wire::PendingTrigger>::new()
    );
    let holder = object(&observed, Viewer::P1, native::ObservedZone::Field).unwrap();
    observed.pending = vec![occurrence(Some(holder.clone()))];
    let wrong = catalog_for(&format!("{}\n", fixture().unwrap().snapshot)).unwrap();
    assert_eq!(
        project_pending(&wrong, &fixture().unwrap().references, &observed),
        Err(ProjectionError::Unavailable)
    );
    observed.pending.clear();
    assert_eq!(
        project_pending(&wrong, &fixture().unwrap().references, &observed),
        Err(ProjectionError::Unavailable)
    );
    let mut retired = observed.clone();
    for player in &mut retired.players {
        for zone in &mut player.zones {
            zone.cards.retain(|member| member.card.handle != holder);
        }
    }
    retired.pending = vec![occurrence(Some(holder.clone()))];
    assert_eq!(
        project(fixture().unwrap(), &retired),
        Err(ProjectionError::Unavailable)
    );
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
        assert_eq!(
            project(fixture().unwrap(), &observed),
            Err(ProjectionError::Unavailable)
        );
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
    assert_eq!(
        project(fixture().unwrap(), &observed),
        Err(ProjectionError::Unavailable)
    );
}

#[test]
fn wire_pending_checks_numeric_bounds_and_controller_only_budgets() {
    for viewer in [Viewer::P1, Viewer::P2] {
        let mut observed = board(viewer, "unit-spell")
            .unwrap()
            .observe(viewer)
            .unwrap();
        let mut pending = occurrence(None);
        pending.controller = viewer;
        pending.source.line = Some(MAX_SAFE_INTEGER);
        pending.admission_remaining = Some(MAX_SAFE_INTEGER);
        observed.pending = vec![pending];
        let result = project(fixture().unwrap(), &observed).unwrap();
        assert_eq!(
            result[0].admission_remaining.unwrap().get(),
            MAX_SAFE_INTEGER
        );
        let SourceRef::Card { line, .. } = &result[0].source else {
            panic!("card source")
        };
        assert_eq!(line.unwrap().get(), MAX_SAFE_INTEGER);
        observed.pending[0].source.line = Some(MAX_SAFE_INTEGER + 1);
        assert_eq!(
            project(fixture().unwrap(), &observed),
            Err(ProjectionError::Range)
        );
        observed.pending[0].source.line = Some(0);
        observed.pending[0].admission_remaining = Some(MAX_SAFE_INTEGER + 1);
        assert_eq!(
            project(fixture().unwrap(), &observed),
            Err(ProjectionError::Range)
        );
        observed.pending[0].admission_remaining = Some(0);
        observed.pending[0].controller = if viewer == Viewer::P1 {
            Viewer::P2
        } else {
            Viewer::P1
        };
        assert_eq!(
            project(fixture().unwrap(), &observed),
            Err(ProjectionError::Unavailable)
        );
        observed.pending[0].admission_remaining = None;
        let public_budget = project(fixture().unwrap(), &observed).unwrap();
        assert_eq!(public_budget[0].admission_remaining, None);
        let SourceRef::Card {
            line: public_line, ..
        } = &public_budget[0].source
        else {
            panic!("card source")
        };
        assert_eq!(public_line.unwrap().get(), 0);
    }
}

#[test]
fn wire_pending_preserves_each_trigger_timing() {
    let mut observed = board(Viewer::P1, "unit-spell")
        .unwrap()
        .observe(Viewer::P1)
        .unwrap();
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
            serde_json::to_value(project(fixture().unwrap(), &observed).unwrap()).unwrap()[0]["timing"],
            expected
        );
    }
}
