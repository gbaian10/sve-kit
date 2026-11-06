use super::*;

fn tree(set_life: &Value) -> Value {
    let mut tree = fanfare::expanded::tree().clone();
    let mut entry = tree["cards"]["BP08-038"]["abilities"][0].clone();
    entry["fanfare"] = json!({"do":[{"set_life":set_life}]});
    tree["cards"]["BP08-038"]["abilities"] = json!([entry]);
    tree
}

#[test]
fn set_life_reaches_only_the_controller_or_the_opponent_leader() {
    for who in ["self", "opponents"] {
        lowerer().lower(&tree(&json!({"who":who,"n":6}))).unwrap();
    }
    for who in [json!("each"), json!("opp"), json!({"master_of":"missing"})] {
        assert!(
            lowerer().lower(&tree(&json!({"who":who,"n":6}))).is_err(),
            "{who}: other life holders are not wired"
        );
    }
}
