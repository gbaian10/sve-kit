use super::*;
use crate::rule_fixtures;
use alloc::collections::BTreeMap;
use sve_engine::catalog::EngineIdentityInput;
use sve_engine::game::internal::{Answer, CardPlay, InputDomain};

fn history_catalog(lesson_role: bool) -> Arc<Catalog> {
    let (mut snapshot, registry, document) = catalog_documents(&json!("1"));
    snapshot.push('\n');
    snapshot.push_str(&json!({"number":"lesson","faces":[{"name":"魔法のアイテム","card_class":"ニュートラル","card_type":"スペル・トークン","cost":"1","power":"-","hp":"-","traits":[],"text":null,"sections":[]}]}).to_string());
    snapshot.push('\n');
    snapshot.push_str(&json!({"number":"unit-other","faces":[{"name":"unit-other","card_class":"ニュートラル","card_type":"フォロワー","cost":"1","power":"4","hp":"3","traits":[],"text":null,"sections":[]}]}).to_string());
    let mut document: Value = serde_json::from_str(&document).unwrap();
    document["cards"]["unit-other"] = json!({"status":"complete","abilities":[]});
    document["cards"]["lesson"] = json!({"status":"complete","abilities":[]});
    if !lesson_role {
        return Arc::new(
            Catalog::from_documents(
                &snapshot,
                &registry,
                &[("history.yaml".into(), document.to_string())],
            )
            .unwrap(),
        );
    }
    let resources = json!([rule_fixtures::resource(
        "lesson_item",
        "lesson",
        "printed",
        true
    )]);
    let rules = rule_fixtures::rules(&snapshot, json!([]), resources).to_string();
    Arc::new(
        Catalog::from_documents_with_rules(
            &snapshot,
            &registry,
            &[("history.yaml".into(), document.to_string())],
            &rules,
            &EngineIdentityInput::LegacyJp,
        )
        .unwrap(),
    )
}

fn loads(head: &Value) {
    let mut programs = lowerer()
        .lower(&equipment_played::tree(head, "unit-follower"))
        .unwrap();
    let mut board = Game::new(
        history_catalog(true),
        &setup(1, json!([])),
        &Value::Null,
        &Value::Null,
        "public-history-preflight",
    )
    .unwrap();
    board
        .load_internal_card("unit-follower", &programs.remove("unit-follower").unwrap())
        .unwrap();
}

fn departure(history: &Value) -> Value {
    json!({"auto":{"on":{"left_field":{"ref":"this"}},"do":[
        {"if":{"cond":{"history":history},"then":[{"draw":{"who":"self","n":1}}]}}
    ]}})
}

#[test]
fn departure_with_a_supported_played_history_condition_is_a_valid_baseline() {
    loads(&departure(
        &json!({"event":"played","window":"this_turn","who":"self"}),
    ));
}

#[test]
fn complete_act_and_self_cemetery_cost_with_a_formal_buff_target_is_a_valid_baseline() {
    loads(&json!({"activate":{
        "cost":{"steps":[{"act_self":true},{"self_to_cemetery":true}]},
        "targets":[{"bind":"friend","from":{"who":"self","zone":"field","kind":"follower"},"n":1}],
        "do":[{"buff":{"on":"friend","power":1,"hp":1}}]
    }}));
}

#[test]
fn ordinary_evolution_time_destruction_of_all_opposing_followers_is_a_valid_baseline() {
    loads(&json!({"evolve_time":{"do":[{"destroy":{"ref":{
        "who":"opp","zone":"field","kind":"follower"
    }}}]}}));
}

#[test]
fn committed_ex_banish_remains_visible_after_the_token_has_been_erased() {
    let mut board = position();
    activate_line(&mut board, "s", "unit-follower", 3);
    let view = board.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["ex"], json!([]));
    assert_eq!(view["P1"]["cemetery"], json!([]));
    read_lesson(&mut board);
    assert_eq!(
        board.projection(View::Referee).unwrap()["P1"]["hand"],
        json!(["d0"])
    );
}

