use super::*;
use sve_engine::game::internal::InstructionOutcome;

fn position(seat: &str, target: bool) -> Value {
    let opponent = if seat == "P1" { "P2" } else { "P1" };
    let mut initial = draw_position();
    initial["turn"]["active"] = json!(seat);
    initial["players"][seat]["zones"]["hand"] = json!([{"id":"spell","card":"BP14-098"}]);
    initial["players"][opponent]["zones"]["field"] = if target {
        json!([{"id":"target","card":"BP01-037"}])
    } else {
        json!([])
    };
    initial
}

#[test]
fn current_spell_banishes_the_selected_opposing_follower_and_rejects_bad_answers() {
    for seat in ["P1", "P2"] {
        let opponent = if seat == "P1" { "P2" } else { "P1" };
        let mut game = game(&position(seat, true));
        let CardPlay::Awaiting(question) = game
            .play_internal_card(
                "spell",
                PlayerId {
                    seat: seat_number(seat),
                },
            )
            .unwrap()
        else {
            panic!("spell needs a target");
        };
        assert!(
            matches!(&question.domain, InputDomain::Targets(domain) if domain.members.len() == 1),
            "one opposing field follower is eligible"
        );
        for (actor, handles) in [
            (
                PlayerId {
                    seat: seat_number(opponent),
                },
                vec![0],
            ),
            (question.actor, vec![99]),
            (question.actor, vec![0, 0]),
        ] {
            let before = format!("{game:?}");
            game.answer_internal_card(actor, question.token, Answer::Targets(handles))
                .unwrap_err();
            assert!(
                format!("{game:?}") == before,
                "invalid answer changed the pending spell"
            );
        }
        let result = resolved(
            game.answer_internal_card(question.actor, question.token, Answer::Targets(vec![0]))
                .unwrap(),
        );
        let outcomes = result
            .progress
            .receipts
            .values()
            .filter_map(|outcome| {
                if let InstructionOutcome::Banish(outcome) = outcome {
                    Some(outcome)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        assert!(
            matches!(outcomes.as_slice(), [outcome] if outcome.successful.len() == 1 && outcome.moves[0].before.generation == Generation(0) && outcome.moves[0].after.is_some_and(|after| after.generation == Generation(1))),
            "one real banishment advances the target generation"
        );
        assert_eq!(result.progress.facts.iter().filter(|fact| matches!(fact, Fact::Movement(movement) if movement.method == MoveMethod::Banish)).count(), 1, "the body produces exactly one Banish movement fact");
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state[opponent]["banish"],
            json!(["target"]),
            "banishment uses the target owner's zone"
        );
        assert_eq!(
            state[seat]["cemetery"],
            json!(["spell"]),
            "the spell finishes its ordinary cleanup"
        );
    }
}

#[test]
fn no_legal_opposing_follower_rejects_the_spell_before_payment() {
    for seat in ["P1", "P2"] {
        let mut game = game(&position(seat, false));
        let before = format!("{game:?}");
        game.play_internal_card(
            "spell",
            PlayerId {
                seat: seat_number(seat),
            },
        )
        .unwrap_err();
        assert!(
            format!("{game:?}") == before,
            "a spell without a legal target changed authority"
        );
    }
}
