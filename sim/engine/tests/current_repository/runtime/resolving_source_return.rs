#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Only the printed Look choice and remainder ordering are expected."
)]
use super::*;

const PLAYER: PlayerId = PlayerId { seat: 1 };

fn finish(game: &mut Game, mut play: CardPlay, take: bool) {
    while let CardPlay::Awaiting(question) = play {
        let answer = match &question.domain {
            InputDomain::Targets(domain) => Answer::Targets(
                domain
                    .members
                    .iter()
                    .take(usize::from(take))
                    .map(|card| card.handle)
                    .collect(),
            ),
            InputDomain::Order(cards) => {
                Answer::Order(cards.iter().rev().map(|card| card.handle).collect())
            }
            _ => panic!("unexpected Avritia choice"),
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}

fn play(card: &str, looked: &str, take: bool) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"chosen","card":looked},
        {"id":"second","card":"CP03-052"},
        {"id":"third","card":"CP03-058"},
        {"id":"fourth","card":"BP01-037"},
        {"id":"tail","card":"CP03-052"}
    ]);
    let mut game = Game::from_selected_effects(
        selection_with_rules().clone(),
        &initial,
        &Value::Null,
        &Value::Null,
        "resolving-source-return",
    )
    .unwrap();
    let step = game.play_internal_card("source", PLAYER).unwrap();
    finish(&mut game, step, take);
    let public = game.projection(View::P2).unwrap();
    let known = public["known_cards"].as_array().unwrap();
    assert_eq!(
        known.iter().any(|known_card| known_card["id"] == "chosen"),
        take,
        "only the selected thief is revealed to the opponent"
    );
    for hidden in ["second", "third", "fourth", "tail"] {
        assert!(
            !known.iter().any(|known_card| known_card["id"] == hidden),
            "the unselected private Look members stay hidden"
        );
    }
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        state["P1"]["pp"]["current"], 9_u8,
        "the spell pays its printed one PP exactly once"
    );
    assert_eq!(
        state["P1"]["resolution"],
        json!([]),
        "the actual resolving source is cleaned up or moved"
    );
    assert_eq!(
        state["P1"]["deck"],
        if take {
            json!(["tail", "fourth", "third", "second"])
        } else {
            json!(["tail", "fourth", "third", "second", "chosen"])
        },
        "only the observed remainder is ordered at the bottom"
    );
    state
}

#[test]
fn bp05_034_all_printings_return_after_selecting_and_revealing_octrice() {
    for card in ["BP05-034", "BP05-P14", "PR-490"] {
        let state = play(card, "BP05-018", true);
        assert_eq!(state["P1"]["hand"], json!(["chosen", "source"]));
        assert_eq!(state["P1"]["cemetery"], json!([]));
        assert_eq!(state["objects"]["source"]["generation"], 2_u8);
    }
}

#[test]
fn bp05_034_all_printings_still_clean_up_on_another_thief_or_declining_selection() {
    for card in ["BP05-034", "BP05-P14", "PR-490"] {
        for (looked, take) in [("BP05-029", true), ("BP05-018", false)] {
            let state = play(card, looked, take);
            assert_eq!(
                state["P1"]["hand"],
                if take { json!(["chosen"]) } else { json!([]) }
            );
            assert_eq!(state["P1"]["cemetery"], json!(["source"]));
        }
    }
}
