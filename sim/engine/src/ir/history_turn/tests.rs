use super::*;
use crate::ir::{CardKind, ObjectRef, ScopeId, TraitId};

#[test]
fn canonical_subject_queries_keep_their_destination_precedence() {
    let filter = SubjectFilter {
        own: Some(true),
        kind: Some(CardKind::Follower),
        ..SubjectFilter::default()
    };
    for expected in [
        SubjectHistoryKind::Cemetery,
        SubjectHistoryKind::Hand,
        SubjectHistoryKind::Leave,
        SubjectHistoryKind::Attack,
        SubjectHistoryKind::Evolve,
        SubjectHistoryKind::Destroy,
    ] {
        let query = subject_query(expected, 10, ScopeId(1), &filter);
        assert!(matches!(HistoryPlan::compile(&query),
            Some(HistoryPlan::Subject { event, filter: actual })
            if event == expected && actual == filter));
    }
}

#[test]
fn played_queries_keep_filtered_card_bindings_separate_from_player_counts() {
    let event = Binding::new(10, ScopeId(1));
    let card = Binding::<ObjectRef>::new(11, ScopeId(1));
    let all = played_query(event, None);
    assert!(matches!(
        HistoryPlan::compile(&all),
        Some(HistoryPlan::Played(PlayedBy::Any))
    ));
    let own = played_query(event, Some(PlayerRef::Controller {}));
    assert!(matches!(
        HistoryPlan::compile(&own),
        Some(HistoryPlan::Played(PlayedBy::Player(
            PlayerRef::Controller {}
        )))
    ));
    let filtered = PlayedHistoryFilter::Trait(TraitId("fixture".into())).query(event, card);
    assert!(matches!(HistoryPlan::compile(&filtered),
        Some(HistoryPlan::PlayedFiltered { card: binding, filter: PlayedHistoryFilter::Trait(id) })
        if binding == card && id.0 == "fixture"));
}

#[test]
fn subject_queries_reject_foreign_windows_and_execution_inclusion() {
    let query = subject_query(
        SubjectHistoryKind::Evolve,
        10,
        ScopeId(1),
        &SubjectFilter::default(),
    );
    assert!(matches!(
        HistoryPlan::compile(&query),
        Some(HistoryPlan::Subject {
            event: SubjectHistoryKind::Evolve,
            ..
        })
    ));
    let mut foreign = query.clone();
    foreign.window.relative_player = Some(PlayerRef::Controller {});
    assert!(HistoryPlan::compile(&foreign).is_none());
    let mut excluding = query;
    excluding.include_execution =
        ub_query(Binding::new(12, ScopeId(1)), None, true).include_execution;
    assert!(HistoryPlan::compile(&excluding).is_none());
}
