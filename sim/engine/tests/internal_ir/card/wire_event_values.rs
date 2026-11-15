use super::event_batches::{attack_the_follower, combat_ready};
use super::presentation_feed::Board;
use super::*;
use sve_battle_contract::event::{DamageClass, EventBody};
use sve_battle_contract::{Int as WireInt, Nat as WireNat, Subject};
use sve_engine::battle::{ProjectedEvent, project_batch};
use sve_engine::game::player::Viewer;
use sve_engine::game::player::observation::ObservedZone;

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