#[test]
fn current_followers_damaged_before_the_consumer_exists_are_destroyed_once() {
    let mut board = position();
    hit(&mut board, 1);
    hit(&mut board, 1);
    destroy_damaged(&mut board);
    let view = board.projection(View::Referee).unwrap();
    assert_eq!(view["P2"]["field"], json!(["untouched"]));
    assert_eq!(view["P2"]["cemetery"], json!(["victim"]));
    assert_eq!(view["P1"]["field"], json!(["s"]));
}

fn position() -> Game {
    position_with(|_| {})
}

fn position_with(change: impl FnOnce(&mut Value)) -> Game {
    position_with_other(change, &operator("unit-other"))
}

fn position_with_other(change: impl FnOnce(&mut Value), other: &WellFormedCardProgram) -> Game {
    let mut initial = setup(
        1,
        json!(
            (0_u8..8)
                .map(|i| json!({"id":format!("d{i}"),"card":"unit-follower"}))
                .collect::<Vec<_>>()
        ),
    );
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
    initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"item","card":"lesson"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"victim","card":"unit-follower","state":{"power":2}},
        {"id":"untouched","card":"unit-other","state":{"power":4}}
    ]);
    initial["players"]["P2"]["zones"]["deck"] = json!(
        (0_u8..8)
            .map(|i| json!({"id":format!("enemy-d{i}"),"card":"unit-follower"}))
            .collect::<Vec<_>>()
    );
    change(&mut initial);
    let mut board = Game::new(
        history_catalog(true),
        &initial,
        &Value::Null,
        &Value::Null,
        "public-history-runtime",
    )
    .unwrap();
    board
        .load_internal_card("unit-follower", &operator("unit-follower"))
        .unwrap();
    board.load_internal_card("unit-other", other).unwrap();
    let lesson = lowerer()
        .lower_with_printed_costs(
            &equipment_played::tree(
                &json!({"spell":{"do":[{"draw":{"who":"self","n":0}}]}}),
                "lesson",
            ),
            &BTreeMap::from([("lesson".to_owned(), Nat(1))]),
        )
        .unwrap()
        .remove("lesson")
        .unwrap();
    board.load_internal_card("lesson", &lesson).unwrap();
    board
}

fn operator_input(number: &str) -> Value {
    let foe = json!({"who":"opp","zone":"field","kind":"follower","name":"unit-follower"});
    let actions = [
        json!([{"damage":{"to":foe,"n":1}}]),
        json!([{"damage":{"to":foe,"n":0}}]),
        json!([{"draw":{"who":"self","n":0}}]),
        json!([{"if":{"cond":{"history":{"event":"banished","window":"this_turn","who":"self","from":"ex","sel":{"name":"魔法のアイテム"}}},"then":[{"draw":{"who":"self","n":1}}]}}]),
        json!([{"destroy":{"ref":{"who":"opp","zone":"field","kind":"follower","where":{"history":{"event":"damaged","window":"this_turn","where":{"recipient":"item"}}}}}}]),
        json!([{"draw":{"who":"self","n":0}}]),
        json!([{"draw":{"who":"self","n":0}}]),
        json!([{"damage":{"to":{"who":"self","zone":"field","kind":"follower"},"n":1}}]),
        json!([{"draw":{"who":"self","n":0}}]),
        json!([{"destroy":{"ref":{"who":"self","zone":"field","kind":"amulet"}}}]),
        json!([{"draw":{"who":"self","n":0}}]),
        json!([{"destroy":{"ref":{"who":"opp","zone":"field","kind":"amulet"}}}]),
    ];
    let mut input = equipment_played::tree(&json!({"activate":{"cost":{"pp":0},"do":[]}}), number);
    let template = input["cards"][number]["abilities"][0].clone();
    input["cards"][number]["abilities"] = json!(
        actions
            .into_iter()
            .enumerate()
            .map(|(i, actions)| {
                let mut entry = template.clone();
                entry["activate"]["do"] = actions;
                entry["line"] = json!(i.checked_add(1).unwrap());
                entry["sources"][0]["ability_scope"] =
                    json!(format!("public_history_operator_{i}"));
                if i == 6 && number == "unit-other" {
                    entry.as_object_mut().unwrap().remove("activate");
                    entry["auto"] = json!({"on":{"left_field":{"who":"self","sel":{"name":"unit-follower"}}},"do":[{"draw":{"who":"self","n":0}}]});
                }
                if i == 10 {
                    entry.as_object_mut().unwrap().remove("activate");
                    entry["auto"] = json!({"on":{"left_field":{"who":"self","sel":{"kind":"amulet"}}},"do":[{"draw":{"who":"self","n":0}}]});
                }
                entry
            })
            .collect::<Vec<_>>()
    );
    input
}

