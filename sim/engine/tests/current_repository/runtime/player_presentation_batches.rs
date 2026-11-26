
use super::player_presentation::{
    SEATS, answer, declare, is_action, position, present, promoted, question, recorded,
};
use super::*;
use sve_engine::game::player::events::{ActionKind, EventKind, PresentationOp, ProvisionalBatch};
use sve_engine::game::player::observation::ObservedZone;
use sve_engine::game::player::{DecisionShape, Input, QuestionShape, SubmissionKind, Viewer};

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

fn shape(batches: &[ProvisionalBatch]) -> Vec<Vec<ActionKind>> {
    batches
        .iter()
        .map(|batch| {
            batch
                .events
                .iter()
                .filter_map(|event| match &event.kind {
                    EventKind::Action(action) => Some(action.kind),
                    EventKind::Movement { .. } | EventKind::Phase(_) | EventKind::Result { .. } => {
                        None
                    }
                })
                .collect()
        })
        .collect()
}

#[test]
fn a_simultaneous_batch_is_one_scope_and_is_promoted_member_for_member() {
    let (mut game, shown) = warden_at_its_private_choice("BP10-039");
    let areas = shown
        .iter()
        .map(|batches| {
            let area = batches
                .iter()
                .filter(|batch| batch.events.len() > 1)
                .collect::<Vec<_>>();
            assert_eq!(
                area.len(),
                1,
                "the two simultaneous hits are one scope, not one scope per target: {batches:?}"
            );
            assert!(
                area[0].events.len() == 2
                    && area[0]
                        .events
                        .iter()
                        .all(|event| is_action(&event.kind, ActionKind::Damage)),
                "the scope holds exactly both hits: {:?}",
                area[0]
            );
            area[0].clone()
        })
        .collect::<Vec<_>>();
    assert_eq!(
        answer(&mut game, Answer::Targets(vec![])),
        SubmissionKind::Applied,
        "declining the pick completes the Fanfare"
    );
    for (viewer, area) in SEATS.into_iter().zip(&areas) {
        let batches = committed(&game, viewer);
        let linked = batches
            .iter()
            .filter(|batch| {
                batch
                    .events
                    .iter()
                    .any(|event| event.presentation.as_ref() == Some(&area.events[0].id))
            })
            .collect::<Vec<_>>();
        assert_eq!(
            linked.len(),
            1,
            "exactly one committed batch stands for the area scope"
        );
        assert_eq!(
            linked[0].events.len(),
            area.events.len(),
            "the committed batch has the scope's members and no others"
        );
        for (event, before) in linked[0].events.iter().zip(&area.events) {
            assert_eq!(
                event.presentation.as_ref(),
                Some(&before.id),
                "each committed hit refers to its own shown occurrence, in the shown order"
            );
            assert_eq!(
                event.kind, before.kind,
                "promotion keeps each hit's content"
            );
        }
        let (steps, _) = recorded(&game, viewer, None);
        assert_eq!(
            promoted(&steps)
                .iter()
                .filter(|scope| **scope == area.scope)
                .count(),
            1,
            "the whole scope is promoted exactly once"
        );
        assert!(
            !steps
                .iter()
                .any(|step| matches!(step, PresentationOp::Cancel { .. })),
            "nothing of this play is cancelled"
        );
    }
}

#[test]
fn a_private_look_changes_only_what_its_owner_is_asked_and_adds_no_scope_for_anyone() {
    let mut opponent = Vec::new();
    for top in ["BP10-039", "BP10-050"] {
        let (game, shown) = warden_at_its_private_choice(top);
        // The seat that looked is asked about exactly the card it privately saw.
        let QuestionShape::Decision(DecisionShape::Targets(pick)) = question(&game).shape else {
            panic!("the optional pick among the looked-at cards");
        };
        assert_eq!(
            pick.members
                .iter()
                .map(|member| member.card.as_deref())
                .collect::<Vec<_>>(),
            vec![Some(top)],
            "the acting seat's candidates are the looked-at card that qualifies"
        );
        // The other seat only waits: it gets no candidates and learns none of the deck.
        assert!(
            matches!(game.input(Viewer::P2), Input::Waiting(waiting) if waiting.actor == Viewer::P1),
            "the opponent waits without a question of its own"
        );
        let deck = game.observe(Viewer::P2).unwrap().players[0]
            .zones
            .iter()
            .find(|zone| zone.kind == ObservedZone::Deck)
            .unwrap()
            .clone();
        assert!(
            deck.cards.is_empty() && deck.count == 4,
            "the opponent sees only how many cards the deck holds: {deck:?}"
        );
        // Both seats are shown exactly the public results completed before the stop: the
        // played Fanfare and the area damage. The look adds no scope, empty or otherwise.
        for batches in &shown {
            assert_eq!(
                shape(batches),
                vec![
                    vec![ActionKind::AbilityPlay],
                    vec![ActionKind::Damage, ActionKind::Damage]
                ],
                "no result, placeholder or extra scope stands for the private look: {batches:?}"
            );
            assert_eq!(
                batches
                    .iter()
                    .map(|batch| batch.events.len())
                    .collect::<Vec<_>>(),
                vec![1, 2],
                "the scopes hold no member besides those results"
            );
        }
        opponent.push(
            shown[1]
                .iter()
                .map(|batch| {
                    batch
                        .events
                        .iter()
                        .map(|event| event.kind.clone())
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>(),
        );
    }
    // Scope identities belong to one game; the comparison is over what each scope contains.
    assert_eq!(
        opponent[0], opponent[1],
        "the opponent's scopes hold the same results, batch for batch, whichever card its \
         rival privately looked at"
    );
}
