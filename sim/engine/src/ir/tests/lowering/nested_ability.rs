use super::*;

fn tree(action: &Value, target: Option<Value>) -> Value {
    let mut tree = expanded::tree().clone();
    let mut card = tree["cards"]["BP01-041"].clone();
    let mut ability = card["abilities"][0].clone();
    ability["activate"] = json!({"cost":{"pp":0_u8},"do":[{"play_ability":action}]});
    if let Some(target) = target {
        ability["activate"]["targets"] = json!([target]);
    }
    card["abilities"] = json!([ability]);
    tree["cards"] = json!({"synthetic":card});
    tree
}

fn target() -> Value {
    json!({"bind":"friend","from":{"who":"self","zone":"field","kind":"follower"},"n":1_u8})
}

#[test]
fn ability_replay_keeps_unrequested_author_variants_closed() {
    for action in [
        json!({"of":"this","trigger":"fanfare"}),
        json!({"of":"this","trigger":"evolve_time","cost_policy":"waive_original"}),
        json!({"of":"this","trigger":"evolve_time","cost_policy":null}),
        json!({"of":"friend","ability":{"ub":true,"n":2_u8},"cost_policy":"waive_original"}),
        json!({"of":"friend","ability":{"ub":false,"n":1_u8},"cost_policy":"waive_original"}),
        json!({"of":"friend","ability":{"ub":true,"n":1_u8}}),
        json!({"of":"friend","ability":{"ub":true,"n":1_u8},"trigger":null,"cost_policy":"waive_original"}),
    ] {
        lowerer().lower(&tree(&action, Some(target()))).unwrap_err();
    }
}