fn operator(number: &str) -> WellFormedCardProgram {
    let accepted = lowerer()
        .lower(&operator_input(number))
        .unwrap()
        .remove(number)
        .unwrap();
    let mut raw = accepted.card().clone();
    for (index, relation) in [
        (2, PlayerRelation::SelfPlayer),
        (8, PlayerRelation::Opponents),
        (10, PlayerRelation::SelfPlayer),
    ] {
        let body = raw.abilities[index].node.body.as_mut().unwrap();
        let objects = Selector::Zone {
            players: players(relation),
            zone: ZoneKind::Ex,
            view: SelectionView::Current {},
            binding: Binding::new(9000, body.scope),
            predicate: Box::new(Expr::Literal(Bool(true))),
        };
        if let Node::Instruction { instruction } = &mut body.steps[0].node {
            instruction.action = Action::Banish { objects };
        }
    }
    for index in [5, 6] {
        let body = raw.abilities[index].node.body.as_mut().unwrap();
        let candidate = Binding::new(9001, body.scope);
        let objects = Selector::Zone {
            players: players(if index == 5 {
                PlayerRelation::Opponents
            } else {
                PlayerRelation::SelfPlayer
            }),
            zone: if index == 5 {
                ZoneKind::Field
            } else {
                ZoneKind::Ex
            },
            view: SelectionView::Current {},
            binding: candidate,
            predicate: Box::new(Expr::Compute(Box::new(Predicate::NamesIntersect {
                left: Expr::Live(Box::new(NamesRead::Names {
                    object: ObjectView::Current {
                        object: Box::new(ObjectOperand::Bound { binding: candidate }),
                    },
                })),
                right: Expr::Literal(vec![CardName("unit-follower".into())]),
            }))),
        };
        if let Node::Instruction { instruction } = &mut body.steps[0].node {
            instruction.action = Action::Move {
                objects,
                movement: MoveSpec {
                    to: if index == 5 {
                        MoveDestination::Owners {
                            zone: OwnerZone::Ex,
                        }
                    } else {
                        MoveDestination::Shared {
                            zone: ZoneRef {
                                player: PlayerRef::Controller {},
                                kind: ZoneKind::Field,
                            },
                        }
                    },
                    position: None,
                    order: None,
                    initial_posture: None,
                    entry_effects: vec![],
                },
                reason: MoveReason::Put,
            };
        }
    }
    WellFormedCardProgram::check(raw).unwrap()
}
fn activate_line(board: &mut Game, holder: &str, number: &str, line: u32) {
    let definition = operator(number);
    let id = definition
        .card()
        .abilities
        .iter()
        .find(|a| matches!(a.node.source.origin,AbilityOrigin::Printed { line:Nat(n), .. } if n==u64::from(line)))
        .unwrap()
        .node
        .id;
    let actor = if holder == "s" { 1 } else { 2 };
    let play = board
        .activate_internal_card(holder, id, PlayerId { seat: actor })
        .unwrap();
    assert!(
        matches!(play, CardPlay::Resolved(_)),
        "the fully loaded synthetic operator must finish"
    );
    if board.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        board.resume_internal_card().unwrap();
    }
}
fn hit(board: &mut Game, amount: i64) {
    activate_line(board, "s", "unit-follower", if amount == 0 { 2 } else { 1 });
}
fn read_lesson(board: &mut Game) {
    activate_line(board, "s", "unit-follower", 4);
}
fn destroy_damaged(board: &mut Game) {
    activate_line(board, "s", "unit-follower", 5);
}
fn next_turn(board: &mut Game) {
    board
        .internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    if let Some(q) = board.internal_quick_choice().unwrap() {
        board
            .answer_internal_card(q.actor, q.token, Answer::Quick(None))
            .unwrap();
    }
    if board.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        board.resume_internal_card().unwrap();
    }
}

