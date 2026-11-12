#![expect(
    clippy::panic,
    reason = "Unexpected fixture questions invalidate the cost proof."
)]
use super::*;
use sve_engine::EngineFailure;
use sve_engine::game::internal::{DecisionView, InputDomain};

fn tree(activation: &Value) -> Value {
    let mut tree = fanfare::expanded::tree().clone();
    let mut seed = tree["cards"]["BP08-038"].clone();
    let mut entry = seed["abilities"][0].clone();
    entry.as_object_mut().unwrap().remove("fanfare");
    entry["activate"] = activation.clone();
    seed["abilities"] = json!([entry]);
    tree["cards"] = json!({"unit-amulet":seed});
    tree
}

fn program(quick: bool, other: bool) -> WellFormedCardProgram {
    let mut query = json!({"who":"self","zone":"field","name":"unit-amulet"});
    if other {
        query["other"] = json!(true);
    }
    lowerer()
        .lower(&tree(&json!({"quick":quick,"cost":{"steps":[
        {"pp":1},{"act_self":true},{"counter":{"on":query,"holders":1,"name":"test","n":2}}
    ]},"do":[]})))
        .unwrap()
        .remove("unit-amulet")
        .unwrap()
}

pub(super) fn initial(other: bool, pool: u64, acted: bool) -> Value {
    let mut initial = setup(1, json!([]));
    for seat in ["P1", "P2"] {
        initial["players"][seat]["zones"] = json!({});
    }
    let mut field = vec![
        json!({"id":"source","card":"unit-amulet","state":{"acted":acted,"counters":{"test":2}}}),
    ];
    if other {
        field.push(json!({"id":"pool","card":"unit-amulet","state":{"silenced":true,"counters":{"test":pool}}}));
    }
    initial["players"]["P1"]["zones"]["field"] = json!(field);
    initial
}

pub(super) fn load(initial: &Value, program: &WellFormedCardProgram) -> Game {
    let mut game = Game::new(
        catalog(),
        initial,
        &Value::Null,
        &Value::Null,
        "quick-counter",
    )
    .unwrap();
    let empty = card(vec![]);
    for number in ["unit-amulet", "unit-follower", "unit-spell"] {
        game.load_internal_card(
            number,
            if number == "unit-amulet" {
                program
            } else {
                &empty
            },
        )
        .unwrap();
    }
    game
}

fn window(game: &mut Game) -> (DecisionView, Vec<u64>) {
    let q = game.internal_quick_choice().unwrap().unwrap();
    let InputDomain::Quick { options, .. } = &q.domain else {
        panic!("Quick")
    };
    let handles = options.iter().map(|option| option.handle).collect();
    (q, handles)
}

fn quick_game(initial: &Value, program: &WellFormedCardProgram) -> Game {
    let mut initial = initial.clone();
    initial["turn"]["active"] = json!("P2");
    let mut game = load(&initial, program);
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    game
}

fn awaiting(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(q) = play else {
        panic!("payment question");
    };
    q
}

fn pay(game: &mut Game, q: &DecisionView) -> Activation {
    let InputDomain::CostMaterials(domain) = &q.domain else {
        panic!("counter materials")
    };
    assert_eq!(
        domain.members.len(),
        1,
        "the exact counter pool has one payable holder"
    );
    let before = exact_authority(game);
    for answer in [
        vec![],
        vec![99],
        vec![domain.members[0].handle, domain.members[0].handle],
    ] {
        game.answer_internal_card(q.actor, q.token, Answer::CostMaterials(answer))
            .unwrap_err();
        assert_eq!(
            exact_authority(game),
            before,
            "invalid material answers preserve the entire question"
        );
    }
    resolved(
        game.answer_internal_card(
            q.actor,
            q.token,
            Answer::CostMaterials(vec![domain.members[0].handle]),
        )
        .unwrap(),
    )
}

fn unchanged(before: &Value, game: &Game) {
    let after = game.projection(View::Referee).unwrap();
    for key in ["P1", "P2", "objects"] {
        assert_eq!(
            after[key], before[key],
            "quoting preserves public authority {key}"
        );
    }
}

#[test]
fn selected_counter_quick_matches_main_for_inclusive_and_exclusive_source_pools() {
    for other in [false, true] {
        let program = program(true, other);
        let initial = initial(other, 2, false);
        let mut main = load(&initial, &program);
        let q = awaiting(
            main.activate_internal_card("source", AbilityId(1), PlayerId { seat: 1 })
                .unwrap(),
        );
        pay(&mut main, &q);
        let mut quick = quick_game(&initial, &program);
        let before = quick.projection(View::Referee).unwrap();
        let (q, handles) = window(&mut quick);
        assert_eq!(handles.len(), 1);
        unchanged(&before, &quick);
        let materials = awaiting(
            quick
                .answer_internal_card(q.actor, q.token, Answer::Quick(Some(handles[0])))
                .unwrap(),
        );
        pay(&mut quick, &materials);
        for game in [&main, &quick] {
            let after = game.projection(View::Referee).unwrap();
            assert_eq!(after["P1"]["pp"]["current"], 1);
            assert_eq!(after["objects"]["source"]["acted"], true);
            assert_eq!(
                after["objects"][if other { "pool" } else { "source" }]["counters"]["test"],
                0
            );
            if other {
                assert_eq!(after["objects"]["source"]["counters"]["test"], 2);
            }
        }
    }
}

