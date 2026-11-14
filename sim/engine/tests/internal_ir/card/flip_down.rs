use super::*;
use sve_engine::game::internal::InputDomain;

fn tree(query: &Value, upto: bool) -> Value {
    let mut tree = fanfare::expanded::tree().clone();
    let mut card = tree["cards"]["BP01-041"].clone();
    let mut target = json!({"bind":"turned","from":query});
    target[if upto { "upto" } else { "n" }] = json!(if upto { 2 } else { 1 });
    card["abilities"][0]["activate"] = json!({"cost":{"pp":0},"targets":[target],"do":[{"flip_down":"turned"},{"ep":{"who":"self","delta":1}}]});
    tree["cards"] = json!({"unit-follower":card});
    tree
}

fn query() -> Value {
    json!({"who":"self","zone":"evolve_deck","kind":"evolve_follower","face_up":true})
}

fn program(upto: bool) -> WellFormedCardProgram {
    lowerer()
        .lower(&tree(&query(), upto))
        .unwrap()
        .remove("unit-follower")
        .unwrap()
}

fn board(program: &WellFormedCardProgram) -> Game {
    board_with_maximum(program, 2)
}

fn board_with_maximum(program: &WellFormedCardProgram, maximum: u64) -> Game {
    board_with_resources(program, 2, maximum)
}

fn board_with_resources(program: &WellFormedCardProgram, current: u64, maximum: u64) -> Game {
    let (mut snapshot, registry, document) = catalog_documents(&json!("1"));
    snapshot.push('\n');
    snapshot.push_str(&json!({"number":"form","faces":[{"name":"form","card_class":"ニュートラル","card_type":"フォロワー・エボルヴ","cost":"-","power":"4","hp":"5","traits":[],"text":null,"sections":[]}]}).to_string());
    let forms=json!({"format":1,"kind":"effect_set","cards":{"form":{"status":"complete","abilities":[]}}}).to_string();
    let catalog = Arc::new(
        Catalog::from_documents(
            &snapshot,
            &registry,
            &[("unit.yaml".into(), document), ("form.yaml".into(), forms)],
        )
        .unwrap(),
    );
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["pp"] = json!({"current":current,"max":maximum});
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["ep"] = json!(0);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([
        {"id":"up-a","card":"form","face_up":true},
        {"id":"up-b","card":"form","face_up":true},
        {"id":"down","card":"form","face_up":false}
    ]);
    initial["players"]["P2"]["zones"]["evolve_deck"] =
        json!([{"id":"opposing","card":"form","face_up":true}]);
    let mut game = Game::new(catalog, &initial, &Value::Null, &Value::Null, "flip-down").unwrap();
    game.load_internal_card("unit-follower", program).unwrap();
    game.load_internal_card("form", &card(vec![])).unwrap();
    game
}

#[test]
fn flip_down_one_selected_public_evolve_card_keeps_generation_and_hides_its_location() {
    let mut game = board(&program(false));
    let before = game.projection(View::Referee).unwrap();
    assert!(
        game.projection(View::P2).unwrap()["objects"]
            .get("up-a")
            .is_some()
    );
    let CardPlay::Awaiting(question) = game
        .activate_internal_card("s", AbilityId(1), PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("target question")
    };
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("targets")
    };
    assert_eq!(domain.members.len(), 2);
    let authority = exact_authority(&game);
    game.answer_internal_card(
        PlayerId { seat: 2 },
        question.token,
        Answer::Targets(vec![0]),
    )
    .unwrap_err();
    assert_eq!(exact_authority(&game), authority);
    let result = resolved(
        game.answer_internal_card(question.actor, question.token, Answer::Targets(vec![0]))
            .unwrap(),
    );
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["objects"]["up-a"]["face_up"], false);
    assert_eq!(
        after["objects"]["up-a"]["generation"],
        before["objects"]["up-a"]["generation"]
    );
    assert_eq!(after["objects"]["up-b"]["face_up"], true);
    assert_eq!(after["objects"]["opposing"]["face_up"], true);
    assert_eq!(after["P1"]["ep"], 1);
    assert!(result.progress.receipts.values().any(|outcome| matches!(outcome, InstructionOutcome::Flip(outcome) if outcome.successful.len()==1)));
    let flip = result
        .progress
        .receipts
        .values()
        .find_map(|outcome| {
            let InstructionOutcome::Flip(flip) = outcome else {
                return None;
            };
            Some(flip)
        })
        .unwrap();
    let successful = flip.successful[0];
    assert_ne!(
        successful.before.unwrap().snapshot,
        successful.after.unwrap().snapshot
    );
    let opponent = game.projection(View::P2).unwrap();
    assert!(opponent["objects"].get("up-a").is_none());
    assert_eq!(opponent["P1"]["evolve_deck"][0], json!({"filler":1}));
    assert!(
        game.projection(View::P1).unwrap()["objects"]
            .get("up-a")
            .is_some()
    );
}

