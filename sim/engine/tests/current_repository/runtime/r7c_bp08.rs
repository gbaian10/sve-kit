#![expect(
    clippy::panic,
    reason = "The printed spell must ask for its formal target and its exact optional action."
)]
use super::*;
use sve_engine::game::internal::DecisionView;

const NUMBER: &str = "BP08-085";
const PLAYER: PlayerId = PlayerId { seat: 1 };

fn fuel(n: usize) -> String {
    format!("fuel{n}")
}

fn initial(count: usize, enemy: bool) -> Value {
    let mut state = draw_position();
    state["players"]["P1"]["leader"]["class"] = json!("ナイトメア");
    state["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":NUMBER}]);
    state["players"]["P1"]["zones"]["field"] = json!([{"id":"ally","card":"BP01-080"}]);
    state["players"]["P1"]["zones"]["cemetery"] = json!(
        (0..count)
            .map(|n| json!({"id":fuel(n),"card":"BP01-171"}))
            .collect::<Vec<_>>()
    );
    state["players"]["P2"]["zones"]["cemetery"] = json!([{"id":"foreign","card":"BP01-080"}]);
    if enemy {
        state["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-080"}]);
    } else {
        state["players"]["P2"]["zones"]["field"] = json!([{"id":"amulet","card":"BP01-T10"}]);
    }
    state
}

fn question(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(question) = play else {
        panic!("printed question");
    };
    question
}

fn target(board: &mut Game) -> CardPlay {
    let q = question(board.play_internal_card("source", PLAYER).unwrap());
    let InputDomain::Targets(domain) = &q.domain else {
        panic!("formal opponent follower target");
    };
    assert_eq!(domain.members.len(), 1, "the own follower is not a target");
    board
        .answer_internal_card(
            q.actor,
            q.token,
            Answer::Targets(vec![domain.members[0].handle]),
        )
        .unwrap()
}

fn banish(board: &mut Game, q: &DecisionView, count: usize) -> CardPlay {
    let InputDomain::Targets(domain) = &q.domain else {
        panic!("exact cemetery choice");
    };
    assert_eq!(
        domain.members.len(),
        count,
        "no foreign card or resolving spell"
    );
    assert!(
        domain
            .members
            .iter()
            .all(|m| m.card.as_deref() == Some("BP01-171")),
        "only the own cemetery fuel is offered"
    );
    assert_eq!(domain.cardinality.minimum, Nat(4), "exactly four required");
    assert_eq!(
        domain.cardinality.maximum,
        Some(Nat(4)),
        "exactly four allowed"
    );
    let handles = domain.members.iter().map(|m| m.handle).collect::<Vec<_>>();
    let mut invalid = vec![
        handles[..3].to_vec(),
        vec![handles[0]; 4],
        vec![handles[0], handles[1], handles[2], u64::MAX],
    ];
    if count > 4 {
        invalid.push(handles.clone());
    }
    for selection in invalid {
        let before = format!("{board:?}");
        board
            .answer_internal_card(q.actor, q.token, Answer::Targets(selection))
            .unwrap_err();
        assert_eq!(
            format!("{board:?}"),
            before,
            "invalid answers keep the whole decision unchanged"
        );
    }
    board
        .answer_internal_card(q.actor, q.token, Answer::Targets(handles[..4].to_vec()))
        .unwrap()
}

fn finish(board: &mut Game, count: usize, accept: bool) {
    let mut play = target(board);
    if count >= 4 {
        let q = question(play);
        assert!(
            matches!(q.domain, InputDomain::Integer(_)),
            "the banish is an optional effect after the formal target"
        );
        play = board
            .answer_internal_card(q.actor, q.token, Answer::Integer(Nat(u64::from(accept))))
            .unwrap();
        if accept {
            play = banish(board, &question(play), count);
        }
    }
    resolved(play);
    while board.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        board.resume_internal_card().unwrap();
    }
}

#[test]
fn evil_possession_returns_only_after_four_actual_banishes() {
    for (count, accept) in [(3_usize, true), (4, false), (4, true), (5, true)] {
        let mut board = game(&initial(count, true));
        finish(&mut board, count, accept);
        let state = board.projection(View::Referee).unwrap();
        let returned = count >= 4 && accept;
        let removed = if returned { 4 } else { 0 };
        assert_eq!(state["P1"]["pp"]["current"], 8_u8, "ordinary two-PP spell");
        assert_eq!(
            state["objects"]["enemy"]["damage"], 3_u8,
            "damage before optional banish"
        );
        assert_eq!(
            state["P1"]["banish"],
            json!((0..removed).map(fuel).collect::<Vec<_>>())
        );
        let mut cemetery = (removed..count).map(fuel).collect::<Vec<_>>();
        if !returned {
            cemetery.push("source".into());
        }
        assert_eq!(
            state["P1"]["cemetery"],
            json!(cemetery),
            "decline or shortage leaves fuel in place"
        );
        assert_eq!(
            state["P1"]["hand"],
            if returned {
                json!(["source"])
            } else {
                json!([])
            }
        );
        assert_eq!(
            state["P2"]["cemetery"],
            json!(["foreign"]),
            "foreign fuel untouched"
        );
    }
}

#[test]
fn evil_possession_without_enemy_follower_cannot_play_or_banish() {
    let mut board = game(&initial(4, false));
    let before = board.projection(View::Referee).unwrap();
    let error = board.play_internal_card("source", PLAYER).unwrap_err();
    assert!(
        matches!(error, EngineFailure::IllegalPlay(_)),
        "Q1116 requires a formal enemy follower"
    );
    assert_eq!(
        board.projection(View::Referee).unwrap(),
        before,
        "illegal play cannot spend PP, banish or move the spell"
    );
}
