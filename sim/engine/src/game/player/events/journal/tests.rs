use super::*;
use crate::game::player::events::{
    ActionEvent, EventCard, EventSubject, PresentationOp, ProvisionalBatch,
};
use crate::game::player::observation::CardProperties;
use serde_json::json;

fn result(rule: &str) -> EventKind {
    EventKind::Result {
        winner: None,
        loser: None,
        rule: Some(rule.into()),
    }
}

/// One sealed single-member batch per rule name, visible to both viewers.
fn journal(rules: &[&str]) -> Journal {
    let mut journal = Journal {
        active: true,
        ..Journal::default()
    };
    for rule in rules {
        journal.push(
            [Ok(Some(result(rule))), Ok(Some(result(rule)))],
            None,
            &json!({"rule":rule}),
            format!("unit/{rule}"),
            None,
            true,
        );
    }
    journal
}

#[test]
fn a_batch_changed_after_it_was_shown_stops_that_viewers_history_only() {
    let mut journal = journal(&["first", "second"]);
    journal.present(Viewer::P1, true, BTreeSet::new()).unwrap();
    assert_eq!(
        journal.read_presentation(Viewer::P1, None).unwrap().count(),
        2,
        "both sealed batches were shown to the first viewer"
    );
    // A producer that rewrites a result after a viewer saw it breaks the promised equality.
    journal.pending[1].views = [Ok(Some(result("rewritten"))), Ok(Some(result("rewritten")))];
    journal.commit();
    assert!(
        matches!(
            journal.read(Viewer::P1, None),
            Err(EventError::ProjectionGap)
        ),
        "the shown batch no longer equals the committed one, so nothing is promoted by guess"
    );
    assert!(
        matches!(
            journal.read_presentation(Viewer::P1, None),
            Err(EventError::ProjectionGap)
        ),
        "the presentation record stops together with the history"
    );
    let other = journal.read(Viewer::P2, None).unwrap().collect::<Vec<_>>();
    assert_eq!(
        other.len(),
        2,
        "the other viewer was shown nothing and commits normally"
    );
    assert!(
        other.iter().all(|event| event.presentation.is_none()),
        "a viewer without a scope has no link"
    );
}

#[test]
fn a_shown_member_that_disappears_before_commit_is_not_promoted() {
    let mut journal = journal(&["only"]);
    journal.present(Viewer::P2, true, BTreeSet::new()).unwrap();
    journal.pending[0].views = [Ok(Some(result("only"))), Ok(None)];
    journal.commit();
    assert!(
        matches!(
            journal.read(Viewer::P2, None),
            Err(EventError::ProjectionGap)
        ),
        "an empty committed batch cannot stand for a scope that had a member"
    );
    assert_eq!(
        journal.read(Viewer::P1, None).unwrap().count(),
        1,
        "the unaffected viewer keeps its history"
    );
}

#[test]
fn an_unchanged_batch_is_promoted_with_matching_identities() {
    let mut journal = journal(&["first", "second"]);
    journal.present(Viewer::P1, true, BTreeSet::new()).unwrap();
    journal.commit();
    let steps = journal
        .read_presentation(Viewer::P1, None)
        .unwrap()
        .map(|entry| entry.op.clone())
        .collect::<Vec<_>>();
    let [
        PresentationOp::Provisional(first),
        PresentationOp::Provisional(second),
        PresentationOp::Promote {
            scope: promoted_first,
        },
        PresentationOp::Promote {
            scope: promoted_second,
        },
    ] = steps.as_slice()
    else {
        panic!("two scopes shown, then promoted in order: {steps:?}");
    };
    assert_eq!(
        (promoted_first, promoted_second),
        (&first.scope, &second.scope),
        "each scope is promoted once, in commit order"
    );
    let events = journal.read(Viewer::P1, None).unwrap().collect::<Vec<_>>();
    assert_eq!(
        events
            .iter()
            .map(|event| event.presentation.clone())
            .collect::<Vec<_>>(),
        vec![
            Some(first.events[0].id.clone()),
            Some(second.events[0].id.clone())
        ],
        "committed events refer to the occurrences they were shown as"
    );
}

