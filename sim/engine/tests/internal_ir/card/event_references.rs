use super::super::battle_costs::object;
use super::super::player_events::reveal_hand;
use super::super::player_pipeline::answer;
use super::battle_pending::{catalog_for, fixture};
use super::presentation_fault::{ask_for_the_follower, prepared_on};
use super::*;
use sve_battle_contract::event::{self as wire, BattleEvent, EventBody, EventDelivery, EventGap};
use sve_battle_contract::observation::{Phase as WirePhase, TurnView};
use sve_battle_contract::presentation::PresentationOp;
use sve_battle_contract::{
    CardRef, Nat as WireNat, ObjectHandle as WireHandle, Seat, SourceRef, Subject,
};
use sve_engine::battle::{
    BattleReferences, EventFeed, EventObjects as _, EventReferences, FeedUpdate,
    PrintingFaceIdentity, ProjectionError, ReferenceInput,
};
use sve_engine::game::player::events::EventCard;
use sve_engine::game::player::observation::{
    CardProperties, EffectSource, ObservedZone, TurnObservation,
};
use sve_engine::game::player::{ObjectHandle, SubmissionKind, Viewer};

const SEATS: [Viewer; 2] = [Viewer::P1, Viewer::P2];

fn face(printing: &str, face: u32) -> CardRef {
    CardRef {
        printing_id: printing.into(),
        face: WireNat::from(face),
    }
}

fn wire_handle(handle: &ObjectHandle) -> WireHandle {
    WireHandle::new(handle.as_str().into())
}

/// The wire subject of the first card `owner` has in `zone`, as `viewer` identifies it now.
fn standing(
    game: &Game,
    viewer: Viewer,
    owner: Viewer,
    zone: ObservedZone,
    card: CardRef,
) -> Subject {
    Subject::Card {
        card,
        object: Some(wire_handle(&object(
            &game.observe(viewer).unwrap(),
            owner,
            zone,
        ))),
    }
}

fn committed(update: &FeedUpdate) -> Vec<BattleEvent> {
    match &update.events {
        EventDelivery::Available { batches, .. } => batches
            .iter()
            .flat_map(|batch| batch.events.clone())
            .collect(),
        EventDelivery::Unavailable { .. } => Vec::new(),
    }
}

/// References of the same cards bound to a snapshot with other bytes.
fn foreign_references() -> BattleReferences {
    let snapshot = format!("{}\n", fixture().snapshot);
    let faces = [
        ("unit-follower", 0_u32),
        ("unit-follower", 1),
        ("unit-spell", 0),
        ("unit-amulet", 0),
    ]
    .into_iter()
    .map(|(number, ordinal)| PrintingFaceIdentity {
        card_no: number.into(),
        native_face: u64::from(ordinal),
        card: face(number, ordinal),
    })
    .collect();
    BattleReferences::new(
        &catalog_for(&snapshot),
        ReferenceInput {
            region: "jp".into(),
            snapshot_sha256: rule_fixtures::hash(snapshot.as_bytes()),
            faces,
        },
    )
    .unwrap()
}

/// Both seats are handed the stop where P1's ability asks for a follower and then its
/// resolution: what each was shown early, and what then committed.
fn resolved() -> (Game, [FeedUpdate; 2], [Vec<BattleEvent>; 2]) {
    let references = &fixture().references;
    let mut game = prepared_on(Arc::clone(&fixture().catalog), false);
    let mut feeds = SEATS.map(EventFeed::new);
    for feed in &mut feeds {
        feed.baseline(&mut game);
    }
    ask_for_the_follower(&mut game);
    let early = feeds
        .each_mut()
        .map(|feed| feed.deliver(&mut game, references));
    assert_eq!(
        answer(&mut game, Viewer::P1, Answer::Targets(vec![0])).kind,
        SubmissionKind::Applied,
        "the chosen follower is damaged and the ability resolves"
    );
    let late = feeds
        .each_mut()
        .map(|feed| committed(&feed.deliver(&mut game, references)));
    (game, early, late)
}

