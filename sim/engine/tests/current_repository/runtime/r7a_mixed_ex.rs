#![expect(
    clippy::panic,
    reason = "Printed capacity decisions must have the expected domain."
)]
use super::event_cemetery::finish_checks;
use super::*;
use sve_engine::game::internal::DecisionView;

fn departing(form: &str, owner: &str, spaces: [usize; 2]) -> Game {
    let mut initial = draw_position();
    initial["turn"]["active"] = json!("P2");
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"victim","card":"BP13-071","owner":owner}
    ]);
    initial["players"]["P2"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"killer","card":"BP03-004","state":{"counters":{"童話カウンター":1_u8}}}
    ]);
    for (index, seat) in ["P1", "P2"].iter().enumerate() {
        initial["players"][seat]["zones"]["ex"] = json!(
            (0..5_usize.saturating_sub(spaces[index]))
                .map(|index| json!({"id":format!("{seat}-filler-{index}"),"card":"BP01-037"}))
                .collect::<Vec<_>>()
        );
        initial["players"][seat]["zones"]["deck"] = json!([
            {"id":format!("{seat}-draw"),"card":"BP01-037"},
            {"id":format!("{seat}-reserve"),"card":"BP01-037"}
        ]);
    }
    let mut board = game(&initial);
    assert_eq!(
        board
            .internal_rule_action(
                &json!({"do":"evolve","source":"victim","evolve_card":"form","pay":{"pp":1_u8}})
            )
            .unwrap()
            .outcome,
        "resolved",
        "the actual base evolves with the controller's physical form"
    );
    finish_checks(&mut board);
    board
        .internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    let quick = board.internal_quick_choice().unwrap().unwrap();
    board
        .answer_internal_card(quick.actor, quick.token, Answer::Quick(None))
        .unwrap();
    finish_checks(&mut board);
    let ability = selection().current["BP03-004"]
        .program
        .card()
        .abilities
        .iter()
        .find(|ability| ability.source.line == 2)
        .unwrap()
        .node
        .id;
    let play = board
        .activate_internal_card("killer", ability, PlayerId { seat: 1 })
        .unwrap();
    let CardPlay::Awaiting(target) = play else {
        panic!("Lila's formal target");
    };
    let InputDomain::Targets(domain) = &target.domain else {
        panic!("Lila's target domain");
    };
    resolved(
        board
            .answer_internal_card(
                target.actor,
                target.token,
                Answer::Targets(vec![domain.members[0].handle]),
            )
            .unwrap(),
    );
    finish_checks(&mut board);
    assert_eq!(
        board.projection(View::Referee).unwrap()[owner]["cemetery"],
        json!(["victim"]),
        "destruction preserves the base's owner and departed generation"
    );
    board
}

fn lastword(board: &mut Game, form: &str) -> CardPlay {
    let question = board.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("Lastword");
    };
    let option = options.iter().find(|option| option.card == form).unwrap();
    board
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(option.handle),
        )
        .unwrap()
}

fn capacity(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(question) = play else {
        panic!("shared EX allocation");
    };
    question
}

fn choose_arrival(board: &mut Game, play: CardPlay, choose_body: bool) {
    let question = capacity(play);
    let InputDomain::SearchPartition(domain) = &question.domain else {
        panic!("joint capacity");
    };
    assert_eq!(
        domain.cards.len(),
        2,
        "one physical base and one not-yet-created token"
    );
    assert_eq!(
        domain.cardinality,
        Cardinality {
            minimum: Nat(1),
            maximum: Some(Nat(1))
        },
        "exactly the last space is filled"
    );
    let body = domain
        .cards
        .iter()
        .find(|card| card.object.as_deref() == Some("victim"))
        .unwrap();
    let token = domain
        .cards
        .iter()
        .find(|card| card.object.is_none())
        .unwrap();
    assert_eq!(
        token.name, "紅の牙",
        "the pending token has printed information but no physical ID"
    );
    let before = format!("{board:?}");
    for (actor, answer) in [
        (PlayerId { seat: 1 }, vec![body.handle]),
        (question.actor, vec![body.handle, body.handle]),
        (question.actor, vec![u64::MAX]),
        (question.actor, vec![]),
    ] {
        board
            .answer_internal_card(actor, question.token, Answer::SearchPartition(answer))
            .unwrap_err();
        assert_eq!(
            format!("{board:?}"),
            before,
            "invalid allocations preserve the complete authority"
        );
    }
    resolved(
        board
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::SearchPartition(vec![if choose_body {
                    body.handle
                } else {
                    token.handle
                }]),
            )
            .unwrap(),
    );
}

#[test]
fn r7a_mixed_ex_alzard_all_printings_share_capacity_with_the_new_token_q1776() {
    for form in ["BP13-072", "BP13-SL16"] {
        for (spaces, choose_body) in [(5, true), (2, true), (1, true), (1, false), (0, false)] {
            let mut board = departing(form, "P2", [5, spaces]);
            let play = lastword(&mut board, form);
            if spaces == 1 {
                choose_arrival(&mut board, play, choose_body);
            } else {
                resolved(play);
            }
            finish_checks(&mut board);
            let after = board.projection(View::Referee).unwrap();
            let moved = spaces >= 2 || (spaces == 1 && choose_body);
            let created = spaces >= 2 || (spaces == 1 && !choose_body);
            assert_eq!(
                after["P2"]["cemetery"],
                if moved { json!([]) } else { json!(["victim"]) },
                "choosing only the token leaves the actual base in its graveyard"
            );
            assert_eq!(
                after["objects"]["victim"]["generation"],
                if moved { 2_u8 } else { 1 },
                "the original base moves at most once"
            );
            assert_eq!(
                after["objects"]["victim"]["counters"]["休眠カウンター"]
                    .as_u64()
                    .unwrap_or(0),
                if moved { 2_u64 } else { 0 },
                "only the successful physical move receives dormancy"
            );
            assert_eq!(
                after["objects"]["victim"]["card"], "BP13-071",
                "the base is never cloned from its evolve form"
            );
            let tokens = after["P2"]["ex"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|id| after["objects"][id.as_str().unwrap()]["card"] == "BP13-P31")
                .count();
            assert_eq!(
                tokens,
                usize::from(created),
                "token creation obeys the same space allocation"
            );
            assert_eq!(
                after["P2"]["evolve_deck"],
                json!(["form"]),
                "the real evolution form returns independently"
            );
        }
    }
}

#[test]
fn r7a_mixed_ex_stolen_alzard_separates_owner_and_controller_destinations() {
    for (owner_spaces, controller_spaces) in [(1, 1), (0, 1), (1, 0), (0, 0)] {
        let mut board = departing("BP13-072", "P1", [owner_spaces, controller_spaces]);
        let play = lastword(&mut board, "BP13-072");
        resolved(play);
        finish_checks(&mut board);
        let after = board.projection(View::Referee).unwrap();
        assert_eq!(
            after["P1"]["ex"]
                .as_array()
                .unwrap()
                .iter()
                .any(|id| id == "victim"),
            owner_spaces == 1,
            "the physical base returns to its owner EX, with that zone's independent capacity"
        );
        assert_eq!(
            after["objects"]["victim"]["counters"]["休眠カウンター"]
                .as_u64()
                .unwrap_or(0),
            if owner_spaces == 1 { 2 } else { 0 },
            "only actual owner EX entry gets counters"
        );
        let tokens = after["P2"]["ex"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|id| after["objects"][id.as_str().unwrap()]["card"] == "BP13-P31")
            .count();
        assert_eq!(
            tokens, controller_spaces,
            "the new token belongs to the Lastword controller"
        );
    }
}
