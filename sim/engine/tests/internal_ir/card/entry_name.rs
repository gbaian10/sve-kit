use super::*;

fn tree(on: &Value) -> Value {
    let mut tree = fanfare::expanded::tree().clone();
    let mut entry = tree["cards"]["BP08-038"]["abilities"][0].clone();
    entry.as_object_mut().unwrap().remove("fanfare");
    entry["auto"] = json!({"on":on,"do":[{"draw":{"n":1}}]});
    tree["cards"]["BP08-038"]["abilities"] = json!([entry]);
    tree
}

#[test]
fn only_an_entering_subject_reads_a_card_name_fragment() {
    let sel = json!({"kind":"follower","class":"ロイヤル","name_contains":"アルベール"});
    lowerer()
        .lower(&tree(&json!({"enter_field":{"who":"self","sel":sel}})))
        .unwrap();
    for on in [
        json!({"left_field":{"who":"self","sel":sel}}),
        json!({"destroyed":{"who":"self","sel":sel}}),
        json!({"enter_field":{"who":"self","sel":{"kind":"follower","name_contains":""}}}),
    ] {
        assert!(
            lowerer().lower(&tree(&on)).is_err(),
            "{on}: name fragments are only wired for entering subjects"
        );
    }
}

#[test]
fn only_an_entering_subject_reads_a_trait_union() {
    let sel = json!({"kind":"follower","other":true,"traits_any":["学院","獣"]});
    lowerer()
        .lower(&tree(&json!({"enter_field":{"who":"self","sel":sel}})))
        .unwrap();
    for on in [
        json!({"left_field":{"who":"self","sel":sel}}),
        json!({"destroyed":{"who":"self","sel":sel}}),
        json!({"enter_field":{"who":"self","sel":{"kind":"follower","traits_any":["学院"]}}}),
        json!({"enter_field":{"who":"self","sel":{"kind":"follower","traits_any":["学院","学院"]}}}),
        json!({"enter_field":{"who":"self","sel":{"kind":"follower","traits_any":["学院",""]}}}),
        json!({"enter_field":{"who":"self","sel":{"trait":"精霊","traits_any":["学院","獣"]}}}),
    ] {
        assert!(
            lowerer().lower(&tree(&on)).is_err(),
            "{on}: trait unions are only wired for distinct traits on entering subjects"
        );
    }
}
