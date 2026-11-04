use super::*;
use crate::ir::events::played::PlayedFilter;
use crate::ir::events::subject::{SubjectFilter, SubjectInformation};
use crate::ir::{Binding, CardKind, Comparison, ObjectRef, ScopeId};
use serde_json::{Value, json};

#[test]
fn unknown_printed_values_do_not_satisfy_zero_or_upper_bounds() {
    for value in [
        Value::Null,
        json!("-"),
        json!("unknown"),
        json!(-1_i64),
        json!(1.5_f64),
    ] {
        for op in [Comparison::Equal, Comparison::LessEqual] {
            let plan = PlayedFilter {
                subject: SubjectFilter {
                    kind: Some(CardKind::Spell),
                    ..SubjectFilter::default()
                },
                card: Binding::<ObjectRef>::new(1, ScopeId(1)),
                original: Some((op, 0)),
                traits_any: vec![],
            };
            let info = SubjectInformation {
                original_cost: known_original_cost(&value),
                keywords: &[],
                kind: Some(CardKind::Spell),
                class: None,
                traits: &[],
                names: &[],
            };
            assert!(plan.valid());
            assert!(!plan.matches_information(
                &info,
                false,
                PlayerId { seat: 1 },
                PlayerId { seat: 1 },
                PlayerId { seat: 1 },
                false
            ));
        }
    }
    for value in [json!("0"), json!(0_u8)] {
        assert_eq!(known_original_cost(&value), Some(0_i64));
    }
    let missing = json!({});
    assert_eq!(missing.get("cost").and_then(known_original_cost), None);
}
