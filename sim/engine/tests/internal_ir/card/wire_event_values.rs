use super::event_batches::{attack_the_follower, combat_ready};
use super::presentation_feed::Board;
use super::*;
use sve_battle_contract::event::{self as wire, DamageClass, EventBody};
use sve_battle_contract::observation::{EvolutionState, Posture};
use sve_battle_contract::{Int as WireInt, Nat as WireNat, Seat, Subject, Zone};
use sve_engine::battle::{ProjectedEvent, project_batch};
use sve_engine::game::player::observation::ObservedZone;
use sve_engine::game::player::{SubmissionKind, Viewer};

/// Every committed batch of this viewer as the wire states it.
fn wire(game: &Game, viewer: Viewer) -> Vec<Vec<ProjectedEvent>> {
    game.event_batches(viewer, None)
        .unwrap()
        .map(|batch| {
            project_batch(
                batch.events.iter().map(|event| &event.kind),
                &Board::default(),
            )
            .unwrap()
        })
        .collect()
}

/// The wire subject of the only card `owner` has on the field, as `viewer` identifies it now.
fn fielded(game: &Game, viewer: Viewer, owner: Viewer) -> Subject {
    let observation = game.observe(viewer).unwrap();
    let card = observation
        .players
        .iter()
        .find(|player| player.player == owner)
        .unwrap()
        .zones
        .iter()
        .find(|zone| zone.kind == ObservedZone::Field)
        .unwrap()
        .cards
        .first()
        .unwrap()
        .card
        .clone();
    Subject::Card {
        card: sve_battle_contract::CardRef {
            printing_id: card.properties.card,
            face: WireNat::try_from(card.properties.face).unwrap(),
        },
        object: Some(sve_battle_contract::ObjectHandle::new(format!(
            "{}@0",
            card.handle.as_str()
        ))),
    }
}