#[test]
fn zero_damage_does_not_mark_a_candidate_or_destroy_the_uninjured() {
    let mut board = position();
    hit(&mut board, 0);
    destroy_damaged(&mut board);
    assert_eq!(
        board.projection(View::Referee).unwrap()["P2"]["field"],
        json!(["victim", "untouched"])
    );
}

#[test]
fn a_new_field_generation_does_not_inherit_the_old_damage_history() {
    let mut board = position();
    hit(&mut board, 1);
    activate_line(&mut board, "s", "unit-follower", 6);
    let question = board.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("the departure must produce the opponent's return occurrence");
    };
    assert_eq!(
        options.len(),
        1,
        "one real departure creates one occurrence"
    );
    resolved(
        board
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap(),
    );
    board.resume_internal_card().unwrap();
    destroy_damaged(&mut board);
    let view = board.projection(View::Referee).unwrap();
    assert_eq!(view["P2"]["cemetery"], json!([]));
    assert_eq!(view["P2"]["field"], json!(["untouched", "victim"]));
}

#[test]
fn the_lesson_counter_and_object_history_reset_at_the_next_turn() {
    let mut board = position();
    hit(&mut board, 1);
    activate_line(&mut board, "s", "unit-follower", 3);
    next_turn(&mut board);
    next_turn(&mut board);
    let hand = board.projection(View::Referee).unwrap()["P1"]["hand"].clone();
    read_lesson(&mut board);
    destroy_damaged(&mut board);
    let view = board.projection(View::Referee).unwrap();
    assert!(
        view["P2"]["field"]
            .as_array()
            .unwrap()
            .iter()
            .any(|id| id == "victim")
    );
    assert_eq!(view["P1"]["hand"], hand);
}

#[test]
fn an_unbound_or_different_lesson_name_is_rejected_by_whole_card_loading() {
    for (role, name) in [(false, "魔法のアイテム"), (true, "unit-follower")] {
        let program=lowerer().lower(&equipment_played::tree(&departure(&json!({"event":"banished","window":"this_turn","who":"self","from":"ex","sel":{"name":name}})),"unit-follower")).unwrap().remove("unit-follower").unwrap();
        let mut board = Game::new(
            history_catalog(role),
            &setup(1, json!([])),
            &Value::Null,
            &Value::Null,
            "history-binding-negative",
        )
        .unwrap();
        assert!(
            board.load_internal_card("unit-follower", &program).is_err(),
            "history requires the actual bound resource role"
        );
    }
}

#[test]
fn banish_history_refuses_unbound_queries_other_players_zones_and_numeric_reads() {
    let accepted = json!({"event":"banished","window":"this_turn","who":"self","from":"ex","sel":{"name":"魔法のアイテム"}});
    for (key, value) in [
        ("who", json!("opp")),
        ("from", json!("cemetery")),
        ("window", json!("this_game")),
        ("exclude", json!("this_execution")),
        ("distinct", json!("subjects")),
        ("at_least", json!(2)),
        ("sel", json!({"name":""})),
        ("sel", json!({"names":["魔法のアイテム","other"]})),
        ("sel", json!({"name":"魔法のアイテム","kind":"spell"})),
    ] {
        let mut query = accepted.clone();
        query[key] = value;
        assert!(
            lowerer()
                .lower(&equipment_played::tree(&departure(&query), "unit-follower"))
                .is_err(),
            "unsupported history variants must fail before runtime"
        );
    }
    let head = json!({"activate":{"cost":{"pp":0},"do":[{"life":{"who":"self","delta":{"history_count":accepted}}}]}});
    assert!(
        lowerer()
            .lower(&equipment_played::tree(&head, "unit-follower"))
            .is_err(),
        "the lesson condition does not expose a general count"
    );
    let head =
        json!({"auto":{"on":{"banished":{"ref":"this"}},"do":[{"draw":{"who":"self","n":1}}]}});
    assert!(
        lowerer()
            .lower(&equipment_played::tree(&head, "unit-follower"))
            .is_err(),
        "the history plan does not admit a new event subscription"
    );
}

