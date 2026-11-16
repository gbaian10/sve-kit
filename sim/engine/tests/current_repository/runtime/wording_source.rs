use super::*;
use sve_engine::game::player::{ObjectHandle, Viewer, observation as native};

fn expected(number: &str) -> native::WordingSource {
    let input = &inputs().2["authors"]
        .as_array()
        .unwrap()
        .iter()
        .find(|author| author["input"]["card_no"] == number)
        .unwrap()["input"];
    let current = lowerer().lower_current(input).unwrap();
    let source = &current.program.card().abilities[0].source;
    let owner: Value = serde_json::from_str(&source.owner).unwrap();
    assert_eq!(
        source.field, "effect",
        "the real fixture uses the main printed field"
    );
    native::WordingSource {
        face_id: source.face.clone(),
        revision_id: owner["revision_id"].as_str().unwrap().into(),
        field: native::WordingField::Effect,
    }
}

fn field_source(game: &Game, viewer: Viewer, actor: Viewer) -> native::EffectSource {
    let observed = game.observe(viewer).unwrap();
    let player = observed
        .players
        .iter()
        .find(|player| player.player == actor)
        .unwrap();
    player.zones.iter().find(|zone| zone.kind == native::ObservedZone::Field).unwrap().cards[0]
        .card.properties.state.as_ref().unwrap().effects.iter().find(|effect| {
            matches!(&effect.change, native::InstalledChange::Grant { definitions, .. } if !definitions.is_empty())
        }).unwrap().source.clone().unwrap()
}

fn recipient(game: &Game, viewer: Viewer, actor: Viewer) -> ObjectHandle {
    let observed = game.observe(viewer).unwrap();
    observed
        .players
        .iter()
        .find(|player| player.player == actor)
        .unwrap()
        .zones
        .iter()
        .find(|zone| zone.kind == native::ObservedZone::Field)
        .unwrap()
        .cards[0]
        .card
        .handle
        .clone()
}

#[test]
fn wording_source_current_lastword_keeps_its_revision_after_the_holder_departs() {
    let expected = expected(NUMBER);
    for actor in ["P1", "P2"] {
        let mut game = with_producer(
            &setup(actor, actor, [20, 20]),
            Action::Destroy {
                objects: subjects(),
            },
        );
        activate(&mut game, actor);
        game.resume_internal_card().unwrap();
        game.internal_pending_choice().unwrap().unwrap();
        let before = format!("{game:?}");
        for viewer in [Viewer::P1, Viewer::P2] {
            let pending = game.observe(viewer).unwrap().pending;
            assert_eq!(pending.len(), 1);
            assert_eq!(pending[0].source.card, NUMBER);
            assert_eq!(pending[0].source.wording.as_deref(), Some(&expected));
            assert_eq!(
                pending[0].source.provenance,
                native::TextProvenance::Printed
            );
            assert_eq!(pending[0].source.holder, None);
        }
        assert!(
            format!("{game:?}") == before,
            "wording reads must not change authority"
        );
    }
}

fn grant_game(own: &str, other: &str, provider: &str) -> Game {
    // The fixture moves the departed provider without replacing its accepted program.
    let producer = producer_steps(vec![
        Action::Move {
            objects: subjects_in(ZoneKind::Cemetery),
            movement: MoveSpec {
                to: MoveDestination::Owners {
                    zone: OwnerZone::Deck,
                },
                initial_posture: None,
                entry_effects: vec![],
                position: Some(Position::Top {}),
                order: None,
            },
            reason: MoveReason::Put,
        },
        Action::Shuffle {
            deck: PlayerRef::Controller {},
            scope: ShuffleScope::FullZone {},
            randomness: RandomnessPolicy::FreshRequest,
        },
    ]);
    let mut initial = draw_position();
    initial["turn"]["active"] = json!(own);
    initial["players"][own]["zones"]["hand"] = json!([{"id":"spell","card":provider}]);
    initial["players"][own]["zones"]["field"] = json!([
        {"id":"ally","card":"BP01-037","state":{"power":2_i64}},
        {"id":"other","card":"BP01-037"}
    ]);
    initial["players"][other]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    with_selected_producer_program(&initial, &producer, producer_selection())
}

#[test]
fn wording_source_current_grant_keeps_provider_revision_after_a_shuffle() {
    const PROVIDER: &str = "BP03-083";
    let expected = expected(PROVIDER);
    for actor in [Viewer::P1, Viewer::P2] {
        let (own, other) = if actor == Viewer::P1 {
            ("P1", "P2")
        } else {
            ("P2", "P1")
        };
        let mut game = grant_game(own, other, PROVIDER);
        let CardPlay::Awaiting(question) =
            game.play_internal_card("spell", actor.player()).unwrap()
        else {
            panic!("the printed grant declares a target")
        };
        let InputDomain::Targets(domain) = question.domain else {
            panic!("printed target")
        };
        let target = domain
            .members
            .iter()
            .find(|member| member.card.as_deref() == Some("BP01-037"))
            .unwrap()
            .handle;
        resolved(
            game.answer_internal_card(
                question.actor,
                question.token,
                Answer::Targets(vec![target]),
            )
            .unwrap(),
        );
        game.resume_internal_card().unwrap();
        assert!(game.internal_pending_choice().unwrap().is_none());
        for viewer in [Viewer::P1, Viewer::P2] {
            assert_eq!(
                field_source(&game, viewer, actor).wording.as_deref(),
                Some(&expected)
            );
        }

        activate(&mut game, own);
        game.resume_internal_card().unwrap();
        let before = format!("{game:?}");
        for viewer in [Viewer::P1, Viewer::P2] {
            let source = field_source(&game, viewer, actor);
            assert_eq!(source.card, PROVIDER);
            assert_eq!(source.wording.as_deref(), Some(&expected));
            // Installation retains the producer link captured before departure.
            assert_eq!(source.holder, None);
            assert_eq!(
                game.observe(viewer)
                    .unwrap()
                    .players
                    .iter()
                    .find(|player| player.player == actor)
                    .unwrap()
                    .zones
                    .iter()
                    .find(|zone| zone.kind == native::ObservedZone::Deck)
                    .unwrap()
                    .cards
                    .len(),
                0,
                "a shuffled deck exposes no identifiable members"
            );
        }
        assert!(
            format!("{game:?}") == before,
            "wording reads must not change authority"
        );
        game.internal_rule_action(
            &json!({"do":"attack","attacker":"ally","target":format!("{other}.leader")}),
        )
        .unwrap();
        game.internal_pending_choice().unwrap().unwrap();
        let before_pending = format!("{game:?}");
        for viewer in [Viewer::P1, Viewer::P2] {
            let pending = game.observe(viewer).unwrap().pending;
            assert_eq!(pending.len(), 1);
            assert_eq!(pending[0].source.card, PROVIDER);
            assert_eq!(pending[0].source.wording.as_deref(), Some(&expected));
            assert_eq!(
                pending[0].source.provenance,
                native::TextProvenance::Granted {
                    by: native::CardFace {
                        card: PROVIDER.into(),
                        face: 0
                    }
                }
            );
            // The trigger occurrence belongs to the recipient's current generation.
            assert_eq!(
                pending[0].source.holder,
                Some(recipient(&game, viewer, actor))
            );
        }
        assert!(
            format!("{game:?}") == before_pending,
            "pending wording reads must not change authority"
        );
    }
}
