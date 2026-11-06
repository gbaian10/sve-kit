use super::*;

pub(super) fn prepared(initial: &Value) -> Game {
    static SELECTED: OnceLock<SelectedEffects> = OnceLock::new();
    let selected = SELECTED.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = producer_inputs();
        for (number, name) in [("cost-capture-base", "異界を統べる者"), ("cheval-capture-base", "シュヴァルグラン")] {
        snapshot.push_str(&json!({"number":number,"faces":[{
            "name":name,"card_class":"ニュートラル","card_type":"フォロワー",
            "traits":[],"cost":"1","power":"1","hp":"1","text":null,"sections":[]
        }]}).to_string());
        snapshot.push('\n');
        input["documents"].as_array_mut().unwrap().push(json!([format!("{number}.yaml"),
            json!({"format":1_u8,"kind":"effect_set","cards":{number:{"status":"complete","abilities":[]}}}).to_string()
        ]));
        }
        repository::selection_with_rules_from(&snapshot, &keywords, &input)
    });
    let mut game = Game::from_selected_effects(selected.clone(), initial, &Value::Null, &Value::Null, "capture-extra").unwrap();
    let mut tree = expanded::tree().clone();
    let mut base = tree["cards"]["BP01-041"].clone();
    let mut ability = base["abilities"][0].clone();
    ability.as_object_mut().unwrap().remove("activate");
    ability["evolve"] = json!({"cost":{"pp":2_u8}});
    base["abilities"] = json!([ability]);
    for number in ["cost-capture-base", "cheval-capture-base"] {
        tree["cards"] = json!({number:base.clone()});
        game.load_internal_card(number, &lowerer().lower(&tree).unwrap()[number]).unwrap();
    }
    game
}

#[test]
fn aurora_healing_printings_stand_and_forbid_attacks_and_draw_only_for_original_cost_one() {
    for number in ["CP04-018", "CP04-P14", "PCS01-007"] {
        for (ally, cost, draw) in [("CP04-001",9_u8,true),("CP04-005",1,false)] {
          for acted in [false,true] {
            let mut initial = draw_position();
            initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"spell","card":number}]);
            initial["players"]["P1"]["zones"]["field"] = json!([{"id":"ally","card":ally,"state":{"acted":acted,"cost":cost,"entered_this_turn":false}}]);
            initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"draw","card":"BP01-037"}]);
            initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037","state":{"acted":true}}]);
            let mut game = Game::from_selected_effects(selection_with_rules().clone(), &initial, &Value::Null, &Value::Null, "aurora-healing").unwrap();
            let CardPlay::Awaiting(question) = game.play_internal_card("spell", PlayerId{seat:1}).unwrap() else {panic!("ally target")};
            resolved(game.answer_internal_card(question.actor,question.token,Answer::Targets(vec![0])).unwrap());
            event_cemetery::finish_checks(&mut game);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["objects"]["ally"]["acted"], false);
            assert_eq!(state["P1"]["hand"], if draw {json!(["draw"])} else {json!([])});
            for target in ["P2.leader","enemy"] {
                assert_eq!(game.internal_rule_action(&json!({"do":"attack","attacker":"ally","target":target})).unwrap().outcome,"cannot-attack");
            }
          }
        }
    }
}

#[test]
fn ruler_of_the_otherworld_printings_return_the_whole_selection_only_when_all_original_costs_match() {
    for number in ["BP08-104", "BP08-SL20"] {
        for (third, returns, short) in [("BP01-159",true,false),("BP01-041",false,false),("BP01-159",false,true)] {
            let mut initial = draw_position();
            initial["players"]["P1"]["zones"]["hand"] = json!([]);
            initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"cost-capture-base"}]);
            initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":number}]);
            initial["players"]["P1"]["zones"]["cemetery"] = json!([
                {"id":"a","card":"BP01-012","state":{"cost":8_u8}},
                {"id":"b","card":"BP01-146","state":{"cost":9_u8}},
                {"id":"c","card":third,"state":{"cost":7_u8}}
            ]);
            if short { initial["players"]["P1"]["zones"]["cemetery"].as_array_mut().unwrap().truncate(2); }
            let mut game = prepared(&initial);
            game.internal_rule_action(&json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":2_u8}})).unwrap();
            let question = game.internal_pending_choice().unwrap().unwrap();
            let InputDomain::Pending(options) = &question.domain else {panic!("evolve event")};
            let play = game.answer_internal_card(question.actor,question.token,Answer::Pending(options[0].handle)).unwrap();
            if short {
                assert!(matches!(play,CardPlay::Cancelled));
                assert_eq!(game.projection(View::Referee).unwrap()["P1"]["cemetery"].as_array().unwrap().len(),2);
                continue;
            }
            let CardPlay::Awaiting(targets) = play else {panic!("three cemetery cards")};
            resolved(game.answer_internal_card(targets.actor,targets.token,Answer::Targets(vec![0,1,2])).unwrap());
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["P1"]["hand"].as_array().unwrap().len(), if returns {3} else {0});
            assert_eq!(state["P1"]["cemetery"].as_array().unwrap().len(), if returns {0} else {3});
        }
    }
}
