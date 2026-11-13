use super::*;

fn fanfare(head: Value) -> Result<(), lowering::LoweringError> {
    let mut tree = fanfare::expanded::tree().clone();
    tree["cards"]["BP08-038"]["abilities"][0]["fanfare"] = head;
    lowerer().lower(&tree).map(|_| ())
}

fn banish_two() -> Value {
    json!({"banish":{"sel":{"who":"self","zone":"cemetery","orig_cost_eq":1},"n":2}})
}

#[test]
fn a_triggered_head_material_cost_sits_beside_fee_free_modes() {
    fanfare(
        json!({"cost":banish_two(),"do":[{"choice":{"upto":2,"options":[
            [{"damage":{"to":{"leaders":"opponents"},"n":2}}],
            [{"draw":{"who":"self","n":1}}]
        ]}}]}),
    )
    .unwrap();
}

#[test]
fn a_mode_with_its_own_material_fee_still_cannot_join_a_head_material_cost() {
    let mode_material =
        json!({"banish":{"sel":{"who":"self","zone":"cemetery","kind":"follower"},"n":1}});
    fanfare(
        json!({"cost":banish_two(),"do":[{"choice":{"upto":2,"options":[
            [{"damage":{"to":{"leaders":"opponents"},"n":2}}],
            [{"pay":{"cost":mode_material,"do":[{"draw":{"who":"self","n":1}}]}}]
        ]}}]}),
    )
    .unwrap_err();
}
