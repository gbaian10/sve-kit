use super::*;
use crate::card::processing_stage::declarations::with_declarations;
use sve_engine::game::internal::NumberDeclaration;

#[test]
fn child_number_facts_do_not_become_the_parents_frame_declarations() {
    for child_fails in [false, true] {
        let (mut game, definition) = build_custom(
            "self",
            "unit-spell",
            true,
            Scenario::ProcessingPause { child_fails },
            |programs| {
                for number in ["unit-amulet", "unit-spell"] {
                    let current = programs.get_mut(number).unwrap();
                    *current = with_declarations(current, &[PlayerRef::Controller {}], true);
                }
            },
        );
        let outer = question(
            game.activate_internal_card("parent", definition, PlayerId { seat: 1 })
                .unwrap(),
        );
        let CardPlay::Awaiting(parent_number) = select(&mut game, &outer) else {
            panic!("parent declares before starting the child")
        };
        assert_eq!(parent_number.token.execution, outer.token.execution);
        let child = game
            .answer_internal_card(
                parent_number.actor,
                parent_number.token,
                Answer::Integer(Nat(4)),
            )
            .unwrap();
        let parent = if child_fails {
            child
        } else {
            let child = question(child);
            assert_ne!(child.token.execution, outer.token.execution);
            assert!(
                game.internal_number_declarations().unwrap().is_empty(),
                "the child does not inherit parent declarations"
            );
            let CardPlay::Awaiting(child_number) = select(&mut game, &child) else {
                panic!("the child must declare its own number")
            };
            assert_eq!(child_number.token.execution, child.token.execution);
            game.answer_internal_card(
                child_number.actor,
                child_number.token,
                Answer::Integer(Nat(2)),
            )
            .unwrap()
        };
        let CardPlay::Awaiting(parent) = parent else {
            panic!("parent reaches its own optional body")
        };
        assert_eq!(parent.token.execution, outer.token.execution);
        assert_eq!(
            game.internal_number_declarations().unwrap(),
            &[NumberDeclaration {
                player: PlayerId { seat: 1 },
                value: Nat(4)
            }]
        );
        let result = resolved(
            game.answer_internal_card(parent.actor, parent.token, Answer::Integer(Nat(0)))
                .unwrap(),
        );
        let numbers = result
            .progress
            .facts
            .iter()
            .filter_map(|fact| {
                if let Fact::Declared { value, .. } = fact {
                    Some(value.0)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        assert_eq!(
            numbers,
            if child_fails { vec![4] } else { vec![4, 2] },
            "the causal history still contains the child's own declaration"
        );
        assert_eq!(game.internal_number_declarations(), None);
    }
}
