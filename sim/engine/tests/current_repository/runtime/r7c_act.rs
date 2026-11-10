#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "The complete printed material procedures must expose their declared choices."
)]
use super::*;
use sve_engine::game::internal::DecisionView;

const PLAYER: PlayerId = PlayerId { seat: 1 };

fn initial(number: &str, guide: bool, count: usize, pp: u8) -> Value {
    let mut position = draw_position();
    position["players"]["P1"]["pp"] = json!({"current":pp,"max":2_u8});
    position["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":number}]);
    position["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"drawn","card":"BP01-171"},{"id":"tail","card":"BP01-171"}
    ]);
    let cards = if guide {
        ["BP21-115", "BP21-100"]
    } else {
        ["BP07-T03", "BP17-T19"]
    };
    let mut field = cards
        .into_iter()
        .take(count)
        .enumerate()
        .map(|(n, card)| json!({"id":format!("fee{n}"),"card":card}))
        .collect::<Vec<_>>();
    field.push(
        json!({"id":"spent","card":if guide {"BP13-042"} else {"BP12-T12"},"state":{"acted":true}}),
    );
    field.push(json!({"id":"decoy","card":if guide {"BP01-171"} else {"BP01-T10"}}));
    if guide {
        field.push(json!({"id":"amulet","card":"BP21-114"}));
    }
    position["players"]["P1"]["zones"]["field"] = json!(field);
    position["players"]["P2"]["zones"]["field"] = json!([{"id":"foreign","card":cards[0]}]);
    position
}

fn materials(board: &mut Game, q: &DecisionView) -> Vec<u64> {
    let InputDomain::CostMaterials(domain) = &q.domain else {
        panic!("optional act materials")
    };
    assert_eq!(
        domain.members.len(),
        2,
        "only the two own standing matching cards can pay"
    );
    let handles = domain
        .members
        .iter()
        .map(|member| member.handle)
        .collect::<Vec<_>>();
    for invalid in [
        vec![handles[0]],
        vec![handles[0], handles[0]],
        vec![handles[0], 999_u64],
    ] {
        let before = format!("{board:?}");
        board
            .answer_internal_card(q.actor, q.token, Answer::CostMaterials(invalid))
            .unwrap_err();
        assert_eq!(
            format!("{board:?}"),
            before,
            "invalid materials preserve the question and all costs"
        );
    }
    handles
}

fn answer(
    board: &mut Game,
    q: &DecisionView,
    count: usize,
    accept: bool,
    payments: &mut usize,
) -> Answer {
    match &q.domain {
        InputDomain::Targets(domain) => {
            let target = domain
                .members
                .iter()
                .find(|m| {
                    m.card.as_deref() == Some(if count > 0 { "BP21-115" } else { "BP01-171" })
                })
                .unwrap();
            Answer::Targets(vec![target.handle])
        }
        InputDomain::OptionalCost => Answer::OptionalCost(accept),
        InputDomain::CostMaterials(_) => {
            *payments = payments.checked_add(1).unwrap();
            Answer::CostMaterials(materials(board, q))
        }
        other => panic!("unexpected printed discount choice {other:?}"),
    }
}

fn finish(
    board: &mut Game,
    mut play: CardPlay,
    count: usize,
    accept: bool,
) -> Result<usize, EngineFailure> {
    let mut payments = 0_usize;
    while let CardPlay::Awaiting(q) = play {
        let answer = answer(board, &q, count, accept, &mut payments);
        play = board.answer_internal_card(q.actor, q.token, answer)?;
    }
    resolved(play);
    Ok(payments)
}

fn insufficient_materials(position: &Value, count: usize) {
    let mut probe = game(position);
    let declared = probe.play_internal_card("source", PLAYER).unwrap();
    let error = finish(&mut probe, declared, count, true).unwrap_err();
    assert!(
        matches!(error, EngineFailure::IllegalPlay(_)),
        "insufficient materials are rejected after declaration and formal targets"
    );
    let rejected = probe.projection(View::Referee).unwrap();
    assert_eq!(
        rejected["P1"]["pp"]["current"], 2_u8,
        "an incomplete cost spends no PP"
    );
    assert_eq!(
        rejected["objects"]["fee0"]["acted"], false,
        "an incomplete cost does not act its one candidate"
    );
    assert_eq!(
        rejected["P1"]["deck"],
        json!(["drawn", "tail"]),
        "failed payment does not execute the draw"
    );
}

#[test]
fn guide_and_embrace_all_printings_act_exactly_two_matching_cards_or_pay_two_pp() {
    for (number, guide) in [
        ("BP21-052", true),
        ("BP21-P26", true),
        ("BP17-007", false),
        ("PR-460", false),
    ] {
        for (count, accept) in [(2_usize, true), (2, false), (1, true), (0, false)] {
            let paid = accept && count == 2;
            let position = initial(number, guide, count, if paid { 0 } else { 2 });
            if accept && count < 2 {
                insufficient_materials(&position, count);
            }
            let mut board = game(&position);
            let before = board.projection(View::Referee).unwrap();
            let play = board.play_internal_card("source", PLAYER).unwrap();
            let payments = finish(&mut board, play, count, paid).unwrap();
            assert_eq!(
                payments,
                usize::from(paid),
                "only an accepted complete optional cost acts materials"
            );
            let state = board.projection(View::Referee).unwrap();
            assert_eq!(state["P1"]["pp"]["current"], 0_u8);
            assert_eq!(state["P1"]["hand"], json!(["drawn"]));
            assert_eq!(state["P1"]["deck"], json!(["tail"]));
            assert_eq!(state["P1"]["cemetery"], json!(["source"]));
            for n in 0..count {
                assert_eq!(state["objects"][format!("fee{n}")]["acted"], paid);
            }
            assert_eq!(state["objects"]["spent"]["acted"], true);
            assert_eq!(state["objects"]["decoy"]["acted"], false);
            assert_eq!(state["objects"]["foreign"]["acted"], false);
            if guide {
                let target = if count > 0 { "fee0" } else { "decoy" };
                for stat in ["power", "hp"] {
                    assert_eq!(
                        state["objects"][target][stat].as_i64().unwrap(),
                        before["objects"][target][stat]
                            .as_i64()
                            .unwrap()
                            .checked_add(1)
                            .unwrap(),
                        "the formal follower target gains exactly one in both stats"
                    );
                }
                assert_eq!(state["objects"]["amulet"]["acted"], false);
                assert_eq!(state["P1"]["leader"]["life"], 20_u8);
            } else {
                assert_eq!(state["P1"]["leader"]["life"], 21_u8);
            }
        }
    }
}

#[test]
fn guide_all_printings_require_a_formal_own_follower_before_drawing() {
    for number in ["BP21-052", "BP21-P26"] {
        let mut position = initial(number, true, 0, 2);
        position["players"]["P1"]["zones"]["field"] = json!([{"id":"amulet","card":"BP21-114"}]);
        let mut board = game(&position);
        let before = board.projection(View::Referee).unwrap();
        let declared = board.play_internal_card("source", PLAYER).unwrap();
        let error = finish(&mut board, declared, 0, false).unwrap_err();
        assert!(
            matches!(error, EngineFailure::IllegalPlay(_)),
            "Q2734 requires a formal own follower after the exchange declaration"
        );
        assert_eq!(
            board.projection(View::Referee).unwrap(),
            before,
            "a failed formal target commits no PP, buff or draw"
        );
    }
}
