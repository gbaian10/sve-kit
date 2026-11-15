use super::super::player_stages::tests::{execution, fixture};
use super::*;
use crate::battle::{EventObjects, ProjectionError, project_batch};
use crate::game::player::events::EventCard;
use crate::game::player::observation::TurnObservation;
use crate::ir::Nat;
use sve_battle_contract::event::{self as wire, EventBody};
use sve_battle_contract::observation::{Declared, DeclaredNumber, TurnView};
use sve_battle_contract::{Int, Seat, SourceRef};

/// A declaration names no card, source or turn, so its projection must need no lookup.
struct NoLookups;

impl EventObjects for NoLookups {
    fn card(&self, _card: &EventCard) -> Result<wire::EventCard, ProjectionError> {
        Err(ProjectionError::Unavailable)
    }

    fn source(&self, _source: &EffectSource) -> Result<SourceRef, ProjectionError> {
        Err(ProjectionError::Unavailable)
    }

    fn turn(&self, _turn: &TurnObservation) -> Result<TurnView, ProjectionError> {
        Err(ProjectionError::Unavailable)
    }
}

#[test]
fn a_declared_number_is_one_public_result_naming_its_declarer_and_value() {
    let mut game = fixture();
    game.enable_player_events();
    let mut execution = execution(&mut game);
    let declared = Fact::Declared {
        player: PlayerId { seat: 2 },
        value: Nat(4),
    };
    execution.record_fact(&mut game, declared.clone());
    game.publish_player_events();
    assert_eq!(
        execution.facts.last(),
        Some(&declared),
        "the declaration is an authority fact later text can read"
    );
    for viewer in [Viewer::P1, Viewer::P2] {
        let events = game.events(viewer, None).unwrap().collect::<Vec<_>>();
        assert!(
            matches!(events.as_slice(), [event]
                if matches!(&event.kind, EventKind::Action(action)
                    if action.kind == ActionKind::Declare
                        && action.player == Some(Viewer::P2)
                        && action.subjects.is_empty()
                        && matches!(action.numbers.as_slice(), [number]
                            if number.amount == 4 && number.before.is_none()))),
            "both seats see one declaration by the second seat with its value: {events:?}"
        );
        let projected = project_batch(events.iter().map(|event| &event.kind), &NoLookups).unwrap();
        assert!(
            matches!(projected.as_slice(), [event]
            if event.about.is_empty()
                && event.body
                    == EventBody::ChoiceDeclared {
                        player: Seat::P2,
                        declared: Declared::Number {
                            role: DeclaredNumber::Free,
                            value: Int::try_from(4_i64).unwrap(),
                        },
                    }),
            "on the wire it is a declared choice whose number is not an X: {projected:?}"
        );
    }
}
