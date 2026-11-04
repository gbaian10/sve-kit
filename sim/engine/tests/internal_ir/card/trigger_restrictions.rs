#![expect(
    clippy::panic,
    reason = "Unexpected pending decisions invalidate these occurrence scenarios."
)]
use super::*;
use alloc::collections::BTreeMap;
use sve_engine::game::internal::InputDomain;

fn tree() -> Value {
    let mut tree = evolution_linked_movement::tree(&[
        ("fanfare", json!({"do":[{"life":{"who":"self","delta":1}}]})),
        (
            "auto",
            json!({"on":{"enter_field":{"ref":"this"}},"do":[{"life":{"who":"self","delta":2}}]}),
        ),
    ]);
    tree["cards"]["unit-amulet"] = tree["cards"]["unit-follower"].clone();
    let mut aura = tree["cards"]["unit-follower"].clone();
    let mut entry = aura["abilities"][0].clone();
    entry.as_object_mut().unwrap().remove("fanfare");
    entry["static"] =
        json!({"effects":[{"no_trigger":{"who":"opp","kinds":["fanfare","evolve_time"]}}]});
    aura["abilities"] = json!([entry]);
    tree["cards"]["unit-other-follower"] = aura;
    tree
}

fn position(programs: &BTreeMap<String, WellFormedCardProgram>, initial: &Value) -> Game {
    let mut game = Game::new(
        event_ability::title_catalog(),
        initial,
        &Value::Null,
        &Value::Null,
        "trigger-permission",
    )
    .unwrap();
    for (number, program) in programs {
        game.load_internal_card(number, program).unwrap();
    }
    game
}

fn finish_pending(game: &mut Game) {
    for _ in 0..8 {
        event_ability::finish(game);
        let Some(q) = game.internal_pending_choice().unwrap() else {
            return;
        };
        let InputDomain::Pending(options) = &q.domain else {
            panic!("pending occurrence");
        };
        assert_ne!(
            options.as_slice(),
            [],
            "a pending question must contain an occurrence"
        );
        resolved(
            game.answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
                .unwrap(),
        );
    }
    panic!("occurrences must finish");
}

#[test]
fn opponent_fanfare_is_suppressed_for_followers_and_amulets_but_entry_observers_still_trigger() {
    let programs = lowerer().lower(&tree()).unwrap();
    for kind in ["unit-follower", "unit-amulet"] {
        for (aura_seat, silenced, expected) in [(1, false, 23), (2, false, 22), (2, true, 23)] {
            let mut initial = setup(1, json!([]));
            initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"incoming","card":kind}]);
            for seat in ["P1", "P2"] {
                initial["players"][seat]["zones"]["field"] = json!([]);
            }
            initial["players"][format!("P{aura_seat}")]["zones"]["field"] =
                json!([{"id":"aura","card":"unit-other-follower","state":{"silenced":silenced}}]);
            let mut game = position(&programs, &initial);
            assert!(matches!(
                game.play_internal_card("incoming", PlayerId { seat: 1 })
                    .unwrap(),
                CardPlay::Entered(_)
            ));
            finish_pending(&mut game);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["P1"]["leader"]["life"], expected);
            assert!(
                state["P1"]["field"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("incoming"))
            );
        }
    }
}