#[test]
fn damaged_candidate_history_stays_in_its_exact_public_destroy_selector() {
    let base = json!({"who":"opp","zone":"field","kind":"follower","where":{"history":{"event":"damaged","window":"this_turn","where":{"recipient":"item"}}}});
    for (key, value) in [
        ("who", json!("self")),
        ("zone", json!("hand")),
        ("kind", json!("amulet")),
        ("name", json!("unit-follower")),
        ("as", json!("another")),
        (
            "where",
            json!({"history":{"event":"damaged","window":"this_turn","where":{"recipient":"this"}}}),
        ),
        (
            "where",
            json!({"history":{"event":"damaged","window":"this_turn","where":{"recipient":"item","kind":"ability"}}}),
        ),
        (
            "where",
            json!({"history":{"event":"damaged","window":"this_game","where":{"recipient":"item"}}}),
        ),
    ] {
        let mut selector = base.clone();
        selector[key] = value;
        let input = equipment_played::tree(
            &json!({"evolve_time":{"do":[{"destroy":{"ref":selector}}]}}),
            "unit-follower",
        );
        assert!(
            lowerer().lower(&input).is_err(),
            "a broader candidate grammar must not be admitted"
        );
    }
    let input = equipment_played::tree(
        &json!({"evolve_time":{"do":[{"banish":{"ref":base}}]}}),
        "unit-follower",
    );
    assert!(
        lowerer().lower(&input).is_err(),
        "the predicate has only its reviewed destruction consumer"
    );
}

#[test]
fn no_lesson_banish_means_no_draw_and_each_wrong_owner_remains_independent() {
    let mut board = position_with(|initial| {
        initial["players"]["P2"]["zones"]["ex"] = json!([{"id":"opponent-item","card":"lesson"}]);
    });
    read_lesson(&mut board);
    assert_eq!(
        board.projection(View::Referee).unwrap()["P1"]["hand"],
        json!([])
    );
    activate_line(&mut board, "s", "unit-follower", 9);
    read_lesson(&mut board);
    assert_eq!(
        board.projection(View::Referee).unwrap()["P1"]["hand"],
        json!([]),
        "banishing the opposing EX item does not satisfy the controller's history"
    );
    activate_line(&mut board, "s", "unit-follower", 3);
    read_lesson(&mut board);
    assert_eq!(
        board.projection(View::Referee).unwrap()["P1"]["hand"],
        json!(["d0"]),
        "each EX holder has independent committed history"
    );
}

#[test]
fn playing_the_ex_spell_into_cemetery_is_not_a_lesson_banish() {
    let mut board = position();
    resolved(
        board
            .play_internal_card("item", PlayerId { seat: 1 })
            .unwrap(),
    );
    board.resume_internal_card().unwrap();
    let view = board.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["ex"], json!([]), "the token was actually played");
    assert_eq!(
        view["P1"]["cemetery"],
        json!([]),
        "the spell token was erased"
    );
    assert_eq!(
        view["P1"]["pp"]["current"], 1,
        "the spell paid its printed cost"
    );
    read_lesson(&mut board);
    assert_eq!(
        board.projection(View::Referee).unwrap()["P1"]["hand"],
        json!([]),
        "normal spell erasure does not count as EX banish"
    );
}

#[test]
fn an_injured_own_follower_is_not_in_the_opposing_destroy_domain() {
    let mut board = position();
    activate_line(&mut board, "s", "unit-follower", 8);
    destroy_damaged(&mut board);
    let view = board.projection(View::Referee).unwrap();
    assert_eq!(
        view["objects"]["s"]["hp"], 2,
        "the own follower really took damage"
    );
    assert_eq!(
        view["P1"]["field"],
        json!(["s"]),
        "own recipients are excluded"
    );
    assert_eq!(
        view["P2"]["field"],
        json!(["victim", "untouched"]),
        "uninjured opponents stay"
    );
}

