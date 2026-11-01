#![expect(
    clippy::panic,
    reason = "The real Quick spell requires its authority window and printed either choice."
)]
use super::*;

fn position(occupied: usize) -> Value {
    let mut state = draw_position();
    state["players"]["P2"]["zones"]["hand"] = json!([{"id":"source","card":"SD03-016"}]);
    state["players"]["P2"]["zones"]["ex"] = json!(
        (0..occupied)
            .map(|n| json!({"id":format!("old-{n}"),"card":"BP01-T09"}))
            .collect::<Vec<_>>()
    );
    state
}

fn quick_choice(game: &mut Game) -> CardPlay {
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    let quick = game.internal_quick_choice().unwrap().unwrap();
    assert_eq!(
        quick.actor.seat, 2,
        "the non-turn player receives Quick authority"
    );
    let InputDomain::Quick { options, .. } = &quick.domain else {
        panic!("a formal Quick window is required");
    };
    assert_eq!(
        options.len(),
        1,
        "only the real Quick spell is playable here"
    );
    assert_eq!(
        options[0].card, "SD03-016",
        "the complete spell supplies Quick"
    );
    assert_eq!(options[0].zone, "hand", "the real spell is cast from hand");
    game.answer_internal_card(
        quick.actor,
        quick.token,
        Answer::Quick(Some(options[0].handle)),
    )
    .unwrap()
}

fn choose_golem(game: &mut Game, play: CardPlay, index: usize) {
    let CardPlay::Awaiting(question) = play else {
        panic!("the spell chooses its token name during resolution");
    };
    let InputDomain::Modes {
        modes,
        count,
        cardinality,
    } = &question.domain
    else {
        panic!("the printed either requires one mode");
    };
    assert_eq!(
        question.actor.seat, 2,
        "the spell controller chooses the name"
    );
    assert_eq!(*count, Nat(1), "one token name is chosen");
    assert_eq!(
        cardinality.minimum,
        Nat(1),
        "the printed choice is required"
    );
    assert_eq!(modes.len(), 2, "both printed Golem names remain available");
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["pp"]["current"],
        9_u8,
        "the real one-PP spell pays before the choice"
    );
    resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Modes(vec![modes[index]]),
        )
        .unwrap(),
    );
}

#[test]
fn sd03_016_uses_the_opponents_quick_window_and_cannot_overfill_ex() {
    for (index, expected, power, hp) in [(0_usize, "BP01-T09", 2_u8, 3_u8), (1, "BP01-T08", 3, 2)] {
        for occupied in [0_usize, 5] {
            let mut game = game(&position(occupied));
            let play = quick_choice(&mut game);
            choose_golem(&mut game, play, index);
            let view = game.projection(View::Referee).unwrap();
            let ex = view["P2"]["ex"].as_array().unwrap();
            assert_eq!(
                ex.len(),
                if occupied == 0 { 1 } else { 5 },
                "Q97 forbids exceeding EX capacity"
            );
            let arrivals = ex
                .iter()
                .filter(|id| id.as_str().unwrap().starts_with("new-"))
                .map(|id| &view["objects"][id.as_str().unwrap()])
                .collect::<Vec<_>>();
            assert_eq!(
                arrivals.len(),
                usize::from(occupied == 0),
                "a full EX receives no token"
            );
            for card in arrivals {
                assert_eq!(
                    card["card"], expected,
                    "only the selected complete Golem is created"
                );
                assert_eq!(card["power"], power, "the token retains printed power");
                assert_eq!(card["hp"], hp, "the token retains printed health");
            }
            assert_eq!(
                view["P2"]["pp"]["current"], 9_u8,
                "a full EX does not refund the spell cost"
            );
            assert_eq!(
                view["P2"]["cemetery"],
                json!(["source"]),
                "the actual spell resolves to its controllers cemetery"
            );
            assert_eq!(
                view["P2"]["field"],
                json!([]),
                "neither Golem enters the field"
            );
            assert_eq!(
                view["P1"]["ex"],
                json!([]),
                "the turn player receives no token"
            );
        }
    }
}
