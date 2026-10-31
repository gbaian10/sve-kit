#![expect(
    clippy::panic,
    reason = "The real mixed creation has only its capacity and Ward entry questions."
)]
use super::*;

const fn actor() -> PlayerId {
    PlayerId { seat: 1 }
}

fn position(number: &str, occupied: usize) -> Value {
    let mut state = draw_position();
    state["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":number}]);
    state["players"]["P1"]["zones"]["field"] = json!(
        (0..occupied)
            .map(|n| json!({"id":format!("old-{n}"),"card":"BP17-T11"}))
            .collect::<Vec<_>>()
    );
    state
}

fn start(game: &mut Game, number: &str) -> CardPlay {
    assert!(
        matches!(
            game.play_internal_card("source", actor()).unwrap(),
            CardPlay::Entered(_)
        ),
        "the actual two-PP follower enters before its fanfare"
    );
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("real printed fanfare");
    };
    assert_eq!(options.len(), 1, "only the source fanfare is pending");
    assert_eq!(
        options[0].card, number,
        "the actual printing supplies the effect"
    );
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::Pending(options[0].handle),
    )
    .unwrap()
}

fn finish(game: &mut Game, mut play: CardPlay, choose_ward: bool) {
    while let CardPlay::Awaiting(question) = play {
        let answer = match &question.domain {
            InputDomain::TokenCapacity { choices } => {
                let name = if choose_ward {
                    "シールドガーディアン"
                } else {
                    "ナイト"
                };
                let handle =
                    u64::try_from(choices.iter().position(|names| names == &[name]).unwrap())
                        .unwrap();
                Answer::TokenCapacity(handle)
            }
            InputDomain::WardEntry { .. } => Answer::WardEntry(true),
            InputDomain::Quick { .. }
            | InputDomain::OptionalCost
            | InputDomain::CostMaterials(_)
            | InputDomain::BeforeDraw
            | InputDomain::Pending(_)
            | InputDomain::TriggerQuota(_)
            | InputDomain::Integer(_)
            | InputDomain::Order(_)
            | InputDomain::Search(_)
            | InputDomain::SearchPartition(_)
            | InputDomain::Distribution(_)
            | InputDomain::SearchReplacement
            | InputDomain::Targets(_)
            | InputDomain::MealPayment { .. }
            | InputDomain::Modes { .. } => {
                panic!("only name capacity and Ward posture can interrupt the creation")
            }
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    resolved(play);
}

#[test]
fn bp17_029_printings_choose_the_mixed_arrival_then_always_create_steel_knight_in_ex() {
    for number in ["BP17-029", "BP17-P06"] {
        for (occupied, choose_ward) in [(0_usize, true), (3, true), (3, false), (4, true)] {
            let mut game = game(&position(number, occupied));
            let play = start(&mut game, number);
            finish(&mut game, play, choose_ward);
            let view = game.projection(View::Referee).unwrap();
            let arrivals = view["P1"]["field"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|id| id.as_str().unwrap().starts_with("new-"))
                .map(|id| &view["objects"][id.as_str().unwrap()])
                .collect::<Vec<_>>();
            assert_eq!(
                arrivals.len(),
                2.min(4 - occupied),
                "only free field slots receive the mixed tokens"
            );
            if occupied == 3 && !choose_ward {
                assert_eq!(
                    arrivals[0]["card"], "BP17-T11",
                    "the last slot may receive the later printed Knight"
                );
            }
            let ward = arrivals
                .iter()
                .filter(|card| card["card"] == "BP17-T13")
                .collect::<Vec<_>>();
            let expected_ward = usize::from(occupied < 3 || (occupied == 3 && choose_ward));
            assert_eq!(
                ward.len(),
                expected_ward,
                "Q2234 allows either name in the last slot"
            );
            for card in ward {
                assert_eq!(
                    card["acted"], true,
                    "the chosen Shield Guardian takes its real Ward entry posture"
                );
                assert!(
                    card["keywords"]
                        .as_array()
                        .unwrap()
                        .contains(&json!("守護")),
                    "the complete Shield Guardian keeps Ward"
                );
            }
            let ex = view["P1"]["ex"].as_array().unwrap();
            assert_eq!(
                ex.len(),
                1,
                "the later EX creation is independent of field capacity"
            );
            assert_eq!(
                view["objects"][ex[0].as_str().unwrap()]["card"],
                "BP17-T12",
                "the later instruction creates the real Steel Knight"
            );
            assert_eq!(
                view["P1"]["pp"]["current"], 8_u8,
                "the source pays its printed two PP"
            );
            assert_eq!(
                view["P2"]["field"],
                json!([]),
                "the opposing field receives no tokens"
            );
        }
    }
}
