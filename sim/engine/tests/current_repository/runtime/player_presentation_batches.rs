#![expect(
    clippy::panic,
    reason = "The real-card protocol must expose the expected typed question."
)]

use super::player_presentation::{SEATS, answer, declare, is_action, position, present, question};
use super::*;
use sve_engine::game::player::events::{ActionKind, EventKind, PlayerEventBatch, ProvisionalBatch};
use sve_engine::game::player::{DecisionShape, QuestionShape, SubmissionKind, Viewer};

/// BP19-045's Fanfare damages every enemy follower at once, then lets its controller look at
/// the top three cards privately and asks which one to take. `top` is the only card of the
/// three that may be taken; the opponent never learns it at this stop.
fn warden_at_its_private_choice(top: &str) -> (Game, [Vec<ProvisionalBatch>; 2]) {
    let mut game = position("BP19-045", |initial| {
        initial["players"]["P1"]["zones"]["deck"] = json!([
            {"id":"top0","card":top}, {"id":"top1","card":"BP01-171"},
            {"id":"top2","card":"BP01-171"}, {"id":"tail","card":"BP01-171"}
        ]);
    });
    let mut cursors = [None, None];
    assert_eq!(
        present(&mut game, &mut cursors),
        [vec![], vec![]],
        "the baseline handoff shows nothing"
    );
    declare(&mut game, "BP19-045");
    let QuestionShape::Decision(DecisionShape::Pending(options)) = question(&game).shape else {
        panic!("pending Fanfare");
    };
    assert_eq!(
        present(&mut game, &mut cursors),
        [vec![], vec![]],
        "the card play committed before its Fanfare was chosen"
    );
    assert_eq!(
        answer(&mut game, Answer::Pending(options[0].handle)),
        SubmissionKind::Applied,
        "the Fanfare is played, damages both enemy followers and looks at three cards"
    );
    assert!(
        matches!(
            question(&game).shape,
            QuestionShape::Decision(DecisionShape::Targets(_))
        ),
        "the optional pick among the looked-at cards is a real stop"
    );
    let shown = present(&mut game, &mut cursors);
    (game, shown)
}

fn damages(kinds: &[&EventKind]) -> usize {
    kinds
        .iter()
        .filter(|kind| is_action(kind, ActionKind::Damage))
        .count()
}

fn committed(game: &Game, viewer: Viewer) -> Vec<PlayerEventBatch> {
    game.event_batches(viewer, None).unwrap().cloned().collect()
}

#[test]
fn a_simultaneous_batch_is_one_scope_and_is_promoted_as_a_whole() {
    let (mut game, shown) = warden_at_its_private_choice("BP10-039");
    for batches in &shown {
        let area = batches
            .iter()
            .filter(|batch| batch.events.len() > 1)
            .collect::<Vec<_>>();
        assert_eq!(
            area.len(),
            1,
            "the two simultaneous hits are one scope, not one scope per target: {batches:?}"
        );
        let kinds = area[0]
            .events
            .iter()
            .map(|event| &event.kind)
            .collect::<Vec<_>>();
        assert_eq!(
            (kinds.len(), damages(&kinds)),
            (2, 2),
            "the scope holds exactly both hits: {kinds:?}"
        );
        let all = batches
            .iter()
            .flat_map(|batch| batch.events.iter().map(|event| &event.kind))
            .collect::<Vec<_>>();
        assert!(
            !all.iter().any(|kind| is_action(kind, ActionKind::Reveal)),
            "a private look produces no result for either seat: {all:?}"
        );
    }
    assert_eq!(
        answer(&mut game, Answer::Targets(vec![])),
        SubmissionKind::Applied,
        "declining the pick completes the Fanfare"
    );
    for viewer in SEATS {
        let batches = committed(&game, viewer);
        let area = batches
            .iter()
            .filter(|batch| {
                batch
                    .events
                    .iter()
                    .all(|event| is_action(&event.kind, ActionKind::Damage))
                    && batch.events.len() == 2
            })
            .collect::<Vec<_>>();
        assert_eq!(area.len(), 1, "the area damage commits as one batch");
        assert!(
            area[0]
                .events
                .iter()
                .all(|event| event.presentation.is_some()),
            "every member of the shown batch is promoted together"
        );
    }
}

#[test]
fn what_a_seat_is_shown_does_not_depend_on_the_other_seats_private_cards() {
    let (_, first) = warden_at_its_private_choice("BP10-039");
    let (_, second) = warden_at_its_private_choice("BP10-050");
    // Scope identities belong to one game; the comparison is over what each scope contains.
    let contents = |batches: &[ProvisionalBatch]| {
        batches
            .iter()
            .map(|batch| {
                batch
                    .events
                    .iter()
                    .map(|event| event.kind.clone())
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>()
    };
    assert!(
        !first[1].is_empty(),
        "the waiting seat is shown the public results completed before the stop"
    );
    assert_eq!(
        contents(&first[1]),
        contents(&second[1]),
        "the opponent's scopes hold the same results, batch for batch, whichever card its \
         rival privately looked at"
    );
}
