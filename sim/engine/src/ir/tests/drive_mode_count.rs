use super::*;

fn tree(query: &Value) -> Value {
    let mut tree = expanded::tree().clone();
    let mut seed = tree["cards"]["BP08-038"].clone();
    let mut entry = seed["abilities"][0].clone();
    entry["fanfare"] = json!({"do":[{"choice":{"n":1_u8,
        "instead":[{"if":{"cmp":{"left":{"count":query},"op":"ge","right":3_u8}},"upto":3_u8}],
        "options":[[{"life":{"who":"self","delta":1_u8}}],[{"life":{"who":"self","delta":2_u8}}],[{"life":{"who":"self","delta":3_u8}}]]
    }}]});
    seed["abilities"] = json!([entry]);
    tree["cards"] = json!({"synthetic":seed});
    tree
}

fn query() -> Value {
    json!({"who":"self","zone":"evolve_deck","name":"resource","face_up":true})
}

#[test]
fn conditional_evolve_deck_modes_accept_only_a_public_named_count() {
    lowerer().lower(&tree(&query())).unwrap();
    for (key, value) in [
        ("who", json!("opp")),
        ("zone", json!("hand")),
        ("face_up", json!(false)),
        ("face_up", Value::Null),
        ("name", Value::Null),
        ("kind", json!("follower")),
        ("trait", json!("trait")),
    ] {
        let mut invalid = query();
        if value.is_null() {
            invalid.as_object_mut().unwrap().remove(key);
        } else {
            invalid[key] = value;
        }
        assert_eq!(
            lowerer().lower(&tree(&invalid)).unwrap_err().code,
            "unsupported_construct",
            "the semantic count restriction rejects {key}"
        );
    }
}

#[test]
fn conditional_modes_accept_public_zone_counts_of_either_player() {
    for (who, zone) in [
        ("self", "cemetery"),
        ("opp", "cemetery"),
        ("self", "field"),
        ("opp", "ex"),
    ] {
        lowerer()
            .lower(&tree(&json!({"who":who,"zone":zone,"trait":"trait"})))
            .unwrap();
    }
    for zone in ["hand", "deck"] {
        assert_eq!(
            lowerer()
                .lower(&tree(&json!({"who":"self","zone":zone,"trait":"trait"})))
                .unwrap_err()
                .code,
            "unsupported_construct",
            "a hidden {zone} count does not set the mode cap"
        );
    }
}

#[test]
fn conditional_modes_do_not_accept_unrelated_integer_conditions_in_ir() {
    let valid = lowerer().lower(&tree(&query())).unwrap();
    for use_life in [false, true] {
        let mut card = valid["synthetic"].card().clone();
        let Node::Control {
            control:
                Control::Mode {
                    count: Expr::Compute(operation),
                    ..
                },
        } = &mut card.abilities[0].node.body.as_mut().unwrap().steps[0].node
        else {
            panic!("mode")
        };
        let NatOperation::Conditional {
            condition: Expr::Compute(predicate),
            ..
        } = operation.as_mut()
        else {
            panic!("conditional cap")
        };
        let Predicate::CompareInt {
            left, comparison, ..
        } = predicate.as_mut()
        else {
            panic!("public count comparison")
        };
        if use_life {
            *left = Expr::Live(Box::new(IntRead::PlayerResource {
                player: PlayerRef::Controller {},
                property: PlayerResource::Life,
            }));
        } else {
            *comparison = Comparison::LessEqual;
        }
        WellFormedCardProgram::check(card).unwrap_err();
    }
}
