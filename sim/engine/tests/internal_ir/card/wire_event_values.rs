use super::event_batches::{attack_the_follower, combat_ready};
use super::presentation_fault::{ask_for_the_follower, prepared};
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
        let pp = game.observe(Viewer::P1).unwrap().players[0].resources.pp;
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
            assert_eq!(
                bodies
                    .iter()
                    .filter(|body| matches!(body, EventBody::ResourceChanged { .. }))
                    .collect::<Vec<_>>(),
                vec![&EventBody::ResourceChanged {
                    player: Seat::P1,
                    resource: wire::Resource::Pp,
                    change: changed(pp, pp.saturating_sub(1)),
                }],
                "the evolution's PP payment is the only resource result"
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
            EventBody::ResourceChanged {
                player: Seat::P2,
                resource: wire::Resource::MaxPp,
                change: raised,
            },
            EventBody::ResourceChanged {
                player: Seat::P2,
                resource: wire::Resource::Pp,
                change: refilled,
            },
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
            panic!(
                "end phase, start phase, maximum PP, PP, stand, rule draw, main phase: {bodies:?}"
            );
        };
        assert_eq!(
            (raised, refilled),
            (&changed(2, 3), &changed(0, 3)),
            "the new turn player's maximum PP rises by one and its PP is refilled to it"
        );
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

#[test]
fn a_turn_start_reports_only_the_pp_values_that_changed() {
    // The maximum cannot rise past 10; PP that is already full is not refilled.
    for (current, refilled) in [(4, Some(changed(4, 10))), (10, None)] {
        let mut game = player_closing::board_with_pp(true, current, 10);
        player_closing::end(&mut game);
        assert_eq!(
            player_pipeline::answer(&mut game, Viewer::P2, Answer::Quick(None)).kind,
            SubmissionKind::Applied,
            "passing the end-phase Quick window starts the next turn"
        );
        for viewer in [Viewer::P1, Viewer::P2] {
            assert_eq!(
                bodies(&game, viewer)
                    .into_iter()
                    .filter(|body| matches!(body, EventBody::ResourceChanged { .. }))
                    .collect::<Vec<_>>(),
                refilled
                    .iter()
                    .map(|change| EventBody::ResourceChanged {
                        player: Seat::P2,
                        resource: wire::Resource::Pp,
                        change: *change,
                    })
                    .collect::<Vec<_>>(),
                "{viewer:?}: starting with {current} of 10 PP"
            );
        }
    }
}

/// A wire value change from `before` to `after`.
fn changed(before: i64, after: i64) -> wire::ValueChange {
    wire::ValueChange {
        before: WireInt::try_from(before).ok(),
        after: WireInt::try_from(after).ok(),
        delta: WireInt::try_from(after.saturating_sub(before)).unwrap(),
    }
}

#[test]
fn an_effects_resource_and_life_changes_state_the_value_before_and_after() {
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["pp"] = json!({"current":3,"max":5});
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "wire-resources",
    )
    .unwrap();
    game.advance_player_input();
    let resource = |id, resource, amount| {
        instruction(
            id,
            Action::Resource {
                player: PlayerRef::Controller {},
                resource,
                operation: ResourceOperation::Add,
                amount: Expr::Literal(Int(amount)),
            },
        )
    };
    game.execute_internal(
        &checked(vec![
            // Only 2 of the 3 requested PP fit under the maximum of 5.
            resource(1, Resource::PpCurrent, 3),
            resource(2, Resource::PpMaximum, 1),
            resource(3, Resource::Ep, 2),
            instruction(
                4,
                Action::Life {
                    player: PlayerRef::Controller {},
                    change: NumericChange::Add {
                        amount: Expr::Literal(Int(-4)),
                    },
                },
            ),
        ]),
        &source("synthetic"),
        context(1),
    )
    .unwrap();
    game.advance_player_input();
    for viewer in [Viewer::P1, Viewer::P2] {
        assert_eq!(
            bodies(&game, viewer),
            vec![
                EventBody::ResourceChanged {
                    player: Seat::P1,
                    resource: wire::Resource::Pp,
                    change: changed(3, 5),
                },
                EventBody::ResourceChanged {
                    player: Seat::P1,
                    resource: wire::Resource::MaxPp,
                    change: changed(5, 6),
                },
                EventBody::ResourceChanged {
                    player: Seat::P1,
                    resource: wire::Resource::Ep,
                    change: changed(0, 2),
                },
                EventBody::LifeChanged {
                    player: Seat::P1,
                    change: changed(20, 16),
                },
            ],
            "each change names its player and resource and freezes the value it left"
        );
    }
}

#[test]
fn a_pp_payment_states_the_value_before_and_after_ahead_of_what_it_pays_for() {
    // The follower's ability costs 1 PP and its controller has 2.
    let mut game = prepared(false);
    ask_for_the_follower(&mut game);
    assert_eq!(
        player_pipeline::answer(&mut game, Viewer::P1, Answer::Targets(vec![0])).kind,
        SubmissionKind::Applied,
        "the chosen follower is damaged and the ability resolves"
    );
    for viewer in [Viewer::P1, Viewer::P2] {
        let bodies = bodies(&game, viewer);
        assert!(
            matches!(
                bodies.as_slice(),
                [
                    EventBody::ResourceChanged {
                        player: Seat::P1,
                        resource: wire::Resource::Pp,
                        change,
                    },
                    EventBody::AbilityPlayed { .. },
                    ..
                ] if *change == changed(2, 1)
            ),
            "{viewer:?}: the payment comes first, then the play it paid for: {bodies:?}"
        );
        assert_eq!(
            bodies
                .iter()
                .filter(|body| matches!(body, EventBody::ResourceChanged { .. }))
                .count(),
            1,
            "{viewer:?}: PP is paid once"
        );
        let causes = game
            .events(viewer, None)
            .unwrap()
            .take(2)
            .map(|event| event.cause)
            .collect::<Vec<_>>();
        assert_eq!(
            causes[0], causes[1],
            "{viewer:?}: the payment has the cause of the play it paid for"
        );
    }
}
