//! Selection projection preserves frozen constraints and opaque answer coordinates.

extern crate alloc;

use alloc::collections::BTreeMap;
use core::num::NonZeroU64;
use sve_battle_contract::question::{Distinct, GroupAssignment, RoleSize};
use sve_battle_contract::{
    Answer, GroupChoice, HostError, Nat, QuestionId, RoleChoice, Seat, Subject,
};
use sve_engine::battle::{CandidateDetails, ProjectionError, SelectionProjection};
use sve_engine::game::player::{
    self, CardOption, SearchAnswer, SearchQuestion, SelectionGroup, SelectionMember,
    SelectionQuestion, SelectionRole, Viewer,
};
use sve_engine::ir::{self, Cardinality, RoleCardinality};

fn bounds(minimum: u64, maximum: Option<u64>) -> Cardinality {
    Cardinality {
        minimum: ir::Nat(minimum),
        maximum: maximum.map(ir::Nat),
    }
}

const fn details() -> CandidateDetails {
    CandidateDetails {
        subject: Some(Subject::Leader { player: Seat::P1 }),
        controller: Some(Seat::P1),
        origin: None,
        classes: None,
    }
}

fn question() -> SelectionQuestion {
    SelectionQuestion {
        members: [0xFFFF_FFFF_FFFF_FFFA, 583]
            .into_iter()
            .map(|handle| SelectionMember {
                handle,
                player: Viewer::P1,
                card: None,
                name: None,
                object: None,
                rules_names: Some(vec!["rule-name".into(), "shared-name".into()]),
                original_cost: Some(3),
            })
            .collect(),
        cardinality: bounds(0, None),
        distinct_names: true,
        original_costs: None,
        distinct_original_costs: true,
        groups: vec![],
        roles: vec![
            SelectionRole {
                position: None,
                role: ir::Nat(7),
                cardinality: RoleCardinality::Fixed {
                    cardinality: bounds(0, Some(1)),
                },
            },
            SelectionRole {
                position: None,
                role: ir::Nat(9),
                cardinality: RoleCardinality::IfNonempty { count: ir::Nat(1) },
            },
            SelectionRole {
                position: None,
                role: ir::Nat(12),
                cardinality: RoleCardinality::Remainder {},
            },
        ],
        minimum_original_cost: Some(1),
        maximum_original_cost: Some(8),
    }
}

#[test]
fn selection_retains_complete_constraints_without_native_coordinates() {
    let question = question();
    let projection = SelectionProjection::new(
        &QuestionId::new("q1".into()),
        &question,
        &BTreeMap::new(),
        |_| Ok(details()),
    )
    .unwrap();
    let selection = projection.selection();
    assert_eq!(selection.bounds.min, Nat::ZERO);
    assert_eq!(selection.bounds.max, None);
    assert_eq!(
        selection.distinct,
        vec![Distinct::RulesName, Distinct::OriginalCost]
    );
    assert_eq!(selection.original_cost_total.min.unwrap().get(), 1);
    assert_eq!(selection.original_cost_total.max.unwrap().get(), 8);
    assert_eq!(
        selection.candidates.first().unwrap().rules_names,
        question.members.first().unwrap().rules_names
    );
    assert_eq!(
        selection
            .candidates
            .first()
            .unwrap()
            .original_cost
            .unwrap()
            .get(),
        3
    );
    let [fixed, conditional, remainder] = selection.roles.as_slice() else {
        panic!("expected three projected roles")
    };
    assert!(
        matches!(fixed.size, RoleSize::Fixed { bounds } if bounds.min == Nat::ZERO && bounds.max == Some(Nat::from(1_u32)))
    );
    assert!(
        matches!(conditional.size, RoleSize::IfNonempty { count } if count == Nat::from(1_u32))
    );
    assert_eq!(remainder.size, RoleSize::Remainder);
    let wire = serde_json::to_string(selection).unwrap();
    // Numeric handles are authority coordinates, even when they exceed JavaScript's range.
    assert!(!wire.contains("18446744073709551610"));
    assert!(!wire.contains("583"));
    let debug = format!("{projection:?}");
    assert!(!debug.contains("18446744073709551610"));
}

