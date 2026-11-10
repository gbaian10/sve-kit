#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Only the fixture's declared choices may occur."
)]
use super::*;
use sve_engine::game::internal::InputDomain;

fn target() -> Value {
    json!({"bind":"enemy","from":{"who":"opp","zone":"field","kind":"follower"},"n":1})
}

fn tree() -> Value {
    let mut tree = fanfare::expanded::tree().clone();
    let mut candidate = tree["cards"]["BP08-038"].clone();
    let mut template = candidate["abilities"][0].clone();
    template.as_object_mut().unwrap().remove("fanfare");
    let bodies = [
        json!({"static":{"effects":[{"grant":{"keywords":["ward"]}}]}}),
        json!({"activate":{"cost":{"pp":0},"targets":[target()],"do":[
            {"act":"enemy"},{"box":{"ref":"enemy","until":"next_opp_turn"}}]}}),
        json!({"activate":{"cost":{"pp":0},"targets":[target()],"do":[{"grant":{"on":"enemy","keywords":["storm"],"abilities":[{"lastword":{"do":[{"life":{"who":"self","delta":1}}]}}]}}]}}),
        json!({"activate":{"cost":{"pp":0},"targets":[target()],"do":[{"grant":{"on":"enemy","keywords":["ward"]}}]}}),
        json!({"activate":{"cost":{"pp":0},"targets":[target()],"do":[{"stand":"enemy"}]}}),
        json!({"activate":{"cost":{"pp":0},"targets":[target()],"do":[{"destroy":"enemy"}]}}),
        json!({"lastword":{"do":[{"draw":{"n":1}}]}}),
        json!({"activate":{"cost":{"pp":0},"targets":[{"bind":"enemy","from":{"who":"self","zone":"cemetery","kind":"follower"},"n":1}],"do":[{"move":{"ref":"enemy","to":"field","result":"returned"}}]}}),
    ];
    candidate["abilities"] = json!(
        bodies
            .into_iter()
            .zip(1_u64..)
            .map(|(body, line)| {
                let mut entry = template.clone();
                entry["line"] = json!(line);
                entry
                    .as_object_mut()
                    .unwrap()
                    .extend(body.as_object().unwrap().clone());
                entry
            })
            .collect::<Vec<_>>()
    );
    tree["cards"] = json!({"unit-follower":candidate});
    tree
}

fn program(tree: &Value) -> WellFormedCardProgram {
    lowerer()
        .lower(tree)
        .unwrap()
        .remove("unit-follower")
        .unwrap()
}

fn board_with(tree: &Value, enemy_acted: bool) -> (Game, WellFormedCardProgram) {
    board_in_zones(tree, enemy_acted, &Value::Null)
}

fn board_in_zones(tree: &Value, enemy_acted: bool, zones: &Value) -> (Game, WellFormedCardProgram) {
    let programs = lowerer().lower(tree).unwrap();
    let card = programs["unit-follower"].clone();
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"s","card":"unit-follower","state":{"acted":true}}]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"unit-follower","owner":"P1","state":{"acted":enemy_acted}}]);
    initial["players"]["P2"]["zones"]["deck"] = json!([{"id":"drawn","card":"unit-follower"}]);
    for player in ["P1", "P2"] {
        if let Some(extra) = zones[player].as_object() {
            initial["players"][player]["zones"]
                .as_object_mut()
                .unwrap()
                .extend(extra.clone());
        }
    }
    let (mut snapshot, _, mut document) = catalog_documents(&json!("1"));
    if tree["cards"].get("unit-form").is_some() {
        snapshot.push('\n');
        snapshot.push_str(&json!({"number":"unit-form","faces":[{
            "name":"unit-follower","card_class":"ニュートラル","card_type":"フォロワー・エボルヴ",
            "cost":"-","power":"4","hp":"5","traits":[],"text":null,"sections":[]
        }]}).to_string());
        let mut baseline: Value = serde_json::from_str(&document).unwrap();
        baseline["cards"]["unit-form"] = json!({"status":"complete","abilities":[]});
        document = baseline.to_string();
    }
    let registry = json!({"format":1,"kind":"keyword_registry","keywords":{
        "guard":{"ja":"guard","rule":"12.8","expansion":{"op":"keyword","name":"guard"}},
        "storm":{"ja":"storm","rule":"12.9","expansion":{"op":"keyword","name":"storm"}}
    }})
    .to_string();
    let catalog = Arc::new(
        Catalog::from_documents(&snapshot, &registry, &[("boxed-fixture".into(), document)])
            .unwrap(),
    );
    let mut game = Game::new(
        catalog,
        &initial,
        &Value::Null,
        &Value::Null,
        "boxed-boundary",
    )
    .unwrap();
    for (number, program) in programs {
        game.load_internal_card(&number, &program).unwrap();
    }
    (game, card)
}

