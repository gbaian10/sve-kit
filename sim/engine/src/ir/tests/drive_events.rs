use super::*;

fn tree(on: &Value, effect: &Value) -> Value {
    let mut tree = expanded::tree().clone();
    let mut seed = tree["cards"]["BP08-038"].clone();
    let mut entry = seed["abilities"][0].clone();
    entry.as_object_mut().unwrap().remove("fanfare");
    entry["auto"] = json!({"on":on,"do":[effect]});
    seed["abilities"] = json!([entry]);
    tree["cards"] = json!({"synthetic":seed});
    tree
}

#[test]
fn drive_headers_distinguish_completed_checks_and_performed_triggers() {
    let checked =
        json!({"drive_checked":{"holder":{"who":"self","zone":"field","kind":"follower"}}});
    lowerer()
        .lower(&tree(&checked, &json!({"buff":{"on":"it","power":1_u8}})))
        .unwrap();
    for on in [
        json!({"drive_trigger":{"who":"self"}}),
        json!({"drive_trigger":{"who":"self","during":{"turn":"self"}}}),
    ] {
        lowerer()
            .lower(&tree(&on, &json!({"draw":{"n":1_u8}})))
            .unwrap();
        assert_eq!(
            lowerer()
                .lower(&tree(&on, &json!({"buff":{"on":"it","power":1_u8}})))
                .unwrap_err()
                .code,
            "unsupported_construct",
            "performed trigger has no object subject"
        );
    }
    for holder in [
        json!("this"),
        json!({"who":"opp","zone":"field","kind":"follower"}),
        json!({"who":"self","zone":"hand","kind":"follower"}),
    ] {
        lowerer()
            .lower(&tree(
                &json!({"drive_checked":{"holder":holder}}),
                &json!({"draw":{"n":1_u8}}),
            ))
            .unwrap_err();
    }
}
