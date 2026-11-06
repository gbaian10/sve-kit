#![expect(clippy::panic, reason = "Printed history choices must have the expected domains.")]
use super::*;
use super::damage_prevent::field_name;
use super::event_cemetery::finish_checks;

fn selected() -> &'static SelectedEffects {
    static SELECTED: OnceLock<SelectedEffects> = OnceLock::new();
    SELECTED.get_or_init(|| {
        let (snapshot, keywords, mut input) = producer_inputs();
        let mut snapshot = snapshot.lines().map(|line| {
            let mut row: Value = serde_json::from_str(line).unwrap();
            if row["number"] == "producer" { row["faces"][0]["traits"] = json!(["ウマ娘"]); }
            row.to_string()
        }).collect::<Vec<_>>().join("\n");
        snapshot.push('\n');
        let mut departurer: Value = serde_json::from_str(snapshot.lines().last().unwrap()).unwrap();
        departurer["number"] = json!("departurer");
        snapshot.push_str(&departurer.to_string());
        snapshot.push('\n');
        input["documents"].as_array_mut().unwrap().push(json!(["departurer.yaml",
            json!({"format":1_u8,"kind":"effect_set","cards":{"departurer":{"status":"complete","abilities":[]}}}).to_string()
        ]));
        departurer["number"] = json!("other-producer");
        departurer["faces"][0]["traits"] = json!(["反例"]);
        snapshot.push_str(&departurer.to_string());
        snapshot.push('\n');
        input["documents"].as_array_mut().unwrap().push(json!(["other-producer.yaml",
            json!({"format":1_u8,"kind":"effect_set","cards":{"other-producer":{"status":"complete","abilities":[]}}}).to_string()
        ]));
        departurer["number"] = json!("returner");
        departurer["faces"][0]["traits"] = json!(["ウマ娘"]);
        snapshot.push_str(&departurer.to_string());
        snapshot.push('\n');
        input["documents"].as_array_mut().unwrap().push(json!(["returner.yaml",
            json!({"format":1_u8,"kind":"effect_set","cards":{"returner":{"status":"complete","abilities":[]}}}).to_string()
        ]));
        repository::selection_with_rules_from(&snapshot, &keywords, &input)
    })
}

fn hit(name: &str, amount: i64) -> Action {
    Action::Damage {
        recipients: RecipientSet::Objects { objects: field_name(name) },
        amount: DamageAmount::Uniform { amount: Expr::Literal(Int(amount)) },
        flags: DamageFlags::Ability, source: ObjectView::SourceInformation {},
        batch: BatchPolicy::Instruction {},
    }
}

fn move_named(name: &str, zone: OwnerZone) -> Action {
    Action::Move {
        objects: field_name(name),
        movement: MoveSpec { to: MoveDestination::Owners { zone }, initial_posture: None, entry_effects: vec![], position: None, order: None },
        reason: MoveReason::Put,
    }
}

fn production(number: &str, mut steps: Vec<Action>, quick: bool) -> WellFormedCardProgram {
    for (index, action) in steps.iter_mut().enumerate() {
        let (Action::Move { objects, .. } | Action::Destroy { objects }
            | Action::Damage { recipients: RecipientSet::Objects { objects }, .. }) = action
            else {panic!("named public action")};
        let Selector::Zone { binding, predicate, .. } = objects else {panic!("named field")};
        *binding = Binding::new(50_u64.checked_add(u64::try_from(index).unwrap()).unwrap(), ScopeId(1));
        let Expr::Compute(operation) = predicate.as_mut() else {panic!("name filter")};
        let Predicate::NamesIntersect { left, .. } = operation.as_mut() else {panic!("name filter")};
        *left = Expr::Live(Box::new(NamesRead::Names { object: ObjectView::Current {
            object: Box::new(ObjectOperand::Bound { binding: *binding }),
        } }));
    }
    let mut raw = producer_steps(steps).card().clone();
    for ability in &mut raw.abilities {
        number.clone_into(&mut ability.source.card_no);
        number.clone_into(&mut ability.source.card_id);
        if quick {
            ability.node.source.origin = AbilityOrigin::Keyword {
                keyword: KeywordId::Quick, origin: Box::new(ability.node.source.origin.clone()),
            };
        }
        let body = ability.node.body.as_mut().unwrap();
        for step in &mut body.steps {
            number.clone_into(&mut step.source.card_no);
            number.clone_into(&mut step.source.card_id);
            let Node::Instruction { instruction } = &mut step.node else {panic!("public instruction")};
            instruction.source = ability.node.source.clone();
        }
    }
    WellFormedCardProgram::check(raw).unwrap()
}

