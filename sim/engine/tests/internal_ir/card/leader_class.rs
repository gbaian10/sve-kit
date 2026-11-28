use super::*;
use sve_engine::game::internal::InputDomain;

fn conditional_draw() -> WellFormedCardProgram {
    let document = evolution_linked_movement::tree(&[(
        "activate",
        json!({"cost":{"pp":0},"do":[{"if":{
            "cond":{"leader_class":"ロイヤル"},
            "then":[{"draw":{"who":"self","n":1}}]
        }}]}),
    )]);
    lowerer()
        .lower(&document)
        .unwrap()
        .remove("unit-follower")
        .unwrap()
}

#[test]
fn leader_class_reads_the_current_controllers_leader() {
    let program = conditional_draw();
    // (controller, P1 leader, P2 leader, draws)
    for (controller, first, second, draws) in [
        ("P1", Some("ロイヤル"), Some("ネクロマンサー"), true),
        ("P1", Some("ネクロマンサー"), Some("ロイヤル"), false),
        // A card owned by P1 but controlled by P2 reads P2's leader.
        ("P2", Some("ロイヤル"), Some("ネクロマンサー"), false),
        ("P2", Some("ネクロマンサー"), Some("ロイヤル"), true),
        // Without a recorded leader class nothing matches.
        ("P1", None, Some("ロイヤル"), false),
    ] {
        let mut initial = setup(1, json!([]));
        initial["turn"]["active"] = json!(controller);
        for (player, class) in [("P1", first), ("P2", second)] {
            initial["players"][player]["zones"] = json!({
                "hand":[],
                "deck":[{"id":format!("draw-{player}"),"card":"unit-spell"}],
                "field":[]
            });
            match class {
                Some(class) => initial["players"][player]["leader"]["class"] = json!(class),
                None => {
                    initial["players"][player]["leader"]
                        .as_object_mut()
                        .unwrap()
                        .remove("class");
                }
            }
        }
        initial["players"][controller]["zones"]["field"] =
            json!([{"id":"source","card":"unit-follower","owner":"P1"}]);
        let mut game = Game::new(
            catalog(),
            &initial,
            &Value::Null,
            &Value::Null,
            "leader-class",
        )
        .unwrap();
        game.load_internal_card("unit-follower", &program).unwrap();
        game.load_internal_card("unit-spell", &card(vec![]))
            .unwrap();
        let seat = if controller == "P1" { 1 } else { 2 };
        resolved(
            game.activate_internal_card(
                "source",
                program.card().abilities[0].node.id,
                PlayerId { seat },
            )
            .unwrap(),
        );
        game.advance_player_input();
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state[controller]["hand"].as_array().unwrap().len(),
            usize::from(draws),
            "{controller} with leaders {first:?}/{second:?}"
        );
    }
}

#[test]
fn leader_class_discount_applies_while_paying_the_play_cost() {
    let mut tree = fanfare::expanded::tree().clone();
    let mut entry = tree["cards"]["BP08-038"]["abilities"][0].clone();
    entry.as_object_mut().unwrap().remove("fanfare");
    entry["play"] = json!({"cost_delta":{"if":{"leader_class":"ドラゴン"},"delta":-2_i8}});
    let mut seed = tree["cards"]["BP08-038"].clone();
    seed["abilities"] = json!([entry]);
    tree["cards"] = json!({"unit-follower":seed});
    let program = lowerer()
        .lower(&tree)
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    for (class, remaining) in [("ドラゴン", 9_u8), ("エルフ", 7)] {
        let mut initial = setup(1, json!([]));
        initial["players"]["P1"]["pp"] = json!({"current":10_u8,"max":10_u8});
        initial["players"]["P1"]["leader"]["class"] = json!(class);
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"s","card":"unit-follower"}]);
        let mut game = Game::new(
            catalog_with_cost(&json!("3")),
            &initial,
            &Value::Null,
            &Value::Null,
            "leader-class-cost",
        )
        .unwrap();
        game.load_internal_card("unit-follower", &program).unwrap();
        game.play_internal_card("s", PlayerId { seat: 1 }).unwrap();
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
            remaining,
            "{class} leader"
        );
    }
}

#[test]
fn leader_class_changes_the_number_of_modes_the_controller_can_resolve() {
    let document = evolution_linked_movement::tree(&[(
        "activate",
        json!({"cost":{"pp":0},"do":[{"choice":{
            "n":1,
            "instead":[{"if":{"leader_class":"ニュートラル"},"upto":3}],
            "options":[
                [{"life":{"who":"self","delta":1}}],
                [{"life":{"who":"self","delta":2}}],
                [{"life":{"who":"self","delta":4}}]
            ]
        }}]}),
    )]);
    let program = lowerer()
        .lower(&document)
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    for (class, maximum, life) in [("ニュートラル", 3, 17), ("ロイヤル", 1, 11)] {
        let mut initial = setup(1, json!([]));
        initial["players"]["P1"]["leader"] = json!({"class":class,"life":10});
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] =
            json!([{"id":"source","card":"unit-follower"}]);
        let mut game = Game::new(
            catalog(),
            &initial,
            &Value::Null,
            &Value::Null,
            "leader-modes",
        )
        .unwrap();
        game.load_internal_card("unit-follower", &program).unwrap();
        let CardPlay::Awaiting(question) = game
            .activate_internal_card(
                "source",
                program.card().abilities[0].node.id,
                PlayerId { seat: 1 },
            )
            .unwrap()
        else {
            panic!("activation must request the printed modes");
        };
        let InputDomain::Modes {
            cardinality, modes, ..
        } = &question.domain
        else {
            panic!("expected mode choice");
        };
        assert_eq!(cardinality.minimum, Nat(1));
        assert_eq!(cardinality.maximum, Some(Nat(maximum)));
        if maximum == 1 {
            game.answer_internal_card(question.actor, question.token, Answer::Modes(modes.clone()))
                .unwrap_err();
        }
        let selected = modes
            .iter()
            .take(usize::try_from(maximum).unwrap())
            .copied()
            .collect();
        resolved(
            game.answer_internal_card(question.actor, question.token, Answer::Modes(selected))
                .unwrap(),
        );
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
            life
        );
    }
}
