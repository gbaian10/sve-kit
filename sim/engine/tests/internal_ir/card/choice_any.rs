use super::*;
use alloc::collections::BTreeMap;
use sve_engine::game::internal::{DecisionView, InputDomain};

fn rule() -> Value {
    json!({"zone":"ex","effects":[{"choice_any":{"who":"self"}}]})
}
fn choice() -> Value {
    json!({"n":1_u8,"options":[[{"life":{"who":"self","delta":1_u8}}],[{"set_life":{"who":"self","n":5_u8}}],[{"life":{"who":"self","delta":2_u8}}]]})
}
fn tree(body: &Value) -> Value {
    let mut tree =
        evolution_linked_movement::tree(&[("activate", json!({"cost":{"pp":1_u8},"do":body}))]);
    let crest = evolution_linked_movement::tree(&[
        ("static", rule()),
        (
            "activate",
            json!({"zone":"ex","cost":{"banish_self":true},"do":[]}),
        ),
    ]);
    tree["cards"]["unit-amulet"] = crest["cards"]["unit-follower"].clone();
    tree
}
fn programs() -> BTreeMap<String, WellFormedCardProgram> {
    lowerer()
        .lower(&tree(&json!([{"choice":choice()}])))
        .unwrap()
}
fn initial(location: &str, silenced: bool) -> Value {
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["pp"] = json!({"current":3_u8,"max":3_u8});
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"unit-follower"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([]);
    match location {
        "own" => {
            initial["players"]["P1"]["zones"]["ex"] =
                json!([{"id":"rule","card":"unit-amulet","state":{"silenced":silenced}}]);
        }
        "opp" => {
            initial["players"]["P2"]["zones"]["ex"] = json!([{"id":"rule","card":"unit-amulet"}]);
        }
        "field" => initial["players"]["P1"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"rule","card":"unit-amulet"})),
        _ => {}
    }
    initial
}
fn game(programs: &BTreeMap<String, WellFormedCardProgram>, initial: &Value) -> Game {
    let mut game = Game::new(catalog(), initial, &Value::Null, &Value::Null, "choice-any").unwrap();
    for (number, program) in programs {
        game.load_internal_card(number, program).unwrap();
    }
    game
}
#[expect(
    clippy::panic,
    reason = "The fixture must ask for its actual Choice modes."
)]
fn modes(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(question) = play else {
        panic!("Choice modes")
    };
    question
}
fn activate(game: &mut Game, programs: &BTreeMap<String, WellFormedCardProgram>) -> DecisionView {
    modes(
        game.activate_internal_card(
            "source",
            programs["unit-follower"].card().abilities[0].node.id,
            PlayerId { seat: 1 },
        )
        .unwrap(),
    )
}
fn finish(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}

