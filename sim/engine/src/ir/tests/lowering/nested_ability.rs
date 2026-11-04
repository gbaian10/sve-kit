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

fn ub() -> Value {
    json!({"of":"friend","ability":{"ub":true,"n":1_u8},"cost_policy":"waive_original"})
}

#[test]
fn the_existing_author_shapes_select_evolution_or_effective_ub_abilities() {
    for policy in [None, Some("normal")] {
        let mut action = json!({"of":"this","trigger":"evolve_time"});
        if let Some(policy) = policy {
            action["cost_policy"] = json!(policy);
        }
        lowerer().lower(&tree(&action, None)).unwrap();
    }
    lowerer().lower(&tree(&ub(), Some(target()))).unwrap();
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

#[test]
fn an_ub_ability_choice_requires_one_own_formal_field_follower() {
    for (field, value) in [
        ("who", json!("opp")),
        ("zone", json!("hand")),
        ("kind", json!("amulet")),
    ] {
        let mut incompatible = target();
        incompatible["from"][field] = value;
        lowerer()
            .lower(&tree(&ub(), Some(incompatible)))
            .unwrap_err();
    }
    let mut multiple = target();
    multiple["n"] = json!(2_u8);
    lowerer().lower(&tree(&ub(), Some(multiple))).unwrap_err();
    lowerer().lower(&tree(&ub(), None)).unwrap_err();
}

#[test]
fn ability_choices_prove_the_consumer_type_policy_and_lexical_scope() {
    let cards = lowerer()
        .lower(&tree(&json!({"of":"this","trigger":"evolve_time"}), None))
        .unwrap();
    let original = cards["synthetic"].card().abilities[0]
        .node
        .body
        .clone()
        .unwrap();
    WellFormedProgram::check(original.clone()).unwrap();
    for change in [0_u8, 1, 2, 3] {
        let mut body = original.clone();
        let action = body
            .steps
            .iter_mut()
            .find_map(|step| {
                if let Node::Instruction { instruction } = &mut step.node {
                    Some(&mut instruction.action)
                } else {
                    None
                }
            })
            .unwrap();
        if let Action::PlayAbility {
            ability,
            cost_policy,
            trigger_policy,
            ..
        } = action
        {
            match change {
                0 => *cost_policy = AbilityCostPolicy::WaiveOriginal,
                1 => *trigger_policy = AbilityTriggerPolicy::Normal,
                2 => {
                    *ability = AbilityOperand::Selected {
                        selection: Binding::new(999, body.scope),
                    }
                }
                _ => {
                    *ability = AbilityOperand::Selected {
                        selection: Binding::new(1, ScopeId(999)),
                    }
                }
            }
        }
        WellFormedProgram::check(body).unwrap_err();
    }
}
