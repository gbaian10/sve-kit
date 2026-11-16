use super::{ProjectionError, position};
use sve_battle_contract::MAX_SAFE_INTEGER;

#[test]
fn occurrence_source_slots_require_exact_zero_based_offsets() {
    assert_eq!(position(1).unwrap().get(), 0);
    assert_eq!(position(2).unwrap().get(), 1);
    assert_eq!(
        position(MAX_SAFE_INTEGER + 1).unwrap().get(),
        MAX_SAFE_INTEGER
    );
    for invalid in [0, MAX_SAFE_INTEGER + 2, u64::MAX] {
        assert_eq!(position(invalid), Err(ProjectionError::Unavailable));
    }
}