#[test]
fn a_delivery_names_each_card_by_its_adopted_face_and_its_object_on_that_viewers_board() {
    let references = &fixture().references;
    let (game, _, late) = resolved();
    for (viewer, events) in SEATS.into_iter().zip(&late) {
        let table = game.battle_objects(viewer, references).unwrap();
        // The catalogue's native face 0 of the follower is adopted as face 1 of its printing.
        let follower = face("printing-follower", 1);
        let holder = standing(
            &game,
            viewer,
            Viewer::P1,
            ObservedZone::Field,
            follower.clone(),
        );
        let target = standing(&game, viewer, Viewer::P2, ObservedZone::Field, follower);
        let text = Some(SourceRef::Card {
            printing_id: "printing-follower".into(),
            face: Some(WireNat::from(1_u32)),
            line: Some(WireNat::from(2_u32)),
            holder: None,
        });
        let bodies = events
            .iter()
            .map(|event| event.body.clone())
            .collect::<Vec<_>>();
        let [
            played,
            EventBody::Movement { after: first, .. },
            damage,
            EventBody::Movement { .. },
            resolved,
        ] = bodies.as_slice()
        else {
            panic!("play, draw, damage, draw, resolution: {bodies:?}");
        };
        assert_eq!(
            (played, resolved),
            (
                &EventBody::AbilityPlayed {
                    player: Seat::P1,
                    holder: Some(holder.clone()),
                    source: text.clone(),
                    ability: None,
                    qualification: wire::AbilityQualification::Ordinary,
                },
                &EventBody::AbilityResolved {
                    player: Seat::P1,
                    holder: Some(holder.clone()),
                    source: text,
                    ability: None,
                },
            ),
            "{viewer:?}: the ability names its holder on this viewer's board and its printed line"
        );
        assert!(
            matches!(
                damage,
                EventBody::Damage { source, target: hit, .. }
                    if source.as_ref() == Some(&holder) && hit.as_ref() == Some(&target)
            ),
            "{viewer:?}: the damage names both followers as this viewer's board does: {damage:?}"
        );
        let drawn = (viewer == Viewer::P1).then(|| wire::EventCard {
            card: face("printing-spell", 0),
            object: Some(wire_handle(&object(
                &game.observe(viewer).unwrap(),
                Viewer::P1,
                ObservedZone::Hand,
            ))),
        });
        assert_eq!(
            first.card, drawn,
            "{viewer:?}: only the drawing player is told the drawn card and its object"
        );
        for subject in events.iter().flat_map(|event| &event.about) {
            let Subject::Card {
                card,
                object: Some(named),
            } = subject
            else {
                panic!("every highlighted subject of this flow is an identified card: {subject:?}");
            };
            assert_eq!(
                table.get(named).map(|view| &view.effective_face),
                Some(card),
                "{viewer:?}: a card that stayed identifiable is the same object, with the same \
                 face, in the event and on the board"
            );
        }
    }
}

#[test]
fn a_promoted_event_states_exactly_what_its_viewer_was_shown() {
    let (_, early, late) = resolved();
    for ((viewer, early), events) in SEATS.into_iter().zip(&early).zip(&late) {
        let shown = early
            .presentation
            .iter()
            .filter_map(|op| match op {
                PresentationOp::Provisional { batch, .. } => Some(batch.events.clone()),
                PresentationOp::Promote { .. } | PresentationOp::Cancel { .. } => None,
            })
            .flatten()
            .collect::<Vec<_>>();
        assert_eq!(
            shown.len(),
            2,
            "{viewer:?}: the play and the first draw were shown early"
        );
        for provisional in shown {
            let promoted = events
                .iter()
                .find(|event| event.presentation_id.as_ref() == Some(&provisional.id))
                .unwrap();
            assert_eq!(
                (&promoted.body, &promoted.about),
                (&provisional.body, &provisional.about),
                "{viewer:?}: a promoted event states exactly what was shown"
            );
        }
    }
}

#[test]
fn an_event_keeps_the_object_it_named_and_never_links_to_a_later_one() {
    let references = &fixture().references;
    let mut game = Game::new(
        Arc::clone(&fixture().catalog),
        &setup(1, json!([])),
        &Value::Null,
        &Value::Null,
        "event-references",
    )
    .unwrap();
    game.advance_player_input();
    let mut feeds = SEATS.map(EventFeed::new);
    for feed in &mut feeds {
        feed.baseline(&mut game);
    }
    let mut named = SEATS.map(|_| Vec::new());
    for _ in 0..2 {
        // Each reveal of P1's only hand card ends before the next stop.
        game.execute_internal(&reveal_hand(), &source("synthetic"), context(1))
            .unwrap();
        game.advance_player_input();
        for ((viewer, feed), seen) in SEATS.into_iter().zip(&mut feeds).zip(&mut named) {
            let events = committed(&feed.deliver(&mut game, references));
            let [
                BattleEvent {
                    body: EventBody::Revealed { cards },
                    ..
                },
            ] = events.as_slice()
            else {
                panic!("one reveal: {events:?}");
            };
            let [
                wire::EventCard {
                    card,
                    object: Some(revealed),
                },
            ] = cards.as_slice()
            else {
                panic!("the revealed card is identified while it is revealed: {cards:?}");
            };
            assert_eq!(
                card,
                &face("printing-spell", 0),
                "{viewer:?}: the revealed face"
            );
            assert_eq!(
                game.battle_objects(viewer, references)
                    .unwrap()
                    .contains_key(revealed),
                viewer == Viewer::P1,
                "{viewer:?}: only its owner still identifies the card on the board handed with \
                 this event"
            );
            seen.push(revealed.clone());
        }
    }
    let [own, other] = named;
    assert_eq!(own[0], own[1], "its owner identified the card throughout");
    assert_ne!(
        other[0], other[1],
        "the opponent's second identification is a new object, not the earlier one"
    );
}

