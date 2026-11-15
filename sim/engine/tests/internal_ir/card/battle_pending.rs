use super::*;
use crate::board_fixture;
use sve_battle_contract::SourceRef;
use sve_engine::game::internal::InputDomain;
use sve_engine::game::player::Viewer;

pub(super) fn catalog_for(snapshot: &str) -> Catalog {
    board_fixture::catalog_for(snapshot).unwrap()
}

pub(super) fn fixture() -> &'static board_fixture::Fixture {
    board_fixture::fixture().unwrap()
}

pub(super) fn board(actor: Viewer, secret: &str) -> Game {
    board_fixture::empty_board(actor, secret).unwrap()
}

pub(super) fn board_with_program(
    actor: Viewer,
    secret: &str,
    follower: &WellFormedCardProgram,
) -> Game {
    board_fixture::board(actor, secret, follower).unwrap()
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
            // The full board reads a prepared player stop, rather than preparing its own input.
            let question = game.internal_pending_choice().unwrap().unwrap();
            assert!(matches!(question.domain, InputDomain::Pending(_)));
        }
        let before = exact_authority(&first);
        for viewer in [Viewer::P1, Viewer::P2] {
            let projected = first
                .battle_observation(viewer, &fixture().references)
                .unwrap()
                .pending;
            assert_eq!(
                projected,
                second
                    .battle_observation(viewer, &fixture().references)
                    .unwrap()
                    .pending
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
