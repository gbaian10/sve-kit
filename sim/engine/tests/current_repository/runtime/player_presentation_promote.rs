#![expect(
    clippy::panic,
    reason = "The real-card protocol must expose the expected typed question."
)]

use super::player_presentation::{
    SEATS, answer, declare, is_action, kinds, position, present, promoted, question, recorded,
    shown,
};
use super::*;
use sve_engine::game::player::events::{
    ActionKind, EventKind, PlayerEventBatch, PresentationCursor, ProvisionalBatch,
};
use sve_engine::game::player::{
    Command, DecisionShape, Input, QuestionShape, SubmissionKind, Viewer,
};

fn committed(game: &Game, viewer: Viewer) -> Vec<PlayerEventBatch> {
    game.event_batches(viewer, None).unwrap().cloned().collect()
}

/// Play BP09-058 up to its discard question; `stops` says at which of the two stops the
/// seats are handed a presentation. Returns what each seat was shown.
fn discard_stop(
    stops: [bool; 2],
) -> (
    Game,
    [Vec<ProvisionalBatch>; 2],
    [Option<PresentationCursor>; 2],
) {
    let mut game = position("BP09-058", |_| {});
    let mut cursors = [None, None];
    let mut seen = [Vec::new(), Vec::new()];
    declare(&mut game, "BP09-058");
    if stops[0] {
        for (all, new) in seen.iter_mut().zip(present(&mut game, &mut cursors)) {
            all.extend(new);
        }
    }
    assert_eq!(
        answer(&mut game, Answer::Targets(vec![0])),
        SubmissionKind::Applied,
        "the only enemy follower is a legal selection"
    );
    if stops[1] {
        for (all, new) in seen.iter_mut().zip(present(&mut game, &mut cursors)) {
            all.extend(new);
        }
    }
    (game, seen, cursors)
}

fn discard_first(game: &mut Game) -> SubmissionKind {
    let QuestionShape::Decision(DecisionShape::Discard { cards, .. }) = question(game).shape else {
        panic!("discard question");
    };
    answer(game, Answer::Targets(vec![cards.members[0].handle]))
}

/// Committed batches must refer to exactly the shown scopes, member for member, and every
/// other batch must stay unlinked.
fn assert_linked(batches: &[PlayerEventBatch], seen: &[ProvisionalBatch]) {
    let linked = batches
        .iter()
        .filter(|batch| {
            batch
                .events
                .iter()
                .any(|event| event.presentation.is_some())
        })
        .collect::<Vec<_>>();
    assert_eq!(
        linked.len(),
        seen.len(),
        "each shown scope corresponds to exactly one committed batch"
    );
    for (batch, scope) in linked.iter().zip(seen) {
        assert_eq!(
            batch.events.len(),
            scope.events.len(),
            "a scope and its committed batch have the same members"
        );
        for (event, before) in batch.events.iter().zip(&scope.events) {
            assert_eq!(
                event.presentation.as_ref(),
                Some(&before.id),
                "the committed event refers to the occurrence it was shown as"
            );
            assert_eq!(event.kind, before.kind, "promotion never changes the body");
        }
    }
}

#[test]
fn promotion_links_each_shown_batch_once_and_leaves_later_batches_unlinked() {
    let (mut game, seen, cursors) = discard_stop([true, true]);
    assert_eq!(
        discard_first(&mut game),
        SubmissionKind::Applied,
        "the discard completes the spell"
    );
    for ((viewer, scopes), cursor) in SEATS.into_iter().zip(&seen).zip(&cursors) {
        assert_eq!(
            scopes.len(),
            4,
            "declaration, play, damage and draw were shown"
        );
        let batches = committed(&game, viewer);
        assert_linked(&batches, scopes);
        let first_unlinked = batches
            .iter()
            .position(|batch| {
                batch
                    .events
                    .iter()
                    .all(|event| event.presentation.is_none())
            })
            .unwrap();
        assert_eq!(
            first_unlinked,
            scopes.len(),
            "the shown prefix commits first; the discard and the resolution follow as ordinary history"
        );
        let (steps, _) = recorded(&game, viewer, cursor.as_ref());
        assert_eq!(
            promoted(&steps),
            scopes
                .iter()
                .map(|batch| batch.scope.clone())
                .collect::<Vec<_>>(),
            "every scope is promoted once, in the order it was shown"
        );
        assert_eq!(shown(&steps), vec![], "promotion shows nothing again");
    }
    let before = SEATS.map(|viewer| recorded(&game, viewer, None).0);
    game.advance_player_input();
    for viewer in SEATS {
        game.present_player_events(viewer).unwrap();
    }
    assert_eq!(
        SEATS.map(|viewer| recorded(&game, viewer, None).0),
        before,
        "later checkpoints and requests never promote or show a committed batch again"
    );
}

