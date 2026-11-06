#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate these game scenarios."
)]
use super::*;

use sve_engine::game::internal::SearchAnswer;

const fn actor() -> PlayerId {
    PlayerId { seat: 1 }
}

fn position(number: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":number}]);
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"d1","card":"BP01-171"}, {"id":"d2","card":"BP01-171"},
        {"id":"d3","card":"BP01-171"}, {"id":"d4","card":"BP01-171"}
    ]);
    initial
}

fn finish(board: &mut Game, mut step: CardPlay, mode: u64) {
    while let CardPlay::Awaiting(question) = step {
        let answer = match &question.domain {
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            InputDomain::OptionalCost => Answer::OptionalCost(true),
            InputDomain::Modes { .. } => Answer::Modes(vec![ModeId(mode)]),
            InputDomain::Targets(domain) => {
                Answer::Targets(domain.members.iter().take(1).map(|m| m.handle).collect())
            }
            InputDomain::Search(domain) => Answer::Search(SearchAnswer::Ungrouped(
                domain.cards.iter().take(1).map(|m| m.handle).collect(),
            )),
            InputDomain::Order(cards) => {
                Answer::Order(cards.iter().rev().map(|m| m.handle).collect())
            }
            InputDomain::CostMaterials(domain) => {
                assert!(
                    domain.members.iter().all(|m| m.player == question.actor),
                    "opponents cannot pay the acting player's material cost"
                );
                Answer::CostMaterials(
                    domain
                        .members
                        .iter()
                        .take(usize::try_from(domain.cardinality.minimum.0).unwrap())
                        .map(|m| m.handle)
                        .collect(),
                )
            }
            _ => panic!("unexpected decision domain"),
        };
        step = board
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
}

fn settle(board: &mut Game, mode: u64) {
    loop {
        if board.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            board.resume_internal_card().unwrap();
        }
        let Some(question) = board.internal_pending_choice().unwrap() else {
            break;
        };
        let InputDomain::Pending(options) = &question.domain else {
            panic!("pending ability expected")
        };
        let step = board
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap();
        finish(board, step, mode);
    }
}

fn play(board: &mut Game, mode: u64) {
    let step = board.play_internal_card("source", actor()).unwrap();
    finish(board, step, mode);
    settle(board, mode);
}

fn activated(number: &str, line: u32) -> AbilityId {
    selection().current[number]
        .program
        .card()
        .abilities
        .iter()
        .find(|a| a.node.kind == AbilityKind::Activated && a.source.line == line)
        .unwrap()
        .node
        .id
}

#[test]
fn bp05_079_discards_from_the_destroyed_followers_controller() {
    let mut initial = position("BP05-079");
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-037","owner":"P1"}]);
    initial["players"]["P2"]["zones"]["hand"] = json!([{"id":"hidden","card":"BP01-171"}]);
    let mut board = game(&initial);
    play(&mut board, 1);
    let state = board.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["cemetery"], json!(["enemy", "source"]));
    assert_eq!(state["P2"]["hand"], json!([]));
    assert_eq!(state["P2"]["cemetery"], json!(["hidden"]));
}

#[test]
fn bp02_109_damages_its_own_leader_and_discards_a_random_hand_card() {
    let mut initial = position("BP02-109");
    initial["players"]["P1"]["zones"]["hand"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"discarded","card":"BP01-171"}));
    let mut board = game(&initial);
    play(&mut board, 1);
    let state = board.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["leader"]["life"], 17_u8);
    assert_eq!(state["P1"]["hand"], json!([]));
    assert_eq!(state["P1"]["cemetery"], json!(["discarded"]));
    assert_eq!(state["P1"]["field"], json!(["source"]));
}

#[test]
fn bp03_039_sacrifices_another_chess_and_locks_further_activations() {
    let mut initial = position("BP03-039");
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"source","card":"BP03-039"},{"id":"material","card":"BP03-039"},
        {"id":"spare","card":"BP03-039"}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"enemy","card":"BP01-037"},{"id":"other","card":"BP01-037"},
        {"id":"opponent-chess","card":"BP03-039"}
    ]);
    let mut board = game(&initial);
    let definition = activated("BP03-039", 2);
    let step = board
        .activate_internal_card("source", definition, actor())
        .unwrap();
    finish(&mut board, step, 1);
    settle(&mut board, 1);
    let state = board.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["cemetery"], json!(["material"]));
    assert_eq!(state["P2"]["cemetery"], json!(["enemy"]));
    assert!(
        state["P2"]["field"]
            .as_array()
            .unwrap()
            .contains(&json!("opponent-chess"))
    );
    board
        .activate_internal_card("source", definition, actor())
        .unwrap_err();
    assert_eq!(
        board.projection(View::Referee).unwrap()["P1"]["field"],
        state["P1"]["field"]
    );
}

#[test]
fn bp06_108_all_three_modes_resolve() {
    for mode in 1..=3 {
        let mut initial = position("BP06-108");
        if mode == 1 {
            initial["players"]["P1"]["zones"]["field"] = json!([{"id":"ally","card":"BP01-037"}]);
        }
        let mut board = game(&initial);
        play(&mut board, mode);
        let state = board.projection(View::Referee).unwrap();
        match mode {
            1 => {
                assert_eq!(state["objects"]["ally"]["power"], 6_u8);
                assert_eq!(state["objects"]["ally"]["hp"], 6_u8);
            }
            2 => assert_eq!(state["P1"]["leader"]["life"], 22_u8),
            _ => assert_eq!(state["P1"]["deck"], json!(["d1", "d4", "d3", "d2"])),
        }
    }
}

#[test]
fn bp21_004_searches_a_fairy_and_replaces_an_enemy_with_the_opponents_token() {
    let mut initial = position("BP21-004");
    initial["players"]["P1"]["zones"]["ex"] = json!(
        (1_u8..=5)
            .map(|i| json!({"id":format!("fairy{i}"),"card":"BP03-T01"}))
            .collect::<Vec<_>>()
    );
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    initial["players"]["P1"]["zones"]["deck"][0] = json!({"id":"found","card":"BP01-014"});
    let mut board = game(&initial);
    play(&mut board, 1);
    assert_eq!(
        board.projection(View::Referee).unwrap()["P1"]["field"],
        json!(["source", "found"])
    );
    let step = board
        .activate_internal_card("source", activated("BP21-004", 2), actor())
        .unwrap();
    finish(&mut board, step, 1);
    settle(&mut board, 1);
    let state = board.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["cemetery"], json!(["source"]));
    assert_eq!(state["P2"]["cemetery"], json!(["enemy"]));
    let opponents = state["P2"]["field"].as_array().unwrap();
    assert_eq!(opponents.len(), 1);
    assert_eq!(
        state["objects"][opponents[0].as_str().unwrap()]["name"],
        "フェアリー"
    );
}
