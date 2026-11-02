use super::*;

fn tree(count: &Value) -> Value {
    let mut tree = expanded::tree().clone();
    let mut seed = tree["cards"]["BP08-038"].clone();
    let mut entry = seed["abilities"][0].clone();
    entry["fanfare"] = json!({"do":[{"life":{"who":"self","delta":count}}]});
    seed["abilities"] = json!([entry]);
    tree["cards"] = json!({"synthetic":seed});
    tree
}

#[test]
fn crest_count_is_limited_to_the_controllers_ex_area() {
    lowerer()
        .lower(&tree(
            &json!({"count":{"who":"self","zone":"ex","kind":"crest"}}),
        ))
        .unwrap();
    for (who, zone) in [
        ("opp", "ex"),
        ("all", "ex"),
        ("self", "field"),
        ("self", "hand"),
        ("self", "cemetery"),
    ] {
        assert_eq!(
            lowerer()
                .lower(&tree(
                    &json!({"count":{"who":who,"zone":zone,"kind":"crest"}})
                ))
                .unwrap_err()
                .code,
            "unsupported_construct"
        );
    }
}

#[test]
fn admitting_crest_counts_does_not_open_crest_target_selection() {
    let mut tree = tree(&json!(1_u8));
    tree["cards"]["synthetic"]["abilities"][0]["fanfare"] = json!({
        "targets":[{"bind":"crest","from":{"who":"self","zone":"field","kind":"crest"},"n":1_u8}],
        "do":[{"life":{"who":"self","delta":1_u8}}]
    });
    lowerer().lower(&tree).unwrap_err();
    tree["cards"]["synthetic"]["abilities"][0]["fanfare"]["targets"][0]["from"]["kind"] =
        json!("follower");
    lowerer().lower(&tree).unwrap();
}
