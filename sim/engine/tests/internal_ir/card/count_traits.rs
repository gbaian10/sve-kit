use super::*;

fn tree(count: &Value) -> Value {
    let mut tree = fanfare::expanded::tree().clone();
    let mut entry = tree["cards"]["BP08-038"]["abilities"][0].clone();
    entry["fanfare"] = json!({"do":[{"if":{
        "cond":{"cmp":{"left":{"count":count},"op":"ge","right":2}},
        "then":[{"draw":{"n":1}}]
    }}]});
    tree["cards"]["BP08-038"]["abilities"] = json!([entry]);
    tree
}

#[test]
fn either_trait_counts_read_public_zones_only() {
    for zone in ["field", "ex", "cemetery"] {
        lowerer()
            .lower(&tree(
                &json!({"who":"self","zone":zone,"traits_any":["天使","堕天使"]}),
            ))
            .unwrap();
    }
    for count in [
        json!({"who":"self","zone":"hand","traits_any":["天使","堕天使"]}),
        json!({"who":"self","zone":"deck","traits_any":["天使","堕天使"]}),
        json!({"who":"self","zone":"evolve_deck","traits_any":["天使","堕天使"]}),
        json!({"who":"self","zone":"field","traits_any":["天使","天使"]}),
    ] {
        assert!(
            lowerer().lower(&tree(&count)).is_err(),
            "{count}: hidden zones and repeated traits stay closed"
        );
    }
}
