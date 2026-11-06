use super::*;

const PLAYER: PlayerId = PlayerId { seat: 1 };
/// Every JP printing that declares 【スタートアミュレット】.
const START_AMULETS: [&str; 16] = [
    "CSD03a-016",
    "CSD03a-P08",
    "CSD03b-016",
    "CSD03b-P09",
    "CP03-021",
    "PR-290",
    "CP03-041",
    "PR-271",
    "CP03-062",
    "PR-291",
    "CP03-082",
    "PR-272",
    "CP03-103",
    "PR-292",
    "CP03-124",
    "PR-293",
];

fn vanguard_opening(amulet: &str) -> Game {
    let filler = |seat: &str| -> Vec<Value> {
        (0_u8..8)
            .map(|index| json!({"id":format!("{seat}-filler{index}"),"card":"BP01-037"}))
            .collect()
    };
    let mut deck = vec![json!({"id":"amulet","card":amulet})];
    deck.extend(filler("P1"));
    let position = json!({"pregame":true,"players":{
        "P1":{"construction":"title","title_code":"cardfight_vanguard",
            "leader":{"class":"ニュートラル"},"deck_list":deck,"evolve_deck_list":[]},
        "P2":{"construction":"class","leader":{"class":"ニュートラル"},
            "deck_list":filler("P2"),"evolve_deck_list":[]}
    }});
    Game::from_selected_effects(
        selection_with_rules().clone(),
        &position,
        &Value::Null,
        &Value::Null,
        "start-amulet",
    )
    .unwrap()
}

#[test]
fn every_start_amulet_printing_is_offered_at_a_vanguard_opening() {
    for amulet in START_AMULETS {
        let mut game = vanguard_opening(amulet);
        let ordinary = game
            .internal_rule_action(&json!({"do":"choose-start-amulet","object":"P1-filler0"}));
        assert!(
            ordinary.is_err() || ordinary.is_ok_and(|step| step.outcome != "resolved"),
            "an ordinary card is not a start amulet"
        );
        let refused = game.projection(View::Referee).unwrap();
        assert_eq!(refused["P1"]["field"], json!([]), "the refusal places nothing");
        assert_eq!(
            game.internal_rule_action(&json!({"do":"choose-start-amulet","object":"amulet"}))
                .unwrap()
                .outcome,
            "resolved",
            "{amulet}"
        );
        assert_eq!(
            game.internal_rule_action(&json!({"do":"choose-first","first":"P1"}))
                .unwrap()
                .outcome,
            "resolved"
        );
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["field"], json!(["amulet"]), "{amulet}");
    }
}

#[test]
fn csd03_start_amulets_sacrifice_themselves_to_buff_only_a_vanguard_follower() {
    for card in ["CSD03a-016", "CSD03b-016"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["field"] = json!([
            {"id":"source","card":card},
            {"id":"vanguard","card":"CSD03a-005"},
            {"id":"plain","card":"BP01-037"}
        ]);
        let mut game = Game::from_selected_effects(
            selection_with_rules().clone(),
            &initial,
            &Value::Null,
            &Value::Null,
            "start-amulet-act",
        )
        .unwrap();
        let ability = selection_with_rules().current[card]
            .program
            .card()
            .abilities
            .iter()
            .find(|entry| entry.node.kind == AbilityKind::Activated)
            .unwrap()
            .node
            .id;
        let mut play = game
            .activate_internal_card("source", ability, PLAYER)
            .unwrap();
        while let CardPlay::Awaiting(question) = play {
            let InputDomain::Targets(domain) = &question.domain else {
                panic!("formal target")
            };
            assert_eq!(domain.members.len(), 1, "only the Vanguard follower qualifies");
            play = game
                .answer_internal_card(
                    question.actor,
                    question.token,
                    Answer::Targets(vec![domain.members[0].handle]),
                )
                .unwrap();
        }
        resolved(play);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["vanguard"]["power"], 3_u8, "{card}");
        assert_eq!(state["objects"]["vanguard"]["hp"], 3_u8);
        assert!(
            state["P1"]["cemetery"]
                .as_array()
                .unwrap()
                .contains(&json!("source")),
            "the amulet pays by going to the cemetery"
        );
    }
}