fn prepared(number: &str, side: &str, steps: Vec<Action>) -> Game {
    prepared_source(number,side,true,steps)
}

fn prepared_source(number: &str, side: &str, uma: bool, steps: Vec<Action>) -> Game {
    let mut enter = move_named("セージコマンダー",OwnerZone::Cemetery);
    let Action::Move {objects: Selector::Zone {zone,players,..}, movement,..} = &mut enter else {panic!("public return")};
    *zone = ZoneKind::Cemetery;
    players.relation = PlayerRelation::SelfPlayer;
    players.cardinality = Cardinality {minimum:Nat(1),maximum:Some(Nat(1))};
    players.text_quantifier = TextQuantifier::Implicit;
    movement.to = MoveDestination::Shared {zone: ZoneRef {player: PlayerRef::Controller {},kind:ZoneKind::Field}};
    prepared_returner(number,side,uma,steps,vec![enter,move_named("セージコマンダー",OwnerZone::Cemetery)])
}

fn prepared_returner(number: &str, side: &str, uma: bool, steps: Vec<Action>, returner: Vec<Action>) -> Game {
    let mut initial = draw_position();
    initial["turn"]["active"] = json!(side);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"returner","card":"returner"}]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"observer","card":number},{"id":"departurer","card":"departurer"}]);
    initial["players"][side]["zones"]["field"].as_array_mut().unwrap().push(json!({"id":"producer","card":if uma {"producer"} else {"other-producer"}}));
    initial["players"]["P2"]["zones"]["field"].as_array_mut().unwrap().extend([
        json!({"id":"victim","card":"BP01-037"}),json!({"id":"other","card":"BP01-118"})
    ]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"draw","card":"BP01-037"},{"id":"draw2","card":"BP01-037"}]);
    initial["players"]["P2"]["zones"]["deck"] = json!([{"id":"p2-draw","card":"BP01-037"},{"id":"p2-draw2","card":"BP01-037"}]);
    let mut game = Game::from_selected_effects(selected().clone(), &initial, &Value::Null, &Value::Null, "source-link").unwrap();
    let producer = if uma {"producer"} else {"other-producer"};
    game.load_internal_card(producer, &production(producer,steps,false)).unwrap();
    game.load_internal_card("departurer", &production("departurer",vec![move_named("セージコマンダー",OwnerZone::Cemetery)],false)).unwrap();
    game.load_internal_card("returner", &production("returner",returner,true)).unwrap();
    game
}

#[test]
fn cheval_grand_printings_join_the_exact_damaged_follower_and_own_source_at_departure() {
    for number in ["BP22-R22","BP22-SL34","ECP01-020","ECP01-SL10","ECP01-SP10","ECP01-SSP10"] {
        for (side, uma, amount, victim, expected) in [
            ("P1",true,1_i64,"セージコマンダー",true),
            ("P1",true,0,"セージコマンダー",false),
            ("P1",true,1,"ファイター",false),
            ("P1",false,1,"セージコマンダー",false),
            ("P2",true,1,"セージコマンダー",false),
        ] {
            let mut game = prepared_source(number, side, uma, vec![hit("セージコマンダー",amount),move_named(victim,OwnerZone::Cemetery)]);
            activate(&mut game, side);
            finish_checks(&mut game);
            assert_eq!(game.internal_pending_choice().unwrap().is_some(),expected);
            if expected {
                let question = game.internal_pending_choice().unwrap().unwrap();
                let InputDomain::Pending(options) = &question.domain else {panic!("departure")};
                assert_eq!(options.len(),1);
                let play = game.answer_internal_card(question.actor,question.token,Answer::Pending(options[0].handle)).unwrap();
                event_subject_capture::finish(&mut game,play);
                let state = game.projection(View::Referee).unwrap();
                assert_eq!(state["P1"]["deck"].as_array().unwrap().len(),1);
                assert_eq!(state["P1"]["hand"].as_array().unwrap().len(),0);
                assert!(state["P1"]["cemetery"].as_array().unwrap().contains(&json!("draw")));
            }
        }
    }
}

