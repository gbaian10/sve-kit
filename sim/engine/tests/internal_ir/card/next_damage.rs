use super::*;

fn tree(payload: Value) -> Value {
    let mut tree = fanfare::expanded::tree().clone();
    let mut entry = tree["cards"]["BP08-038"]["abilities"][0].clone();
    entry.as_object_mut().unwrap().remove("fanfare");
    entry["activate"] = payload;
    let meta = tree["cards"]["BP08-038"]["meta"].clone();
    tree["cards"] = json!({"unit-follower":{"meta":meta,"abilities":[entry]}});
    tree
}

#[test]
fn one_use_damage_prevention_and_reduction_lower_through_the_existing_installation() {
    for effect in [
        json!({"prevent":{"kind":"any","next_only":true}}),
        json!({"damage_mod":{"delta":-5,"next_only":true}}),
    ] {
        lowerer().lower(&tree(json!({"cost":{"pp":0},"do":[{"player_effect":{"who":"self","effect":effect,"until":"turn"}}]}))).unwrap();
    }
    for subject in [json!("this"), json!("target")] {
        let mut payload = json!({"cost":{"pp":0},"do":[{"apply":{"on":subject,"effect":{"prevent":{"kind":"any","next_only":true}},"until":"turn"}}]});
        if subject == "target" {
            payload["targets"] = json!([{"bind":"target","from":{"who":"self","zone":"field","kind":"follower"},"n":1}]);
        }
        lowerer().lower(&tree(payload)).unwrap();
    }
}

#[test]
fn next_damage_keeps_other_players_durations_filters_and_multiple_recipients_closed() {
    let valid = json!({"cost":{"pp":0},"do":[{"player_effect":{"who":"self","effect":{"damage_mod":{"delta":-5,"next_only":true}},"until":"turn"}}]});
    for (pointer, value) in [
        ("/do/0/player_effect/who", json!("opp")),
        ("/do/0/player_effect/until", json!("next_turn")),
        ("/do/0/player_effect/effect/damage_mod/delta", json!(1)),
        (
            "/do/0/player_effect/effect/damage_mod/next_only",
            json!(false),
        ),
    ] {
        let mut payload = valid.clone();
        *payload.pointer_mut(pointer).unwrap() = value;
        lowerer().lower(&tree(payload)).unwrap_err();
    }
    let mut filtered = valid;
    filtered["do"][0]["player_effect"]["effect"]["damage_mod"]["kind"] = json!("ability");
    lowerer().lower(&tree(filtered)).unwrap_err();
    lowerer().lower(&tree(json!({"cost":{"pp":0},"do":[{"apply":{"on":{"who":"self","zone":"field","kind":"follower"},"effect":{"prevent":{"kind":"any","next_only":true}},"until":"turn"}}]}))).unwrap_err();
}

#[test]
fn raw_object_reduction_cannot_bypass_the_seeded_prevention_boundary() {
    let programs = lowerer().lower(&tree(json!({"cost":{"pp":0},"do":[{"apply":{"on":"this","effect":{"prevent":{"kind":"any","next_only":true}},"until":"turn"}}]}))).unwrap();
    let mut raw = programs["unit-follower"].card().clone();
    let Node::Instruction { instruction } =
        &mut raw.abilities[0].node.body.as_mut().unwrap().steps[0].node
    else {
        panic!("installation instruction")
    };
    let Action::InstallEffect { effect, .. } = &mut instruction.action else {
        panic!("installed prevention")
    };
    let Effect::Replacement { rewrite, .. } = effect.as_mut() else {
        panic!("damage replacement")
    };
    *rewrite = EventRewrite::AddDamage {
        amount: Expr::Literal(Int(-1)),
    };
    let candidate = WellFormedCardProgram::check(raw).unwrap();
    let error = game(1, 0)
        .load_internal_card("unit-follower", &candidate)
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("object damage prevention rewrite")
    );
}