#[test]
fn flip_down_up_to_two_accepts_zero_and_two_and_preserves_the_later_ep_step() {
    for handles in [vec![], vec![0, 1]] {
        let mut game = board(&program(true));
        let CardPlay::Awaiting(question) = game
            .activate_internal_card("s", AbilityId(1), PlayerId { seat: 1 })
            .unwrap()
        else {
            panic!("target question")
        };
        let result = resolved(
            game.answer_internal_card(
                question.actor,
                question.token,
                Answer::Targets(handles.clone()),
            )
            .unwrap(),
        );
        assert!(result.progress.receipts.values().any(|outcome| matches!(outcome, InstructionOutcome::Flip(outcome) if outcome.successful.len()==handles.len())));
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["ep"], 1);
        assert_eq!(state["objects"]["up-a"]["face_up"], handles.is_empty());
        assert_eq!(state["objects"]["up-b"]["face_up"], handles.is_empty());
    }
}

#[test]
fn flip_down_rejects_private_opposing_and_non_evolve_queries() {
    for query in [
        json!({"who":"self","zone":"evolve_deck","face_up":false}),
        json!({"who":"self","zone":"evolve_deck"}),
        json!({"who":"opp","zone":"evolve_deck","face_up":true}),
        json!({"who":"self","zone":"field","face_up":true}),
    ] {
        lowerer().lower(&tree(&query, false)).unwrap_err();
    }
    for action in [json!({"flip_up":"turned"}), json!({"flip_down":"this"})] {
        let mut tree = tree(&query(), false);
        tree["cards"]["unit-follower"]["abilities"][0]["activate"]["do"] = json!([action]);
        lowerer().lower(&tree).unwrap_err();
    }
}

#[test]
fn flip_down_raw_ir_cannot_turn_a_selected_field_card_face_down() {
    let mut raw = program(false).card().clone();
    let play = raw.abilities[0].node.play.as_mut().unwrap();
    let ChoiceDomain::Objects { constraint, .. } = &mut play.parameters[0].domain else {
        panic!("object parameter")
    };
    let Selector::Zone {
        zone, predicate, ..
    } = &mut constraint.groups[0].selector
    else {
        panic!("zone selector")
    };
    *zone = ZoneKind::Field;
    **predicate = Expr::Literal(Bool(true));
    let checked = WellFormedCardProgram::check(raw).unwrap();
    let mut game = board(&checked);
    // Decision epochs remain monotonic even when a body rolls back.
    let views = [View::Referee, View::P1, View::P2];
    let before = views.map(|view| game.projection(view).unwrap());
    let CardPlay::Awaiting(question) = game
        .activate_internal_card("s", AbilityId(1), PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("field target")
    };
    let error = game
        .answer_internal_card(question.actor, question.token, Answer::Targets(vec![0]))
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("flip-down requires the controller's evolve deck")
    );
    let after = views.map(|view| game.projection(view).unwrap());
    assert!(
        after == before,
        "a raw bound selector cannot change cards, resources or knowledge outside the evolve deck"
    );
    assert!(game.internal_decision(View::P1).is_none());
}

