#![expect(
    clippy::panic,
    reason = "The real-card protocol must expose the expected typed question."
)]

use super::drive::{drive_game, position};
use super::player_presentation::{SEATS, is_action, kinds, present, promoted, question, recorded};
use super::*;
use sve_engine::game::player::events::{
    ActionKind, EventKind, PlayerEvent, PresentationCursor, PresentationOp, ScopeId,
};
use sve_engine::game::player::observation::Observation;
use sve_engine::game::player::{
    AttackTarget, Command, DecisionShape, Input, MainActionAnswer, MainIntentKind, QuestionShape,
    SubmissionKind, Viewer,
};

/// CP03-026 checks twice when it attacks. The first checked card has a verified icon, so a
/// Drive question is asked; the next card on the deck has no verified icon facts, which the
/// engine refuses to guess: the second check stops with an internal fault.
fn twin_drive() -> Game {
    let mut initial = position("CP03-025", "CP03-026", 5);
    initial["players"]["P1"]["construction"] = json!("title");
    initial["players"]["P1"]["title_code"] = json!("cardfight_vanguard");
    let facts = json!({"BP01-037":{"verified":true,"trigger_icon":"draw"}});
    let mut game = drive_game(&initial, &facts);
    game.advance_player_input();
    game
}

fn submit(game: &mut Game, command: Command) -> SubmissionKind {
    game.submit_player(Viewer::P1, command).kind
}

/// Attack through the player interface and answer everything before the Drive check (Quick
/// passes, the pending attack trigger), handing both seats a presentation at every stop.
fn reach_the_drive_question(game: &mut Game, cursors: &mut [Option<PresentationCursor>; 2]) {
    assert_eq!(
        present(game, cursors),
        [vec![], vec![]],
        "the baseline handoff shows nothing"
    );
    let main = question(game);
    let QuestionShape::ActionWindow(actions) = &main.shape else {
        panic!("main declarations");
    };
    // The only follower able to attack is the evolved one; its ally has already acted.
    let attack = actions
        .intents
        .iter()
        .find(|intent| matches!(intent.kind, MainIntentKind::Attack { .. }))
        .unwrap();
    assert_eq!(
        submit(
            game,
            Command::MainAction {
                token: main.token,
                intent: attack.id.clone(),
                answer: MainActionAnswer::Attack(AttackTarget::Leader(Viewer::P2)),
            },
        ),
        SubmissionKind::Applied,
        "the evolved follower attacks the opposing leader"
    );
    loop {
        let current = question(game);
        let reply = match &current.shape {
            QuestionShape::Decision(DecisionShape::DriveTrigger { .. }) => return,
            QuestionShape::Decision(DecisionShape::Pending(options)) => Answer::Pending(
                options
                    .iter()
                    .find(|option| option.card == "CP03-026")
                    .unwrap()
                    .handle,
            ),
            QuestionShape::Decision(DecisionShape::Quick { .. }) => Answer::Quick(None),
            QuestionShape::Decision(_)
            | QuestionShape::Evolution(_)
            | QuestionShape::Opening(_)
            | QuestionShape::Closing(_)
            | QuestionShape::ActionWindow(_) => panic!("unexpected question {current:?}"),
        };
        let _shown = present(game, cursors);
        assert_eq!(
            submit(
                game,
                Command::Answer {
                    token: current.token,
                    answer: reply,
                },
            ),
            SubmissionKind::Applied,
            "each step before the Drive check is legal"
        );
    }
}

type Snapshot = (Vec<PresentationOp>, Vec<PlayerEvent>, Observation);

/// Everything a seat can read: its presentation record, committed history and board.
fn snapshot(game: &Game, viewer: Viewer) -> Snapshot {
    (
        recorded(game, viewer, None).0,
        game.events(viewer, None).unwrap().cloned().collect(),
        game.observe(viewer).unwrap(),
    )
}

/// Scopes shown to this seat that are neither promoted nor cancelled.
fn open_scopes(steps: &[PresentationOp]) -> Vec<ScopeId> {
    let closed = promoted(steps);
    steps
        .iter()
        .filter_map(|step| match step {
            PresentationOp::Provisional(batch) if !closed.contains(&batch.scope) => {
                Some(batch.scope.clone())
            }
            PresentationOp::Provisional(_)
            | PresentationOp::Promote { .. }
            | PresentationOp::Cancel { .. } => None,
        })
        .collect()
}

#[test]
fn a_fault_that_returns_to_the_previous_stop_neither_cancels_nor_promotes_what_was_shown() {
    let mut game = twin_drive();
    let mut cursors = [None, None];
    reach_the_drive_question(&mut game, &mut cursors);

    // The previous stop: the Drive ability was played and revealed its first checked card.
    for batches in present(&mut game, &mut cursors) {
        let kinds = kinds(&batches);
        assert!(
            kinds
                .iter()
                .any(|kind| is_action(kind, ActionKind::AbilityPlay)),
            "the played Drive ability is shown at its first question: {kinds:?}"
        );
    }
    let before = SEATS.map(|viewer| snapshot(&game, viewer));
    let open = [open_scopes(&before[0].0), open_scopes(&before[1].0)];
    assert!(
        open.iter().all(|scopes| !scopes.is_empty()),
        "both seats hold scopes of the suspended Drive ability"
    );

    // Declining the first trigger makes the engine check the next card, whose icon is not
    // verified. A Drive answer never returns to the play's origin: the checked card was
    // already revealed.
    let token = question(&game).token;
    let declined = Command::Answer {
        token,
        answer: Answer::DriveTrigger {
            accept: false,
            target: None,
        },
    };
    assert_eq!(
        submit(&mut game, declined),
        SubmissionKind::EngineFault,
        "the unverified icon is an internal fault, not a rules result"
    );
    for (viewer, previous) in SEATS.into_iter().zip(&before) {
        assert_eq!(
            game.input(viewer),
            Input::EngineFault,
            "player progression stopped without a winner"
        );
        assert_eq!(
            &snapshot(&game, viewer),
            previous,
            "nothing was discarded and nothing committed: no scope is cancelled or promoted, \\
             history is unchanged and the board is the one at the previous stop"
        );
        game.present_player_events(viewer).unwrap();
        assert_eq!(
            recorded(&game, viewer, None).0,
            previous.0,
            "a faulted game shows nothing further"
        );
    }

    // The authority still accepts a concession; it commits what really happened.
    assert_eq!(
        game.submit_player(Viewer::P2, Command::Concede).kind,
        SubmissionKind::Applied,
        "the waiting seat ends the faulted game"
    );
    for ((viewer, scopes), (steps, history, _)) in SEATS.into_iter().zip(&open).zip(&before) {
        let after = recorded(&game, viewer, None).0;
        assert_eq!(
            promoted(&after[steps.len()..]),
            *scopes,
            "the executed prefix, including what was shown, is promoted once"
        );
        assert!(
            !after
                .iter()
                .any(|step| matches!(step, PresentationOp::Cancel { .. })),
            "nothing is cancelled: the fault discarded nothing"
        );
        let events = game
            .events(viewer, None)
            .unwrap()
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(
            &events[..history.len()],
            history.as_slice(),
            "earlier history is untouched"
        );
        assert!(
            matches!(
                events.last().map(|event| &event.kind),
                Some(EventKind::Result {
                    loser: Some(Viewer::P2),
                    ..
                })
            ),
            "the result follows the committed prefix"
        );
    }
}
