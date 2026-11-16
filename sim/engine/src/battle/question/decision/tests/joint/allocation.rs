use super::*;
use crate::ir::Nat;
use sve_battle_contract::{OptionId, Subject};

fn mixed(game: &Game, actor: Viewer, minimum: u64, total: u64) -> Question {
    let mut native = question(game, actor).unwrap();
    let Some(DecisionShape::Distribution(original)) = domain(&native) else {
        panic!("allocation");
    };
    let mut allocation = original.clone();
    allocation.members.push(player::SelectionMember {
        handle: 41,
        player: actor,
        card: None,
        name: None,
        object: None,
        rules_names: None,
        original_cost: None,
    });
    allocation.minimum = Nat(minimum);
    allocation.total = Nat(total);
    native.shape = player::QuestionShape::Decision(DecisionShape::Distribution(allocation));
    native
}

#[test]
fn every_recipient_including_a_zero_share_is_required_exactly_once() {
    let game = distribution(Viewer::P1);
    let native = mixed(&game, Viewer::P1, 0, 2);
    let (_, shape, binding) = project_question(
        &native,
        &components(&game, Viewer::P1),
        &QuestionId::new("mixed".into()),
    )
    .unwrap();
    let wire::QuestionShape::Distribute { recipients, .. } = shape else {
        panic!("allocation");
    };
    assert!(matches!(
        recipients.last().unwrap().subject,
        Some(Subject::Leader { .. })
    ));
    let a = Share {
        option: recipients.first().unwrap().id.clone(),
        amount: WireNat::from(2_u32),
    };
    let b = Share {
        option: recipients.last().unwrap().id.clone(),
        amount: WireNat::ZERO,
    };
    let Ok(player::Answer::Distribution(shares)) = binding.decode(&Answer::Distribute {
        shares: vec![b.clone(), a.clone()],
    }) else {
        panic!("complete allocation");
    };
    assert_eq!(
        shares
            .iter()
            .map(|share| (share.handle, share.amount.0))
            .collect::<Vec<_>>(),
        vec![(41, 0), (0, 2)]
    );
    for values in [
        vec![],
        vec![a.clone()],
        vec![a.clone(), a.clone()],
        vec![
            a.clone(),
            Share {
                option: OptionId::new("foreign".into()),
                ..b.clone()
            },
        ],
        vec![
            a.clone(),
            Share {
                amount: WireNat::from(1_u32),
                ..b.clone()
            },
        ],
    ] {
        assert_eq!(
            binding.decode(&Answer::Distribute { shares: values }),
            Err(AnswerError)
        );
    }
    assert_eq!(
        binding.decode(&Answer::Confirm { accept: true }),
        Err(AnswerError)
    );
    let (_, _, positive) = project_question(
        &mixed(&game, Viewer::P1, 1, 2),
        &components(&game, Viewer::P1),
        &QuestionId::new("mixed".into()),
    )
    .unwrap();
    assert_eq!(
        positive.decode(&Answer::Distribute { shares: vec![a, b] }),
        Err(AnswerError)
    );
}

#[test]
fn unsafe_infeasible_duplicate_or_unresolved_allocations_fail_as_a_whole() {
    let game = distribution(Viewer::P1);
    let board = components(&game, Viewer::P1);
    for native in [
        mixed(&game, Viewer::P1, 2, 3),
        mixed(&game, Viewer::P1, 0, WireNat::MAX.get() + 1),
        mixed(&game, Viewer::P1, WireNat::MAX.get() + 1, 2),
        mixed(&game, Viewer::P1, WireNat::MAX.get(), WireNat::MAX.get()),
    ] {
        assert!(project_question(&native, &board, &QuestionId::new("bad".into())).is_err());
    }
    let mut native = mixed(&game, Viewer::P1, 0, 2);
    let player::QuestionShape::Decision(DecisionShape::Distribution(allocation)) =
        &mut native.shape
    else {
        panic!("allocation");
    };
    allocation
        .members
        .push(allocation.members.first().unwrap().clone());
    assert!(matches!(
        project_question(&native, &board, &QuestionId::new("duplicate".into())),
        Err(ProjectionError::Unavailable)
    ));
    let mut absent = board;
    absent.objects.clear();
    assert!(matches!(
        project_question(
            &mixed(&game, Viewer::P1, 0, 2),
            &absent,
            &QuestionId::new("missing".into())
        ),
        Err(ProjectionError::Unavailable)
    ));
}

#[test]
fn empty_allocations_keep_the_native_bypass_of_the_total() {
    let game = distribution(Viewer::P1);
    let mut native = mixed(&game, Viewer::P1, 1, 2);
    let player::QuestionShape::Decision(DecisionShape::Distribution(allocation)) =
        &mut native.shape
    else {
        panic!("allocation");
    };
    allocation.members.clear();
    let (_, shape, binding) = project_question(
        &native,
        &components(&game, Viewer::P1),
        &QuestionId::new("empty".into()),
    )
    .unwrap();
    assert!(matches!(
        shape,
        wire::QuestionShape::Distribute {
            empty: wire::EmptyDistribution::AllowWithoutTotal,
            ..
        }
    ));
    assert_eq!(
        binding.decode(&Answer::Distribute { shares: vec![] }),
        Ok(player::Answer::Distribution(vec![]))
    );
}

#[test]
fn distribution_meaning_does_not_replace_the_native_purpose() {
    let game = distribution(Viewer::P1);
    let mut native = question(&game, Viewer::P1).unwrap();
    let board = components(&game, Viewer::P1);
    let id = QuestionId::new("purpose".into());
    let (_, expected, _) = project_question(&native, &board, &id).unwrap();
    for purpose in [
        player::QuestionPurpose::PlayParameter,
        player::QuestionPurpose::ResolutionChoice,
        player::QuestionPurpose::RuleChoice,
        player::QuestionPurpose::QuickWindow,
    ] {
        native.purpose = purpose;
        let (usage, shape, _) = project_question(&native, &board, &id).unwrap();
        assert_eq!(usage, wire::QuestionUse::DamageDistribution);
        assert_eq!(shape, expected);
    }
}

#[test]
fn each_share_advertises_the_reachable_maximum_after_other_recipients_minima() {
    let game = distribution(Viewer::P1);
    let board = components(&game, Viewer::P1);
    for (count, minimum, total, maximum) in [
        (2_usize, 2_u64, 5_u64, Some(3_u64)),
        (2, 2, 4, Some(2)),
        (2, 0, 5, Some(5)),
        (2, 0, 0, Some(0)),
        (1, 2, 5, Some(5)),
        (0, 2, 5, None),
        (
            2,
            0x000F_FFFF_FFFF_FFFF,
            0x001F_FFFF_FFFF_FFFF,
            Some(0x0010_0000_0000_0000),
        ),
    ] {
        let mut native = mixed(&game, Viewer::P1, minimum, total);
        let player::QuestionShape::Decision(DecisionShape::Distribution(allocation)) =
            &mut native.shape
        else {
            panic!("allocation");
        };
        allocation.members.truncate(count);
        let (_, shape, _) =
            project_question(&native, &board, &QuestionId::new("reachable".into())).unwrap();
        let wire::QuestionShape::Distribute { share, .. } = shape else {
            panic!("allocation");
        };
        assert_eq!(share.min, WireNat::try_from(minimum).unwrap());
        assert_eq!(
            share.max,
            maximum.map(|value| WireNat::try_from(value).unwrap()),
            "each upper bound leaves every other recipient its minimum"
        );
    }
}
