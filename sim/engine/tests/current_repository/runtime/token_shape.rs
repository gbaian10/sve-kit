#![expect(
    clippy::panic,
    reason = "The printed token scenarios require their exact choice boundaries."
)]
use super::*;
use sve_engine::game::internal::{DecisionView, InstructionOutcome};

fn fixture(number: &str, zone: &str, occupied: usize) -> Game {
    let mut state = draw_position();
    state["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":number}]);
    state["players"]["P1"]["zones"][zone] = json!(
        (0..occupied)
            .map(|n| json!({"id":format!("old-{n}"),"card":"BP17-T11"}))
            .collect::<Vec<_>>()
    );
    game(&state)
}

fn question(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(question) = play else {
        panic!("printed choice");
    };
    question
}

fn trigger(game: &mut Game, number: &str) -> CardPlay {
    assert!(
        matches!(
            game.play_internal_card("source", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ),
        "real follower pays and enters before its fanfare"
    );
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("real fanfare occurrence");
    };
    assert_eq!(options.len(), 1, "one printed fanfare");
    assert_eq!(
        options[0].card, number,
        "the actual printing supplies the effect"
    );
    game.answer_internal_card(
        pending.actor,
        pending.token,
        Answer::Pending(options[0].handle),
    )
    .unwrap()
}

fn capacity(game: &mut Game, question: &DecisionView, names: &[&str]) -> CardPlay {
    let InputDomain::TokenCapacity { choices } = &question.domain else {
        panic!("mixed creation asks public names");
    };
    let handle = u64::try_from(
        choices
            .iter()
            .position(|offered| offered.iter().map(String::as_str).eq(names.iter().copied()))
            .unwrap(),
    )
    .unwrap();
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::TokenCapacity(handle),
    )
    .unwrap()
}

fn cards(game: &Game, zone: &str) -> Vec<String> {
    let view = game.projection(View::Referee).unwrap();
    view["P1"][zone]
        .as_array()
        .unwrap()
        .iter()
        .filter(|id| id.as_str().unwrap().starts_with("new-"))
        .map(|id| {
            view["objects"][id.as_str().unwrap()]["card"]
                .as_str()
                .unwrap()
                .to_owned()
        })
        .collect()
}

#[test]
fn bp01_032_mixed_creation_and_q171_q274_capacity_choices() {
    for number in ["BP01-032", "BP01-P06"] {
        for occupied in [0_usize, 3, 4, 5] {
            let mut game = fixture(number, "field", occupied);
            let mut play = game
                .play_internal_card("source", PlayerId { seat: 1 })
                .unwrap();
            if occupied == 3 || occupied == 4 {
                let choice = question(play);
                let wanted = if occupied == 3 {
                    vec!["スティールナイト", "ナイト"]
                } else {
                    vec!["ナイト"]
                };
                assert_eq!(
                    cards(&game, "field").len(),
                    0,
                    "no object exists before selecting names"
                );
                play = capacity(&mut game, &choice, &wanted);
            }
            let result = resolved(play);
            let expected = 3.min(5 - occupied);
            assert_eq!(
                result
                    .progress
                    .receipts
                    .values()
                    .filter_map(
                        |outcome| if let InstructionOutcome::Token(value) = outcome {
                            Some(value.created.len())
                        } else {
                            None
                        }
                    )
                    .sum::<usize>(),
                expected,
                "receipt includes actual simultaneous arrivals only"
            );
            let created = cards(&game, "field");
            assert_eq!(created.len(), expected, "only available slots are filled");
            if occupied == 4 {
                assert_eq!(
                    created,
                    ["BP17-T11"],
                    "the last printed name may fill the last slot"
                );
            }
            let view = game.projection(View::Referee).unwrap();
            assert_eq!(
                view["P1"]["pp"]["current"], 5_i32,
                "the real spell costs five PP"
            );
            assert_eq!(
                view["P1"]["cemetery"],
                json!(["source"]),
                "the real spell finishes in the cemetery"
            );
            if occupied == 0 {
                assert_eq!(
                    created,
                    ["BP01-T06", "BP17-T12", "BP17-T11"],
                    "all three different real templates arrive"
                );
                assert_eq!(
                    view["objects"]
                        .as_object()
                        .unwrap()
                        .values()
                        .find(|object| object["card"] == "BP01-T06")
                        .unwrap()["keywords"],
                    json!(["疾走"]),
                    "Viking retains its complete intrinsic Storm"
                );
            }
        }
    }
}

#[test]
fn bp05_037_q697_ex_capacity_can_choose_black_artifact() {
    for number in ["BP05-037", "BP05-SL09", "BP05-U03", "BP20-PR01", "BP20-R07"] {
        for occupied in [0_usize, 4, 5] {
            let mut game = fixture(number, "ex", occupied);
            let mut play = trigger(&mut game, number);
            if occupied == 4 {
                let choice = question(play);
                play = capacity(&mut game, &choice, &["黒き破壊のアーティファクト"]);
            }
            resolved(play);
            let created = cards(&game, "ex");
            let expected = match occupied {
                0 => vec!["BP05-T01", "BP05-T02"],
                4 => vec!["BP05-T02"],
                _ => vec![],
            };
            assert_eq!(
                created, expected,
                "both artifacts share one capacity-limited creation"
            );
            assert_eq!(
                game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
                6_i32,
                "the real follower paid four PP"
            );
        }
    }
}

#[test]
fn bp07_008_chooses_tree_or_fairy_during_fanfare_resolution() {
    for number in ["BP07-008", "BP07-P02"] {
        for (index, token) in [(0_usize, "BP07-T03"), (1, "BP01-T03")] {
            let mut game = fixture(number, "ex", 0);
            let choice = question(trigger(&mut game, number));
            assert!(
                cards(&game, "ex").is_empty(),
                "no alternative exists before the choice"
            );
            let InputDomain::Modes { modes, count, .. } = &choice.domain else {
                panic!("resolution name alternatives");
            };
            assert_eq!(count.0, 1, "choose exactly one alternative");
            assert_eq!(
                game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
                7_i32,
                "three-PP play completes before the fanfare choice"
            );
            resolved(
                game.answer_internal_card(
                    choice.actor,
                    choice.token,
                    Answer::Modes(vec![modes[index]]),
                )
                .unwrap(),
            );
            assert_eq!(
                cards(&game, "ex"),
                [token],
                "the selected complete real token reaches EX"
            );
        }
    }
}
