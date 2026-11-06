use super::*;

const PLAYER: PlayerId = PlayerId { seat: 1 };

fn hand_activation(game: &mut Game, card: &str) -> CardPlay {
    let ability = selection_with_rules().current[card]
        .program
        .card()
        .abilities
        .iter()
        .find(|entry| {
            entry.node.kind == AbilityKind::Activated
                && entry.node.active_zones == [ZoneKind::Hand]
        })
        .unwrap()
        .node
        .id;
    game.activate_internal_card("source", ability, PLAYER).unwrap()
}

fn rules_game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "hand-ex-cost",
    )
    .unwrap()
}

fn paid_from_hand(state: &Value, remaining_pp: u8) {
    assert_eq!(state["P1"]["pp"]["current"], remaining_pp, "the printed PP is paid");
    assert!(
        state["P1"]["ex"].as_array().unwrap().contains(&json!("source")),
        "the card pays by moving from the hand to EX"
    );
    assert!(
        !state["P1"]["hand"].as_array().unwrap().contains(&json!("source")),
        "the source left the hand"
    );
}

#[test]
fn bp10_017_moves_itself_to_ex_and_then_a_chosen_field_card() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP10-017"}]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"ally","card":"BP01-037"}]);
    let mut game = rules_game(&initial);
    let mut play = hand_activation(&mut game, "BP10-017");
    while let CardPlay::Awaiting(question) = play {
        let InputDomain::Targets(domain) = &question.domain else {
            panic!("formal target")
        };
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
    paid_from_hand(&state, 9);
    assert_eq!(state["P1"]["field"], json!([]));
    assert!(state["P1"]["ex"].as_array().unwrap().contains(&json!("ally")));
}

#[test]
fn bp08_072_pays_from_the_hand_and_keeps_an_undead_from_the_top_two() {
    for card in ["BP08-072", "BP08-P17"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
        initial["players"]["P1"]["zones"]["deck"] = json!([
            {"id":"undead","card":"BP01-124"},
            {"id":"plain","card":"BP01-037"}
        ]);
        let mut game = rules_game(&initial);
        let mut play = hand_activation(&mut game, "BP08-072");
        while let CardPlay::Awaiting(question) = play {
            let InputDomain::Targets(domain) = &question.domain else {
                panic!("the undead choice")
            };
            assert_eq!(domain.members.len(), 1, "only the undead qualifies");
            let answer = Answer::Targets(vec![domain.members[0].handle]);
            play = game
                .answer_internal_card(question.actor, question.token, answer)
                .unwrap();
        }
        resolved(play);
        let state = game.projection(View::Referee).unwrap();
        paid_from_hand(&state, 9);
        assert_eq!(state["P1"]["hand"], json!(["undead"]), "{card}");
        assert_eq!(state["P1"]["cemetery"], json!(["plain"]));
    }
}
