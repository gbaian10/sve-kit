//! Temporary synthetic failure for the public CI summary.

#[test]
fn ci_failure_location_probe() {
    let wording = ["synthetic failure probe", "ci-private-rust-wording"].join(" ");
    assert_eq!(wording, "synthetic comparison value");
}
