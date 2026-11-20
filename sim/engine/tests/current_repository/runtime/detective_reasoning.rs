use super::player_presentation::{SEATS, answer, declare, is_action, position, question};
use super::*;
use sve_engine::game::player::events::{ActionKind, EventKind};
use sve_engine::game::player::{DecisionShape, Input, QuestionShape, SubmissionKind, Viewer};

#[test]
fn detective_reasoning_declares_a_public_rules_name_before_revealing_the_matching_collab_print() {
    let mut board = position("BP12-115", |initial| {
        initial["players"]["P1"]["zones"]["deck"][0]["card"] = json!("BP06-SP01");
    });
    declare(&mut board, "BP12-115");
    let QuestionShape::Decision(DecisionShape::DeclareName(options)) = question(&board).shape
    else {
        panic!("public card name declaration");
    };
    let rules_name = "陰陽の開祖・クオン";
    let collab_name = "澄明なる頭脳・博衣こより";
    assert_eq!(
        options
            .iter()
            .filter(|name| name.as_str() == rules_name)
            .count(),
        1
    );
    assert!(options.iter().any(|name| name == "フェアリー"));
    assert!(!options.iter().any(|name| name == collab_name));
    assert!(matches!(board.input(Viewer::P2), Input::Waiting(_)));
    for viewer in SEATS {
        assert!(
            board
                .events(viewer, None)
                .unwrap()
                .all(|event| !is_action(&event.kind, ActionKind::Reveal))
        );
    }
    assert_eq!(
        answer(&mut board, Answer::DeclareName(collab_name.to_owned())),
        SubmissionKind::Rejected
    );
    assert_eq!(
        answer(&mut board, Answer::DeclareName(rules_name.to_owned())),
        SubmissionKind::Applied
    );
    for viewer in SEATS {
        let events = board
            .events(viewer, None)
            .unwrap()
            .map(|event| &event.kind)
            .collect::<Vec<_>>();
        let declaration = events
            .iter()
            .position(|kind| {
                matches!(
                    kind, EventKind::Action(action) if action.kind == ActionKind::Declare
                        && action.name.as_deref() == Some(rules_name)
                )
            })
            .unwrap();
        let reveal = events
            .iter()
            .position(|kind| is_action(kind, ActionKind::Reveal))
            .unwrap();
        assert!(declaration < reveal);
    }
    let state = board.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["kept", "draw0", "draw1"]));
    assert_eq!(state["P1"]["deck"], json!(["tail"]));
}
