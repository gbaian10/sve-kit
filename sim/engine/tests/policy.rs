//! Pure policy tests import public player data, never Game or internal authority.

use sve_engine::game::player::{
    CardOption, DamageShare, DistributionQuestion, SearchAnswer, SearchQuestion, SelectionGroup,
    SelectionMember, SelectionQuestion, SelectionRole, TargetRoleAssignment, Viewer,
};
use sve_engine::ir::{Cardinality, Nat, RoleCardinality};
use sve_engine::policy::{self, PolicyFailure};

const fn bounds(minimum: u64, maximum: u64) -> Cardinality {
    Cardinality {
        minimum: Nat(minimum),
        maximum: Some(Nat(maximum)),
    }
}

fn member(handle: u64, names: &[&str], cost: i64) -> SelectionMember {
    SelectionMember {
        handle,
        player: Viewer::P1,
        card: None,
        name: None,
        object: None,
        rules_names: Some(names.iter().map(|name| (*name).into()).collect()),
        original_cost: Some(cost),
    }
}

fn question(members: Vec<SelectionMember>, cardinality: Cardinality) -> SelectionQuestion {
    SelectionQuestion {
        original_costs: Some(
            members
                .iter()
                .map(|member| (member.handle, member.original_cost.unwrap_or_default()))
                .collect(),
        ),
        groups: vec![SelectionGroup {
            handles: members.iter().map(|member| member.handle).collect(),
            cardinality,
        }],
        members,
        cardinality,
        distinct_names: false,
        distinct_original_costs: false,
        roles: Vec::new(),
        minimum_original_cost: None,
        maximum_original_cost: None,
    }
}

fn searching(selection: &SelectionQuestion, grouped: bool) -> SearchQuestion {
    SearchQuestion {
        cards: selection
            .members
            .iter()
            .map(|member| CardOption {
                handle: member.handle,
                card: String::new(),
                name: String::new(),
                object: None,
                rules_names: member.rules_names.clone(),
                original_cost: member.original_cost,
            })
            .collect(),
        cardinality: selection.cardinality,
        groups: grouped.then(|| selection.groups.clone()),
        distinct_names: selection.distinct_names,
        maximum_original_cost: selection.maximum_original_cost,
    }
}

#[test]
fn sparse_coordinates_preserve_candidate_order_and_question() {
    let question = question(
        vec![member(99, &["a"], 1), member(4, &["b"], 2)],
        bounds(0, 2),
    );
    let before = question.clone();
    let answer = policy::select(&question).unwrap();
    assert_eq!(answer.targets, vec![99]);
    assert_eq!(answer.roles, Vec::<TargetRoleAssignment>::new());
    assert_eq!(policy::select(&question).unwrap(), answer);
    assert_eq!(question, before);
}

#[test]
fn full_rules_names_reject_alias_overlap_without_display_name_fallback() {
    let mut question = question(
        vec![
            member(1, &["a", "alias"], 1),
            member(2, &["b", "alias"], 2),
            member(3, &["c"], 3),
        ],
        bounds(2, 2),
    );
    question.distinct_names = true;
    assert_eq!(policy::select(&question).unwrap().targets, vec![1, 3]);
    question.members.truncate(2);
    question.groups[0].handles = vec![1, 2];
    assert_eq!(policy::select(&question), Err(PolicyFailure::NoSolution));
    question.members[0].rules_names = None;
    question.members[0].name = Some("display".into());
    assert_eq!(
        policy::select(&question),
        Err(PolicyFailure::InvalidQuestion)
    );
}

#[test]
fn cost_map_and_aggregate_bounds_do_not_imply_distinct_costs() {
    let mut question = question(
        vec![member(8, &["a"], 2), member(9, &["b"], 2)],
        bounds(2, 2),
    );
    question.minimum_original_cost = Some(4);
    question.maximum_original_cost = Some(4);
    assert_eq!(policy::select(&question).unwrap().targets, vec![8, 9]);
    question.distinct_original_costs = true;
    assert_eq!(policy::select(&question), Err(PolicyFailure::NoSolution));
}

#[test]
fn cost_floor_requires_larger_selection_and_cost_cap_changes_the_first_answer() {
    let mut question = question(
        vec![
            member(7, &["a"], 1),
            member(8, &["b"], 2),
            member(9, &["c"], 3),
        ],
        bounds(1, 3),
    );
    question.minimum_original_cost = Some(5);
    assert_eq!(policy::select(&question).unwrap().targets, vec![8, 9]);
    question.minimum_original_cost = None;
    question.cardinality = bounds(2, 3);
    question.maximum_original_cost = Some(3);
    assert_eq!(policy::select(&question).unwrap().targets, vec![7, 8]);
}

#[test]
fn negative_costs_do_not_make_aggregate_pruning_unsound() {
    let mut question = question(
        vec![member(1, &["a"], 9), member(2, &["b"], -8)],
        bounds(2, 2),
    );
    question.maximum_original_cost = Some(1);
    assert_eq!(policy::select(&question).unwrap().targets, vec![1, 2]);
}

