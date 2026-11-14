use super::*;
use crate::game::player::events::PresentationOp;
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
    journal.present(Viewer::P1).unwrap();
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
    journal.present(Viewer::P2).unwrap();
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
    journal.present(Viewer::P1).unwrap();
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
