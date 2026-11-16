use super::super::*;
use crate::game::player::IntegerSource;
use crate::ir::{Cardinality, Nat};
use sve_battle_contract::Nat as WireNat;

#[cfg(feature = "runner")]
mod native;

#[test]
fn another_native_integer_source_does_not_guess_a_wire_use() {
    assert!(matches!(
        project(&DecisionShape::Integer {
            bounds: Cardinality {
                minimum: Nat(0),
                maximum: Some(Nat(3)),
            },
            source: IntegerSource::Other,
        }),
        Err(ProjectionError::Unavailable)
    ));
}

#[test]
fn integer_domain_keeps_bounds_and_rejects_answers_outside_them() {
    let (usage, shape, binding) = project(&DecisionShape::Integer {
        bounds: Cardinality {
            minimum: Nat(2),
            maximum: Some(Nat(5)),
        },
        source: IntegerSource::XCost,
    })
    .unwrap();
    assert_eq!(usage, wire::QuestionUse::DeclareX);
    assert_eq!(
        shape,
        wire::QuestionShape::Integer {
            bounds: wire::Bounds {
                min: WireNat::from(2_u32),
                max: Some(WireNat::from(5_u32)),
            },
            unit: None,
            source: None,
        }
    );
    for value in [2_u32, 5_u32] {
        assert_eq!(
            binding.decode(&Answer::Integer {
                value: WireNat::from(value),
            }),
            Ok(player::Answer::Integer(Nat(u64::from(value))))
        );
    }
    for value in [1_u32, 6_u32] {
        assert_eq!(
            binding.decode(&Answer::Integer {
                value: WireNat::from(value),
            }),
            Err(AnswerError)
        );
    }
    assert_eq!(
        binding.decode(&Answer::Confirm { accept: true }),
        Err(AnswerError)
    );
}

#[test]
fn integer_keeps_an_unbounded_maximum_and_exact_wire_limit() {
    let (_, shape, binding) = project(&DecisionShape::Integer {
        bounds: Cardinality {
            minimum: Nat(WireNat::MAX.get()),
            maximum: None,
        },
        source: IntegerSource::XCost,
    })
    .unwrap();
    assert!(matches!(shape, wire::QuestionShape::Integer { bounds, .. }
        if bounds.min == WireNat::MAX && bounds.max.is_none()));
    assert_eq!(
        binding.decode(&Answer::Integer {
            value: WireNat::MAX
        }),
        Ok(player::Answer::Integer(Nat(WireNat::MAX.get())))
    );
    assert_eq!(
        binding.decode(&Answer::Integer {
            value: WireNat::ZERO
        }),
        Err(AnswerError)
    );
}

#[test]
fn integer_refuses_invalid_or_unencodable_bounds_without_clamping() {
    let unsafe_value = WireNat::MAX.get().checked_add(1).unwrap();
    for (bounds, expected) in [
        (
            Cardinality {
                minimum: Nat(3),
                maximum: Some(Nat(2)),
            },
            ProjectionError::Unavailable,
        ),
        (
            Cardinality {
                minimum: Nat(unsafe_value),
                maximum: None,
            },
            ProjectionError::Range,
        ),
        (
            Cardinality {
                minimum: Nat(0),
                maximum: Some(Nat(unsafe_value)),
            },
            ProjectionError::Range,
        ),
    ] {
        assert!(
            matches!(project(&DecisionShape::Integer { bounds, source: IntegerSource::XCost }), Err(actual) if actual == expected)
        );
    }
}

#[test]
fn dice_keeps_the_public_roll_and_only_offers_remaining_permissions() {
    for (remaining, allowed) in [
        (0_u64, vec![wire::DiceChoice::Keep]),
        (
            1_u64,
            vec![wire::DiceChoice::Keep, wire::DiceChoice::Reroll],
        ),
    ] {
        let (usage, shape, binding) = project(&DecisionShape::DiceReroll {
            value: Nat(6),
            remaining: Nat(remaining),
        })
        .unwrap();
        assert_eq!(usage, wire::QuestionUse::DiceReroll);
        assert_eq!(
            shape,
            wire::QuestionShape::DiceReroll {
                value: WireNat::from(6_u32),
                remaining: WireNat::try_from(remaining).unwrap(),
                allowed,
            }
        );
        assert_eq!(
            binding.decode(&Answer::DiceReroll {
                choice: wire::DiceChoice::Keep,
            }),
            Ok(player::Answer::Integer(Nat(0)))
        );
        let expected = if remaining == 0 {
            Err(AnswerError)
        } else {
            Ok(player::Answer::Integer(Nat(1)))
        };
        assert_eq!(
            binding.decode(&Answer::DiceReroll {
                choice: wire::DiceChoice::Reroll,
            }),
            expected
        );
        assert_eq!(
            binding.decode(&Answer::Integer {
                value: WireNat::ZERO
            }),
            Err(AnswerError)
        );
    }
}

#[test]
fn dice_refuses_every_unencodable_public_number() {
    let unsafe_value = WireNat::MAX.get().checked_add(1).unwrap();
    for (value, remaining) in [(unsafe_value, 1_u64), (6_u64, unsafe_value)] {
        assert!(matches!(
            project(&DecisionShape::DiceReroll {
                value: Nat(value),
                remaining: Nat(remaining)
            }),
            Err(ProjectionError::Range)
        ));
    }
}
