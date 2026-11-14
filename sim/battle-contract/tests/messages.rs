//! Constructors and failure reports that fixtures do not reach.

extern crate alloc;

use alloc::collections::BTreeMap;
use sve_battle_contract::{
    HostError, HostErrorReason, HostMessage, ObjectHandle, OptionId, QuestionId, Receipt,
    ReceiptResult, Rejection, RequestId, decode, encode,
};

#[test]
fn host_errors_name_only_their_reason() {
    assert_eq!(HostError::FORMAT.to_string(), "host error: format");
    assert_eq!(HostError::RANGE.to_string(), "host error: range");
    assert_eq!(HostError::PROJECTION.to_string(), "host error: projection");
    assert_eq!(HostError::BUDGET.to_string(), "host error: budget");
    assert_eq!(HostError::new(HostErrorReason::Budget), HostError::BUDGET);
    assert_eq!(
        encode(&HostError::RANGE).as_deref(),
        Ok(r#"{"format":1,"reason":"range"}"#)
    );
}

#[test]
fn a_value_json_cannot_hold_is_a_projection_error() {
    let keyed_by_pair = BTreeMap::from([((1_u8, 2_u8), 3_u8)]);
    assert_eq!(encode(&keyed_by_pair), Err(HostError::PROJECTION));
}

#[test]
fn codes_travel_as_plain_strings() {
    let handle = ObjectHandle::new("o1".to_owned());
    assert_eq!(handle.as_str(), "o1");
    assert_eq!(encode(&handle).as_deref(), Ok("\"o1\""));
    assert_eq!(QuestionId::new("q1".to_owned()).as_str(), "q1");
    assert_eq!(OptionId::new("c1".to_owned()).as_str(), "c1");
    assert_eq!(RequestId::new("r1".to_owned()).as_str(), "r1");
}

#[test]
fn a_receipt_states_a_rejection_exactly_when_it_is_one() {
    let request = || RequestId::new("r1".to_owned());
    let rejected = Receipt::new(request(), ReceiptResult::Rejected);
    assert_eq!(rejected.rejection(), Some(Rejection::InvalidSubmission));
    assert_eq!(rejected.result(), ReceiptResult::Rejected);
    assert_eq!(rejected.request_id().as_str(), "r1");
    for result in [
        ReceiptResult::Applied,
        ReceiptResult::Reverted,
        ReceiptResult::Cancelled,
        ReceiptResult::EngineFault,
    ] {
        assert_eq!(Receipt::new(request(), result).rejection(), None);
    }

    for contradiction in [
        r#"{"kind":"receipt","request_id":"r1","result":"applied","rejection":"invalid_submission"}"#,
        r#"{"kind":"receipt","request_id":"r1","result":"rejected","rejection":null}"#,
    ] {
        assert_eq!(
            decode::<HostMessage>(contradiction),
            Err(HostError::FORMAT),
            "{contradiction}"
        );
    }
}