#[test]
fn actual_attack_and_counter_damage_commit_history_without_damage_observers() {
    let mut board = position_with(|initial| {
        initial["players"]["P1"]["zones"]["field"][0]["state"] = json!({"entered_this_turn":false});
        initial["players"]["P2"]["zones"]["field"][0]["state"]["acted"] = json!(true);
    });
    board
        .internal_rule_action(&json!({"do":"attack","attacker":"s","target":"victim"}))
        .unwrap();
    let question = board.internal_quick_choice().unwrap().unwrap();
    assert!(
        matches!(
            board
                .answer_internal_card(question.actor, question.token, Answer::Quick(None))
                .unwrap(),
            CardPlay::Continued(_)
        ),
        "a real defender pass commits combat"
    );
    if board.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        board.resume_internal_card().unwrap();
    }
    let view = board.projection(View::Referee).unwrap();
    assert_eq!(
        view["objects"]["s"]["hp"], 1,
        "the attacker received real counter damage"
    );
    assert_eq!(
        view["objects"]["victim"]["hp"], 1,
        "the defender received real attack damage"
    );
    assert_eq!(
        view["objects"]["s"]["damaged_this_turn"], true,
        "counter damage commits the same history"
    );
    destroy_damaged(&mut board);
    let view = board.projection(View::Referee).unwrap();
    assert_eq!(
        view["P2"]["cemetery"],
        json!(["victim"]),
        "the public consumer finds attack damage"
    );
    assert_eq!(
        view["P1"]["field"],
        json!(["s"]),
        "the damaged attacker is outside its opponent selector"
    );
}

#[test]
fn full_prevention_does_not_record_positive_damage_history() {
    let mut protective = operator("unit-other").card().clone();
    let declaration = lowerer().lower(&equipment_played::tree(&json!({"static":{"effects":[{"prevent":{"kind":"ability","on":{"who":"self","zone":"field","kind":"follower"}}}]}}), "unit-other")).unwrap().remove("unit-other").unwrap();
    protective.abilities = declaration.card().abilities.clone();
    let protective = WellFormedCardProgram::check(protective).unwrap();
    let mut board = position_with_other(|_| {}, &protective);
    hit(&mut board, 1);
    destroy_damaged(&mut board);
    let view = board.projection(View::Referee).unwrap();
    assert_eq!(
        view["objects"]["victim"]["hp"], 3,
        "the proposed damage was actually prevented"
    );
    assert_eq!(
        view["P2"]["field"],
        json!(["victim", "untouched"]),
        "a fully prevented hit does not qualify"
    );
}

#[test]
fn a_departure_condition_reads_a_later_banish_at_resolution() {
    let mut board = position_with(|initial| {
        initial["players"]["P1"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"reader","card":"unit-amulet"}));
    });
    let reader = lowerer().lower(&equipment_played::tree(&departure(&json!({"event":"banished","window":"this_turn","who":"self","from":"ex","sel":{"name":"魔法のアイテム"}})), "unit-amulet")).unwrap().remove("unit-amulet").unwrap();
    board.load_internal_card("unit-amulet", &reader).unwrap();
    activate_line(&mut board, "s", "unit-follower", 10);
    let question = board.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("two departure occurrences")
    };
    assert_eq!(
        options.len(),
        2,
        "the source leaves before either occurrence resolves"
    );
    let producer = options
        .iter()
        .find(|option| option.card == "unit-follower")
        .unwrap();
    resolved(
        board
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(producer.handle),
            )
            .unwrap(),
    );
    board.resume_internal_card().unwrap();
    assert_eq!(
        board.projection(View::Referee).unwrap()["P1"]["ex"],
        json!([]),
        "the later pending producer really banished the item"
    );
    let question = board.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("the remaining departure reader")
    };
    assert_eq!(
        options.len(),
        1,
        "only the previously created reader remains"
    );
    resolved(
        board
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap(),
    );
    board.resume_internal_card().unwrap();
    assert_eq!(
        board.projection(View::Referee).unwrap()["P1"]["hand"],
        json!(["d0"]),
        "the guard is read at resolution, after pending generation"
    );
}

fn rejected(raw: CardProgram) {
    if let Ok(program) = WellFormedCardProgram::check(raw) {
        let mut board = Game::new(
            history_catalog(true),
            &setup(1, json!([])),
            &Value::Null,
            &Value::Null,
            "raw-public-history-negative",
        )
        .unwrap();
        assert!(
            board.load_internal_card("unit-follower", &program).is_err(),
            "broader raw IR must fail before execution"
        );
    }
}