#[test]
fn overflowing_branch_does_not_hide_a_later_valid_answer() {
    let mut question = question(
        vec![
            member(1, &["a"], i64::MAX),
            member(2, &["b"], 1),
            member(3, &["c"], -1),
        ],
        bounds(2, 2),
    );
    question.maximum_original_cost = Some(i64::MAX);
    assert_eq!(policy::select(&question).unwrap().targets, vec![1, 3]);
    question.members.truncate(2);
    question.groups[0].handles = vec![1, 2];
    assert_eq!(policy::select(&question), Err(PolicyFailure::NoSolution));
}

#[test]
fn overlapping_group_eligibility_requires_backtracking_and_all_rows() {
    let mut selection = question(
        vec![member(70, &["a"], 1), member(80, &["b"], 2)],
        bounds(2, 2),
    );
    selection.groups = vec![
        SelectionGroup {
            handles: vec![70, 80],
            cardinality: bounds(1, 1),
        },
        SelectionGroup {
            handles: vec![70],
            cardinality: bounds(1, 1),
        },
        SelectionGroup {
            handles: Vec::new(),
            cardinality: bounds(0, 0),
        },
    ];
    assert_eq!(policy::select(&selection).unwrap().targets, vec![70, 80]);
    assert_eq!(
        policy::search(&searching(&selection, true)).unwrap(),
        SearchAnswer::Grouped(vec![vec![80], vec![70], vec![]])
    );
    assert_eq!(
        policy::search(&searching(&selection, false)).unwrap(),
        SearchAnswer::Ungrouped(vec![70, 80])
    );
}

#[test]
fn empty_search_keeps_group_shape() {
    let selection = question(Vec::new(), bounds(0, 0));
    assert_eq!(
        policy::search(&searching(&selection, true)).unwrap(),
        SearchAnswer::Grouped(vec![vec![]])
    );
    assert_eq!(
        policy::search(&searching(&selection, false)).unwrap(),
        SearchAnswer::Ungrouped(vec![])
    );
    let mut invalid = searching(&selection, true);
    invalid.groups = Some(Vec::new());
    assert_eq!(
        policy::search(&invalid),
        Err(PolicyFailure::InvalidQuestion)
    );
}

const fn role(role: u64, cardinality: RoleCardinality) -> SelectionRole {
    SelectionRole {
        role: Nat(role),
        position: None,
        cardinality,
    }
}

#[test]
fn roles_partition_every_target_independently_of_groups() {
    let mut question = question(
        vec![
            member(1, &["a"], 1),
            member(2, &["b"], 2),
            member(3, &["c"], 3),
        ],
        bounds(3, 3),
    );
    question.roles = vec![
        role(
            8,
            RoleCardinality::Fixed {
                cardinality: bounds(1, 1),
            },
        ),
        role(2, RoleCardinality::IfNonempty { count: Nat(1) }),
        role(9, RoleCardinality::Remainder {}),
    ];
    let answer = policy::select(&question).unwrap();
    assert_eq!(answer.targets, vec![1, 2, 3]);
    assert_eq!(
        answer
            .roles
            .iter()
            .map(|role| (role.role.0, role.targets.clone()))
            .collect::<Vec<_>>(),
        vec![(8, vec![1]), (2, vec![2]), (9, vec![3])]
    );
}

#[test]
fn fixed_roles_absorb_flexible_counts_and_preserve_empty_roles() {
    let mut question = question(
        vec![member(1, &["a"], 1), member(2, &["b"], 2)],
        bounds(2, 2),
    );
    question.roles = vec![
        role(
            7,
            RoleCardinality::Fixed {
                cardinality: bounds(0, 2),
            },
        ),
        role(
            6,
            RoleCardinality::Fixed {
                cardinality: bounds(0, 0),
            },
        ),
    ];
    let answer = policy::select(&question).unwrap();
    assert_eq!(answer.roles[0].targets, vec![1, 2]);
    assert_eq!(answer.roles[1].targets, Vec::<u64>::new());
    question.roles[0].cardinality = RoleCardinality::Fixed {
        cardinality: bounds(0, 1),
    };
    assert_eq!(policy::select(&question), Err(PolicyFailure::NoSolution));
}

#[test]
fn empty_if_nonempty_role_has_an_explicit_empty_assignment() {
    let mut question = question(Vec::new(), bounds(0, 0));
    question.roles = vec![role(42, RoleCardinality::IfNonempty { count: Nat(1) })];
    let answer = policy::select(&question).unwrap();
    assert_eq!(answer.targets, Vec::<u64>::new());
    assert_eq!(answer.roles.len(), 1);
    assert_eq!(answer.roles[0].role, Nat(42));
    assert_eq!(answer.roles[0].targets, Vec::<u64>::new());
}

