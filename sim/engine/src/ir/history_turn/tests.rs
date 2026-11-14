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

fn played_subject_query(filter: &SubjectFilter) -> HistoryQuery {
    let event = Binding::new(20, ScopeId(1));
    let card = Binding::new(21, ScopeId(1));
    let mut query = played_query(event, None);
    query.pattern = EventPattern::Play {
        binding: event,
        predicate: Expr::Compute(Box::new(Predicate::PlayedCard {
            subject: Expr::Frozen(Box::new(super::super::PlaySubjectFields::PlaySubject {
                event,
            })),
            card,
            predicate: filter.predicate(SubjectFields::Played { event, card }),
        })),
    };
    query
}

#[test]
fn played_intersections_accept_saved_names_and_reject_unsupported_fields() {
    let filter = SubjectFilter {
        own: Some(true),
        kind: Some(CardKind::Follower),
        trait_id: Some(TraitId("fixture".into())),
        token: Some(true),
        name: Some(super::super::CardName("saved-name".into())),
        ..SubjectFilter::default()
    };
    assert!(matches!(
        HistoryPlan::compile(&played_subject_query(&filter)),
        Some(HistoryPlan::PlayedFiltered { .. })
    ));
    for invalid in [
        SubjectFilter {
            kind: Some(CardKind::EvolveFollower),
            ..filter.clone()
        },
        SubjectFilter {
            kind: Some(CardKind::Crest),
            ..filter.clone()
        },
        SubjectFilter {
            name: Some(super::super::CardName(String::new())),
            ..filter.clone()
        },
        SubjectFilter {
            class: Some(super::super::ClassId("fixture".into())),
            ..filter.clone()
        },
        SubjectFilter {
            original_cost: Some((super::super::Comparison::Equal, 1)),
            ..filter.clone()
        },
        SubjectFilter {
            keyword: Some(super::super::KeywordId::Ward),
            ..filter.clone()
        },
        SubjectFilter {
            itself: Some(true),
            ..filter.clone()
        },
        SubjectFilter {
            own_turn: true,
            ..filter.clone()
        },
        SubjectFilter {
            own: Some(false),
            ..filter
        },
    ] {
        assert!(HistoryPlan::compile(&played_subject_query(&invalid)).is_none());
    }
}
