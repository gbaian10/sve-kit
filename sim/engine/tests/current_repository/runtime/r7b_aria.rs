#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Aria has a single search, optional return and the removal spell's formal target."
)]
use super::event_cemetery::finish_checks;
use super::*;
use sve_engine::game::internal::SearchAnswer;

const PLAYER: PlayerId = PlayerId { seat: 1 };

fn finish(game: &mut Game, mut step: CardPlay, base: &str, form: &str) {
    while let CardPlay::Awaiting(question) = step {
        let answer = match &question.domain {
            InputDomain::Search(domain) => {
                assert_eq!(domain.cards.len(), 1, "Only the cheap Fairy amulet is eligible.");
                assert_eq!(domain.cards[0].card, "BP13-012", "The real Wonder Tree is found.");
                assert!(game.internal_decision(View::P2).is_none(), "The opponent cannot answer the private search.");
                Answer::Search(SearchAnswer::Ungrouped(vec![domain.cards[0].handle]))
            }
            InputDomain::Integer(bounds) => {
                assert_eq!(bounds.minimum, Nat(0), "The printed Lastword may be declined.");
                assert_eq!(bounds.maximum, Some(Nat(1)), "The printed Lastword is a single optional return.");
                Answer::Integer(Nat(1))
            }
            InputDomain::Targets(domain) => {
                let source = domain.members.iter().find(|member| {
                    member.card.as_deref().is_some_and(|card| card == base || card == form)
                }).unwrap();
                Answer::Targets(vec![source.handle])
            }
            _ => panic!("Only Aria's printed choices and Soul Conversion's target occur."),
        };
        step = game.answer_internal_card(question.actor, question.token, answer).unwrap();
    }
    resolved(step);
    finish_checks(game);
}

fn pending(game: &mut Game, base: &str, form: &str) -> usize {
    let mut count = 0_usize;
    loop {
        finish_checks(game);
        let Some(question) = game.internal_pending_choice().unwrap() else { break };
        let InputDomain::Pending(options) = &question.domain else {
            panic!("The printed Fanfare, evolution or Lastword is pending.")
        };
        assert_eq!(options.len(), 1, "Each actual entry or departure supplies one pending ability.");
        let step = game.answer_internal_card(question.actor, question.token, Answer::Pending(options[0].handle)).unwrap();
        finish(game, step, base, form);
        count = count.checked_add(1).unwrap();
    }
    count
}

fn position(base: &str, form: &str) -> Value {
    let mut initial = draw_position();
    initial["turn"]["elapsed_turns"]["P1"] = json!(7_u8);
    initial["players"]["P1"]["leader"]["class"] = json!("エルフ");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"not-fairy","card":"BP01-T09"}]);
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"source","card":base}, {"id":"removal","card":"BP01-116"}
    ]);
    initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"waiting","card":"BP13-T07"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"tree","card":"BP13-012"}, {"id":"d0","card":"BP01-043"},
        {"id":"d1","card":"BP01-043"}, {"id":"d2","card":"BP01-043"}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"opposing","card":"BP13-T07","state":{"acted":true}}]);
    initial
}

fn recipients(game: &Game, fairy: &str, rush: bool) {
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["objects"][fairy]["keywords"], if rush { json!(["突進"]) } else { json!([]) }, "The own field Fairy token follows the live provider.");
    assert!(!view["objects"]["not-fairy"]["keywords"].as_array().unwrap().contains(&json!("突進")), "A token of another trait receives no Rush.");
    assert_eq!(view["objects"]["opposing"]["keywords"], json!([]), "The opponent's Fairy token receives no Rush.");
}

fn entered(game: &mut Game, id: &str) {
    assert!(matches!(game.play_internal_card(id, PLAYER).unwrap(), CardPlay::Entered(_)), "The real follower must enter after paying its printed cost.");
    finish_checks(game);
}

