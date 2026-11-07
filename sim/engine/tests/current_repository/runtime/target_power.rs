use super::*;

const PLAYER: PlayerId = PlayerId { seat: 1 };

fn rules_game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "target-power",
    )
    .unwrap()
}

fn has_storm(state: &Value, id: &str) -> bool {
    state["objects"][id]["keywords"]
        .as_array()
        .is_some_and(|keywords| keywords.contains(&json!("疾走")))
}

#[test]
fn csd02b_001_grants_storm_only_to_another_ally_with_three_or_less_power() {
    for printing in ["CSD02b-001", "CSD02b-SL01"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":printing}]);
        // Three クール followers with power 2, 3 and 4.
        initial["players"]["P1"]["zones"]["field"] = json!([
            {"id":"low","card":"CP02-042"},
            {"id":"mid","card":"CP02-039"},
            {"id":"high","card":"CP02-066"}
        ]);
        let mut game = rules_game(&initial);
        let mut step = Some(game.play_internal_card("source", PLAYER).unwrap());
        let mut asked = false;
        loop {
            while let Some(CardPlay::Awaiting(question)) = &step {
                let InputDomain::Targets(domain) = &question.domain else {
                    panic!("the power-filtered choice")
                };
                let mut cards: Vec<_> =
                    domain.members.iter().map(|member| member.card.clone()).collect();
                cards.sort();
                assert_eq!(
                    cards,
                    [Some("CP02-039".to_owned()), Some("CP02-042".to_owned())],
                    "{printing}: only the other allies with power 3 or less qualify"
                );
                asked = true;
                let low = domain
                    .members
                    .iter()
                    .find(|member| member.card.as_deref() == Some("CP02-042"))
                    .unwrap();
                step = Some(
                    game.answer_internal_card(
                        question.actor,
                        question.token,
                        Answer::Targets(vec![low.handle]),
                    )
                    .unwrap(),
                );
            }
            while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
                game.resume_internal_card().unwrap();
            }
            let Some(pending) = game.internal_pending_choice().unwrap() else {
                break;
            };
            let InputDomain::Pending(options) = &pending.domain else {
                panic!("the fanfare")
            };
            step = Some(
                game.answer_internal_card(pending.actor, pending.token, Answer::Pending(options[0].handle))
                    .unwrap(),
            );
        }
        assert!(asked, "the fanfare asked for an ally");
        let state = game.projection(View::Referee).unwrap();
        assert!(has_storm(&state, "low"), "the chosen ally gains Storm");
        assert!(!has_storm(&state, "high"));
    }
}

#[test]
fn cp04_032_ub_attack_targets_only_an_enemy_with_three_or_more_power() {
    for printing in ["CP04-032", "CP04-P24"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["construction"] = json!("title");
        initial["players"]["P1"]["title_code"] = json!("princess_connect_re_dive");
        initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
        initial["players"]["P1"]["zones"]["field"] =
            json!([{"id":"source","card":printing,"state":{"evolved":true}}]);
        // CP02-042 has power 2; CP02-066 has power 4.
        initial["players"]["P2"]["zones"]["field"] = json!([
            {"id":"weak","card":"CP02-042"},
            {"id":"strong","card":"CP02-066"}
        ]);
        let mut game = rules_game(&initial);
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"P2.leader"}))
            .unwrap();
        let pending = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = &pending.domain else {
            panic!("the UB attack trigger")
        };
        let mut play = game
            .answer_internal_card(pending.actor, pending.token, Answer::Pending(options[0].handle))
            .unwrap();
        while let CardPlay::Awaiting(question) = play {
            let InputDomain::Targets(domain) = &question.domain else {
                panic!("the power-filtered target")
            };
            assert_eq!(domain.members.len(), 1, "{printing}: only the power-4 enemy qualifies");
            assert_eq!(domain.members[0].card.as_deref(), Some("CP02-066"));
            play = game
                .answer_internal_card(
                    question.actor,
                    question.token,
                    Answer::Targets(vec![domain.members[0].handle]),
                )
                .unwrap();
        }
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["strong"]["damage"], 2_u8);
        assert_eq!(state["objects"]["weak"]["damage"], 0_u8);
    }
}
