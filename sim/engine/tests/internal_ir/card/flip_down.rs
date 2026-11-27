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

pub(super) fn program(upto: bool) -> WellFormedCardProgram {
    lowerer()
        .lower(&tree(&query(), upto))
        .unwrap()
        .remove("unit-follower")
        .unwrap()
}

pub(super) fn board(program: &WellFormedCardProgram) -> Game {
    board_with_maximum(program, 2)
}

fn board_with_maximum(program: &WellFormedCardProgram, maximum: u64) -> Game {
    board_with_resources(program, 2, maximum)
}

fn board_with_resources(program: &WellFormedCardProgram, current: u64, maximum: u64) -> Game {
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
    board_from_position(program, &initial)
}

fn board_from_position(program: &WellFormedCardProgram, initial: &Value) -> Game {
    let (mut snapshot, registry, document) = catalog_documents(&json!("1"));
    let forms = [
        ("form", "フォロワー・エボルヴ", "土属性"),
        ("other-form", "フォロワー・エボルヴ", "闇属性"),
        ("evolved-amulet", "アミュレット・エボルヴ", "土属性"),
        ("advanced-follower", "フォロワー・アドバンス", "土属性"),
    ];
    let mut effects = json!({"format":1,"kind":"fixture_cards","cards":{}});
    for (number, kind, attribute) in forms {
        snapshot.push('\n');
        snapshot.push_str(&json!({"number":number,"faces":[{"name":number,"card_class":"ニュートラル","card_type":kind,"cost":"-","power":"4","hp":"5","traits":[attribute],"text":null,"sections":[]}]}).to_string());
        effects["cards"][number] = json!({"status":"complete"});
    }
    let catalog = Arc::new(
        Catalog::from_fixture_documents(
            &snapshot,
            &registry,
            &[
                ("unit.yaml".into(), document),
                ("form.yaml".into(), effects.to_string()),
            ],
        )
        .unwrap(),
    );
    let mut game = Game::new(catalog, initial, &Value::Null, &Value::Null, "evolve-flip").unwrap();
    game.load_internal_card("unit-follower", program).unwrap();
    let form_program = card(vec![]);
    for (number, _, _) in forms {
        game.load_internal_card(number, &form_program).unwrap();
    }
    game
}

#[test]
fn end_phase_flip_up_payment_accepts_any_owned_facedown_evolve_deck_card() {
    let expanded = evolution_linked_movement::tree(&[(
        "auto",
        json!({
            "on":{"phase":{"whose":"self","which":"end"}},
            "cost":{"flip_up":{"sel":{"who":"self","zone":"evolve_deck"},"n":1_u8}},
            "do":[{"life":{"who":"self","delta":1}}]
        }),
    )]);
    let programs = lowerer().lower(&expanded).unwrap();
    let program = &programs["unit-follower"];
    for chosen in [Some("evolved-amulet"), Some("advanced-follower"), None] {
        let mut initial = setup(1, json!([]));
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([
            {"id":"evolved-amulet","card":"evolved-amulet"},
            {"id":"advanced-follower","card":"advanced-follower"},
            {"id":"open","card":"form","face_up":true}
        ]);
        initial["players"]["P2"]["zones"]["evolve_deck"] =
            json!([{"id":"enemy-hidden","card":"evolved-amulet"}]);
        let mut game = board_from_position(program, &initial);
        game.internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap();
        let pending = game.internal_pending_choice().unwrap().unwrap();
        let CardPlay::Awaiting(declaration) = game
            .answer_internal_card(pending.actor, pending.token, Answer::Pending(1))
            .unwrap()
        else {
            panic!("flip payment declaration");
        };
        assert_eq!(declaration.domain, InputDomain::OptionalCost);
        let result = game
            .answer_internal_card(
                declaration.actor,
                declaration.token,
                Answer::OptionalCost(chosen.is_some()),
            )
            .unwrap();
        if let Some(printing) = chosen {
            let CardPlay::Awaiting(materials) = result else {
                panic!("flip payment materials");
            };
            let InputDomain::CostMaterials(domain) = &materials.domain else {
                panic!("one private material");
            };
            assert_eq!(domain.members.len(), 2);
            assert_eq!(domain.cardinality.minimum, Nat(1));
            assert!(game.internal_decision(View::P2).is_none());
            let member = domain
                .members
                .iter()
                .find(|member| member.card.as_deref() == Some(printing))
                .unwrap();
            let before = exact_authority(&game);
            game.answer_internal_card(
                materials.actor,
                materials.token,
                Answer::CostMaterials(vec![]),
            )
            .unwrap_err();
            assert_eq!(exact_authority(&game), before);
            resolved(
                game.answer_internal_card(
                    materials.actor,
                    materials.token,
                    Answer::CostMaterials(vec![member.handle]),
                )
                .unwrap(),
            );
        } else {
            assert!(matches!(result, CardPlay::Cancelled));
        }
        let authority = game.projection(View::Referee).unwrap();
        let opponent = game.projection(View::P2).unwrap();
        for material in ["evolved-amulet", "advanced-follower"] {
            let paid = chosen == Some(material);
            assert_eq!(authority["objects"][material]["face_up"], paid);
            assert_eq!(authority["objects"][material]["generation"], 0_u8);
            assert_eq!(opponent["objects"].get(material).is_some(), paid);
        }
        assert_eq!(authority["objects"]["enemy-hidden"]["face_up"], false);
        assert_eq!(
            authority["P1"]["leader"]["life"],
            20_u8 + u8::from(chosen.is_some())
        );
    }
}

