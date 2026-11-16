use super::*;
use crate::EngineFailure;
use crate::game::player::Input;

fn opposite(actor: Viewer) -> Viewer {
    if actor == Viewer::P1 {
        Viewer::P2
    } else {
        Viewer::P1
    }
}

fn private_candidates(game: &Game, actor: Viewer) -> Vec<player::SelectionMember> {
    let native = question(game, actor).unwrap();
    let Some(DecisionShape::ResourceEntryPayment { materials, .. }) = domain(&native) else {
        panic!("resource stop");
    };
    materials.members.clone()
}

#[test]
fn equal_hidden_deck_sizes_with_different_candidates_preserve_the_waiting_seat_both_directions() {
    let refs = &board_fixture::fixture().unwrap().references;
    for actor in [Viewer::P1, Viewer::P2] {
        let other = opposite(actor);
        let a = make(
            actor,
            &["unit-follower", "unit-spell", "unit-amulet"],
            "unit-spell",
            1,
            0,
        );
        let b = make(
            actor,
            &["unit-follower", "unit-follower", "unit-amulet"],
            "unit-spell",
            1,
            0,
        );
        assert_ne!(
            private_candidates(&a, actor).len(),
            private_candidates(&b, actor).len(),
            "the real candidates differ before projection"
        );
        let before = [
            board_fixtures::exact_authority(&a),
            board_fixtures::exact_authority(&b),
        ];
        assert!(
            a.observe(other).unwrap() == b.observe(other).unwrap(),
            "complete waiting native boards hide resource candidates"
        );
        assert!(
            a.input(other) == b.input(other),
            "complete Waiting summaries hide candidates"
        );
        assert!(matches!(a.input(other), Input::Waiting(_)));
        assert_eq!(components(&a, other), components(&b, other));
        assert!(matches!(
            a.battle_question(other, &QuestionId::new("private".into()), refs),
            Ok(None)
        ));
        assert!(matches!(
            b.battle_question(other, &QuestionId::new("private".into()), refs),
            Ok(None)
        ));
        let (_, _, shape_a, _) = projected(&a, actor);
        let (_, _, shape_b, _) = projected(&b, actor);
        assert_ne!(
            shape_a, shape_b,
            "only the chooser learns its complete payment choices"
        );
        assert!(matches!(
            a.battle_question(actor, &QuestionId::new("private".into()), refs),
            Err(ProjectionError::Unavailable)
        ));
        assert!(
            board_fixtures::exact_authority(&a) == before[0],
            "reads preserve authority"
        );
        assert!(
            board_fixtures::exact_authority(&b) == before[1],
            "reads preserve authority"
        );
    }
}

#[test]
fn opponent_secrets_do_not_change_the_actor_complete_board_or_resource_question() {
    for actor in [Viewer::P1, Viewer::P2] {
        let a = make(actor, &["unit-follower", "unit-spell"], "unit-spell", 1, 0);
        let b = make(actor, &["unit-follower", "unit-spell"], "unit-amulet", 1, 0);
        assert!(
            a.observe(actor).unwrap() == b.observe(actor).unwrap(),
            "complete actor native boards hide the opponent secret"
        );
        assert_eq!(components(&a, actor), components(&b, actor));
        assert!(
            a.input(actor) == b.input(actor),
            "the full native question is independent of opponent secrets"
        );
        assert_eq!(projected(&a, actor).2, projected(&b, actor).2);
    }
}

#[test]
fn no_matching_resource_has_no_payment_stop_or_spend_both_seats() {
    let program = ExpandedLowerer::new()
        .unwrap()
        .lower(
            &payment_fixtures::x_document(
                payment_expanded_tree(),
                "feed",
                json!({"cost":{"pp":1_u32}}),
            )
            .unwrap(),
        )
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    for actor in [Viewer::P1, Viewer::P2] {
        let (own, _) = seats(actor);
        let mut initial = board_fixtures::setup(actor.player().seat, json!([]));
        initial["turn"]["active"] = json!(own);
        initial["players"][own]["zones"] = json!({"field":[{"id":"source","card":"unit-follower"}],"evolve_deck":[{"id":"wrong","card":"unit-spell"}]});
        let mut game = Game::new(
            catalog(),
            &initial,
            &Value::Null,
            &Value::Null,
            "resource-empty",
        )
        .unwrap();
        let empty = WellFormedCardProgram::check(CardProgram {
            abilities: vec![],
            card_play_costs: vec![],
        })
        .unwrap();
        game.load_internal_card("unit-spell", &empty).unwrap();
        game.load_internal_card("unit-amulet", &empty).unwrap();
        game.load_internal_card("unit-follower", &program).unwrap();
        let before = board_fixtures::exact_authority(&game);
        let error = game
            .activate_internal_card(
                "source",
                program.card().abilities.first().unwrap().node.id,
                PlayerId {
                    seat: actor.player().seat,
                },
            )
            .unwrap_err();
        assert!(
            matches!(error, EngineFailure::IllegalPlay(_)),
            "a missing native resource is an illegal play"
        );
        assert!(
            board_fixtures::exact_authority(&game) == before,
            "insufficient resources cannot publish or pay an entry"
        );
    }
}
