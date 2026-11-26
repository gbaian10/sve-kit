use super::event_cemetery::finish_checks;
use super::*;
use sve_engine::game::internal::DecisionView;

fn trigger(board: &mut Game, number: &str, line: u64) -> CardPlay {
    finish_checks(board);
    let question = board.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("pending ability")
    };
    let option = options
        .iter()
        .find(|option| option.card == number && option.line == line)
        .unwrap();
    board
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(option.handle),
        )
        .unwrap()
}

fn targets(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(question) = play else {
        panic!("formal targets")
    };
    assert!(
        matches!(question.domain, InputDomain::Targets(_)),
        "the printed groups are chosen before resolution"
    );
    question
}

fn select_all(board: &mut Game, question: &DecisionView, count: usize) -> CardPlay {
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("formal targets")
    };
    assert_eq!(
        domain.members.len(),
        count,
        "only matching public members enter the domain"
    );
    let handles = domain.members.iter().map(|member| member.handle).collect();
    board
        .answer_internal_card(question.actor, question.token, Answer::Targets(handles))
        .unwrap()
}

fn activation(number: &str) -> AbilityId {
    selection().current[number]
        .program
        .card()
        .abilities
        .iter()
        .find(|ability| ability.node.kind == AbilityKind::Activated)
        .unwrap()
        .node
        .id
}

#[test]
fn bp19_080_all_prints_revive_two_distinct_followers_and_observe_each_cemetery_entry() {
    for number in ["BP19-080", "BP19-P21"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":number}]);
        initial["players"]["P1"]["zones"]["cemetery"] = json!([
            {"id":"first","card":"BP19-082"},{"id":"second","card":"BP19-082"},
            {"id":"wrong_trait","card":"BP01-037"}
        ]);
        initial["players"]["P2"]["zones"]["field"] = json!([
            {"id":"enemy","card":"BP01-171","state":{"hp":20_u8,"max_hp":20_u8}}
        ]);
        let mut board = game(&initial);
        let question = targets(
            board
                .activate_internal_card("source", activation(number), PlayerId { seat: 1 })
                .unwrap(),
        );
        resolved(select_all(&mut board, &question, 2));
        resolved(trigger(&mut board, number, 1));
        resolved(trigger(&mut board, number, 1));
        finish_checks(&mut board);
        assert!(board.internal_pending_choice().unwrap().is_none());
        let state = board.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["cemetery"], json!(["wrong_trait"]));
        assert_eq!(state["P1"]["field"].as_array().unwrap().len(), 3);
        assert_eq!(state["objects"]["source"]["acted"], true);
        assert_eq!(state["objects"]["enemy"]["damage"], 2_u8);
    }
}

#[test]
fn ecp01_039_all_prints_revive_both_cost_groups_acted() {
    for number in ["ECP01-039", "ECP01-SL19", "ECP01-SP19", "ECP01-SSP19"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":number}]);
        initial["players"]["P1"]["zones"]["cemetery"] = json!([
            {"id":"large","card":"CP01-022"},{"id":"small","card":"CP01-023"},
            {"id":"wrong_trait","card":"BP01-037"}
        ]);
        let mut board = game(&initial);
        assert!(matches!(
            board
                .play_internal_card("source", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ));
        let question = targets(trigger(&mut board, number, 3));
        resolved(select_all(&mut board, &question, 2));
        let state = board.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["large"]["acted"], true);
        assert_eq!(state["objects"]["small"]["acted"], true);
        assert_eq!(state["P1"]["cemetery"], json!(["wrong_trait"]));
        assert_eq!(state["P1"]["field"].as_array().unwrap().len(), 3);
        assert_eq!(state["P1"]["pp"]["current"], 4_u8);
    }
}