#[test]
fn battle_damage_names_both_fighters_whether_or_not_anything_observes_damage() {
    for observed in [false, true] {
        let mut game = combat_ready(observed);
        let fighters = [Viewer::P1, Viewer::P2].map(|viewer| {
            (
                fielded(&game, viewer, Viewer::P1),
                fielded(&game, viewer, Viewer::P2),
            )
        });
        attack_the_follower(&mut game);
        for (viewer, (attacker, defender)) in [Viewer::P1, Viewer::P2].into_iter().zip(fighters) {
            let bodies = wire(&game, viewer)
                .into_iter()
                .map(|batch| {
                    batch
                        .into_iter()
                        .map(|event| event.body)
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>();
            assert!(
                bodies.contains(&vec![EventBody::AttackDeclared {
                    attacker: Some(attacker.clone()),
                    target: Some(defender.clone()),
                }]),
                "the attack names the attacker and then its target: {bodies:?}"
            );
            let lethal = Some(WireInt::try_from(0_i64).unwrap());
            assert!(
                bodies.contains(&vec![
                    EventBody::Damage {
                        source: Some(attacker.clone()),
                        target: Some(defender.clone()),
                        class: DamageClass::AttackCombat,
                        amount: WireNat::try_from(3_u64).unwrap(),
                        life_after: lethal,
                    },
                    EventBody::Damage {
                        source: Some(defender),
                        target: Some(attacker),
                        class: DamageClass::Combat,
                        amount: WireNat::try_from(3_u64).unwrap(),
                        life_after: lethal,
                    },
                ]),
                "observed={observed}: both hits of the battle are one batch, and each names \
                 who dealt it, who took it, its class, its amount and the life left: {bodies:?}"
            );
        }
    }
}

/// Every committed wire body of this viewer, in order.
fn bodies(game: &Game, viewer: Viewer) -> Vec<EventBody> {
    wire(game, viewer)
        .into_iter()
        .flatten()
        .map(|event| event.body)
        .collect()
}

/// The card that arrived in `zone`, as the wire names it.
fn arrived(bodies: &[EventBody], zone: Zone) -> Option<wire::EventCard> {
    bodies.iter().find_map(|body| {
        if let EventBody::Movement { after, .. } = body
            && after.zone == Some(zone)
        {
            after.card.clone()
        } else {
            None
        }
    })
}

#[test]
fn an_equip_names_the_created_equipment_and_then_the_follower_that_bears_it() {
    let mut game = equipment::board("イクイップメント・トークン");
    game.load_internal_card(
        "unit-follower",
        &card(vec![ability(
            1,
            vec![instruction(1, equipment::equip_action())],
        )]),
    )
    .unwrap();
    game.advance_player_input();
    activate(&mut game, 1);
    game.resume_internal_card().unwrap();
    game.advance_player_input();
    for viewer in [Viewer::P1, Viewer::P2] {
        let bodies = bodies(&game, viewer);
        let created = arrived(&bodies, Zone::Equipment);
        assert!(
            created
                .as_ref()
                .is_some_and(|card| card.card.printing_id == "unit-spell" && card.object.is_some()),
            "the equipment token arrives in the equipment zone: {bodies:?}"
        );
        let equipped = bodies
            .iter()
            .filter(|body| matches!(body, EventBody::Equipped { .. }))
            .collect::<Vec<_>>();
        assert_eq!(
            equipped,
            vec![&EventBody::Equipped {
                equipment: created.map(|card| Subject::Card {
                    card: card.card,
                    object: card.object,
                }),
                bearer: Some(fielded(&game, viewer, Viewer::P1)),
            }],
            "one result names that equipment and then the follower bearing it"
        );
    }
}

#[test]
fn an_evolution_names_the_follower_its_new_form_and_each_stage() {
    let (base, form) = evolution::programs();
    for super_payment in [0_u8, 1] {
        let mut game = evolution::position(1, base, form);
        game.advance_player_input();
        game.internal_rule_action(&json!({
            "do":"evolve","source":"s","evolve_card":"e",
            "pay":{"pp":1,"ep":0,"sep":super_payment}
        }))
        .unwrap();
        game.advance_player_input();
        for viewer in [Viewer::P1, Viewer::P2] {
            let bodies = bodies(&game, viewer);
            let follower = Some(fielded(&game, viewer, Viewer::P1));
            let placed = arrived(&bodies, Zone::Evolution);
            assert!(
                placed.as_ref().is_some_and(
                    |card| card.card.printing_id == "unit-form" && card.object.is_some()
                ),
                "the evolution card is placed in the evolution zone: {bodies:?}"
            );
            let mut expected = vec![EventBody::Evolved {
                subject: follower.clone(),
                form: placed,
                state: EvolutionState::Evolved,
                role: wire::EvolutionRole::Normal,
            }];
            if super_payment == 1 {
                expected.push(EventBody::Evolved {
                    subject: follower,
                    form: None,
                    state: EvolutionState::SuperEvolved,
                    role: wire::EvolutionRole::Normal,
                });
            }
            assert_eq!(
                bodies
                    .into_iter()
                    .filter(|body| matches!(body, EventBody::Evolved { .. }))
                    .collect::<Vec<_>>(),
                expected,
                "each stage names the follower; only the first places a card"
            );
        }
    }
}

#[test]
fn a_turn_change_stands_the_new_players_cards_and_draws_by_rule_before_its_main_phase() {
    let mut game = player_closing::board(true);
    player_closing::end(&mut game);
    assert_eq!(
        player_pipeline::answer(&mut game, Viewer::P2, Answer::Quick(None)).kind,
        SubmissionKind::Applied,
        "passing the end-phase Quick window starts the next turn"
    );
    for viewer in [Viewer::P1, Viewer::P2] {
        let bodies = bodies(&game, viewer);
        let stood = fielded(&game, viewer, Viewer::P2);
        let [
            EventBody::PhaseStarted { turn: ending },
            EventBody::PhaseStarted { turn: starting },
            EventBody::PostureChanged {
                subject,
                posture: Posture::Standing,
            },
            EventBody::Movement {
                method: wire::MovementMethod::Draw,
                by_effect: false,
                before,
                after,
            },
            EventBody::PhaseStarted { turn: main },
        ] = bodies.as_slice()
        else {
            panic!("end phase, start phase, stand, rule draw, main phase: {bodies:?}");
        };
        assert_eq!(
            (ending.active, starting.active, main.active),
            (Some(Seat::P1), Some(Seat::P2), Some(Seat::P2)),
            "the first phase belongs to the ending turn and the rest to the new one"
        );
        assert_eq!(
            subject.as_ref(),
            Some(&stood),
            "the new turn player's acted follower stands"
        );
        assert_eq!(
            (before.player, before.zone, after.player, after.zone),
            (Seat::P2, Some(Zone::Deck), Seat::P2, Some(Zone::Hand)),
            "the new turn player draws from its deck to its hand"
        );
        assert_eq!(
            (before.card.is_some(), after.card.is_some()),
            (false, viewer == Viewer::P2),
            "only the drawing player is told which card it drew"
        );
    }
}
