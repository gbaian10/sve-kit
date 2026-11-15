use super::battle_costs::{board, object};
use super::*;
use sve_battle_contract::{MAX_SAFE_INTEGER, observation as wire};
use sve_engine::battle::{ProjectionError, project_history};
use sve_engine::game::player::{ObjectHandle, Viewer, observation as native};

fn subject(current: Option<ObjectHandle>) -> native::SubjectHistory {
    native::SubjectHistory {
        event: native::HistoryEvent::Leave,
        current,
        controller: Viewer::P2,
        active: Viewer::P1,
        kind: CardKind::Spell,
        class: Some("class".into()),
        traits: vec!["trait-b".into(), "trait-a".into()],
        names: Some(vec!["name-b".into(), "name-a".into()]),
        token: true,
        destroyed: true,
        destination: Some(native::ObservedZone::Cemetery),
    }
}

#[test]
fn wire_history_preserves_public_facts_when_current_links_are_absent() {
    let mut observed = board(Viewer::P1, "unit-spell").observe(Viewer::P1).unwrap();
    let holder = object(&observed, Viewer::P1, native::ObservedZone::Field);
    observed.history = native::TurnHistory {
        subjects: vec![subject(None)],
        damage: vec![native::DamageHistory {
            recipient: Some(holder.clone()),
            source: None,
            controller: Viewer::P2,
        }],
        stat_gains: vec![native::StatGainHistory {
            current: None,
            attack: true,
            life: false,
        }],
        dice: vec![native::DiceHistory {
            player: Viewer::P2,
            value: 6,
        }],
        played: [0, MAX_SAFE_INTEGER],
        union_bursts: [2, 3],
    };
    let projected = project_history(&observed).unwrap();
    assert_eq!(
        serde_json::to_value(&projected).unwrap(),
        json!({
            "subjects":[{
                "event":"leave", "current":null, "controller":"p2", "active":"p1",
                "card_kind":"spell", "class":"class", "traits":["trait-b","trait-a"],
                "names":["name-b","name-a"], "token":true, "destroyed":true, "destination":"cemetery"
            }],
            "damage":[{"recipient":holder.as_str(), "source":null,"controller":"p2"}],
            "stat_gains":[{"current":null,"attack":true,"life":false}],
            "dice":[{"player":"p2","value":6}],
            "played":[0, MAX_SAFE_INTEGER],"union_bursts":[2,3]
        })
    );
    observed.history.subjects[0].class = None;
    observed.history.subjects[0].names = None;
    let unknown = project_history(&observed).unwrap();
    assert_eq!(unknown.subjects[0].class, None);
    assert_eq!(unknown.subjects[0].names, None);
    observed.history.subjects[0].names = Some(vec![]);
    assert_eq!(
        project_history(&observed).unwrap().subjects[0].names,
        Some(vec![])
    );
    assert_eq!(projected.subjects[0].current, None);
    assert_eq!(projected.stat_gains[0].current, None);
}

#[test]
fn wire_history_covers_every_recorded_event_kind_and_destination() {
    let mut observed = board(Viewer::P2, "unit-spell").observe(Viewer::P2).unwrap();
    let holder = object(&observed, Viewer::P2, native::ObservedZone::Field);
    for (event, kind) in [
        (native::HistoryEvent::Play, CardKind::Crest),
        (native::HistoryEvent::Attack, CardKind::EvolveFollower),
        (native::HistoryEvent::Evolve, CardKind::Follower),
        (native::HistoryEvent::Leave, CardKind::Spell),
        (native::HistoryEvent::Play, CardKind::Amulet),
    ] {
        let mut fact = subject(Some(holder.clone()));
        fact.event = event;
        fact.kind = kind;
        observed.history.subjects.push(fact);
    }
    let projected = serde_json::to_value(project_history(&observed).unwrap()).unwrap();
    assert_eq!(
        projected["subjects"]
            .as_array()
            .unwrap()
            .iter()
            .map(|fact| json!([fact["event"], fact["card_kind"]]))
            .collect::<Vec<_>>(),
        [
            json!(["play", "crest"]),
            json!(["attack", "evolve_follower"]),
            json!(["evolve", "follower"]),
            json!(["leave", "spell"]),
            json!(["play", "amulet"]),
        ]
    );
    observed.history.subjects.clear();
    for destination in [
        native::ObservedZone::Deck,
        native::ObservedZone::EvolveDeck,
        native::ObservedZone::Hand,
        native::ObservedZone::Field,
        native::ObservedZone::Ex,
        native::ObservedZone::Cemetery,
        native::ObservedZone::Banish,
        native::ObservedZone::Resolution,
        native::ObservedZone::Evolution,
        native::ObservedZone::Race,
        native::ObservedZone::Drive,
        native::ObservedZone::Trigger,
        native::ObservedZone::Equipment,
    ] {
        let mut fact = subject(None);
        fact.destination = Some(destination);
        observed.history.subjects.push(fact);
    }
    observed.history.subjects.push(native::SubjectHistory {
        destination: None,
        ..subject(None)
    });
    let projected = serde_json::to_value(project_history(&observed).unwrap()).unwrap();
    assert_eq!(
        projected["subjects"]
            .as_array()
            .unwrap()
            .iter()
            .map(|fact| fact["destination"].clone())
            .collect::<Vec<_>>(),
        [
            json!("deck"),
            json!("evolve_deck"),
            json!("hand"),
            json!("field"),
            json!("ex"),
            json!("cemetery"),
            json!("banish"),
            json!("resolution"),
            json!("evolution"),
            json!("race"),
            json!("drive"),
            json!("trigger"),
            json!("equipment"),
            Value::Null,
        ]
    );
}

