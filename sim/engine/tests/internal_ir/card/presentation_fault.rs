#![expect(
    clippy::panic,
    reason = "A fixture whose lowered shape changed must fail immediately."
)]

use super::super::player_pipeline::answer;
use super::*;
use sve_engine::game::player::events::{EventKind, PresentationOp, ScopeId};
use sve_engine::game::player::{Command, Input, SubmissionKind, Viewer};

const SEATS: [Viewer; 2] = [Viewer::P1, Viewer::P2];

/// A follower whose activated ability pays, draws, asks for a follower, damages it and draws
/// again. With `fault`, the last draw requests more cards than any draw may, which is an
/// internal fault rather than a rules result.
pub(super) fn prepared(fault: bool) -> Game {
    let mut tree = fanfare::expanded::tree().clone();
    let mut entry = tree["cards"]["BP08-038"]["abilities"][0].clone();
    entry.as_object_mut().unwrap().remove("fanfare");
    entry["activate"] = json!({"cost":{"pp":1},"do":[
        {"draw":{"n":1}},
        {"choose":{"bind":"t","from":{"who":"opp","zone":"field","kind":"follower"},"n":1}},
        {"damage":{"to":"t","n":1}},
        {"draw":{"n":1}}
    ]});
    let meta = tree["cards"]["BP08-038"]["meta"].clone();
    tree["cards"] = json!({"unit-follower":{"meta":meta,"abilities":[entry]}});
    let mut program = lowerer().lower(&tree).unwrap()["unit-follower"]
        .card()
        .clone();
    let Node::Instruction { instruction } = &mut program.abilities[0]
        .node
        .body
        .as_mut()
        .unwrap()
        .steps
        .last_mut()
        .unwrap()
        .node
    else {
        panic!("final draw instruction");
    };
    let Action::Draw { count, .. } = &mut instruction.action else {
        panic!("final draw action");
    };
    if fault {
        *count = Expr::Literal(Int(10_001));
    }

    let deck = (0..3)
        .map(|index| json!({"id":format!("d{index}"),"card":"unit-spell"}))
        .collect::<Vec<_>>();
    let mut initial = setup(1, json!(deck));
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
    let mut game = Game::new(catalog(), &initial, &Value::Null, &Value::Null, "card-seed").unwrap();
    game.load_internal_card(
        "unit-follower",
        &WellFormedCardProgram::check(program).unwrap(),
    )
    .unwrap();
    game.load_internal_card("unit-spell", &card(vec![]))
        .unwrap();
    game.advance_player_input();
    game
}

/// Activate the ability up to its question for a follower.
pub(super) fn ask_for_the_follower(game: &mut Game) {
    let play = game
        .activate_internal_card("s", AbilityId(1), PlayerId { seat: 1 })
        .unwrap();
    assert!(
        matches!(play, CardPlay::Awaiting(_)),
        "the follower is chosen after the ability was played and drew once"
    );
    game.advance_player_input();
}

fn stopped_before_the_fault() -> Game {
    let mut game = prepared(true);
    ask_for_the_follower(&mut game);
    game
}

fn steps(game: &Game, viewer: Viewer) -> Vec<PresentationOp> {
    game.presentation(viewer, None)
        .unwrap()
        .map(|entry| entry.op.clone())
        .collect()
}

fn scopes(steps: &[PresentationOp], cancelled: bool) -> Vec<ScopeId> {
    steps
        .iter()
        .filter_map(|step| match step {
            PresentationOp::Provisional(batch) if !cancelled => Some(batch.scope.clone()),
            PresentationOp::Cancel { scope } if cancelled => Some(scope.clone()),
            PresentationOp::Provisional(_)
            | PresentationOp::Promote { .. }
            | PresentationOp::Cancel { .. } => None,
        })
        .collect()
}

#[test]
fn a_fault_cancels_exactly_what_each_seat_was_shown_and_concession_promotes_nothing() {
    for audience in [vec![Viewer::P1, Viewer::P2], vec![Viewer::P1]] {
        let mut game = stopped_before_the_fault();
        for viewer in &audience {
            game.present_player_events(*viewer).unwrap();
        }
        let shown = SEATS.map(|viewer| steps(&game, viewer));
        assert!(
            shown[0].len() >= 2,
            "the play and the completed draw were shown: {:?}",
            shown[0]
        );
        assert_eq!(
            shown[1].is_empty(),
            audience.len() == 1,
            "only a seat that was handed a presentation has scopes"
        );

        let failed = answer(&mut game, Viewer::P1, Answer::Targets(vec![0]));
        assert_eq!(
            failed.kind,
            SubmissionKind::EngineFault,
            "the oversized draw is an internal fault"
        );
        for (viewer, before) in SEATS.into_iter().zip(&shown) {
            let after = steps(&game, viewer);
            assert_eq!(
                &after[..before.len()],
                before.as_slice(),
                "delivered steps are never rewritten"
            );
            assert_eq!(
                scopes(&after[before.len()..], true),
                scopes(before, false),
                "exactly the scopes the fault discarded are cancelled, once and in order"
            );
            assert_eq!(
                after.len(),
                before.len().checked_mul(2).unwrap(),
                "a discarded scope is never promoted"
            );
            assert_eq!(
                game.events(viewer, None).unwrap().count(),
                0,
                "the faulted play left no committed history"
            );
            game.present_player_events(viewer).unwrap();
            assert_eq!(
                steps(&game, viewer),
                after,
                "a faulted game shows nothing further"
            );
        }

        let settled = SEATS.map(|viewer| steps(&game, viewer));
        assert_eq!(
            game.submit_player(Viewer::P2, Command::Concede).kind,
            SubmissionKind::Applied,
            "the authority still accepts a concession after the fault"
        );
        assert_eq!(
            game.input(Viewer::P1),
            Input::Finished {
                winner: Some(Viewer::P1)
            },
            "the concession, not the fault, decides the game"
        );
        for (viewer, before) in SEATS.into_iter().zip(&settled) {
            assert_eq!(
                &steps(&game, viewer),
                before,
                "nothing that was discarded is promoted or cancelled again"
            );
            let history = game.events(viewer, None).unwrap().collect::<Vec<_>>();
            assert!(
                matches!(
                    history.as_slice(),
                    [event] if event.presentation.is_none()
                        && matches!(event.kind, EventKind::Result { loser: Some(Viewer::P2), .. })
                ),
                "only the result is history: {history:?}"
            );
        }
    }
}
