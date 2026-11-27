use super::*;

#[test]
fn lu_woh_super_evolves_and_weakens_only_the_opposing_storm_attacker() {
    let mut initial = position_with_source("synthetic-evolution", "field");
    initial["turn"]["elapsed_turns"] = json!({"P1":7_u8,"P2":6_u8});
    initial["players"]["P1"]["sep"] = json!(1_u8);
    initial["players"]["P1"]["zones"]["evolve_deck"] =
        json!([{"id":"form","card":"CP05-022"}]);
    initial["players"]["P1"]["zones"]["deck"] =
        json!([{"id":"own-draw","card":"BP01-037"}]);
    initial["players"]["P2"]["zones"]["deck"] =
        json!([{"id":"enemy-draw","card":"BP01-037"}]);
    initial["players"]["P1"]["zones"]["field"].as_array_mut().unwrap()
        .push(json!({"id":"ally","card":"BP01-048"}));
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"ordinary","card":"BP01-037"},
        {"id":"storm","card":"BP01-048","state":{"power":8_u8}}
    ]);
    let mut game = super_evolution_pp_cost::synthetic_base("CP05-022", &initial);
    assert_eq!(
        game.internal_rule_action(&json!({
            "do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8,"sep":1_u8}
        })).unwrap().outcome,
        "resolved"
    );
    resolve_grant_trigger(&mut game, 2, None);
    assert!(has_granted_keyword(&state(&game), "source", "オーラ"));
    assert_eq!(resolve_pending_card(&mut game, "CP05-022"), CardPlay::Cancelled);
    finish_checks(&mut game);
    assert_eq!(game.internal_rule_action(&json!({
        "do":"attack","attacker":"ally","target":"P2.leader"
    })).unwrap().outcome, "resolved");
    assert!(game.internal_pending_choice().unwrap().is_none());
    pass_quick(&mut game);
    attack_batch::end_turn(&mut game);

    assert_eq!(game.internal_rule_action(&json!({
        "do":"attack","attacker":"ordinary","target":"P1.leader"
    })).unwrap().outcome, "resolved");
    assert!(game.internal_pending_choice().unwrap().is_none());
    pass_quick(&mut game);

    let before = state(&game);
    assert_eq!(game.internal_rule_action(&json!({
        "do":"attack","attacker":"storm","target":"P1.leader"
    })).unwrap().outcome, "resolved");
    resolve_grant_trigger(&mut game, 2, None);
    let after = state(&game);
    assert_eq!(after["objects"]["storm"]["power"], 3_u8);
    assert_eq!(after["objects"]["ordinary"]["power"], before["objects"]["ordinary"]["power"]);
}

#[test]
fn matching_search_grants_end_phase_banish_only_to_the_entered_card() {
    let (source, source_author) = current_fixture::author(
        "source",
        "11111111111111111111111111111111",
        "フォロワー",
        vec![json!({"fanfare":{
            "targets":[{"bind":"chosen","from":{"who":"self","zone":"field","kind":"follower","other":true},"n":1_u8}],
            "do":[
                {"search":{"sel":{"who":"self","zone":"deck","kind":"follower"},
                    "same_name_as":"chosen","n":1_u8,"to":"field","result":"entered"}},
                {"grant":{"on":{"follow":"entered"},"abilities":[{"auto":{
                    "on":{"phase":{"which":"end","whose":"self"}},
                    "do":[{"banish":{"ref":"this"}}]
                }}]}},
                {"banish":{"ref":"this"}}
            ]
        }})],
    );
    let (match_card, match_author) = current_fixture::author(
        "match",
        "22222222222222222222222222222222",
        "フォロワー",
        vec![],
    );
    let (other_card, other_author) = current_fixture::author(
        "other",
        "33333333333333333333333333333333",
        "フォロワー",
        vec![],
    );
    let (alias_card, alias_author) = current_fixture::author(
        "alias",
        "44444444444444444444444444444444",
        "フォロワー",
        vec![json!({"static":{"zone":"field","effects":[{"also_named":{"name":"match"}}]}})],
    );
    let snapshot = format!("{source}\n{match_card}\n{other_card}\n{alias_card}\n");
    let input = json!({"documents":[],"authors":[source_author,match_author,other_author,alias_author]});
    let keywords = json!({"format":1_u8,"kind":"keyword_registry","rule_names":{}}).to_string();
    let selected = repository::select_without_rules(&snapshot, &keywords, &input).unwrap();

    for full in [false, true] {
        let mut initial = position_with_source("source", "hand");
        initial["players"]["P2"]["zones"] = json!({});
        initial["players"]["P1"]["zones"]["deck"] = json!([
            {"id":"found","card":"match"}, {"id":"decoy","card":"other"}
        ]);
        let field = (0_u8..if full { 4 } else { 1 })
            .map(|number| json!({"id":format!("candidate-{number}"),"card":"alias"}))
            .collect::<Vec<_>>();
        initial["players"]["P1"]["zones"]["field"] = json!(field);
        let mut game = Game::from_selected_effects(
            selected.clone(), &initial, &Value::Null, &Value::Null, "cp05-b-name",
        ).unwrap();
        let play = enter_with_fanfare(&mut game, "source");
        let question = await_card_choice(choose_card_by_number(&mut game, play, "alias"));
        let InputDomain::Search(domain) = &question.domain else {
            panic!("private search after the formal target");
        };
        assert!(game.internal_decision(View::P2).is_none());
        assert_eq!(domain.cards.len(), 1);
        assert_eq!(domain.cards[0].card, "match");
        resolved(game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Search(SearchAnswer::Ungrouped(vec![domain.cards[0].handle])),
        ).unwrap());
        game.resume_internal_card().unwrap();
        assert_eq!(
            state(&game)["P1"]["field"].as_array().unwrap().contains(&json!("found")),
            !full,
        );
        assert_eq!(state(&game)["P1"]["banish"], json!(["source"]));
        game.internal_rule_action(&json!({"do":"end-phase"})).unwrap();
        if full {
            assert!(game.internal_pending_choice().unwrap().is_none());
        } else {
            resolve_grant_trigger(&mut game, 1, None);
            assert_eq!(state(&game)["P1"]["banish"], json!(["source", "found"]));
        }
        assert!(state(&game)["P1"]["field"].as_array().unwrap().contains(&json!("candidate-0")));
    }
}