#[test]
fn wire_history_refuses_each_out_of_scope_link_and_duplicate_object_tables() {
    let game = board(Viewer::P1, "unit-spell");
    let observed = game.observe(Viewer::P1).unwrap();
    let foreign = object(
        &game.observe(Viewer::P2).unwrap(),
        Viewer::P2,
        native::ObservedZone::Hand,
    );
    for history in [
        native::TurnHistory {
            subjects: vec![subject(Some(foreign.clone()))],
            ..native::TurnHistory::default()
        },
        native::TurnHistory {
            damage: vec![native::DamageHistory {
                recipient: Some(foreign.clone()),
                source: None,
                controller: Viewer::P1,
            }],
            ..native::TurnHistory::default()
        },
        native::TurnHistory {
            damage: vec![native::DamageHistory {
                recipient: None,
                source: Some(foreign.clone()),
                controller: Viewer::P2,
            }],
            ..native::TurnHistory::default()
        },
        native::TurnHistory {
            stat_gains: vec![native::StatGainHistory {
                current: Some(foreign),
                attack: false,
                life: true,
            }],
            ..native::TurnHistory::default()
        },
    ] {
        let mut bad = observed.clone();
        bad.history = history;
        assert_eq!(
            project_history(&bad).unwrap_err(),
            ProjectionError::Unavailable
        );
    }
    let mut duplicate = observed.clone();
    let member = duplicate.players[0]
        .zones
        .iter()
        .find_map(|zone| zone.cards.first())
        .unwrap()
        .clone();
    duplicate.players[0].zones[0].cards.push(member);
    assert_eq!(
        project_history(&duplicate).unwrap_err(),
        ProjectionError::Unavailable
    );
    let mut wrong_seats = observed;
    wrong_seats.players.swap(0, 1);
    assert_eq!(
        project_history(&wrong_seats).unwrap_err(),
        ProjectionError::Unavailable
    );
}

#[test]
fn wire_history_checks_dice_and_both_seats_counters_without_clamping() {
    let observed = board(Viewer::P1, "unit-spell").observe(Viewer::P1).unwrap();
    let too_large = MAX_SAFE_INTEGER.checked_add(1).unwrap();
    for history in [
        native::TurnHistory {
            dice: vec![native::DiceHistory {
                player: Viewer::P1,
                value: too_large,
            }],
            ..native::TurnHistory::default()
        },
        native::TurnHistory {
            played: [too_large, 0],
            ..native::TurnHistory::default()
        },
        native::TurnHistory {
            played: [0, too_large],
            ..native::TurnHistory::default()
        },
        native::TurnHistory {
            union_bursts: [too_large, 0],
            ..native::TurnHistory::default()
        },
        native::TurnHistory {
            union_bursts: [0, too_large],
            ..native::TurnHistory::default()
        },
    ] {
        let mut bad = observed.clone();
        bad.history = history;
        assert_eq!(project_history(&bad).unwrap_err(), ProjectionError::Range);
    }
    let zero = sve_battle_contract::Nat::try_from(0_u64).unwrap();
    assert_eq!(
        project_history(&observed).unwrap(),
        wire::TurnHistory {
            subjects: vec![],
            damage: vec![],
            stat_gains: vec![],
            dice: vec![],
            played: [zero; 2],
            union_bursts: [zero; 2],
        }
    );
}