#[test]
fn target_roles_roundtrip_and_reject_foreign_or_overlapping_assignments() {
    let question = question();
    let id = QuestionId::new("roles".into());
    let projection =
        SelectionProjection::new(&id, &question, &BTreeMap::new(), |_| Ok(details())).unwrap();
    let selected = projection
        .selection()
        .candidates
        .iter()
        .map(|candidate| candidate.id.clone())
        .collect::<Vec<_>>();
    let mut answer = Answer::Select {
        selected: selected.clone(),
        groups: None,
        roles: vec![
            RoleChoice {
                role: Nat::from(7_u32),
                selected: vec![],
            },
            RoleChoice {
                role: Nat::from(9_u32),
                selected: selected.iter().take(1).cloned().collect(),
            },
            RoleChoice {
                role: Nat::from(12_u32),
                selected: selected.iter().skip(1).cloned().collect(),
            },
        ],
    };
    let native = projection.targets(&answer).unwrap();
    let player::Answer::TargetsWithRoles { targets, roles } = native else {
        panic!()
    };
    assert_eq!(
        targets,
        question
            .members
            .iter()
            .map(|member| member.handle)
            .collect::<Vec<_>>()
    );
    let [empty, fixed, _] = roles.as_slice() else {
        panic!("expected three decoded roles")
    };
    assert_eq!(empty.targets, Vec::<u64>::new());
    assert_eq!(
        fixed.targets,
        targets.iter().take(1).copied().collect::<Vec<_>>()
    );
    let other = SelectionProjection::new(
        &QuestionId::new("other".into()),
        &question,
        &BTreeMap::new(),
        |_| Ok(details()),
    )
    .unwrap();
    assert_eq!(
        other.targets(&answer).unwrap_err(),
        ProjectionError::Unavailable
    );
    if let Answer::Select {
        roles: wire_roles, ..
    } = &mut answer
    {
        wire_roles.last_mut().unwrap().selected = selected;
    }
    assert_eq!(
        projection.targets(&answer).unwrap_err(),
        ProjectionError::Unavailable
    );
}

#[test]
fn search_preserves_empty_groups_and_printed_order() {
    let question = SearchQuestion {
        cards: vec![CardOption {
            handle: 931,
            card: "native-card-key".into(),
            name: "display".into(),
            object: None,
            rules_names: Some(vec!["rule-name".into(), "alias".into()]),
            original_cost: Some(4),
        }],
        cardinality: bounds(0, Some(1)),
        groups: Some(vec![
            SelectionGroup {
                handles: vec![931],
                cardinality: bounds(0, Some(1)),
            },
            SelectionGroup {
                handles: vec![],
                cardinality: bounds(0, Some(0)),
            },
        ]),
        distinct_names: true,
        maximum_original_cost: Some(5),
    };
    let projection =
        SelectionProjection::search(&QuestionId::new("search".into()), &question, |_| {
            Ok(details())
        })
        .unwrap();
    assert_eq!(
        projection.selection().group_assignment,
        GroupAssignment::Explicit
    );
    assert_eq!(projection.selection().groups.last().unwrap().eligible, []);
    let selected = vec![
        projection
            .selection()
            .candidates
            .first()
            .unwrap()
            .id
            .clone(),
    ];
    let mut answer = Answer::Select {
        selected: selected.clone(),
        roles: vec![],
        groups: Some(vec![
            GroupChoice {
                group: Nat::from(1_u32),
                selected: vec![],
            },
            GroupChoice {
                group: Nat::ZERO,
                selected,
            },
        ]),
    };
    assert_eq!(
        projection.search_answer(&answer).unwrap(),
        SearchAnswer::Grouped(vec![vec![931], vec![]])
    );
    if let Answer::Select {
        groups: Some(groups),
        ..
    } = &mut answer
    {
        groups.remove(0);
    }
    assert_eq!(
        projection.search_answer(&answer).unwrap_err(),
        ProjectionError::Unavailable
    );
}

#[test]
fn required_facts_and_out_of_range_numbers_stop_projection() {
    let id = QuestionId::new("q".into());
    let mut question = question();
    question.members.first_mut().unwrap().rules_names = None;
    assert_eq!(
        SelectionProjection::new(&id, &question, &BTreeMap::new(), |_| Ok(details())).unwrap_err(),
        ProjectionError::Unavailable
    );
    question.distinct_names = false;
    question.members.first_mut().unwrap().original_cost = None;
    assert_eq!(
        SelectionProjection::new(&id, &question, &BTreeMap::new(), |_| Ok(details())).unwrap_err(),
        ProjectionError::Unavailable
    );
    question.distinct_original_costs = false;
    question.minimum_original_cost = None;
    question.maximum_original_cost = None;
    question.cardinality.maximum = Some(ir::Nat(u64::MAX));
    assert_eq!(
        SelectionProjection::new(&id, &question, &BTreeMap::new(), |_| Ok(details()))
            .unwrap_err()
            .host_error(),
        HostError::RANGE,
    );
    question.cardinality.maximum = Some(ir::Nat(5));
    question.members.first_mut().unwrap().original_cost = Some(i64::MAX);
    assert_eq!(
        SelectionProjection::new(&id, &question, &BTreeMap::new(), |_| Ok(details())).unwrap_err(),
        ProjectionError::Range
    );
}

