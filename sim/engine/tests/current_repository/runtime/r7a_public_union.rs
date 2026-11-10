#![expect(
    clippy::panic,
    reason = "Only the printed target and flip decisions may occur."
)]
use super::*;
use sve_engine::game::internal::DecisionView;

fn targets(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(question) = play else {
        panic!("targets")
    };
    assert!(
        matches!(question.domain, InputDomain::Targets(_)),
        "the ability asks for selected cards"
    );
    question
}

fn select(game: &mut Game, question: &DecisionView, all: bool) -> CardPlay {
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("target domain")
    };
    let handles = if all {
        domain.members.iter().map(|member| member.handle).collect()
    } else {
        vec![]
    };
    game.answer_internal_card(question.actor, question.token, Answer::Targets(handles))
        .unwrap()
}

fn activation(card: &str) -> AbilityId {
    selection().current[card]
        .program
        .card()
        .abilities
        .iter()
        .find(|ability| ability.node.kind == AbilityKind::Activated)
        .unwrap()
        .node
        .id
}

#[test]
fn bp14_077_both_printings_return_public_cards_from_both_zones_and_flip_only_the_named_form() {
    for card in ["BP14-077", "BP14-U05"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"tail","card":"BP01-037"}]);
        initial["players"]["P1"]["zones"]["cemetery"] = json!([
            {"id":"source","card":card},{"id":"grave","card":"BP14-072"},{"id":"other","card":"BP01-037"}
        ]);
        initial["players"]["P1"]["zones"]["banish"] = json!([
            {"id":"out","card":"BP14-072"},{"id":"hidden","card":"BP14-072","face_up":false}
        ]);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([
            {"id":"form","card":"BP14-073","face_up":true},
            {"id":"down","card":"BP14-073","face_up":false},
            {"id":"wrong_name","card":"BP01-002","face_up":true}
        ]);
        initial["players"]["P2"]["zones"]["cemetery"] = json!([{"id":"foreign","card":"BP14-072"}]);
        let mut game = game(&initial);
        let question = targets(
            game.activate_internal_card("source", activation(card), PlayerId { seat: 1 })
                .unwrap(),
        );
        let InputDomain::Targets(domain) = &question.domain else {
            panic!("return targets")
        };
        assert_eq!(domain.members.len(), 2);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
            10_u8
        );
        let flip = targets(select(&mut game, &question, true));
        let InputDomain::Targets(forms) = &flip.domain else {
            panic!("flip targets")
        };
        assert_eq!(forms.members.len(), 1);
        assert_eq!(forms.members[0].card.as_deref(), Some("BP14-073"));
        resolved(select(&mut game, &flip, true));
        let state = game.projection(View::Referee).unwrap();
        let deck = state["P1"]["deck"].as_array().unwrap();
        assert_eq!(deck.len(), 3);
        for id in ["tail", "grave", "out"] {
            assert!(deck.contains(&json!(id)));
        }
        assert_eq!(state["P1"]["cemetery"], json!(["other"]));
        assert_eq!(state["P1"]["pp"]["current"], 9_u8);
        assert!(
            state["P1"]["banish"]
                .as_array()
                .unwrap()
                .contains(&json!("source"))
        );
        assert!(
            state["P1"]["banish"]
                .as_array()
                .unwrap()
                .contains(&json!("hidden"))
        );
        assert_eq!(state["objects"]["form"]["face_up"], false);
        assert_eq!(state["objects"]["wrong_name"]["face_up"], true);
        assert_eq!(state["P2"]["cemetery"], json!(["foreign"]));
    }
}

#[test]
fn bp14_077_returning_zero_still_shuffles_and_flips_the_evolve_card() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["cemetery"] = json!([{"id":"source","card":"BP14-077"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"tail","card":"BP01-037"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] =
        json!([{"id":"form","card":"BP14-073","face_up":true}]);
    let mut game = game(&initial);
    let question = targets(
        game.activate_internal_card("source", activation("BP14-077"), PlayerId { seat: 1 })
            .unwrap(),
    );
    let flip = targets(select(&mut game, &question, false));
    let result = resolved(select(&mut game, &flip, true));
    assert!(
        result
            .progress
            .facts
            .iter()
            .any(|fact| matches!(fact, Fact::Shuffled { .. }))
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["form"]["face_up"],
        false
    );
}

#[test]
fn bp14_077_full_ex_keeps_the_deck_and_banishes_two_private_hand_cards() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"source","card":"BP14-077"},{"id":"hand1","card":"BP01-037"},{"id":"hand2","card":"BP01-034"}
    ]);
    initial["players"]["P1"]["zones"]["ex"] = json!(
        (0_u8..5)
            .map(|index| json!({"id":format!("ex{index}"),"card":"BP01-037"}))
            .collect::<Vec<_>>()
    );
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"top","card":"BP01-037"}]);
    let mut game = game(&initial);
    let mut question = targets(
        game.play_internal_card("source", PlayerId { seat: 1 })
            .unwrap(),
    );
    for index in 0_u8..2 {
        assert!(game.internal_decision(View::P2).is_none());
        let InputDomain::Targets(domain) = &question.domain else {
            panic!("private hand")
        };
        let handle = domain.members[0].handle;
        let result = game
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Targets(vec![handle]),
            )
            .unwrap();
        if index == 0 {
            question = targets(result);
        } else {
            resolved(result);
            break;
        }
    }
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["deck"], json!(["top"]));
    assert_eq!(state["P1"]["ex"].as_array().unwrap().len(), 5);
    assert_eq!(state["P1"]["hand"], json!([]));
    assert_eq!(state["P1"]["banish"], json!(["hand1", "hand2"]));
    assert_eq!(state["P1"]["cemetery"], json!(["source"]));
}

#[test]
fn bp14_077_empty_hand_still_moves_two_deck_cards_to_ex() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP14-077"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"top1","card":"BP01-037"},{"id":"top2","card":"BP01-034"}
    ]);
    let mut game = game(&initial);
    let question = targets(
        game.play_internal_card("source", PlayerId { seat: 1 })
            .unwrap(),
    );
    let second = targets(select(&mut game, &question, false));
    resolved(select(&mut game, &second, false));
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["ex"], json!(["top1", "top2"]));
    assert_eq!(state["P1"]["deck"], json!([]));
}
