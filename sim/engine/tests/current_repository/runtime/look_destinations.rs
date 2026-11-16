#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected questions invalidate the printed Look destination scenario."
)]
use super::*;

fn position(full_ex: bool) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP20-049"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"chosen","card":"BP20-038"},
        {"id":"mage","card":"BP01-051"},
        {"id":"omen","card":"BP20-003"},
        {"id":"unseen","card":"BP20-038"}
    ]);
    if full_ex {
        initial["players"]["P1"]["zones"]["ex"] = json!(
            (0_u8..5)
                .map(|n| json!({"id":format!("old{n}"),"card":"BP01-171"}))
                .collect::<Vec<_>>()
        );
    }
    initial
}

fn finish(game: &mut Game, mut play: CardPlay, mode: u64, take: bool) {
    while let CardPlay::Awaiting(question) = play {
        let answer = match &question.domain {
            InputDomain::Modes { .. } => Answer::Modes(vec![ModeId(mode)]),
            InputDomain::Targets(domain) => {
                assert!(
                    game.internal_decision(View::P2).is_none(),
                    "the card choice stays with the Look viewer"
                );
                assert_eq!(
                    domain.members.len(),
                    1,
                    "both traits are required, and the unseen matching card is not offered"
                );
                assert_eq!(
                    domain.members[0].card.as_deref(),
                    Some("BP20-038"),
                    "the only eligible card has both required traits"
                );
                Answer::Targets(if take {
                    vec![domain.members[0].handle]
                } else {
                    vec![]
                })
            }
            InputDomain::Order(cards) => {
                assert!(
                    game.internal_decision(View::P2).is_none(),
                    "only the viewer orders the remaining Look"
                );
                Answer::Order(cards.iter().rev().map(|card| card.handle).collect())
            }
            _ => panic!("unexpected Look destination decision"),
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    resolved(play);
}

#[test]
fn false_formula_chooses_hand_or_ex_from_one_look_and_bottoms_only_the_remainder() {
    for (mode, take, full_ex) in [
        (1_u64, true, false),
        (2, true, false),
        (1, false, false),
        (2, true, true),
    ] {
        let mut game = Game::from_selected_effects(
            selection_with_rules().clone(),
            &position(full_ex),
            &Value::Null,
            &Value::Null,
            "look-destinations",
        )
        .unwrap();
        let play = game
            .play_internal_card("source", PlayerId { seat: 1 })
            .unwrap();
        finish(&mut game, play, mode, take);
        let view = game.projection(View::Referee).unwrap();
        let moved = take && !(mode == 2 && full_ex);
        assert_eq!(
            view["P1"]["hand"],
            if moved && mode == 1 {
                json!(["chosen"])
            } else {
                json!([])
            }
        );
        if full_ex {
            assert_eq!(view["P1"]["ex"].as_array().unwrap().len(), 5);
        } else {
            assert_eq!(
                view["P1"]["ex"],
                if moved && mode == 2 {
                    json!(["chosen"])
                } else {
                    json!([])
                }
            );
        }
        assert_eq!(
            view["P1"]["deck"],
            if moved {
                json!(["unseen", "omen", "mage"])
            } else {
                json!(["unseen", "omen", "mage", "chosen"])
            }
        );
        assert_eq!(view["P1"]["pp"]["current"], 9_u8);
        let opponent = game.projection(View::P2).unwrap();
        let known = opponent["known_cards"].as_array().unwrap();
        for hidden in ["mage", "omen", "unseen"] {
            assert!(
                !known.iter().any(|card| card["id"] == hidden),
                "the unselected card {hidden} stays private"
            );
        }
        if moved {
            assert!(
                known.iter().any(|card| card["id"] == "chosen"),
                "the selected card is revealed before its movement"
            );
        }
    }
}
