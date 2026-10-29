#![expect(clippy::panic, reason = "Unexpected decisions invalidate the real Move scenario.")]

use super::*;
use sve_engine::game::internal::DecisionView;

fn position(printing: &str, occupied: usize) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"spell","card":printing}]);
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"first","card":"BP01-T03"}, {"id":"second","card":"BP01-T03"},
        {"id":"other-trait","card":"BP01-037"}
    ]);
    initial["players"]["P1"]["zones"]["ex"] = json!((0..occupied).map(|n|
        json!({"id":format!("ex{n}"),"card":"BP01-T03"})
    ).collect::<Vec<_>>());
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"opponent-fairy","card":"BP01-T03"}]);
    initial
}

fn game(initial: &Value) -> Game {
    Game::from_selected_effects(selection_with_rules().clone(), initial, &Value::Null, &Value::Null, "move-success").unwrap()
}

fn targets(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(question) = play else {panic!("printed target group")};
    let InputDomain::Targets(domain) = &question.domain else {panic!("targets")};
    assert_eq!(domain.members.len(), 2, "only own Fairy followers qualify");
    assert!(domain.members.iter().all(|m|m.card.as_deref()==Some("BP01-T03")), "all candidates are own Fairy tokens");
    question
}

#[test]
fn bp05_007_and_pr_118_heal_once_only_for_actual_ex_moves() {
    for printing in ["BP05-007", "PR-118"] {
        for (selected, occupied, healed) in [(0,0,false),(1,0,true),(2,0,true),(1,5,false)] {
            let mut game = game(&position(printing, occupied));
            let question = targets(game.play_internal_card("spell", PlayerId{seat:1}).unwrap());
            let InputDomain::Targets(domain) = &question.domain else {panic!("targets")};
            let before = game.projection(View::Referee).unwrap();
            for (actor, answer) in [(PlayerId{seat:2},vec![]),(question.actor,vec![0,0])] {
                game.answer_internal_card(actor,question.token,Answer::Targets(answer)).unwrap_err();
                assert_eq!(game.projection(View::Referee).unwrap(),before);
                assert_eq!(game.internal_decision(View::P1).unwrap().token,question.token);
            }
            let handles=domain.members.iter().take(selected).map(|m|m.handle).collect();
            let result=resolved(game.answer_internal_card(question.actor,question.token,Answer::Targets(handles)).unwrap());
            assert_eq!(result.progress.facts.iter().filter(|f|matches!(f,Fact::Movement(_))).count(),if healed{selected}else{0});
            game.resume_internal_card().unwrap();
            let after=game.projection(View::Referee).unwrap();
            assert_eq!(after["P1"]["leader"]["life"],if healed{12_u8}else{10_u8});
            assert_eq!(after["P1"]["pp"]["current"],9_u8);
            assert_eq!(after["P1"]["ex"].as_array().unwrap().len(),occupied+if healed{selected}else{0});
            assert_eq!(after["P1"]["field"].as_array().unwrap().len(),3-if healed{selected}else{0});
            assert_eq!(after["objects"]["other-trait"]["card"],"BP01-037");
            assert_eq!(after["P2"]["field"],json!(["opponent-fairy"]));
            if healed {
                assert!(after["P1"]["ex"].as_array().unwrap().contains(&json!("first")));
                assert_eq!(after["objects"]["first"]["power"],1_u8, "the Fairy token survives in EX");
            }
        }
        let mut game=game(&position(printing,4));
        let before=game.projection(View::Referee).unwrap();
        let question=targets(game.play_internal_card("spell",PlayerId{seat:1}).unwrap());
        let error=game.answer_internal_card(question.actor,question.token,Answer::Targets(vec![0,1])).unwrap_err();
        assert!(format!("{error:?}").contains("capacity selection"));
        assert_eq!(game.projection(View::Referee).unwrap(),before,"a commit failure restores the whole play's public state");
        assert!(game.internal_decision(View::P1).is_none(),"the failed play has no outstanding target question");
    }
}

#[test]
fn bp05_007_quick_moves_the_defender_to_ex_and_stops_combat() {
    for printing in ["BP05-007","PR-118"] {
        let mut initial=draw_position();
        initial["players"]["P1"]["zones"]["field"]=json!([{"id":"attacker","card":"BP01-037"}]);
        initial["players"]["P2"]["leader"]["life"]=json!(10_u8);
        initial["players"]["P2"]["zones"]["field"]=json!([
            {"id":"first","card":"BP01-T03","state":{"acted":true}},
            {"id":"second","card":"BP01-T03"}
        ]);
        initial["players"]["P2"]["zones"]["hand"]=json!([{"id":"spell","card":printing}]);
        let mut game=game(&initial);
        game.internal_rule_action(&json!({"do":"attack","attacker":"attacker","target":"first"})).unwrap();
        let quick=game.internal_quick_choice().unwrap().unwrap();
        assert_eq!(quick.actor,PlayerId{seat:2});
        let InputDomain::Quick{options,..}=&quick.domain else{panic!("Quick")};
        assert_eq!(options.len(),1);
        assert_eq!(options[0].card,printing);
        let question=targets(game.answer_internal_card(quick.actor,quick.token,Answer::Quick(Some(options[0].handle))).unwrap());
        let InputDomain::Targets(domain)=&question.domain else{panic!("targets")};
        resolved(game.answer_internal_card(question.actor,question.token,Answer::Targets(vec![domain.members[0].handle])).unwrap());
        game.resume_internal_card().unwrap();
        let after=game.projection(View::Referee).unwrap();
        assert_eq!(after["P2"]["ex"],json!(["first"]));
        assert_eq!(after["P2"]["field"],json!(["second"]));
        assert_eq!(after["P2"]["leader"]["life"],12_u8);
        assert_eq!(after["P2"]["pp"]["current"],9_u8);
        assert_eq!(after["objects"]["attacker"]["damage"],0_u8);
        assert_eq!(after["objects"]["attacker"]["acted"],true);
        assert_eq!(after["objects"]["first"]["acted"],false);
        let pass=game.internal_quick_choice().unwrap().unwrap();
        assert_eq!(pass.actor,PlayerId{seat:2});
        assert!(matches!(game.answer_internal_card(pass.actor,pass.token,Answer::Quick(None)).unwrap(),CardPlay::Continued(_)));
        assert!(game.internal_quick_choice().unwrap().is_none(),"the vanished combat target ends the attack");
        assert_eq!(game.projection(View::Referee).unwrap()["objects"]["attacker"]["damage"],0_u8);
    }
}