fn activate(game: &mut Game, card: &WellFormedCardProgram, line: u32) {
    let ability = card
        .card()
        .abilities
        .iter()
        .find(|ability| ability.source.line == line)
        .unwrap()
        .node
        .id;
    let mut play = game
        .activate_internal_card("s", ability, PlayerId { seat: 1 })
        .unwrap();
    while let CardPlay::Awaiting(question) = play {
        let answer = match &question.domain {
            InputDomain::Targets(domain) => Answer::Targets(vec![domain.members[0].handle]),
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            other => panic!("unexpected fixture choice {other:?}"),
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    resolved(play);
    game.resume_internal_card().unwrap();
}

fn keywords(game: &Game) -> Value {
    game.projection(View::Referee).unwrap()["objects"]["enemy"]["keywords"].clone()
}

fn end_turn(game: &mut Game) {
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    quick::pass(game);
}

#[test]
fn box_cutoff_preserves_later_grants_and_expiry_restores_earlier_ones() {
    let (mut game, card) = board_with(&tree(), true);
    activate(&mut game, &card, 3);
    assert!(
        keywords(&game)
            .as_array()
            .unwrap()
            .contains(&json!("storm"))
    );
    activate(&mut game, &card, 2);
    assert_eq!(keywords(&game), json!([]));
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["enemy"]["boxed"],
        true
    );
    activate(&mut game, &card, 4);
    assert_eq!(keywords(&game), json!(["guard"]));
    end_turn(&mut game);
    let before_expiry = game.projection(View::Referee).unwrap();
    assert_eq!(before_expiry["turn"]["active"], "P2");
    assert_eq!(before_expiry["objects"]["enemy"]["acted"], true);
    assert_eq!(before_expiry["objects"]["enemy"]["boxed"], true);
    end_turn(&mut game);
    let after_expiry = game.projection(View::Referee).unwrap();
    assert_eq!(after_expiry["objects"]["enemy"]["boxed"], false);
    assert_eq!(keywords(&game), json!(["guard", "storm"]));
}

#[test]
fn box_blocks_rule_stand_but_allows_effect_stand_and_reboxing_captures_new_grants() {
    let (mut game, card) = board_with(&tree(), false);
    activate(&mut game, &card, 2);
    activate(&mut game, &card, 5);
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["enemy"]["acted"],
        false
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["enemy"]["boxed"],
        true
    );
    activate(&mut game, &card, 4);
    assert_eq!(keywords(&game), json!(["guard"]));
    activate(&mut game, &card, 2);
    assert_eq!(keywords(&game), json!([]));
}

#[test]
fn boxed_follower_has_no_lastword_and_move_does_not_carry_box_to_the_new_generation() {
    let (mut game, card) = board_with(&tree(), false);
    activate(&mut game, &card, 3);
    activate(&mut game, &card, 2);
    activate(&mut game, &card, 6);
    assert!(game.internal_pending_choice().unwrap().is_none());
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["cemetery"], json!(["enemy"]));
    assert_eq!(state["P2"]["deck_count"], 1);
    assert_eq!(state["objects"]["enemy"]["generation"], 1);
    activate(&mut game, &card, 8);
    let returned = game.projection(View::Referee).unwrap();
    assert_eq!(returned["objects"]["enemy"]["generation"], 2);
    assert_eq!(returned["objects"]["enemy"]["boxed"], false);
    assert_eq!(keywords(&game), json!(["guard"]));
}

#[test]
fn a_posture_trigger_waiting_before_box_keeps_its_saved_ability() {
    let mut input = tree();
    input["cards"]["unit-follower"]["abilities"][6]
        .as_object_mut()
        .unwrap()
        .remove("lastword");
    input["cards"]["unit-follower"]["abilities"][6]["auto"] =
        json!({"on":{"acted":{"ref":"this"}},"do":[{"life":{"who":"self","delta":1}}]});
    let (mut game, card) = board_with(&input, false);
    activate(&mut game, &card, 2);
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("saved posture trigger")
    };
    assert_eq!(options.len(), 1);
    resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap(),
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
        21
    );
}

#[test]
fn boxed_lowering_rejects_unregistered_durations_and_nonformal_references() {
    program(&tree());
    for reference in [json!("this"), json!("unknown")] {
        let mut input = tree();
        input["cards"]["unit-follower"]["abilities"][1]["activate"]["do"][1]["box"]["ref"] =
            reference;
        assert_eq!(
            lowerer().lower(&input).unwrap_err().code,
            "unsupported_construct"
        );
    }
    for (field, value) in [
        ("kind", json!("amulet")),
        ("who", json!("self")),
        ("zone", json!("cemetery")),
    ] {
        let mut input = tree();
        input["cards"]["unit-follower"]["abilities"][1]["activate"]["targets"][0]["from"][field] =
            value;
        assert_eq!(
            lowerer().lower(&input).unwrap_err().code,
            "unsupported_construct"
        );
    }
    for until in [json!("turn"), json!("game")] {
        let mut input = tree();
        input["cards"]["unit-follower"]["abilities"][1]["activate"]["do"][1]["box"]["until"] =
            until;
        assert_eq!(
            lowerer().lower(&input).unwrap_err().code,
            "unsupported_construct"
        );
    }
}

