// The shared real repository fixture dominates cold runs; each printing uses the same selection.
#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate the printed extra-turn scenario."
)]
use super::event_cemetery::finish_checks;
use super::*;
use core::iter::once;

fn position(number: &str, spells: u8) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    initial["players"]["P1"]["zones"]["hand"] =
        json!([{"id":"source","card":number},{"id":"second","card":number}]);
    initial["players"]["P2"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["cemetery"] = json!(
        (0..spells)
            .map(|n| json!({"id":format!("m-{n}"),"card":"BP01-179"}))
            .chain(once(json!({"id":"wrong-kind","card":"BP01-037"})))
            .collect::<Vec<_>>()
    );
    initial["players"]["P1"]["zones"]["deck"] = json!(
        (0..5_u8)
            .map(|n| json!({"id":format!("d-{n}"),"card":"BP01-171"}))
            .collect::<Vec<_>>()
    );
    initial
}

fn actual(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "printed-extra-turn",
    )
    .unwrap()
}

#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "The printed payment admits only its optional choice and material group."
)]
fn cast(board: &mut Game, source: &str) {
    let mut play = board
        .play_internal_card(source, PlayerId { seat: 1 })
        .unwrap();
    let mut paid = false;
    while let CardPlay::Awaiting(question) = play {
        let answer = match &question.domain {
            InputDomain::OptionalCost => Answer::OptionalCost(true),
            InputDomain::CostMaterials(domain) => {
                assert!(
                    domain.members.len() >= 10,
                    "payment must offer ten cemetery spells"
                );
                assert!(
                    domain
                        .members
                        .iter()
                        .all(|member| member.card.as_deref() != Some("BP01-037")),
                    "followers must stay outside the spell material domain"
                );
                paid = true;
                Answer::CostMaterials(domain.members.iter().take(10).map(|m| m.handle).collect())
            }
            _ => panic!("printed optional banish payment"),
        };
        play = board
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    assert!(paid, "the discount requires ten actual cemetery spells");
    resolved(play);
    finish_checks(board);
}

fn finish_turn(board: &mut Game) {
    board
        .internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    finish_checks(board);
    let quick = board.internal_quick_choice().unwrap().unwrap();
    assert!(
        matches!(
            board
                .answer_internal_card(quick.actor, quick.token, Answer::Quick(None))
                .unwrap(),
            CardPlay::Continued(_)
        ),
        "Quick Pass resumes the end phase"
    );
    finish_checks(board);
}

#[test]
fn dimensionshift_all_printings_pay_ten_cemetery_spells_and_chain_real_extra_turns() {
    for number in ["BP01-057", "BP01-P11", "EBD02-007", "PR-029", "PR-301"] {
        let mut insufficient = actual(&position(number, 9));
        let before = insufficient.projection(View::Referee).unwrap();
        let CardPlay::Awaiting(optional) = insufficient
            .play_internal_card("source", PlayerId { seat: 1 })
            .unwrap()
        else {
            panic!("optional exchange choice")
        };
        insufficient
            .answer_internal_card(optional.actor, optional.token, Answer::OptionalCost(true))
            .unwrap_err();
        assert_eq!(
            insufficient.projection(View::Referee).unwrap()["P1"],
            before["P1"],
            "Q192 excludes the spell being played from cemetery materials"
        );
        let mut board = actual(&position(number, 20));
        cast(&mut board, "source");
        let first = board.projection(View::Referee).unwrap();
        assert_eq!(first["P1"]["pp"]["current"], 3_u8);
        assert_eq!(first["P1"]["banish"].as_array().unwrap().len(), 10);
        assert!(
            first["P1"]["cemetery"]
                .as_array()
                .unwrap()
                .contains(&json!("source"))
        );
        assert_eq!(first["turn"]["active"], "P1");
        finish_turn(&mut board);
        assert_eq!(
            board.projection(View::Referee).unwrap()["turn"]["active"],
            "P1"
        );
        cast(&mut board, "second");
        let second = board.projection(View::Referee).unwrap();
        assert_eq!(second["P1"]["pp"]["current"], 3_u8);
        assert_eq!(second["P1"]["banish"].as_array().unwrap().len(), 20);
        assert!(
            second["P1"]["cemetery"]
                .as_array()
                .unwrap()
                .contains(&json!("second"))
        );
        finish_turn(&mut board);
        assert_eq!(
            board.projection(View::Referee).unwrap()["turn"]["active"],
            "P1",
            "Q193 permits another extra turn inside an extra turn"
        );
        finish_turn(&mut board);
        assert_eq!(
            board.projection(View::Referee).unwrap()["turn"]["active"],
            "P2",
            "the suspended ordinary turn resumes after both extras"
        );
    }
}