#[test]
fn malformed_roles_and_metadata_are_not_reported_as_infeasible() {
    let mut question = question(vec![member(1, &["a"], 1)], bounds(1, 1));
    question.roles = vec![role(1, RoleCardinality::IfNonempty { count: Nat(2) })];
    assert_eq!(
        policy::select(&question),
        Err(PolicyFailure::InvalidQuestion)
    );
    question.roles = vec![
        role(1, RoleCardinality::Remainder {}),
        role(2, RoleCardinality::Remainder {}),
    ];
    assert_eq!(
        policy::select(&question),
        Err(PolicyFailure::InvalidQuestion)
    );
    question.roles[1] = role(
        1,
        RoleCardinality::Fixed {
            cardinality: bounds(0, 1),
        },
    );
    assert_eq!(
        policy::select(&question),
        Err(PolicyFailure::InvalidQuestion)
    );
    question.roles.clear();
    question.minimum_original_cost = Some(1);
    question.original_costs = None;
    assert_eq!(
        policy::select(&question),
        Err(PolicyFailure::InvalidQuestion)
    );
    question.minimum_original_cost = None;
    question.cardinality = bounds(2, 1);
    assert_eq!(
        policy::select(&question),
        Err(PolicyFailure::InvalidQuestion)
    );
}

#[test]
fn malformed_domains_and_missing_search_costs_are_rejected() {
    let mut selection = question(
        vec![member(1, &["a"], 1), member(1, &["b"], 2)],
        bounds(1, 2),
    );
    assert_eq!(
        policy::select(&selection),
        Err(PolicyFailure::InvalidQuestion)
    );
    selection.members.pop();
    selection.groups[0].handles = vec![99];
    assert_eq!(
        policy::select(&selection),
        Err(PolicyFailure::InvalidQuestion)
    );
    selection.groups[0].handles = vec![1];
    let mut search = searching(&selection, false);
    search.maximum_original_cost = Some(2);
    search.cards[0].original_cost = None;
    assert_eq!(policy::search(&search), Err(PolicyFailure::InvalidQuestion));
}

#[test]
fn large_domains_are_not_truncated_to_sixteen_or_the_first_ten() {
    let members = (0..40)
        .map(|index| member(index, &["same"], i64::try_from(index).unwrap()))
        .collect();
    let mut question = question(members, bounds(1, 1));
    question.minimum_original_cost = Some(39);
    assert_eq!(policy::select(&question).unwrap().targets, vec![39]);
    question.distinct_names = true;
    question.minimum_original_cost = None;
    question.cardinality = bounds(20, 40);
    assert_eq!(policy::select(&question), Err(PolicyFailure::NoSolution));
}

#[test]
fn hard_group_search_reports_budget_exhaustion_without_claiming_no_solution() {
    let members = (0..40).map(|index| member(index, &["a"], 0)).collect();
    let mut question = question(members, bounds(40, 40));
    question.groups = vec![
        SelectionGroup {
            handles: (0..20).collect(),
            cardinality: bounds(15, 15),
        },
        SelectionGroup {
            handles: (0..20).collect(),
            cardinality: bounds(15, 15),
        },
        SelectionGroup {
            handles: (20..40).collect(),
            cardinality: bounds(10, 10),
        },
    ];
    assert_eq!(
        policy::select(&question),
        Err(PolicyFailure::SearchExhausted)
    );
}

#[test]
fn group_search_skips_candidates_without_any_group_eligibility() {
    let members = (0..40).map(|index| member(index, &["a"], 0)).collect();
    let mut question = question(members, bounds(1, 40));
    question.groups = vec![SelectionGroup {
        handles: vec![39],
        cardinality: bounds(1, 1),
    }];
    assert_eq!(policy::select(&question).unwrap().targets, vec![39]);
    question.cardinality = bounds(40, 40);
    question.groups[0].cardinality = bounds(0, 40);
    assert_eq!(policy::select(&question), Err(PolicyFailure::NoSolution));
}

#[test]
fn distribution_is_complete_and_obeys_empty_membership_exception() {
    let mut question = DistributionQuestion {
        members: vec![member(9, &[], 0), member(2, &[], 0)],
        total: Nat(7),
        minimum: Nat(2),
    };
    let answer = policy::distribute(&question).unwrap();
    assert_eq!(
        answer
            .iter()
            .map(|share| (share.handle, share.amount.0))
            .collect::<Vec<_>>(),
        vec![(9, 5), (2, 2)]
    );
    question.total = Nat(3);
    assert_eq!(
        policy::distribute(&question),
        Err(PolicyFailure::NoSolution)
    );
    question.total = Nat(u64::MAX);
    question.minimum = Nat(u64::MAX);
    assert_eq!(
        policy::distribute(&question),
        Err(PolicyFailure::NoSolution)
    );
    question.members.clear();
    assert_eq!(
        policy::distribute(&question).unwrap(),
        Vec::<DamageShare>::new()
    );
    question.members = vec![member(1, &[], 0), member(1, &[], 0)];
    assert_eq!(
        policy::distribute(&question),
        Err(PolicyFailure::InvalidQuestion)
    );
}