#[test]
fn event_references_convert_only_what_the_event_froze() {
    let fixture = fixture();
    let objects = EventReferences::new(&fixture.catalog, &fixture.references).unwrap();
    let game = prepared_on(Arc::clone(&fixture.catalog), false);
    let holder = object(
        &game.observe(Viewer::P2).unwrap(),
        Viewer::P1,
        ObservedZone::Field,
    );
    let remembered = |native_face| EventCard {
        handle: None,
        properties: CardProperties {
            card: "unit-follower".into(),
            face: native_face,
            names: Vec::new(),
            kind: String::new(),
            traits: Vec::new(),
            owner: Viewer::P1,
            controller: Viewer::P1,
            state: None,
        },
    };
    assert_eq!(
        [0, 1].map(|native_face| objects.card(&remembered(native_face))),
        [1, 0].map(|adopted| Ok(wire::EventCard {
            card: face("printing-follower", adopted),
            object: None,
        })),
        "a remembered face is its adopted face and has no object"
    );
    assert_eq!(
        objects.card(&remembered(2)),
        Err(ProjectionError::Unavailable),
        "a face outside the catalogue is refused"
    );
    assert_eq!(
        objects.source(&EffectSource {
            card: "unit-spell".into(),
            face: None,
            line: None,
            holder: Some(holder.clone()),
        }),
        Ok(SourceRef::Card {
            printing_id: "printing-spell".into(),
            face: None,
            line: None,
            holder: Some(wire_handle(&holder)),
        }),
        "a source keeps the holder its event froze and invents no face or line"
    );
    for (phase, expected) in [
        (None, None),
        (Some(Phase::Start), Some(WirePhase::Start)),
        (Some(Phase::Main), Some(WirePhase::Main)),
        (Some(Phase::End), Some(WirePhase::End)),
    ] {
        assert_eq!(
            objects.turn(&TurnObservation {
                active: Some(Viewer::P2),
                first_player: Some(Viewer::P1),
                phase,
                elapsed_turns: [3, 2],
            }),
            Ok(TurnView {
                active: Some(Seat::P2),
                first_player: Some(Seat::P1),
                phase: expected,
                elapsed_turns: [WireNat::from(3_u32), WireNat::from(2_u32)],
            }),
            "the turn an event froze, phase {phase:?}"
        );
    }
}

#[test]
fn references_of_another_catalogue_end_the_events_without_touching_the_board() {
    let fixture = fixture();
    let foreign = foreign_references();
    assert_eq!(
        EventReferences::new(&fixture.catalog, &foreign).err(),
        Some(ProjectionError::Unavailable),
        "references are bound to one catalogue"
    );
    let mut game = prepared_on(Arc::clone(&fixture.catalog), false);
    let mut feed = EventFeed::new(Viewer::P1);
    feed.baseline(&mut game);
    let unavailable = FeedUpdate {
        events: EventDelivery::Unavailable {
            reason: EventGap::ProjectionGap,
        },
        presentation: Vec::new(),
    };
    assert_eq!(
        feed.deliver(&mut game, &foreign),
        unavailable,
        "nothing has happened yet, and still the wrong references are not a quiet delivery"
    );
    ask_for_the_follower(&mut game);
    assert_eq!(
        feed.deliver(&mut game, &fixture.references),
        unavailable,
        "a feed that ended stays ended"
    );
    assert_eq!(
        game.battle_objects(Viewer::P1, &fixture.references)
            .unwrap()
            .len(),
        3,
        "the board is still projected: both followers and the drawn card"
    );
}