#[test]
fn raw_damage_history_cannot_escape_its_selector_scope_window_or_consumer() {
    for variant in 0..5 {
        let mut raw = operator("unit-follower").card().clone();
        let body = raw.abilities[4].node.body.as_mut().unwrap();
        let Node::Instruction { instruction } = &mut body.steps[0].node else {
            panic!("canonical destruction instruction")
        };
        let Action::Destroy { objects } = &mut instruction.action else {
            panic!("canonical destruction selector")
        };
        if variant == 0 {
            instruction.action = Action::Banish {
                objects: objects.clone(),
            };
        } else {
            let Selector::Zone {
                players,
                binding,
                predicate,
                ..
            } = objects
            else {
                panic!("canonical public zone")
            };
            match variant {
                1 => players.relation = PlayerRelation::SelfPlayer,
                2 => *binding = Binding::new(99999, ScopeId(99999)),
                _ => {
                    let Expr::Compute(operation) = predicate.as_mut() else {
                        panic!("candidate conjunction")
                    };
                    let Predicate::And { predicates } = operation.as_mut() else {
                        panic!("candidate conjunction")
                    };
                    let Expr::Live(read) = &mut predicates[1] else {
                        panic!("candidate history")
                    };
                    let BoolRead::HistoryExists { query } = read.as_mut() else {
                        panic!("candidate history exists")
                    };
                    if variant == 3 {
                        query.include_execution = InclusionPolicy::ExcludeCurrentExecution {
                            execution: ExecutionRef::Current {},
                        };
                    } else {
                        let EventPattern::Damage { binding, .. } = &mut query.pattern else {
                            panic!("damage history event")
                        };
                        *binding = Binding::new(99999, ScopeId(99999));
                    }
                }
            }
        }
        rejected(raw);
    }
}

#[test]
fn raw_banish_history_cannot_change_its_snapshot_scope_or_execution_window() {
    for variant in 0..3 {
        let mut raw = operator("unit-follower").card().clone();
        let body = raw.abilities[3].node.body.as_mut().unwrap();
        let Node::Control {
            control:
                Control::If {
                    condition: Expr::Live(read),
                    ..
                },
        } = &mut body.steps[0].node
        else {
            panic!("canonical history condition")
        };
        let BoolRead::HistoryExists { query } = read.as_mut() else {
            panic!("canonical history existence")
        };
        if variant == 0 {
            query.include_execution = InclusionPolicy::ExcludeCurrentExecution {
                execution: ExecutionRef::Current {},
            };
        } else {
            let EventPattern::Banish { binding, predicate } = &mut query.pattern else {
                panic!("canonical banish history")
            };
            if variant == 1 {
                *binding = Binding::new(99999, ScopeId(99999));
            } else {
                let Expr::Compute(operation) = predicate else {
                    panic!("canonical conjunction")
                };
                let Predicate::And { predicates } = operation.as_mut() else {
                    panic!("canonical conjunction")
                };
                let Expr::Compute(operation) = &mut predicates[1] else {
                    panic!("canonical names")
                };
                let Predicate::NamesIntersect {
                    left: Expr::Live(read),
                    ..
                } = operation.as_mut()
                else {
                    panic!("canonical name snapshot")
                };
                let NamesRead::Names {
                    object: ObjectView::EventSnapshot { policy, .. },
                } = read.as_mut()
                else {
                    panic!("canonical before-exit snapshot")
                };
                *policy = EventSnapshotPolicy::AfterEntry;
            }
        }
        rejected(raw);
    }
}