fn run_case(base: &str, form: &str, evolve: bool) {
    let mut game = Game::from_selected_effects(selection_with_rules().clone(), &position(base, form), &Value::Null, &Value::Null, "r7b-aria").unwrap();
    entered(&mut game, "source");
    assert_eq!(pending(&mut game, base, form), 1, "The base Fanfare creates one real Fairy in EX.");
    let before = game.projection(View::Referee).unwrap();
    assert_eq!(before["P1"]["ex"].as_array().unwrap().len(), 2, "The new Fairy joins the waiting EX Fairy.");
    assert_eq!(before["objects"]["source"]["keywords"], json!([]), "Aria is a Fairy follower but is not a token.");
    let fairy = before["P1"]["ex"].as_array().unwrap().iter().filter_map(Value::as_str).find(|id| *id != "waiting").unwrap().to_owned();
    assert_eq!(before["objects"][fairy.as_str()]["name"], "フェアリー", "The Fanfare creates the printed Fairy token.");
    entered(&mut game, &fairy);
    recipients(&game, &fairy, true);
    if evolve {
        game.internal_rule_action(&json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})).unwrap();
        assert_eq!(pending(&mut game, base, form), 2, "The actual evolution searches Wonder Tree, whose Fanfare creates another Fairy.");
        let view = game.projection(View::Referee).unwrap();
        assert!(view["P1"]["field"].as_array().unwrap().contains(&json!("tree")), "The searched amulet actually entered.");
        assert_eq!(view["objects"]["tree"]["keywords"], json!([]), "The Fairy amulet is outside the follower-token recipient filter.");
        assert_eq!(view["objects"]["source"]["keywords"], json!([]), "The evolved Aria is still a non-token follower.");
        recipients(&game, &fairy, true);
    }
    let step = game.play_internal_card("removal", PLAYER).unwrap();
    finish(&mut game, step, base, form);
    let departed = game.projection(View::Referee).unwrap();
    assert!(departed["P1"]["cemetery"].as_array().unwrap().contains(&json!("source")), "Soul Conversion destroyed the actual Aria.");
    recipients(&game, &fairy, false);
    assert_eq!(pending(&mut game, base, form), 1, "The base or evolved Lastword returns the same physical card.");
    recipients(&game, &fairy, true);
    check_return(&mut game, base, evolve, &fairy);
}

fn finish_battle(game: &mut Game) {
    loop {
        finish_checks(game);
        assert!(game.internal_pending_choice().unwrap().is_none(), "The two vanilla Fairies have no combat triggered abilities.");
        let Some(question) = game.internal_quick_choice().unwrap() else { break };
        let InputDomain::Quick { options, .. } = &question.domain else {
            panic!("The declared attack offers its normal Quick timing.")
        };
        assert!(options.is_empty(), "Neither player has a legal Quick play in this position.");
        let continued = game.answer_internal_card(question.actor, question.token, Answer::Quick(None)).unwrap();
        assert!(matches!(continued, CardPlay::Continued(_)), "A Quick Pass continues the stored combat procedure.");
    }
}

fn check_return(game: &mut Game, base: &str, evolved: bool, fairy: &str) {
    let view = game.projection(View::Referee).unwrap();
    assert!(view["P1"]["ex"].as_array().unwrap().contains(&json!("source")), "The Lastword places Aria in EX.");
    assert_eq!(view["objects"]["source"]["card"], base, "The EX source is the original physical base printing.");
    assert_eq!(view["P1"]["evolve_deck"], json!(["form"]), "The evolution resource is separate from the returned base.");
    assert_eq!(view["P1"]["ex"].as_array().unwrap().len(), if evolved { 3 } else { 2 }, "Only evolution added Wonder Tree's extra Fairy.");
    assert_eq!(view["P1"]["pp"]["current"], if evolved { 4_u8 } else { 5_u8 }, "Play, Fairy, removal and optional evolution pay the real printed PP costs.");
    assert_eq!(view["P1"]["hand"].as_array().unwrap().len(), 2, "Soul Conversion drew its actual two cards.");
    assert!(game.play_internal_card("source", PLAYER).is_err(), "Aria's printed prohibition prevents playing this affordable card from EX into an open field.");
    let rejected = game.projection(View::Referee).unwrap();
    assert_eq!(rejected["P1"]["pp"], view["P1"]["pp"], "The rejected EX play spends no PP.");
    assert_eq!(rejected["P1"]["ex"], view["P1"]["ex"], "The rejected EX play leaves all EX objects intact.");
    assert_eq!(game.internal_rule_action(&json!({"do":"attack","attacker":fairy,"target":"P2.leader"})).unwrap().outcome, "cannot-attack", "The new Fairy has Rush, which does not permit attacking a leader.");
    assert_eq!(game.internal_rule_action(&json!({"do":"attack","attacker":fairy,"target":"opposing"})).unwrap().outcome, "resolved", "The EX provider's live Rush permits the new Fairy to attack a rested enemy.");
    finish_battle(game);
    let fought = game.projection(View::Referee).unwrap();
    assert!(!fought["P1"]["field"].as_array().unwrap().contains(&json!(fairy)), "The real Fairy dealt and received its one combat damage.");
    assert_eq!(fought["P2"]["field"], json!([]), "The opposing Fairy was destroyed by actual combat.");

}

#[test]
fn r7b_aria_all_base_and_evolved_printings_keep_rush_across_the_actual_lastword_return() {
    for (base, form) in [("BP13-003", "BP13-004"), ("BP13-SL03", "BP13-SL04"), ("EBD01-005", "EBD01-006")] {
        assert!(selection_with_rules().current.contains_key(base));
        assert!(selection_with_rules().current.contains_key(form));
        for evolve in [false, true] {
            run_case(base, form, evolve);
        }
    }
}
