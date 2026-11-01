use super::*;

#[test]
fn mode_payment_rejects_resolution_alternatives_and_nonleading_payment_steps() {
    let paid = json!({"pay":{"cost":{"pp":1_i64},"do":[{"draw":{"n":1_i64}}]}});
    let cases = [
        json!([paid]),
        json!([{"either":{"options":[[paid],[{"draw":{"n":1_i64}}]]}}]),
        json!([{"choice":{"n":1_i64,"options":[[{"draw":{"n":1_i64}},paid],[{"draw":{"n":1_i64}}]]}}]),
    ];
    for steps in cases {
        let mut tree = fanfare::expanded::tree().clone();
        tree["cards"]["BP08-038"]["abilities"][0]["fanfare"]["do"] = steps;
        lowerer().lower(&tree).unwrap_err();
    }
}