#[test]
fn selected_counter_quotes_skip_wrong_pools_and_acted_sources_without_committing() {
    for scenario in [
        "good",
        "poor",
        "acted",
        "wrong-zone",
        "foreign",
        "wrong-name",
    ] {
        let mut initial = initial(
            true,
            if scenario == "poor" { 1 } else { 2 },
            scenario == "acted",
        );
        if scenario == "wrong-zone" || scenario == "foreign" {
            let moved = initial["players"]["P1"]["zones"]["field"]
                .as_array_mut()
                .unwrap()
                .pop()
                .unwrap();
            if scenario == "wrong-zone" {
                initial["players"]["P1"]["zones"]["ex"] = json!([moved]);
            } else {
                initial["players"]["P2"]["zones"]["field"] = json!([moved]);
            }
        }
        if scenario == "wrong-name" {
            initial["players"]["P1"]["zones"]["field"][1]["card"] = json!("unit-follower");
        }
        let mut game = quick_game(&initial, &program(true, true));
        let before = game.projection(View::Referee).unwrap();
        let (_, handles) = window(&mut game);
        assert_eq!(handles.len(), usize::from(scenario == "good"), "{scenario}");
        unchanged(&before, &game);
    }
}

pub(super) fn stale_program() -> WellFormedCardProgram {
    let mut raw = program(true, true).card().clone();
    let play = raw.abilities[0].node.play.as_mut().unwrap();
    let CostSpec::Ordered { steps } = &mut play.costs else {
        panic!("ordered")
    };
    let mut departure = steps[2].clone();
    let Action::Counter {
        holder:
            Holder::Object {
                object: ObjectOperand::Selected { selection },
            },
        ..
    } = departure.instruction.action
    else {
        panic!("selected counter")
    };
    departure.instruction.id = InstructionId(999);
    departure.instruction.receipt = ReceiptId(999);
    departure.instruction.action = Action::Banish {
        objects: Selector::Bound {
            reference: selection,
        },
    };
    steps.insert(2, departure);
    let mut fallback = quick::quick_activated(ability(2, vec![]));
    fallback.source.card_no = "unit-amulet".into();
    fallback.source.card_id = "unit-amulet".into();
    raw.abilities.push(fallback);
    WellFormedCardProgram::check(raw).unwrap()
}

#[test]
fn a_moved_counter_material_is_illegal_instead_of_making_the_window_unsupported() {
    let program = stale_program();
    let mut game = quick_game(&initial(true, 2, false), &program);
    let before = game.projection(View::Referee).unwrap();
    let (_, handles) = window(&mut game);
    assert_eq!(
        handles.len(),
        1,
        "the independent free ability remains available"
    );
    unchanged(&before, &game);
    let mut main = load(&initial(true, 2, false), &program);
    let origin = main.clone();
    let q = awaiting(
        main.activate_internal_card("source", AbilityId(1), PlayerId { seat: 1 })
            .unwrap(),
    );
    let InputDomain::CostMaterials(domain) = &q.domain else {
        panic!("counter materials")
    };
    assert!(matches!(
        main.answer_internal_card(
            q.actor,
            q.token,
            Answer::CostMaterials(vec![domain.members[0].handle])
        ),
        Err(EngineFailure::IllegalPlay(_))
    ));
    transactions::assert_restored(&main, &origin);
}

#[test]
fn exhausted_quick_limits_are_checked_with_and_without_materials() {
    for materials in [false, true] {
        let cost = if materials {
            json!({"counter":{"on":{"who":"self","zone":"field","name":"unit-amulet","other":true},"holders":1,"name":"test","n":1}})
        } else {
            json!({"pp":0})
        };
        let program = lowerer()
            .lower(&tree(
                &json!({"quick":true,"per_turn":1,"cost":cost,"do":[]}),
            ))
            .unwrap()
            .remove("unit-amulet")
            .unwrap();
        let mut game = quick_game(&initial(true, 4, false), &program);
        let (q, handles) = window(&mut game);
        let result = game
            .answer_internal_card(q.actor, q.token, Answer::Quick(Some(handles[0])))
            .unwrap();
        if materials {
            pay(&mut game, &awaiting(result));
        } else {
            resolved(result);
        }
        game.resume_internal_card().unwrap();
        let (_, handles) = window(&mut game);
        assert!(
            handles.is_empty(),
            "an exhausted instance cannot be offered again"
        );
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["pool"]["counters"]["test"],
            if materials { 3 } else { 4 }
        );
    }
}