fn flip_up_tree(selection: &Value) -> Value {
    let mut tree = tree(&query(), false);
    tree["cards"]["unit-follower"]["abilities"][0]["activate"] = json!({
        "cost":{"pp":0},
        "do":[
            {"flip_up":selection},
            {"if":{"cond":{"cmp":{"op":"ge","right":4,"left":{"count":{
                "who":"self","zone":"evolve_deck","kind":"evolve_follower","face_up":true,"trait":"土属性"
            }}}},"then":[{"life":{"who":"self","delta":2}}]}},
            {"ep":{"who":"self","delta":1}}
        ]
    });
    tree
}

fn flip_up_position() -> Value {
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
    initial["players"]["P1"]["ep"] = json!(0);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([
        {"id":"open-a","card":"form","face_up":true},
        {"id":"open-b","card":"form","face_up":true},
        {"id":"hidden-a","card":"form","face_up":false},
        {"id":"hidden-b","card":"form","face_up":false},
        {"id":"other","card":"other-form","face_up":false}
    ]);
    initial["players"]["P2"]["zones"]["evolve_deck"] =
        json!([{"id":"enemy","card":"form","face_up":false}]);
    initial
}

#[test]
fn body_flip_up_keeps_options_private_and_updates_the_later_public_count() {
    // CP05-054 may flip up to two earth evolutions. CP05-069 and CP05-091
    // read the new face-up total after their optional flip.
    let program = lowerer()
        .lower(&flip_up_tree(&json!({
            "sel":{"who":"self","zone":"evolve_deck","kind":"evolve_follower","trait":"土属性"},
            "upto":2
        })))
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    let initial = flip_up_position();
    for choose in [false, true] {
        let mut game = board_from_position(&program, &initial);
        let before = game.projection(View::Referee).unwrap();
        let CardPlay::Awaiting(question) = game
            .activate_internal_card("s", AbilityId(1), PlayerId { seat: 1 })
            .unwrap()
        else {
            panic!("face-up selection")
        };
        assert!(
            game.internal_decision(View::P2).is_none(),
            "the opponent cannot see this choice"
        );
        let InputDomain::Targets(domain) = &question.domain else {
            panic!("private evolve-deck options")
        };
        assert_eq!(
            domain.members.len(),
            2,
            "only face-down earth evolutions in the chooser's deck qualify"
        );
        assert_eq!(domain.cardinality.minimum, Nat(0), "the flip is optional");
        assert_eq!(
            domain.cardinality.maximum,
            Some(Nat(2)),
            "the printed upper bound applies"
        );
        let selected = domain
            .members
            .iter()
            .filter(|_| choose)
            .map(|member| member.handle)
            .collect();
        resolved(
            game.answer_internal_card(question.actor, question.token, Answer::Targets(selected))
                .unwrap(),
        );
        let after = game.projection(View::Referee).unwrap();
        let opponent = game.projection(View::P2).unwrap();
        assert_eq!(
            after["P1"]["evolve_deck"], before["P1"]["evolve_deck"],
            "the cards keep their positions"
        );
        for id in ["hidden-a", "hidden-b"] {
            assert_eq!(
                after["objects"][id]["face_up"], choose,
                "only a selected card flips"
            );
            assert_eq!(
                after["objects"][id]["generation"], before["objects"][id]["generation"],
                "flipping does not create a new generation"
            );
            assert_eq!(
                opponent["objects"].get(id).is_some(),
                choose,
                "only the successful flip exposes the card"
            );
        }
        assert_eq!(
            after["objects"]["other"]["face_up"], false,
            "the unrelated attribute stays down"
        );
        assert!(
            opponent["objects"].get("other").is_none(),
            "the unrelated card stays hidden"
        );
        assert_eq!(
            after["objects"]["enemy"]["face_up"], false,
            "the other player's cards stay down"
        );
        assert_eq!(
            after["P1"]["ep"], 1,
            "the next instruction executes even with a zero selection"
        );
        assert_eq!(
            after["P1"]["leader"]["life"],
            if choose { 22_u8 } else { 20_u8 },
            "the follow-up condition reads the new face-up total"
        );
    }
}

#[test]
fn body_flip_up_requires_own_facedown_followers_and_a_positive_bound() {
    for selection in [
        json!({"sel":{"who":"self","zone":"evolve_deck"},"upto":1}),
        json!({"sel":{"who":"opp","zone":"evolve_deck","kind":"evolve_follower"},"upto":1}),
        json!({"sel":{"who":"self","zone":"evolve_deck","kind":"evolve_follower","face_up":true},"upto":1}),
        json!({"sel":{"who":"self","zone":"evolve_deck","kind":"evolve_follower"},"upto":0}),
    ] {
        lowerer().lower(&flip_up_tree(&selection)).unwrap_err();
    }
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
            .contains("flip requires the controller's evolve deck")
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