#[test]
fn boxed_boolean_selects_only_the_current_state_and_rejects_nonfield_queries() {
    for (boxed, count) in [(true, 1), (false, 0)] {
        let mut input = tree();
        let target = &mut input["cards"]["unit-follower"]["abilities"][5]["activate"]["targets"][0];
        target.as_object_mut().unwrap().remove("n");
        target["upto"] = json!(1);
        target["from"]["boxed"] = json!(boxed);
        let (mut game, card) = board_with(&input, false);
        activate(&mut game, &card, 2);
        let id = card
            .card()
            .abilities
            .iter()
            .find(|ability| ability.source.line == 6)
            .unwrap()
            .node
            .id;
        let CardPlay::Awaiting(question) = game
            .activate_internal_card("s", id, PlayerId { seat: 1 })
            .unwrap()
        else {
            panic!("boxed predicate")
        };
        let InputDomain::Targets(domain) = &question.domain else {
            panic!("boxed targets")
        };
        assert_eq!(domain.members.len(), count);
        let mut nonfield = input;
        nonfield["cards"]["unit-follower"]["abilities"][5]["activate"]["targets"][0]["from"] =
            json!({"who":"self","zone":"cemetery","kind":"follower","boxed":boxed});
        assert_eq!(
            lowerer().lower(&nonfield).unwrap_err().code,
            "unsupported_construct"
        );
    }
}

#[test]
fn a_lastword_granted_after_box_works_while_the_printed_one_stays_suppressed() {
    let (mut game, card) = board_with(&tree(), false);
    activate(&mut game, &card, 2);
    activate(&mut game, &card, 3);
    activate(&mut game, &card, 6);
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("granted Lastword")
    };
    assert_eq!(options.len(), 1);
    resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap(),
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["leader"]["life"], 21);
    assert_eq!(state["P2"]["deck_count"], 1);
}

#[path = "boxed/evolution.rs"]
mod evolution;

#[test]
fn boxed_filter_stays_closed_for_resolution_choices() {
    for boxed in [None, Some(true), Some(false)] {
        let mut input = tree();
        let mut from = json!({"who":"opp","zone":"field","kind":"follower"});
        if let Some(boxed) = boxed {
            from["boxed"] = json!(boxed);
        }
        input["cards"]["unit-follower"]["abilities"][5]["activate"] = json!({"cost":{"pp":0},"do":[
            {"choose":{"bind":"enemy","from":from,"n":1}}, {"destroy":"enemy"}
        ]});
        let result = lowerer().lower(&input);
        if boxed.is_some() {
            assert_eq!(result.unwrap_err().code, "unsupported_construct");
        } else {
            result.unwrap();
        }
    }
}

#[test]
fn lastword_filter_follows_granted_abilities_and_box_suppression() {
    let mut tree = tree();
    let entries = tree["cards"]["unit-follower"]["abilities"]
        .as_array_mut()
        .unwrap();
    entries.retain(|entry| entry.get("lastword").is_none());
    let mut query = entries[1].clone();
    query["line"] = json!(9);
    query["activate"] = json!({"cost":{"pp":0},"targets":[{"bind":"enemy","from":{"who":"opp","zone":"field","keyword":"lastword"},"n":1}],"do":[{"life":{"who":"self","delta":1}}]});
    entries.push(query);
    let (mut game, card) = board_with(&tree, true);
    let ability = card
        .card()
        .abilities
        .iter()
        .find(|a| a.source.line == 9)
        .unwrap()
        .node
        .id;
    assert!(matches!(
        game.activate_internal_card("s", ability, PlayerId { seat: 1 }),
        Err(sve_engine::EngineFailure::IllegalPlay(_))
    ));
    activate(&mut game, &card, 3);
    let CardPlay::Awaiting(question) = game
        .activate_internal_card("s", ability, PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("the granted Lastword must match");
    };
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("formal target");
    };
    assert_eq!(domain.members.len(), 1);
    let play = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Targets(vec![domain.members[0].handle]),
        )
        .unwrap();
    resolved(play);
    game.resume_internal_card().unwrap();
    activate(&mut game, &card, 2);
    assert!(matches!(
        game.activate_internal_card("s", ability, PlayerId { seat: 1 }),
        Err(sve_engine::EngineFailure::IllegalPlay(_))
    ));
}