fn steps(journal: &Journal, viewer: Viewer) -> Vec<PresentationOp> {
    journal
        .read_presentation(viewer, None)
        .unwrap()
        .map(|entry| entry.op.clone())
        .collect()
}

/// Return to `origin` the way an authority rollback does: the published prefix and the
/// presentation record survive, everything uncommitted after the origin is gone.
fn roll_back(journal: &mut Journal, origin: &Journal) {
    let published = journal.published();
    let mut target = origin.clone();
    target.restore(&published);
    *journal = target;
}

#[test]
fn a_fault_withdraws_exactly_the_discarded_scopes_and_never_reissues_them() {
    let mut journal = journal(&[]);
    let origin = journal.clone();
    for rule in ["first", "second"] {
        journal.push(
            [Ok(Some(result(rule))), Ok(Some(result(rule)))],
            None,
            &json!({"rule":rule}),
            format!("unit/{rule}"),
            None,
            true,
        );
    }
    journal.present(Viewer::P1, true, BTreeSet::new()).unwrap();
    roll_back(&mut journal, &origin);
    journal.settle_presentation(true);
    let withdrawn = steps(&journal, Viewer::P1);
    let [
        PresentationOp::Provisional(first),
        PresentationOp::Provisional(second),
        PresentationOp::Cancel {
            scope: cancelled_first,
        },
        PresentationOp::Cancel {
            scope: cancelled_second,
        },
    ] = withdrawn.as_slice()
    else {
        panic!("both shown scopes are withdrawn once, in order: {withdrawn:?}");
    };
    assert_eq!(
        (cancelled_first, cancelled_second),
        (&first.scope, &second.scope),
        "exactly the discarded scopes are cancelled"
    );
    assert_eq!(
        steps(&journal, Viewer::P2),
        vec![],
        "a viewer that was shown nothing has nothing to withdraw"
    );

    // The rolled-back journal issues the same batch numbers again for different results.
    journal.push(
        [Ok(Some(result("retry"))), Ok(Some(result("retry")))],
        None,
        &json!({"rule":"retry"}),
        "unit/retry".into(),
        None,
        true,
    );
    journal.present(Viewer::P1, true, BTreeSet::new()).unwrap();
    journal.settle_presentation(true);
    journal.commit();
    let after = steps(&journal, Viewer::P1);
    let [
        PresentationOp::Provisional(retry),
        PresentationOp::Promote { scope: promoted },
    ] = &after[4..]
    else {
        panic!("the new batch is shown under its own scope and promoted: {after:?}");
    };
    assert!(
        retry.scope != first.scope && retry.scope != second.scope && *promoted == retry.scope,
        "a withdrawn scope is never issued again or promoted"
    );
    assert_eq!(
        journal.read(Viewer::P1, None).unwrap().count(),
        1,
        "only the batch that really happened is history"
    );
}

#[test]
fn a_restoration_that_keeps_a_shown_batch_withdraws_and_promotes_nothing() {
    let mut journal = journal(&["kept"]);
    journal.present(Viewer::P1, true, BTreeSet::new()).unwrap();
    let stop = journal.clone();
    journal.push(
        [Ok(Some(result("lost"))), Ok(Some(result("lost")))],
        None,
        &json!({"rule":"lost"}),
        "unit/lost".into(),
        None,
        true,
    );
    roll_back(&mut journal, &stop);
    journal.settle_presentation(true);
    assert!(
        matches!(
            steps(&journal, Viewer::P1).as_slice(),
            [PresentationOp::Provisional(_)]
        ),
        "the shown batch still exists, so it is neither cancelled nor promoted"
    );
    journal.commit();
    assert!(
        matches!(
            steps(&journal, Viewer::P1).as_slice(),
            [
                PresentationOp::Provisional(_),
                PresentationOp::Promote { .. }
            ]
        ),
        "it is promoted when it really commits"
    );
}

