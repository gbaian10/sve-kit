#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate the scenario."
)]
use super::*;
use sve_engine::game::internal::DecisionView;

fn begin(number: &str, available: bool) -> (Game, DecisionView) {
    let mut game = rescan_043::form_game("evolve-choice-base", number, available);
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("evolution trigger");
    };
    assert_eq!(options.len(), 1, "one evolution ability is pending");
    assert_eq!(
        options[0].card, number,
        "the real evolved printing triggers"
    );
    let CardPlay::Awaiting(question) = game
        .answer_internal_card(
            pending.actor,
            pending.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap()
    else {
        panic!("mode declaration");
    };
    (game, question)
}

fn choose(game: &mut Game, question: &DecisionView, modes: Vec<ModeId>) {
    let before = game.projection(View::Referee).unwrap();
    for (actor, answer) in [
        (PlayerId { seat: 2 }, modes.clone()),
        (question.actor, vec![]),
        (question.actor, vec![ModeId(2), ModeId(2)]),
    ] {
        game.answer_internal_card(actor, question.token, Answer::Modes(answer))
            .unwrap_err();
        assert_eq!(
            game.projection(View::Referee).unwrap(),
            before,
            "invalid mode answers preserve authority"
        );
    }
    let targeted = modes.contains(&ModeId(1));
    let mut play = game
        .answer_internal_card(question.actor, question.token, Answer::Modes(modes))
        .unwrap();
    if targeted {
        let CardPlay::Awaiting(target) = play else {
            panic!("selected mode's target");
        };
        let InputDomain::Targets(domain) = &target.domain else {
            panic!("target declaration");
        };
        assert_eq!(
            domain.members.len(),
            1,
            "only the opposing field follower is legal"
        );
        let declared = game.projection(View::Referee).unwrap();
        assert_eq!(
            declared["P1"]["leader"]["life"], 10_u8,
            "mode effects wait for targets"
        );
        assert_eq!(
            declared["P1"]["hand"],
            json!([]),
            "drawing waits for targets"
        );
        assert_eq!(
            declared["objects"]["enemy"]["damage"], 0_u8,
            "damage waits for targets"
        );
        play = game
            .answer_internal_card(
                target.actor,
                target.token,
                Answer::Targets(vec![domain.members[0].handle]),
            )
            .unwrap();
    }
    resolved(play);
    game.resume_internal_card().unwrap();
    assert!(
        game.internal_pending_choice().unwrap().is_none(),
        "the selected modes finish once"
    );
}

#[test]
fn bp17_075_and_alias_declare_one_two_or_three_modes_before_their_effects() {
    for number in ["BP17-075", "BP17-SL19"] {
        for selected in [vec![1_u64], vec![2], vec![3], vec![1, 2], vec![1, 2, 3]] {
            let (mut game, question) = begin(number, true);
            let InputDomain::Modes {
                modes, cardinality, ..
            } = &question.domain
            else {
                panic!("modes");
            };
            assert_eq!(modes, &vec![ModeId(1), ModeId(2), ModeId(3)]);
            assert_eq!(cardinality.minimum, Nat(1));
            assert_eq!(cardinality.maximum, Some(Nat(3)));
            choose(
                &mut game,
                &question,
                selected.iter().map(|id| ModeId(*id)).collect(),
            );
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["P1"]["leader"]["life"],
                10_u64 - u64::try_from(selected.len()).unwrap()
            );
            let damage = u8::from(selected.contains(&1)) * 3 + u8::from(selected.contains(&2));
            assert_eq!(state["objects"]["enemy"]["damage"], damage);
            assert_eq!(state["objects"]["ally"]["damage"], 0_u8);
            assert_eq!(state["objects"]["off-zone"]["damage"], 0_u8);
            assert_eq!(
                state["P1"]["hand"],
                if selected.contains(&3) {
                    json!(["first"])
                } else {
                    json!([])
                }
            );
            assert_eq!(
                state["P1"]["deck"],
                if selected.contains(&3) {
                    json!(["second"])
                } else {
                    json!(["first", "second"])
                }
            );
            assert_eq!(state["P1"]["pp"]["current"], 9_u8);
        }
        let (mut game, question) = begin(number, false);
        let InputDomain::Modes { modes, .. } = &question.domain else {
            panic!("modes");
        };
        assert_eq!(modes, &vec![ModeId(2), ModeId(3)]);
        let before = game.projection(View::Referee).unwrap();
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Modes(vec![ModeId(1)]),
        )
        .unwrap_err();
        assert_eq!(game.projection(View::Referee).unwrap(), before);
        choose(&mut game, &question, vec![ModeId(2), ModeId(3)]);
        let after = game.projection(View::Referee).unwrap();
        assert_eq!(after["P1"]["leader"]["life"], 8_u8);
        assert_eq!(after["P1"]["hand"], json!(["first"]));
    }
}
