use super::*;

fn tree(draw: &Value) -> Value {
    let mut tree = fanfare::expanded::tree().clone();
    let mut entry = tree["cards"]["BP08-038"]["abilities"][0].clone();
    entry["fanfare"] = json!({
        "targets":[{"bind":"victim","from":{"who":"all","zone":"field","kind":"follower"},"n":1}],
        "do":[{"draw":draw}]
    });
    tree["cards"]["BP08-038"]["abilities"] = json!([entry]);
    tree
}

#[test]
fn a_draw_reaches_only_its_own_controller_or_a_chosen_objects_controller() {
    for who in [json!("self"), json!({"master_of":"victim"})] {
        lowerer().lower(&tree(&json!({"n":1,"who":who}))).unwrap();
    }
    for (who, rejection) in [
        (json!("opp"), "unsupported_construct"),
        (json!("opponent"), "unsupported_construct"),
        (json!("p"), "unsupported_construct"),
        (json!({"master_of":"missing"}), "unsupported_construct"),
        (json!({"leaders":"all"}), "invalid_expanded"),
    ] {
        assert_eq!(
            lowerer()
                .lower(&tree(&json!({"n":1,"who":who})))
                .unwrap_err()
                .code,
            rejection,
            "{who}: unsupported drawers must remain rejected"
        );
    }
}
