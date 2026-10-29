#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate the scenario."
)]
use super::*;
use sve_engine::game::internal::DecisionView;

fn awaiting(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(question) = play else {
        panic!("expected a printed decision");
    };
    question
}

fn declare(game: &mut Game, number: &str) -> CardPlay {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("expected the evolution trigger");
    };
    assert_eq!(options.len(), 1, "only this evolution ability is pending");
    assert_eq!(
        options[0].card, number,
        "the real evolved printing triggers"
    );
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::Pending(options[0].handle),
    )
    .unwrap()
}

#[test]
fn bp20_047_and_alias_require_a_target_before_damage_draw_and_discard() {
    for number in ["BP20-047", "BP20-P23"] {
        for available in [false, true] {
            let mut game = rescan_043::form_game("play-targets-base", number, available);
            let play = declare(&mut game, number);
            if !available {
                assert_eq!(play, CardPlay::Cancelled);
                game.resume_internal_card().unwrap();
                assert!(game.internal_pending_choice().unwrap().is_none());
                let state = game.projection(View::Referee).unwrap();
                assert_eq!(state["P1"]["deck"], json!(["first", "second"]));
                assert_eq!(state["P1"]["hand"], json!([]));
                assert_eq!(state["P1"]["cemetery"], json!([]));
                continue;
            }
            let target = awaiting(play);
            let InputDomain::Targets(domain) = &target.domain else {
                panic!("expected the opposing follower target");
            };
            assert_eq!(domain.members.len(), 1);
            let before = game.projection(View::Referee).unwrap();
            assert_eq!(before["objects"]["enemy"]["damage"], 0_u8);
            assert_eq!(before["P1"]["deck"], json!(["first", "second"]));
            assert_eq!(before["P1"]["hand"], json!([]));
            game.answer_internal_card(
                PlayerId { seat: 2 },
                target.token,
                Answer::Targets(vec![domain.members[0].handle]),
            )
            .unwrap_err();
            assert_eq!(game.projection(View::Referee).unwrap(), before);
            game.answer_internal_card(target.actor, target.token, Answer::Targets(vec![]))
                .unwrap_err();
            assert_eq!(game.projection(View::Referee).unwrap(), before);
            let discard = awaiting(
                game.answer_internal_card(
                    target.actor,
                    target.token,
                    Answer::Targets(vec![domain.members[0].handle]),
                )
                .unwrap(),
            );
            let InputDomain::Targets(choices) = &discard.domain else {
                panic!("expected discard after damage and draw");
            };
            assert_eq!(choices.members.len(), 1);
            let drawn = game.projection(View::Referee).unwrap();
            assert_eq!(drawn["objects"]["enemy"]["damage"], 3_u8);
            assert_eq!(drawn["objects"]["ally"]["damage"], 0_u8);
            assert_eq!(drawn["P2"]["ex"], json!(["off-zone"]));
            assert_eq!(drawn["P1"]["hand"], json!(["first"]));
            assert_eq!(drawn["P1"]["deck"], json!(["second"]));
            resolved(
                game.answer_internal_card(
                    discard.actor,
                    discard.token,
                    Answer::Targets(vec![choices.members[0].handle]),
                )
                .unwrap(),
            );
            game.resume_internal_card().unwrap();
            assert!(game.internal_pending_choice().unwrap().is_none());
            let after = game.projection(View::Referee).unwrap();
            assert_eq!(after["P1"]["hand"], json!([]));
            assert_eq!(after["P1"]["cemetery"], json!(["first"]));
            assert_eq!(after["P1"]["pp"]["current"], 9_u8);
        }
    }
}
