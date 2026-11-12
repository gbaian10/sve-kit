use super::GroupConstraints;
use crate::ir::{Cardinality, Generation, Nat, ObjectId, ObjectRef};

const fn card(id: u64) -> ObjectRef {
    ObjectRef {
        id: ObjectId(id),
        generation: Generation(1),
    }
}

const fn exact(count: u64) -> Cardinality {
    Cardinality {
        minimum: Nat(count),
        maximum: Some(Nat(count)),
    }
}

#[test]
fn witness_reaches_later_candidates_without_treating_a_prefix_as_payment() {
    let candidates = (1_u64..=40).map(card).collect::<Vec<_>>();
    let constraint = GroupConstraints {
        members: vec![
            candidates.iter().take(38).copied().collect(),
            vec![card(39)],
            vec![card(40)],
        ],
        bounds: vec![exact(1); 3],
    };
    assert!(!constraint.accepts(&candidates.iter().take(3).copied().collect::<Vec<_>>()));
    let answer = constraint.witness(&candidates, exact(3)).unwrap().unwrap();
    assert_eq!(answer.len(), 3);
    assert!(answer.contains(&card(39)) && answer.contains(&card(40)));
    assert!(constraint.accepts(&answer));
    assert_eq!(constraint.capacity().unwrap(), 3);
}

#[test]
fn overlapping_members_can_be_reassigned_to_complete_every_group() {
    let flexible = card(1);
    let narrow = card(2);
    let third = card(3);
    let constraint = GroupConstraints {
        members: vec![vec![flexible, narrow], vec![flexible], vec![third]],
        bounds: vec![exact(1); 3],
    };
    let answer = constraint
        .witness(&[flexible, narrow, third], exact(3))
        .unwrap()
        .unwrap();
    assert!(constraint.accepts(&answer));
    assert_eq!(answer.len(), 3);
    assert!(answer.contains(&narrow));
}

#[test]
fn a_member_cannot_supply_multiple_required_groups_or_duplicate_answers() {
    let constraint = GroupConstraints {
        members: vec![vec![card(1)], vec![card(1)], vec![card(2)]],
        bounds: vec![exact(1); 3],
    };
    assert!(!constraint.accepts(&[card(1), card(1), card(2)]));
    assert_eq!(constraint.capacity().unwrap(), 0);
    assert_eq!(
        constraint.witness(&[card(1), card(2)], exact(3)).unwrap(),
        None
    );
}

#[test]
fn empty_groups_preserve_required_and_optional_bounds() {
    let mut constraint = GroupConstraints {
        members: vec![Vec::new(); 3],
        bounds: vec![exact(1); 3],
    };
    assert_eq!(constraint.witness(&[], exact(3)).unwrap(), None);
    constraint.bounds = vec![
        Cardinality {
            minimum: Nat(0),
            maximum: Some(Nat(1))
        };
        3
    ];
    assert_eq!(constraint.capacity().unwrap(), 0);
    assert_eq!(
        constraint
            .witness(
                &[],
                Cardinality {
                    minimum: Nat(0),
                    maximum: Some(Nat(3))
                }
            )
            .unwrap(),
        Some(Vec::new())
    );
    assert!(constraint.accepts(&[]));
}

#[test]
fn single_group_witness_preserves_order_and_strict_minimum() {
    let constraint = GroupConstraints {
        members: vec![vec![card(2), card(1), card(3)]],
        bounds: vec![exact(2)],
    };
    assert_eq!(
        constraint
            .witness(&[card(2), card(1), card(3)], exact(2))
            .unwrap(),
        Some(vec![card(2), card(1)])
    );
    assert_eq!(constraint.witness(&[card(2)], exact(2)).unwrap(), None);
    assert!(!constraint.accepts(&[card(2)]));
    assert!(!constraint.accepts(&[card(2), card(99)]));
}

#[test]
fn joint_costs_reassign_broad_slots_without_a_group_or_candidate_cutoff() {
    let broad = GroupConstraints {
        members: vec![(1..=40).map(card).collect()],
        bounds: vec![exact(2)],
    };
    let mut proofs = vec![broad];
    for id in 1..=4 {
        proofs.push(GroupConstraints {
            members: vec![vec![card(id)]],
            bounds: vec![exact(1)],
        });
    }
    let answer = GroupConstraints::joint_witness(&proofs).unwrap().unwrap();
    assert_eq!(answer[0].len(), 2);
    assert!(
        answer[0]
            .iter()
            .all(|member| ![card(1), card(2), card(3), card(4)].contains(member))
    );
    for (proof, chosen) in proofs.iter().zip(&answer) {
        assert!(proof.accepts(chosen));
    }
    assert_eq!(
        GroupConstraints::joint_witness(&proofs).unwrap(),
        Some(answer)
    );
    proofs.push(GroupConstraints {
        members: vec![vec![card(1)]],
        bounds: vec![exact(1)],
    });
    assert_eq!(GroupConstraints::joint_witness(&proofs).unwrap(), None);
}