#[test]
fn a_rules_rollback_of_a_shown_batch_stops_that_viewers_history() {
    let mut journal = journal(&[]);
    let origin = journal.clone();
    journal.push(
        [Ok(Some(result("shown"))), Ok(Some(result("shown")))],
        None,
        &json!({"rule":"shown"}),
        "unit/shown".into(),
        None,
        true,
    );
    journal.present(Viewer::P1, true, BTreeSet::new()).unwrap();
    roll_back(&mut journal, &origin);
    journal.settle_presentation(false);
    assert!(
        matches!(
            journal.read_presentation(Viewer::P1, None),
            Err(EventError::ProjectionGap)
        ),
        "a rules rollback is not a cancellation; the correspondence is simply lost"
    );
    assert_eq!(
        steps(&journal, Viewer::P2),
        vec![],
        "the other viewer is unaffected"
    );
}

/// A reveal naming one card by `handle`, visible to both viewers.
fn reveal(handle: &str) -> EventKind {
    EventKind::Action(ActionEvent {
        kind: ActionKind::Reveal,
        player: None,
        subjects: vec![Some(EventSubject::Card(Box::new(EventCard {
            handle: Some(ObjectHandle(handle.into())),
            properties: CardProperties {
                card: "unit".into(),
                face: 0,
                names: vec!["unit".into()],
                kind: "follower".into(),
                traits: Vec::new(),
                owner: Viewer::P1,
                controller: Viewer::P1,
                state: None,
            },
        })))],
        source: None,
        numbers: Vec::new(),
        name: None,
        face_up: Some(true),
        details: None,
    })
}

fn handles(names: &[&str]) -> BTreeSet<ObjectHandle> {
    names
        .iter()
        .map(|name| ObjectHandle((*name).to_owned()))
        .collect()
}

fn batches(journal: &Journal, viewer: Viewer) -> Vec<ProvisionalBatch> {
    steps(journal, viewer)
        .into_iter()
        .filter_map(|step| match step {
            PresentationOp::Provisional(batch) => Some(batch),
            PresentationOp::Promote { .. } | PresentationOp::Cancel { .. } => None,
        })
        .collect()
}

/// Three sequential batches: a card still on the board, a card that was revealed and
/// hidden again, and a result that names no card.
fn revealed_and_hidden() -> Journal {
    let mut journal = journal(&[]);
    for (name, kind) in [
        ("on-board", reveal("board")),
        ("hidden-again", reveal("hidden")),
        ("after", result("after")),
    ] {
        journal.push(
            [Ok(Some(kind.clone())), Ok(Some(kind))],
            None,
            &json!({"rule":name}),
            format!("unit/{name}"),
            None,
            true,
        );
    }
    journal
}

#[test]
fn a_card_seen_only_inside_the_uncommitted_window_stops_early_presentation_there() {
    let mut journal = revealed_and_hidden();
    journal
        .present(Viewer::P1, true, handles(&["board"]))
        .unwrap();
    assert_eq!(
        batches(&journal, Viewer::P1).len(),
        1,
        "the card revealed and hidden again is not on any board this viewer was handed, so \
         its batch and everything after it wait for commitment"
    );
    journal
        .present(Viewer::P1, true, handles(&["board"]))
        .unwrap();
    assert_eq!(
        batches(&journal, Viewer::P1).len(),
        1,
        "a later handoff cannot make the hidden card identifiable"
    );
    journal.commit();
    let history = journal.read(Viewer::P1, None).unwrap().collect::<Vec<_>>();
    assert_eq!(
        history
            .iter()
            .map(|event| event.presentation.is_some())
            .collect::<Vec<_>>(),
        vec![true, false, false],
        "the waiting batches commit as ordinary history"
    );
}

#[test]
fn a_card_on_the_previously_handed_board_stays_identifiable_after_it_leaves() {
    let mut journal = revealed_and_hidden();
    // An earlier handoff, before any result could be shown, already included this card.
    journal
        .present(Viewer::P1, false, handles(&["board", "hidden"]))
        .unwrap();
    assert_eq!(
        batches(&journal, Viewer::P1).len(),
        0,
        "nothing is shown outside a qualifying stop"
    );
    journal.present(Viewer::P1, true, handles(&[])).unwrap();
    assert_eq!(
        batches(&journal, Viewer::P1).len(),
        3,
        "cards the viewer held at its previous handoff are evidence enough"
    );
    journal.present(Viewer::P2, true, handles(&[])).unwrap();
    assert_eq!(
        batches(&journal, Viewer::P2).len(),
        0,
        "what one viewer held says nothing about the other"
    );
}
