use super::*;
use sve_engine::game::player::{
    Command, Input, OpeningAnswer, OpeningQuestion, QuestionShape, SubmissionKind, Viewer,
};

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

fn vanguard_opening(amulet: &str, random: &Value) -> Game {
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
        random,
        "start-amulet",
    )
    .unwrap()
}

#[test]
fn every_start_amulet_printing_is_offered_at_a_vanguard_opening() {
    for amulet in START_AMULETS {
        let mut game = vanguard_opening(amulet, &Value::Null);
        let ordinary =
            game.internal_rule_action(&json!({"do":"choose-start-amulet","object":"P1-filler0"}));
        assert!(
            ordinary.is_err() || ordinary.is_ok_and(|step| step.outcome != "resolved"),
            "an ordinary card is not a start amulet"
        );
        let refused = game.projection(View::Referee).unwrap();
        assert_eq!(
            refused["P1"]["field"],
            json!([]),
            "the refusal places nothing"
        );
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
fn mulligan_matches_a_complete_order_with_input_metadata() {
    let mut game = vanguard_opening(START_AMULETS[0], &Value::Null);
    game.internal_rule_action(&json!({"do":"choose-start-amulet","object":"amulet"}))
        .unwrap();
    game.internal_rule_action(&json!({"do":"choose-first","first":"P1"}))
        .unwrap();

    let hand = game.projection(View::Referee).unwrap()["P1"]["hand"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(hand.len(), 4);
    let incomplete_order = hand.iter().take(3).cloned().collect::<Vec<_>>();

    let invalid = game
        .internal_rule_action(&json!({
            "do":"mulligan",
            "redo":true,
            "order":incomplete_order,
            "by":"P1",
            "at":"pregame"
        }))
        .unwrap();
    assert_ne!(invalid.outcome, "resolved");

    let accepted = game
        .internal_rule_action(&json!({
            "do":"mulligan",
            "redo":true,
            "order":hand.into_iter().rev().collect::<Vec<_>>(),
            "by":"P1",
            "at":"pregame"
        }))
        .unwrap();
    assert_eq!(accepted.outcome, "resolved");
}

#[test]
fn start_amulet_shuffle_fault_rolls_back_placement_and_closes_input() {
    let mut game = vanguard_opening(
        "CP03-021",
        &json!({"shuffles":[{"player":"P2","zone":"deck","result":[]}]}),
    );
    let Input::Question(question) = game.input(Viewer::P1) else {
        panic!("expected start amulet choice");
    };
    let QuestionShape::Opening(OpeningQuestion::StartAmulet(cards)) = question.shape else {
        panic!("expected start amulet candidates");
    };
    let command = Command::Opening {
        token: question.token,
        answer: OpeningAnswer::StartAmulet(cards.first().unwrap().handle.clone()),
    };
    let before = [Viewer::P1, Viewer::P2].map(|viewer| game.observe(viewer).unwrap());
    assert_eq!(
        game.submit_player(Viewer::P1, command.clone()).kind,
        SubmissionKind::EngineFault
    );
    for (viewer, observation) in [Viewer::P1, Viewer::P2].into_iter().zip(before) {
        assert_eq!(game.observe(viewer).unwrap(), observation);
        assert_eq!(game.input(viewer), Input::EngineFault);
    }
    assert_eq!(
        game.submit_player(Viewer::P1, command).kind,
        SubmissionKind::EngineFault
    );
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
            assert_eq!(
                domain.members.len(),
                1,
                "only the Vanguard follower qualifies"
            );
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