#[test]
fn cheval_grand_evolve_damage_uses_the_current_own_cemetery_trait_count() {
    for (count, damage) in [(9_usize,2_u8),(10,4)] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"cheval-capture-base"}]);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP22-R22"}]);
        initial["players"]["P1"]["zones"]["cemetery"] = json!((0..count).map(|i|json!({"id":format!("uma-{i}"),"card":"CP01-001"})).collect::<Vec<_>>());
        initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
        let mut game = capture_extra::prepared(&initial);
        game.internal_rule_action(&json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":2_u8}})).unwrap();
        event_subject_capture::resolve(&mut game,"BP22-R22",1,None);
        assert_eq!(game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],damage);
    }
}

fn opposing_quick(game: &mut Game) {
    game.internal_rule_action(&json!({"do":"end-phase"})).unwrap();
    finish_checks(game);
    let question = game.internal_quick_choice().unwrap().unwrap();
    let InputDomain::Quick { options, .. } = &question.domain else {panic!("opposing Quick")};
    let option = options.iter().find(|option|option.card == "returner").unwrap();
    let play = game.answer_internal_card(question.actor,question.token,Answer::Quick(Some(option.handle))).unwrap();
    event_subject_capture::finish(game,play);
}

#[test]
fn departure_history_does_not_follow_a_recipient_into_a_new_field_generation() {
    let mut game = prepared("BP22-R22", "P1", vec![hit("セージコマンダー",1),move_named("セージコマンダー",OwnerZone::Cemetery)]);
    activate(&mut game,"P1");
    finish_checks(&mut game);
    event_subject_capture::resolve(&mut game,"BP22-R22",1,None);
    opposing_quick(&mut game);
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {panic!("returned follower Fanfare")};
    assert!(options.iter().all(|option|option.card == "BP01-037"),"{options:?}");
    let play = game.answer_internal_card(question.actor,question.token,Answer::Pending(options[0].handle)).unwrap();
    event_subject_capture::finish(&mut game,play);
    assert!(game.internal_pending_choice().unwrap().is_none());
    assert_eq!(game.projection(View::Referee).unwrap()["P1"]["deck"].as_array().unwrap().len(),1);
}

#[test]
fn an_opposing_uma_source_does_not_qualify_during_the_observers_own_turn() {
    let mut game = prepared_returner("BP22-R22","P1",true,vec![],vec![hit("セージコマンダー",1),move_named("セージコマンダー",OwnerZone::Cemetery)]);
    opposing_quick(&mut game);
    assert!(game.internal_pending_choice().unwrap().is_none());
}

#[test]
fn departure_history_is_cleared_before_the_next_turn() {
    let mut game = prepared("BP22-R22", "P1", vec![hit("セージコマンダー",1)]);
    activate(&mut game,"P1");
    finish_checks(&mut game);
    for _ in 0_u8..2 {
        game.internal_rule_action(&json!({"do":"end-phase"})).unwrap();
        finish_checks(&mut game);
        damage_prevent::pass_quick(&mut game);
        game.internal_rule_action(&json!({"do":"end-discard","select":[]})).unwrap();
        finish_checks(&mut game);
    }
    resolved(game.activate_internal_card("departurer",AbilityId(1),PlayerId{seat:1}).unwrap());
    finish_checks(&mut game);
    assert!(game.internal_pending_choice().unwrap().is_none());
}
