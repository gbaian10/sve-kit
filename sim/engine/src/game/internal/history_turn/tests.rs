use super::*;

fn names(values: &[&str]) -> Vec<String> {
    values.iter().map(|&name| name.to_owned()).collect()
}

#[test]
fn holder_name_exclusion_matches_any_saved_name_and_rejects_unknown_names() {
    let holder = names(&["サルビアパンサー", "alias"]);
    assert!(
        shares_holder_name(Some(&names(&["サルビアパンサー"])), &holder).unwrap(),
        "the same printed name is excluded"
    );
    assert!(
        shares_holder_name(Some(&names(&["ガザニアフォックス", "alias"])), &holder).unwrap(),
        "one shared alias is enough to exclude"
    );
    assert!(
        !shares_holder_name(Some(&names(&["ガザニアフォックス"])), &holder).unwrap(),
        "disjoint names count"
    );
    assert!(
        shares_holder_name(None, &holder).is_err(),
        "a subject without saved names is never treated as differently named"
    );
}