#[test]
fn promotion_follows_only_the_seat_that_was_shown() {
    let mut game = position("BP08-045", |_| {});
    declare(&mut game, "BP08-045");
    game.present_player_events(Viewer::P1).unwrap();
    let owner = shown(&recorded(&game, Viewer::P1, None).0);
    assert_eq!(
        discard_first(&mut game),
        SubmissionKind::Applied,
        "the discard completes the spell"
    );
    assert_linked(&committed(&game, Viewer::P1), &owner);
    assert_linked(&committed(&game, Viewer::P2), &[]);
    assert_eq!(
        recorded(&game, Viewer::P2, None).0,
        vec![],
        "a seat that was never shown a batch has nothing to promote"
    );
    assert_eq!(
        committed(&game, Viewer::P1).len(),
        committed(&game, Viewer::P2).len(),
        "both seats still receive the same committed batches"
    );
}

#[test]
fn concession_commits_the_executed_prefix_promotes_it_and_adds_one_result() {
    for stops in [[true, true], [true, false]] {
        let (mut game, seen, cursors) = discard_stop(stops);
        assert_eq!(
            game.submit_player(Viewer::P2, Command::Concede).kind,
            SubmissionKind::Applied,
            "the waiting seat may concede in the middle of the resolution"
        );
        assert_eq!(
            game.input(Viewer::P1),
            Input::Finished {
                winner: Some(Viewer::P1)
            },
            "the concession is a normal rules result"
        );
        for ((viewer, scopes), cursor) in SEATS.into_iter().zip(&seen).zip(&cursors) {
            let batches = committed(&game, viewer);
            assert_linked(&batches, scopes);
            let (steps, _) = recorded(&game, viewer, cursor.as_ref());
            assert_eq!(
                promoted(&steps),
                scopes
                    .iter()
                    .map(|batch| batch.scope.clone())
                    .collect::<Vec<_>>(),
                "the executed prefix is promoted once: {stops:?}"
            );
            let all = batches
                .iter()
                .flat_map(|batch| batch.events.iter().map(|event| &event.kind))
                .collect::<Vec<_>>();
            assert_eq!(
                all.iter()
                    .filter(|kind| is_action(kind, ActionKind::Damage))
                    .count(),
                1,
                "the damage happened and is committed whether or not it was shown"
            );
            assert!(
                !all.iter().any(|kind| is_action(kind, ActionKind::Resolve)),
                "steps that never ran produce no event"
            );
            assert_eq!(
                all.iter()
                    .filter(|kind| matches!(kind, EventKind::Result { .. }))
                    .count(),
                1,
                "the result is delivered once"
            );
            let last = batches.last().unwrap();
            assert!(
                matches!(
                    last.events.as_slice(),
                    [event] if event.presentation.is_none()
                        && matches!(event.kind, EventKind::Result { loser: Some(Viewer::P2), .. })
                ),
                "the result follows the promoted prefix in its own unlinked batch"
            );
        }
        let before = SEATS.map(|viewer| recorded(&game, viewer, None).0);
        assert_eq!(
            game.submit_player(Viewer::P1, Command::Concede).kind,
            SubmissionKind::Rejected,
            "a finished game accepts no further command"
        );
        assert_eq!(
            SEATS.map(|viewer| recorded(&game, viewer, None).0),
            before,
            "nothing is promoted twice"
        );
        assert_eq!(
            kinds(&seen[0]).len(),
            if stops[1] { 4 } else { 2 },
            "the scenario showed the intended stops"
        );
    }
}
