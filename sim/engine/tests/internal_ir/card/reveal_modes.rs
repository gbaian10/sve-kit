use super::*;

fn tree(cost: &Value) -> Value {
    let mut tree = fanfare::expanded::tree().clone();
    let mut seed = tree["cards"]["BP08-038"].clone();
    seed["abilities"][0]["fanfare"] = json!({
        "cost":cost,
        "targets":[{"bind":"enemy","option":1,"from":{"who":"opp","zone":"field","kind":"follower"},"n":1}],
        "do":[{"choice":{"n":1,"options":[[{"destroy":"enemy"}],[{"draw":{"n":1}}]]}}]
    });
    tree["cards"] = json!({"unit-follower":seed});
    tree
}

fn program(n: u64) -> WellFormedCardProgram {
    lowerer()
        .lower(&tree(
            &json!({"reveal":{"sel":{"who":"self","zone":"hand","kind":"spell"},"n":n}}),
        ))
        .unwrap()
        .remove("unit-follower")
        .unwrap()
}

#[test]
fn required_hand_revelation_and_formal_mode_targets_share_a_play_entry() {
    for n in [1, 2] {
        program(n);
    }
}

#[test]
fn revealed_cost_binding_is_unavailable_to_the_chosen_mode_body() {
    let mut raw = program(1).card().clone();
    let ability = &mut raw.abilities[0].node;
    let ChoiceDomain::Objects { binding, .. } = ability.play.as_ref().unwrap().parameters[0].domain
    else {
        panic!("reveal material")
    };
    let body = ability.body.as_mut().unwrap();
    let Node::Control {
        control: Control::Mode { modes, .. },
    } = &mut body.steps[1].node
    else {
        panic!("formal modes")
    };
    let Node::Instruction { instruction } = &mut modes[0].body.steps[0].node else {
        panic!("mode body")
    };
    instruction.action = Action::Reveal {
        objects: Selector::Bound { reference: binding },
    };
    let error = WellFormedCardProgram::check(raw).unwrap_err();
    assert_eq!(error.reason, WellFormedReason::UnavailableBinding);
    assert!(
        error.ir_path.contains("/modes/0/body/steps/0/instruction/"),
        "the retired binding is rejected in the selected mode's instruction: {}",
        error.ir_path
    );
}

#[test]
fn ordinary_targets_still_cannot_be_hoisted_beside_mode_parameters() {
    let mut raw = program(1).card().clone();
    let ability = &mut raw.abilities[0].node;
    let Node::Control {
        control: Control::Mode { modes, .. },
    } = &ability.body.as_ref().unwrap().steps[1].node
    else {
        panic!("formal modes")
    };
    let target = modes[0].parameters[0].clone();
    ability.play.as_mut().unwrap().parameters.push(target);
    let error = WellFormedCardProgram::check(raw).unwrap_err();
    assert_eq!(error.reason, WellFormedReason::UnsupportedShape);
    assert!(
        error.ir_path.ends_with("/abilities/0"),
        "the unproved outer parameter is rejected at the ability header: {}",
        error.ir_path
    );
}