#[test]
fn choice_any_accepts_zero_or_all_and_executes_in_printed_order_with_normal_payment() {
    let programs = programs();
    for selected in [vec![], vec![ModeId(3), ModeId(2), ModeId(1)]] {
        let mut game = game(&programs, &initial("own", false));
        assert_eq!(game.legal().unwrap(), Vec::<Value>::new());
        let before = game.projection(View::Referee).unwrap();
        game.submit(&json!({"do":"end"})).unwrap_err();
        assert_eq!(game.projection(View::Referee).unwrap(), before);
        let question = activate(&mut game, &programs);
        let InputDomain::Modes {
            modes,
            cardinality,
            count,
        } = &question.domain
        else {
            panic!("mode domain")
        };
        assert_eq!(modes.len(), 3);
        assert_eq!(
            *cardinality,
            Cardinality {
                minimum: Nat(0),
                maximum: Some(Nat(3))
            }
        );
        assert_eq!(*count, Nat(3));
        for invalid in [vec![ModeId(1), ModeId(1)], vec![ModeId(4)]] {
            let before = game.projection(View::Referee).unwrap();
            game.answer_internal_card(question.actor, question.token, Answer::Modes(invalid))
                .unwrap_err();
            assert_eq!(game.projection(View::Referee).unwrap(), before);
        }
        resolved(
            game.answer_internal_card(
                question.actor,
                question.token,
                Answer::Modes(selected.clone()),
            )
            .unwrap(),
        );
        finish(&mut game);
        let after = game.projection(View::Referee).unwrap();
        assert_eq!(after["P1"]["pp"]["current"], 2);
        assert_eq!(
            after["P1"]["leader"]["life"],
            if selected.is_empty() { 20 } else { 7 }
        );
    }
}
#[test]
fn choice_any_ignores_opponents_field_and_suppressed_sources_then_expires_on_leaving_ex() {
    let programs = programs();
    for (location, silenced) in [
        ("none", false),
        ("opp", false),
        ("field", false),
        ("own", true),
    ] {
        let mut game = game(&programs, &initial(location, silenced));
        let question = activate(&mut game, &programs);
        let InputDomain::Modes {
            cardinality, count, ..
        } = question.domain
        else {
            panic!("mode domain")
        };
        assert_eq!(cardinality.minimum, Nat(1));
        assert_eq!(count, Nat(1));
        game.answer_internal_card(question.actor, question.token, Answer::Modes(vec![]))
            .unwrap_err();
    }
    let mut game = game(&programs, &initial("own", false));
    resolved(
        game.activate_internal_card(
            "rule",
            programs["unit-amulet"].card().abilities[1].node.id,
            PlayerId { seat: 1 },
        )
        .unwrap(),
    );
    finish(&mut game);
    let question = activate(&mut game, &programs);
    let InputDomain::Modes {
        cardinality, count, ..
    } = question.domain
    else {
        panic!("mode domain")
    };
    assert_eq!(cardinality.minimum, Nat(1));
    assert_eq!(count, Nat(1));
}
#[test]
fn choice_any_does_not_modify_resolution_alternatives() {
    let programs=lowerer().lower(&tree(&json!([{"either":{"options":[[{"life":{"who":"self","delta":1}}],[{"life":{"who":"self","delta":2}}]]}}]))).unwrap();
    let mut game = game(&programs, &initial("own", false));
    let question = activate(&mut game, &programs);
    let InputDomain::Modes {
        cardinality, count, ..
    } = &question.domain
    else {
        panic!("mode domain")
    };
    assert_eq!(
        *cardinality,
        Cardinality {
            minimum: Nat(1),
            maximum: Some(Nat(1))
        }
    );
    assert_eq!(*count, Nat(1));
    game.answer_internal_card(question.actor, question.token, Answer::Modes(vec![]))
        .unwrap_err();
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::Modes(vec![ModeId(1), ModeId(2)]),
    )
    .unwrap_err();
}
#[test]
fn choice_any_keeps_only_legal_modes_and_cannot_forge_a_banned_mode() {
    let mut tree = tree(
        &json!([{"choice":{"n":1,"options":[[{"damage":{"to":"enemy","n":1}}],[{"life":{"who":"self","delta":1}}]]}}]),
    );
    tree["cards"]["unit-follower"]["abilities"][0]["activate"]["targets"] = json!([{"bind":"enemy","option":1,"from":{"who":"opp","zone":"field","kind":"follower"},"n":1}]);
    let programs = lowerer().lower(&tree).unwrap();
    let mut game = game(&programs, &initial("own", false));
    let question = activate(&mut game, &programs);
    let InputDomain::Modes {
        modes,
        cardinality,
        count,
    } = &question.domain
    else {
        panic!("mode domain")
    };
    assert_eq!(*modes, vec![ModeId(2)]);
    assert_eq!(*count, Nat(1));
    assert_eq!(
        *cardinality,
        Cardinality {
            minimum: Nat(0),
            maximum: Some(Nat(1))
        }
    );
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::Modes(vec![ModeId(1)]),
    )
    .unwrap_err();
    resolved(
        game.answer_internal_card(question.actor, question.token, Answer::Modes(vec![]))
            .unwrap(),
    );
    finish(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        2
    );
}
#[test]
fn choice_any_quick_preflight_can_pay_for_zero_when_every_mode_lacks_a_target() {
    let mut tree = tree(&json!([{"choice":choice()}]));
    let spell = evolution_linked_movement::tree(&[(
        "spell",
        json!({"targets":[{"bind":"first","option":1,"from":{"who":"opp","zone":"field","kind":"follower"},"n":1},{"bind":"second","option":2,"from":{"who":"opp","zone":"field","kind":"follower"},"n":1}],"do":[{"choice":{"n":1,"options":[[{"damage":{"to":"first","n":1}}],[{"damage":{"to":"second","n":2}}]]}}]}),
    )]);
    tree["cards"]["unit-spell"] = spell["cards"]["unit-follower"].clone();
    let mut programs = lowerer()
        .lower_with_printed_costs(&tree, &BTreeMap::from([("unit-spell".into(), Nat(1))]))
        .unwrap();
    let spell = programs["unit-spell"].card().abilities[0].clone();
    programs.insert("unit-spell".into(), card(vec![spell, quick::permit()]));
    for enabled in [false, true] {
        let mut initial = initial(if enabled { "own" } else { "none" }, false);
        initial["turn"]["active"] = json!("P2");
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"spell","card":"unit-spell"}]);
        let mut game = game(&programs, &initial);
        game.internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap();
        finish(&mut game);
        let question = game.internal_quick_choice().unwrap().unwrap();
        let InputDomain::Quick { options, .. } = &question.domain else {
            panic!("Quick")
        };
        assert_eq!(options.len(), usize::from(enabled));
        if enabled {
            resolved(
                game.answer_internal_card(
                    question.actor,
                    question.token,
                    Answer::Quick(Some(options[0].handle)),
                )
                .unwrap(),
            );
            finish(&mut game);
            let after = game.projection(View::Referee).unwrap();
            assert_eq!(after["P1"]["pp"]["current"], 2);
            assert_eq!(after["P1"]["cemetery"], json!(["spell"]));
        }
    }
}
#[test]
fn choice_any_rejects_other_zones_players_conditions_and_raw_permission_shapes() {
    for declaration in [
        json!({"effects":[{"choice_any":{"who":"self"}}]}),
        json!({"zone":"field","effects":[{"choice_any":{"who":"self"}}]}),
        json!({"zone":"ex","effects":[{"choice_any":{"who":"opp"}}]}),
        json!({"zone":"ex","while":{"awakened":true},"effects":[{"choice_any":{"who":"self"}}]}),
    ] {
        lowerer()
            .lower(&evolution_linked_movement::tree(&[("static", declaration)]))
            .unwrap_err();
    }
    let mut raw = programs()["unit-amulet"].card().clone();
    let Effect::Permission {
        action: ActionPattern::ChooseMode { cardinality },
        ..
    } = &mut raw.abilities[0].node.declarations[0]
    else {
        panic!("permission")
    };
    cardinality.maximum = Some(Nat(9));
    WellFormedCardProgram::check(raw).unwrap_err();
}

#[test]
fn choice_any_does_not_expand_an_original_zero_count() {
    let mut document = tree(&json!([]));
    let header = &mut document["cards"]["unit-follower"]["abilities"][0];
    header.as_object_mut().unwrap().remove("activate");
    header["fanfare"] = json!({"do":[{"choice":{"upto":{"count":{"who":"self","zone":"field","trait":"absent"}},"options":[[{"life":{"who":"self","delta":1}}],[{"life":{"who":"self","delta":2}}]]}}]});
    let programs = lowerer().lower(&document).unwrap();
    let mut initial = initial("own", false);
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"unit-follower"}]);
    let mut game = game(&programs, &initial);
    assert!(matches!(
        game.play_internal_card("source", PlayerId { seat: 1 })
            .unwrap(),
        CardPlay::Entered(_)
    ));
    finish(&mut game);
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("fanfare")
    };
    resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap(),
    );
    finish(&mut game);
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["P1"]["leader"]["life"], 20);
    assert_eq!(after["P1"]["pp"]["current"], 2);
}