#[test]
fn contradictory_domains_and_incomplete_destinations_are_refused() {
    let id = QuestionId::new("invalid-domain".into());
    let check = |question: &SelectionQuestion| {
        assert_eq!(
            SelectionProjection::new(&id, question, &BTreeMap::new(), |_| Ok(details()))
                .unwrap_err(),
            ProjectionError::Unavailable
        );
    };
    let mut duplicate_candidate = question();
    duplicate_candidate
        .members
        .push(duplicate_candidate.members.first().unwrap().clone());
    check(&duplicate_candidate);
    let mut unknown_group = question();
    unknown_group.groups.push(SelectionGroup {
        handles: vec![888],
        cardinality: bounds(0, Some(1)),
    });
    check(&unknown_group);
    let mut duplicate_role = question();
    duplicate_role
        .roles
        .push(*duplicate_role.roles.first().unwrap());
    check(&duplicate_role);
    let mut contradictory_cost = question();
    contradictory_cost.original_costs = Some(
        contradictory_cost
            .members
            .iter()
            .map(|member| (member.handle, 2))
            .collect(),
    );
    check(&contradictory_cost);
    let mut incomplete_cost = question();
    incomplete_cost.original_costs = Some(BTreeMap::new());
    check(&incomplete_cost);
    let mut invalid_range = question();
    invalid_range.cardinality = bounds(2, Some(1));
    check(&invalid_range);
    let mut invalid_total = question();
    invalid_total.minimum_original_cost = Some(9);
    check(&invalid_total);
    let mut missing_destination = question();
    missing_destination.roles.first_mut().unwrap().position = Some(ir::Position::Bottom {});
    check(&missing_destination);
    let mut unsupported_destination = question();
    unsupported_destination.roles.first_mut().unwrap().position = Some(ir::Position::Index {
        index: ir::PositiveNat(NonZeroU64::new(2).unwrap()),
    });
    check(&unsupported_destination);
    let wrong_controller = |_| {
        let mut candidate = details();
        candidate.controller = Some(Seat::P2);
        Ok(candidate)
    };
    assert_eq!(
        SelectionProjection::new(&id, &question(), &BTreeMap::new(), wrong_controller).unwrap_err(),
        ProjectionError::Unavailable
    );
}

#[test]
fn authorized_role_destinations_and_existence_groups_stay_attached() {
    use sve_battle_contract::Zone;
    use sve_battle_contract::question::{Destination, Edge};

    let mut question = question();
    question.roles.first_mut().unwrap().position = Some(ir::Position::Bottom {});
    let destination = Destination {
        player: Seat::P1,
        zone: Zone::Deck,
        position: Some(Edge::Bottom),
    };
    let destinations = BTreeMap::from([(7, destination)]);
    let handles = question
        .members
        .iter()
        .map(|member| member.handle)
        .collect::<Vec<_>>();
    question.groups.push(SelectionGroup {
        handles,
        cardinality: bounds(0, Some(2)),
    });
    let projection = SelectionProjection::new(
        &QuestionId::new("destinations".into()),
        &question,
        &destinations,
        |_| Ok(details()),
    )
    .unwrap();
    let unexpected_edge = BTreeMap::from([(9, destination)]);
    let mut unordered_role = question.clone();
    unordered_role.roles.first_mut().unwrap().position = None;
    assert_eq!(
        SelectionProjection::new(
            &QuestionId::new("edge".into()),
            &unordered_role,
            &unexpected_edge,
            |_| Ok(details())
        )
        .unwrap_err(),
        ProjectionError::Unavailable
    );
    assert_eq!(
        projection.selection().roles.first().unwrap().destination,
        Some(destination)
    );
    assert_eq!(
        projection.selection().group_assignment,
        GroupAssignment::Existence
    );
    assert_eq!(
        projection.selection().groups.first().unwrap().eligible,
        projection
            .selection()
            .candidates
            .iter()
            .map(|candidate| candidate.id.clone())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        ProjectionError::Unavailable.host_error(),
        HostError::PROJECTION
    );
}

#[test]
fn selection_decoders_refuse_the_other_native_answer_channel() {
    let targets = SelectionProjection::new(
        &QuestionId::new("targets".into()),
        &question(),
        &BTreeMap::new(),
        |_| Ok(details()),
    )
    .unwrap();
    let empty = Answer::Select {
        selected: vec![],
        groups: None,
        roles: vec![],
    };
    assert_eq!(
        targets.search_answer(&empty).unwrap_err(),
        ProjectionError::Unavailable
    );
    let search = SearchQuestion {
        cards: vec![],
        cardinality: bounds(0, Some(0)),
        groups: Some(vec![SelectionGroup {
            handles: vec![],
            cardinality: bounds(0, Some(0)),
        }]),
        distinct_names: false,
        maximum_original_cost: None,
    };
    let projection = SelectionProjection::search(
        &QuestionId::new("search".into()),
        &search,
        |_| Ok(details()),
    )
    .unwrap();
    assert_eq!(
        projection.targets(&empty).unwrap_err(),
        ProjectionError::Unavailable
    );
}
