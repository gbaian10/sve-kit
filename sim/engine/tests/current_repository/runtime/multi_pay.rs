#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected questions invalidate these payment scenarios."
)]
use super::*;

fn run(number: &str, hand: Value) -> (Game, Value) {
    let mut initial = lesson::position();
    initial["players"]["P1"]["zones"]["hand"] = hand;
    initial["players"]["P1"]["zones"]["hand"]
        .as_array_mut()
        .unwrap()
        .insert(0, json!({"id":"source","card":number}));
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-037","state":{"hp":10_u8,"max_hp":10_u8}}]);
    let mut game = lesson::game(&initial);
    assert!(
        matches!(
            game.play_internal_card("source", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ),
        "the real follower enters before its Fanfare"
    );
    let before = game.projection(View::Referee).unwrap();
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("Fanfare")
    };
    let handle = options
        .iter()
        .find(|option| option.card == number)
        .unwrap()
        .handle;
    let mut play = game
        .answer_internal_card(pending.actor, pending.token, Answer::Pending(handle))
        .unwrap();
    while let CardPlay::Awaiting(question) = play {
        let answer = match &question.domain {
            InputDomain::OptionalCost => Answer::OptionalCost(true),
            InputDomain::Targets(_) => Answer::Targets(vec![0]),
            InputDomain::CostMaterials(domain) => {
                let view = game.projection(View::Referee).unwrap();
                for key in ["P1", "P2", "objects"] {
                    assert_eq!(view[key], before[key], "all selections precede every cost");
                }
                Answer::CostMaterials(
                    domain
                        .members
                        .iter()
                        .take(usize::try_from(domain.cardinality.minimum.0).unwrap())
                        .map(|member| member.handle)
                        .collect(),
                )
            }
            _ => panic!("unexpected payment question"),
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    resolved(play);
    (game, before)
}

#[test]
fn csd02a_005_pays_pp_then_lesson_before_buffing_all_printings() {
    for number in ["CSD02a-005", "CSD02a-P02"] {
        let (game, before) = run(number, json!([]));
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(
            view["P1"]["pp"]["current"],
            before["P1"]["pp"]["current"].as_i64().unwrap() - 2
        );
        assert_eq!(view["P1"]["ex"], json!(["second", "wrong"]));
        assert_eq!(
            view["objects"]["source"]["power"],
            before["objects"]["source"]["power"].as_i64().unwrap() + 1
        );
        assert_eq!(
            view["objects"]["source"]["hp"],
            before["objects"]["source"]["hp"].as_i64().unwrap() + 1
        );
    }
}

#[test]
fn ecp02_017_pays_pp_lesson_and_filtered_hand_card_before_damage_all_printings() {
    for number in ["ECP02-017", "ECP02-SL11"] {
        let (game, before) = run(
            number,
            json!([{"id":"payment","card":"CSD02c-004"},{"id":"wrong-hand","card":"BP01-037"}]),
        );
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(
            view["P1"]["pp"]["current"],
            before["P1"]["pp"]["current"].as_i64().unwrap() - 1
        );
        assert_eq!(view["P1"]["ex"], json!(["second", "wrong"]));
        assert_eq!(view["P1"]["cemetery"], json!(["payment"]));
        assert_eq!(view["P1"]["hand"], json!(["wrong-hand"]));
        assert_eq!(view["objects"]["enemy"]["damage"], 3_u8);
    }
}

#[test]
fn bp14_116_pays_pp_and_goblin_hand_card_before_damage_and_buff_all_printings() {
    for number in ["BP14-116", "BP14-P26"] {
        let (game, before) = run(
            number,
            json!([{"id":"payment","card":"BP01-171"},{"id":"wrong-hand","card":"BP01-037"}]),
        );
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(
            view["P1"]["pp"]["current"],
            before["P1"]["pp"]["current"].as_i64().unwrap() - 2
        );
        assert_eq!(view["P1"]["cemetery"], json!(["payment"]));
        assert_eq!(view["P1"]["hand"], json!(["wrong-hand"]));
        assert_eq!(view["objects"]["enemy"]["damage"], 4_u8);
        assert_eq!(
            view["objects"]["source"]["power"],
            before["objects"]["source"]["power"].as_i64().unwrap() + 2
        );
        assert_eq!(
            view["objects"]["source"]["hp"],
            before["objects"]["source"]["hp"].as_i64().unwrap() + 2
        );
    }
}
