use super::*;

fn automatic() -> Value {
    json!({"on":{"dealt_damage":{"ref":"this","kind":"ability","recipient":{"who":"opp","zone":"field","kind":"follower"}}},"aggregation":"batch","do":[{"damage":{"to":{"leaders":"opponents"},"n":3}}]})
}

fn tree(auto: &Value) -> Value {
    equipment_played::tree(
        &json!({"static":{"zone":"equipment","effects":[{"grant":{"on":"host","abilities":[{"auto":auto}]}}]}}),
        "unit-spell",
    )
}

fn game() -> Game {
    Game::new(
        equipment::equipment_catalog("イクイップメント・トークン"),
        &setup(1, json!([])),
        &Value::Null,
        &Value::Null,
        "closed-dealt-batch",
    )
    .unwrap()
}

#[test]
fn a_self_ability_damage_batch_has_one_supported_field_recipient_shape() {
    let valid = lowerer()
        .lower(&tree(&automatic()))
        .unwrap()
        .remove("unit-spell")
        .unwrap();
    game().load_internal_card("unit-spell", &valid).unwrap();
    for field in ["who", "zone", "kind", "trait"] {
        let mut changed = automatic();
        changed["on"]["dealt_damage"]["recipient"][field] = json!(match field {
            "who" => "self",
            "zone" => "hand",
            "kind" => "amulet",
            _ => "妖精",
        });
        lowerer().lower(&tree(&changed)).unwrap_err();
    }
}

#[test]
fn a_one_or_more_damage_subscription_cannot_run_once_per_hit_or_reuse_other_batch_shapes() {
    let mut per_hit = automatic();
    per_hit.as_object_mut().unwrap().remove("aggregation");
    let candidate = lowerer()
        .lower(&tree(&per_hit))
        .unwrap()
        .remove("unit-spell")
        .unwrap();
    let error = game()
        .load_internal_card("unit-spell", &candidate)
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("printed timing aggregation or self-event header")
    );
    for case in 0_u8..4 {
        let mut changed = automatic();
        match case {
            0 => {
                changed["on"]["dealt_damage"]
                    .as_object_mut()
                    .unwrap()
                    .remove("ref");
            }
            1 => {
                changed["on"]["dealt_damage"]["kind"] = json!("combat");
            }
            2 => {
                changed["on"]["dealt_damage"]["during"] = json!({"turn":"self"});
            }
            _ => {
                changed["on"] = json!({"damaged":{"recipient":"this","kind":"ability"}});
            }
        }
        lowerer().lower(&tree(&changed)).unwrap_err();
    }
}
