#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate printed conditional mode scenarios."
)]
use super::*;
use sve_engine::game::internal::DecisionView;

fn position(card: &str, used: u8) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    initial["players"]["P1"]["zones"]["cemetery"] = json!([{"id":"returned","card":"BP01-009"}]);
    let mut points: Vec<_> = (0..used)
        .map(|i| json!({"id":format!("used-{i}"),"card":"CSD03a-017","face_up":true}))
        .collect();
    points.extend([
        json!({"id":"unused","card":"CSD03a-017"}),
        json!({"id":"other-role","card":"CSD01-032","face_up":true}),
    ]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!(points);
    initial["players"]["P2"]["zones"]["evolve_deck"] = json!(
        (0_u8..3)
            .map(|i| json!({"id":format!("foreign-{i}"),"card":"CSD03a-017","face_up":true}))
            .collect::<Vec<_>>()
    );
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-037","state":{"hp":10_u8,"max_hp":10_u8}}]);
    initial
}

fn validate_declaration(board: &mut Game, q: &DecisionView, used: u8) {
    let InputDomain::Modes { count, modes, .. } = &q.domain else {
        panic!("modes")
    };
    assert_eq!(
        *count,
        Nat(if used == 3 { 3_u64 } else { 1 }),
        "only own face-up named resources determine the mode cap"
    );
    assert_eq!(
        modes,
        &vec![ModeId(1), ModeId(2), ModeId(3)],
        "all three printed modes have legal formal targets"
    );
    let before = board.projection(View::Referee).unwrap();
    for invalid in [vec![], vec![ModeId(3), ModeId(3)]] {
        board
            .answer_internal_card(q.actor, q.token, Answer::Modes(invalid))
            .unwrap_err();
        assert_eq!(
            board.projection(View::Referee).unwrap(),
            before,
            "an invalid mode answer changes no authority state"
        );
    }
    if used == 2 {
        board
            .answer_internal_card(q.actor, q.token, Answer::Modes(vec![ModeId(1), ModeId(3)]))
            .unwrap_err();
        assert_eq!(
            board.projection(View::Referee).unwrap(),
            before,
            "an invalid mode answer changes no authority state"
        );
    }
}

fn resolve_modes(board: &mut Game, q: &DecisionView, all: bool) {
    let chosen = if all {
        vec![ModeId(3), ModeId(2), ModeId(1)]
    } else {
        vec![ModeId(3)]
    };
    let mut play = board
        .answer_internal_card(q.actor, q.token, Answer::Modes(chosen))
        .unwrap();
    while let CardPlay::Awaiting(target) = play {
        let InputDomain::Targets(domain) = &target.domain else {
            panic!("formal mode target")
        };
        assert_eq!(
            board.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
            10_u8,
            "targets are declared before any mode resolves"
        );
        play = board
            .answer_internal_card(
                target.actor,
                target.token,
                Answer::Targets(vec![domain.members[0].handle]),
            )
            .unwrap();
    }
    resolved(play);
}

fn run_case(card: &str, used: u8, all: bool) {
    let mut board = Game::from_selected_effects(
        selection_with_rules().clone(),
        &position(card, used),
        &Value::Null,
        &Value::Null,
        "drive-count",
    )
    .unwrap();
    let CardPlay::Awaiting(q) = board
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("mode declaration")
    };
    validate_declaration(&mut board, &q, used);
    resolve_modes(&mut board, &q, all);
    let view = board.projection(View::Referee).unwrap();
    assert_eq!(
        view["P1"]["leader"]["life"], 14_u8,
        "selected mode three heals four"
    );
    assert_eq!(
        view["P1"]["pp"]["current"], 7_u8,
        "the spell pays its printed cost once"
    );
    assert_eq!(
        view["objects"]["enemy"]["hp"],
        if all { 6_u8 } else { 10 },
        "damage follows selection of mode one"
    );
    assert_eq!(
        view["P1"]["hand"],
        if all { json!(["returned"]) } else { json!([]) },
        "the cemetery follower returns only when mode two is selected"
    );
}

#[test]
fn all_printings_read_only_own_face_up_drive_points_before_declaring_modes() {
    for (card, used, all) in [
        ("CP03-125", 2_u8, false),
        ("CP03-126", 3, true),
        ("CP03-SL24", 2, false),
        ("CP03-SL25", 3, false),
    ] {
        run_case(card, used, all);
    }
}
