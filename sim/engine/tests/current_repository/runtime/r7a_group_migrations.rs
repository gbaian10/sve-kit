#![expect(
    clippy::panic,
    reason = "Unexpected choices invalidate these grouped target scenarios."
)]
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
fn bp19_080_hand_entry_pays_only_a_matching_discard_and_deals_no_cemetery_damage() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"source","card":"BP19-080"},{"id":"material","card":"BP19-082"},
        {"id":"wrong_trait","card":"BP01-037"}
    ]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"drawn","card":"BP01-034"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-171"}]);
    let mut board = game(&initial);
    assert!(matches!(
        board
            .play_internal_card("source", PlayerId { seat: 1 })
            .unwrap(),
        CardPlay::Entered(_)
    ));
    let CardPlay::Awaiting(optional) = trigger(&mut board, "BP19-080", 2) else {
        panic!("optional head payment")
    };
    assert_eq!(optional.domain, InputDomain::OptionalCost);
    let CardPlay::Awaiting(materials) = board
        .answer_internal_card(optional.actor, optional.token, Answer::OptionalCost(true))
        .unwrap()
    else {
        panic!("private hand material")
    };
    let InputDomain::CostMaterials(domain) = &materials.domain else {
        panic!("head discard domain")
    };
    assert_eq!(
        domain.members.len(),
        1,
        "the unrelated hand card cannot pay the printed trait cost"
    );
    assert_eq!(domain.members[0].card.as_deref(), Some("BP19-082"));
    assert!(board.internal_decision(View::P2).is_none());
    resolved(
        board
            .answer_internal_card(
                materials.actor,
                materials.token,
                Answer::CostMaterials(vec![domain.members[0].handle]),
            )
            .unwrap(),
    );
    // Entering from hand still triggers line 1, but its saved origin is not the cemetery.
    resolved(trigger(&mut board, "BP19-080", 1));
    let state = board.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["wrong_trait", "drawn"]));
    assert_eq!(state["P1"]["cemetery"], json!(["material"]));
    assert_eq!(state["objects"]["enemy"]["damage"], 0_u8);
}

#[test]
fn cp03_042_all_prints_fill_three_disjoint_groups_from_public_banish() {
    for number in ["CP03-042", "CP03-SL08", "CP03-U03"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":number}]);
        initial["players"]["P1"]["zones"]["banish"] = json!([
            {"id":"first","card":"CP03-050"},{"id":"second","card":"CP03-050"},
            {"id":"small","card":"CP03-052"},{"id":"hidden","card":"CP03-052","face_up":false},
            {"id":"wrong_trait","card":"BP01-037"}
        ]);
        let mut board = game(&initial);
        assert!(matches!(
            board
                .play_internal_card("source", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ));
        let question = targets(trigger(&mut board, number, 2));
        resolved(select_all(&mut board, &question, 3));
        let state = board.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["field"].as_array().unwrap().len(), 4);
        assert_eq!(state["P1"]["banish"], json!(["hidden", "wrong_trait"]));
        assert_eq!(state["P1"]["pp"]["current"], 3_u8);
    }
}

#[test]
fn csd02a_001_all_prints_buff_only_the_successful_grouped_arrivals_then_pay_lesson_once() {
    for number in ["CSD02a-001", "CSD02a-SL01"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":number}]);
        initial["players"]["P1"]["zones"]["cemetery"] = json!([
            {"id":"large","card":"CP02-013"},{"id":"small","card":"CP02-014"}
        ]);
        initial["players"]["P1"]["zones"]["ex"] = json!([
            {"id":"lesson","card":"PR-155"},{"id":"spare","card":"PR-156"}
        ]);
        let mut board = lesson::game(&initial);
        assert!(matches!(
            board
                .play_internal_card("source", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ));
        let question = targets(trigger(&mut board, number, 1));
        let mut play = select_all(&mut board, &question, 2);
        while let CardPlay::Awaiting(ward) = play {
            assert!(
                matches!(ward.domain, InputDomain::WardEntry { .. }),
                "the revived cost-4 follower offers its Ward posture"
            );
            play = board
                .answer_internal_card(ward.actor, ward.token, Answer::WardEntry(false))
                .unwrap();
        }
        resolved(play);
        finish_checks(&mut board);
        let state = board.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["large"]["power"], 4_u8);
        assert_eq!(state["objects"]["large"]["hp"], 6_u8);
        assert_eq!(state["objects"]["small"]["power"], 3_u8);
        assert_eq!(state["objects"]["small"]["hp"], 3_u8);
        let CardPlay::Awaiting(materials) = board
            .activate_internal_card("source", activation(number), PlayerId { seat: 1 })
            .unwrap()
        else {
            panic!("lesson materials")
        };
        assert!(
            matches!(materials.domain, InputDomain::CostMaterials(_)),
            "Lesson selects a named EX item"
        );
        resolved(
            board
                .answer_internal_card(
                    materials.actor,
                    materials.token,
                    Answer::CostMaterials(vec![0]),
                )
                .unwrap(),
        );
        assert_eq!(
            board.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
            13_u8
        );
        finish_checks(&mut board);
        assert!(
            board.internal_pending_choice().unwrap().is_none(),
            "no pending occurrence hides the per-turn use check"
        );
        let error = board
            .activate_internal_card("source", activation(number), PlayerId { seat: 1 })
            .unwrap_err();
        assert!(
            matches!(error, EngineFailure::IllegalPlay(_)),
            "the printed per-turn quota rejects the second use"
        );
        assert_eq!(
            board.projection(View::Referee).unwrap()["P1"]["ex"],
            json!(["spare"])
        );
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

#[test]
fn ecp01_039_observes_another_own_uma_even_when_both_depart_together_q1843() {
    let mut initial = draw_position();
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"spell","card":"BP01-131"}]);
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"source","card":"ECP01-039"},{"id":"friend","card":"CP01-023"},
        {"id":"wrong_trait","card":"BP01-037"}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"opponent","card":"CP01-023"}]);
    let mut board = game(&initial);
    resolved(
        board
            .play_internal_card("spell", PlayerId { seat: 1 })
            .unwrap(),
    );
    resolved(trigger(&mut board, "ECP01-039", 2));
    finish_checks(&mut board);
    assert!(
        board.internal_pending_choice().unwrap().is_none(),
        "only the other own Uma contributes an occurrence"
    );
    let state = board.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["leader"]["life"], 11_u8);
    assert_eq!(state["P2"]["leader"]["life"], 19_u8);
    assert_eq!(state["P1"]["field"], json!([]));
    assert_eq!(state["P2"]["field"], json!([]));
}