#[test]
fn flip_down_named_count_drives_scalar_effects_from_actual_successful_members() {
    let mut input = tree(&query(), true);
    input["cards"]["unit-follower"]["abilities"][0]["activate"]["do"] = json!([
        {"flip_down":{"ref":"turned","result":"flipped"}},
        {"if":{"cond":{"cmp":{"left":{"count_of":"flipped"},"op":"eq","right":2}},
            "then":[{"ep":{"who":"self","delta":2}}]}}
    ]);
    let program = lowerer()
        .lower(&input)
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    for handles in [vec![], vec![0], vec![0, 1]] {
        let mut game = board(&program);
        let CardPlay::Awaiting(question) = game
            .activate_internal_card("s", AbilityId(1), PlayerId { seat: 1 })
            .unwrap()
        else {
            panic!("printed public targets")
        };
        let complete = handles.len() == 2;
        let result = resolved(
            game.answer_internal_card(
                question.actor,
                question.token,
                Answer::Targets(handles.clone()),
            )
            .unwrap(),
        );
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["ep"],
            if complete { 2 } else { 0 }
        );
        assert!(
            result
                .progress
                .receipts
                .values()
                .any(|outcome| matches!(outcome,
            InstructionOutcome::Flip(outcome) if outcome.successful.len() == handles.len())),
            "the scalar comparison uses actual successful flips, including zero"
        );
    }
}

#[test]
fn flip_result_does_not_grant_attributes_or_escape_its_optional_branch() {
    let flip = json!({"flip_down":{"ref":"turned","result":"flipped"}});
    for body in [
        json!([flip, {"life":{"who":"self","delta":{"cost_of":"flipped"}}}]),
        json!([flip, {"move":{"ref":{"in":{"objects_of":"flipped","view":"before"}},"to":"hand"}}]),
        json!([flip, {"if":{"cond":{"did":"flipped"},"then":[{"ep":{"delta":1}}]}}]),
        json!([{"optional":{"do":[flip]}}, {"life":{"who":"self","delta":{"count_of":"flipped"}}}]),
        json!([{"flip_down":{"ref":"turned","result":"turned"}}]),
    ] {
        let mut input = tree(&query(), false);
        input["cards"]["unit-follower"]["abilities"][0]["activate"]["do"] = body;
        lowerer().lower(&input).unwrap_err();
    }
}

#[test]
fn public_face_up_evolve_set_is_counted_before_all_its_members_flip() {
    let mut expanded = tree(&query(), false);
    expanded["cards"]["unit-follower"]["abilities"][0]["activate"] = json!({"cost":{"pp":0},"do":[
        {"pp":{"who":"self","delta":{"count":{"who":"self","zone":"evolve_deck","kind":"evolve_follower","face_up":true}}}},
        {"flip_down":{"ref":{"who":"self","zone":"evolve_deck","kind":"evolve_follower","face_up":true}}}
    ]});
    let program = lowerer()
        .lower(&expanded)
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    let mut game = board_with_resources(&program, 0, 10);
    let before = game.projection(View::Referee).unwrap();
    let result = resolved(
        game.activate_internal_card(
            "s",
            program.card().abilities[0].node.id,
            PlayerId { seat: 1 },
        )
        .unwrap(),
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["pp"]["current"], 2_u8);
    for object in ["up-a", "up-b"] {
        assert_eq!(state["objects"][object]["face_up"], false);
        assert_eq!(
            state["objects"][object]["generation"],
            before["objects"][object]["generation"]
        );
        assert!(
            game.projection(View::P2).unwrap()["objects"]
                .get(object)
                .is_none()
        );
    }
    assert_eq!(state["objects"]["opposing"]["face_up"], true);
    assert_eq!(state["objects"]["down"]["face_up"], false);
    assert!(
        result
            .progress
            .receipts
            .values()
            .any(|r| matches!(r,InstructionOutcome::Flip(f) if f.successful.len()==2))
    );
}