#[test]
fn prohibition_is_captured_at_entry_before_the_mother_removes_its_provider() {
    for before_removal in [false, true] {
        let mut input = tree();
        let mut mother = input["cards"]["unit-follower"]["abilities"][0].clone();
        mother.as_object_mut().unwrap().remove("fanfare");
        let enter = json!({"move":{"ref":"incoming","to":"field","result":"entered"}});
        let remove = json!({"destroy":{"ref":{"who":"opp","zone":"field","kind":"follower"}}});
        mother["activate"] = json!({"cost":{"pp":0},"targets":[{"bind":"incoming","from":{"who":"self","zone":"cemetery","kind":"amulet"},"n":1}],"do":if before_removal {vec![enter,remove]} else {vec![remove,enter]}});
        input["cards"]["unit-follower"]["abilities"] = json!([mother]);
        let programs = lowerer().lower(&input).unwrap();
        let mut initial = setup(1, json!([]));
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] =
            json!([{"id":"mother","card":"unit-follower"}]);
        initial["players"]["P1"]["zones"]["cemetery"] =
            json!([{"id":"incoming","card":"unit-amulet"}]);
        initial["players"]["P2"]["zones"]["field"] =
            json!([{"id":"aura","card":"unit-other-follower"}]);
        let mut game = position(&programs, &initial);
        let CardPlay::Awaiting(q) = game
            .activate_internal_card(
                "mother",
                programs["unit-follower"].card().abilities[0].node.id,
                PlayerId { seat: 1 },
            )
            .unwrap()
        else {
            panic!("incoming target")
        };
        let InputDomain::Targets(domain) = &q.domain else {
            panic!("incoming target")
        };
        resolved(
            game.answer_internal_card(
                q.actor,
                q.token,
                Answer::Targets(vec![domain.members[0].handle]),
            )
            .unwrap(),
        );
        finish_pending(&mut game);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P2"]["cemetery"], json!(["aura"]));
        assert_eq!(
            state["P1"]["leader"]["life"],
            if before_removal { 22 } else { 23 }
        );
    }
}

#[test]
fn trigger_permission_keeps_other_subjects_keywords_conditions_and_zones_closed() {
    for (pointer, value) in [
        ("/who", json!("self")),
        ("/kinds", json!(["lastword"])),
        ("/kinds", json!(["attack_time"])),
    ] {
        let mut input = tree();
        *input["cards"]["unit-other-follower"]["abilities"][0]["static"]["effects"][0]["no_trigger"].pointer_mut(pointer).unwrap() = value;
        lowerer().lower(&input).unwrap_err();
    }
    for (key, value) in [
        ("while", json!({"during":{"turn":"self"}})),
        ("zone", json!("cemetery")),
    ] {
        let mut input = tree();
        input["cards"]["unit-other-follower"]["abilities"][0]["static"][key] = value;
        lowerer().lower(&input).unwrap_err();
    }
    let programs = lowerer().lower(&tree()).unwrap();
    let mut raw = programs["unit-other-follower"].card().clone();
    let Effect::Permission { polarity, .. } = &mut raw.abilities[0].node.declarations[0] else {
        panic!("permission declaration")
    };
    *polarity = Polarity::Allow;
    WellFormedCardProgram::check(raw).unwrap_err();
}

#[test]
fn prohibiting_occurrences_does_not_prohibit_an_effects_direct_evolution_time_play() {
    let mut input = tree();
    let child = evolution_linked_movement::tree(&[
        (
            "activate",
            json!({"cost":{"pp":0},"do":[{"play_ability":{"of":"this","trigger":"evolve_time"}},{"life":{"who":"self","delta":1}}]}),
        ),
        (
            "evolve_time",
            json!({"do":[{"life":{"who":"self","delta":2}}]}),
        ),
    ]);
    input["cards"]["unit-follower"] = child["cards"]["unit-follower"].clone();
    let programs = lowerer().lower(&input).unwrap();
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"unit-follower"}]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"aura","card":"unit-other-follower"}]);
    let mut game = position(&programs, &initial);
    let CardPlay::Awaiting(q) = game
        .activate_internal_card(
            "source",
            programs["unit-follower"].card().abilities[0].node.id,
            PlayerId { seat: 1 },
        )
        .unwrap()
    else {
        panic!("ability choice")
    };
    let InputDomain::Abilities(options) = &q.domain else {
        panic!("ability choice")
    };
    assert_eq!(options.len(), 1);
    resolved(
        game.answer_internal_card(q.actor, q.token, Answer::Ability(options[0].handle))
            .unwrap(),
    );
    finish_pending(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        23
    );
}