#[test]
fn paid_departure_cannot_be_announced_without_a_legal_follower_target() {
    let mut board = position_with(|initial| {
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"reader","card":"unit-amulet"}]);
    });
    let head = json!({"activate":{"cost":{"steps":[{"act_self":true},{"self_to_cemetery":true}]},"targets":[{"bind":"friend","from":{"who":"self","zone":"field","kind":"follower"},"n":1}],"do":[{"buff":{"on":"friend","power":1,"hp":1}}]}});
    let program = lowerer()
        .lower(&equipment_played::tree(&head, "unit-amulet"))
        .unwrap()
        .remove("unit-amulet")
        .unwrap();
    let id = program
        .card()
        .abilities
        .iter()
        .find(|ability| ability.node.kind == AbilityKind::Activated)
        .unwrap()
        .node
        .id;
    board.load_internal_card("unit-amulet", &program).unwrap();
    let before = board.projection(View::Referee).unwrap();
    assert!(
        board
            .activate_internal_card("reader", id, PlayerId { seat: 1 })
            .is_err(),
        "no legal formal target prevents announcement"
    );
    assert_eq!(
        board.projection(View::Referee).unwrap(),
        before,
        "neither act nor self-cemetery cost was committed"
    );
}

#[test]
fn evolution_keeps_the_committed_damage_history_of_the_same_object() {
    let mut tree = equipment_played::tree(&json!({"evolve":{"cost":{"pp":1}}}), "unit-follower");
    let mut hit = equipment_played::tree(&json!({"activate":{"cost":{"pp":0},"do":[{"damage":{"to":{"who":"self","zone":"field","kind":"follower"},"n":1}}]}}), "unit-follower")["cards"]["unit-follower"]["abilities"][0].clone();
    hit["line"] = json!(2);
    hit["sources"][0]["ability_scope"] = json!("damage_before_evolution");
    tree["cards"]["unit-follower"]["abilities"]
        .as_array_mut()
        .unwrap()
        .push(hit);
    let program = lowerer()
        .lower(&tree)
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    let damage = program
        .card()
        .abilities
        .iter()
        .find(|ability| {
            matches!(
                ability.node.source.origin,
                AbilityOrigin::Printed { line: Nat(2), .. }
            )
        })
        .unwrap()
        .node
        .id;
    let mut board = evolution::position(1, &program, &card(vec![]));
    resolved(
        board
            .activate_internal_card("s", damage, PlayerId { seat: 1 })
            .unwrap(),
    );
    board.resume_internal_card().unwrap();
    assert_eq!(
        board.projection(View::Referee).unwrap()["objects"]["s"]["hp"],
        2,
        "a real ability commits damage before evolution"
    );
    board
        .internal_rule_action(&json!({"do":"evolve","source":"s","evolve_card":"e","pay":{"pp":1}}))
        .unwrap();
    let view = board.projection(View::Referee).unwrap();
    assert_eq!(
        view["P1"]["evolution"],
        json!(["e"]),
        "the printed one-PP evolution actually completed"
    );
    assert_eq!(
        view["objects"]["s"]["hp"], 4,
        "evolution preserves the actual one damage on five life"
    );
    assert_eq!(
        view["objects"]["s"]["damaged_this_turn"], true,
        "the same object retains its committed history"
    );
}

#[test]
fn an_opposing_effect_records_the_ex_holder_and_not_the_banishing_controller() {
    let mut board = position_with(|initial| {
        initial["turn"]["active"] = json!("P2");
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"reader","card":"unit-amulet"}]);
    });
    let reader = lowerer().lower(&equipment_played::tree(&departure(&json!({"event":"banished","window":"this_turn","who":"self","from":"ex","sel":{"name":"魔法のアイテム"}})), "unit-amulet")).unwrap().remove("unit-amulet").unwrap();
    board.load_internal_card("unit-amulet", &reader).unwrap();
    activate_line(&mut board, "untouched", "unit-other", 9);
    activate_line(&mut board, "untouched", "unit-other", 12);
    let question = board.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("the opposing effect created the reader's own departure occurrence")
    };
    assert_eq!(question.actor.seat, 1, "the EX holder owns the reader");
    assert_eq!(options.len(), 1, "only the reader left the field");
    resolved(
        board
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap(),
    );
    board.resume_internal_card().unwrap();
    let view = board.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["ex"], json!([]), "P2 really banished P1's item");
    assert_eq!(
        view["P1"]["hand"],
        json!(["d0"]),
        "P1's committed history includes the opposing effect"
    );
    assert_eq!(
        view["P2"]["hand"],
        json!([]),
        "the banishing controller does not receive the draw"
    );
}
