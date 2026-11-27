use super::*;
use sve_engine::game::player::{DecisionShape, Input, QuestionShape, Viewer};

fn class_groups(game: &Game) -> Vec<Vec<u64>> {
    let Input::Question(question) = game.input(Viewer::P1) else {
        panic!("player Look question")
    };
    let QuestionShape::Decision(DecisionShape::Targets(selection)) = question.shape else {
        panic!("player Look selection")
    };
    selection
        .groups
        .into_iter()
        .map(|group| {
            assert_eq!(group.cardinality.minimum, Nat(0), "each class is optional");
            assert_eq!(
                group.cardinality.maximum,
                Some(Nat(1)),
                "each class permits at most one card"
            );
            group.handles
        })
        .collect()
}

fn finish_order(game: &mut Game, mut play: CardPlay) {
    while let CardPlay::Awaiting(order) = play {
        let InputDomain::Order(cards) = &order.domain else {
            panic!("remainder order")
        };
        play = game
            .answer_internal_card(
                order.actor,
                order.token,
                Answer::Order(cards.iter().rev().map(|card| card.handle).collect()),
            )
            .unwrap();
    }
    resolved(play);
}

fn position() -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP21-049"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP21-050"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"royal","card":"BP01-042"},
        {"id":"royal2","card":"BP01-034"},
        {"id":"elf","card":"BP01-009"},
        {"id":"costly","card":"BP01-051"},
        {"id":"spell","card":"BP01-057"},
        {"id":"unseen","card":"BP01-042"}
    ]);
    initial
}

fn evolve(game: &mut Game) -> CardPlay {
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
        )
        .unwrap()
        .outcome,
        "resolved",
        "the printed base evolves for one PP"
    );
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("evolution trigger")
    };
    game.answer_internal_card(
        pending.actor,
        pending.token,
        Answer::Pending(options[0].handle),
    )
    .unwrap()
}

#[test]
fn summoner_evolves_and_chooses_different_classes_from_only_the_looked_low_cost_followers() {
    for count in [0_usize, 1, 2] {
        let mut game = Game::from_selected_effects(
            selection().clone(),
            &position(),
            &Value::Null,
            &Value::Null,
            "look-classes",
        )
        .unwrap();
        let CardPlay::Awaiting(question) = evolve(&mut game) else {
            panic!("private Look selection")
        };
        assert!(
            game.internal_decision(View::P2).is_none(),
            "the opponent cannot see the Look question"
        );
        let InputDomain::Targets(domain) = &question.domain else {
            panic!("Look candidates")
        };
        assert_eq!(
            domain.members.len(),
            3,
            "high-cost followers, spells and unseen cards are excluded"
        );
        let groups = class_groups(&game);
        assert_eq!(groups.len(), 2, "the player receives one group per class");
        assert!(
            groups
                .iter()
                .any(|group| group == &vec![domain.members[0].handle, domain.members[1].handle]),
            "the two royal members share a maximum-one group"
        );
        if count == 2 {
            assert!(
                game.answer_internal_card(
                    question.actor,
                    question.token,
                    Answer::Targets(vec![domain.members[0].handle, domain.members[1].handle])
                )
                .is_err(),
                "two royal cards violate the class constraint"
            );
        }
        let handles = [0_usize, 2]
            .into_iter()
            .take(count)
            .map(|index| domain.members[index].handle)
            .collect();
        let play = game
            .answer_internal_card(question.actor, question.token, Answer::Targets(handles))
            .unwrap();
        finish_order(&mut game, play);
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["P1"]["field"].as_array().unwrap().len(), 1 + count);
        let mut remainder = vec!["royal", "royal2", "elf", "costly", "spell"];
        if count > 0 {
            remainder.retain(|id| *id != "royal");
        }
        if count > 1 {
            remainder.retain(|id| *id != "elf");
        }
        remainder.reverse();
        remainder.insert(0, "unseen");
        assert_eq!(view["P1"]["deck"], json!(remainder));
        assert_eq!(view["objects"]["unseen"]["generation"], 0_u8);
        let opponent = game.projection(View::P2).unwrap();
        for id in ["royal2", "costly", "spell", "unseen"] {
            assert!(
                !opponent["known_cards"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|card| card["id"] == id),
                "unselected cards stay private"
            );
        }
    }
}

#[test]
fn summoner_handles_one_class_empty_deck_and_field_capacity_without_exposing_remainders() {
    for (same_class, extra_field, empty, take) in [
        (true, 0_usize, false, 1_usize),
        (false, 3, false, 1),
        (false, 4, false, 0),
        (false, 0, true, 0),
    ] {
        let mut initial = position();
        if same_class {
            initial["players"]["P1"]["zones"]["deck"][2]["card"] = json!("BP01-042");
        }
        if empty {
            initial["players"]["P1"]["zones"]["deck"] = json!([]);
        }
        for n in 0..extra_field {
            initial["players"]["P1"]["zones"]["field"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":format!("old{n}"),"card":"BP01-042"}));
        }
        let mut game = Game::from_selected_effects(
            selection().clone(),
            &initial,
            &Value::Null,
            &Value::Null,
            "look-class-limits",
        )
        .unwrap();
        let CardPlay::Awaiting(question) = evolve(&mut game) else {
            panic!("bounded Look selection")
        };
        let InputDomain::Targets(domain) = &question.domain else {
            panic!("bounded candidates")
        };
        assert!(
            matches!(game.input(Viewer::P1), Input::Question(_)),
            "empty or full-field selections still project a valid player question"
        );
        assert_eq!(
            domain.cardinality.maximum,
            Some(Nat(u64::try_from(2_usize.min(4 - extra_field)).unwrap()))
        );
        if same_class {
            assert_eq!(class_groups(&game).len(), 1);
            assert!(
                game.answer_internal_card(
                    question.actor,
                    question.token,
                    Answer::Targets(vec![domain.members[0].handle, domain.members[1].handle])
                )
                .is_err(),
                "only one card can be chosen when every candidate has one class"
            );
        }
        let handles = domain
            .members
            .iter()
            .take(take)
            .map(|member| member.handle)
            .collect();
        let play = game
            .answer_internal_card(question.actor, question.token, Answer::Targets(handles))
            .unwrap();
        finish_order(&mut game, play);
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(
            view["P1"]["field"].as_array().unwrap().len(),
            1 + extra_field + take
        );
        assert_eq!(
            view["P1"]["deck"].as_array().unwrap().len(),
            if empty { 0 } else { 6 - take }
        );
        assert_ne!(view["game"]["ended"], true);
    }
}

#[test]
fn summoner_exposes_four_class_groups_without_limiting_the_supported_class_count() {
    let mut initial = position();
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"royal","card":"BP01-042"},
        {"id":"elf","card":"BP01-009"},
        {"id":"witch","card":"BP01-062"},
        {"id":"dragon","card":"BP01-092"},
        {"id":"royal2","card":"BP01-034"}
    ]);
    let mut game = Game::from_selected_effects(
        selection().clone(),
        &initial,
        &Value::Null,
        &Value::Null,
        "look-four-classes",
    )
    .unwrap();
    let CardPlay::Awaiting(question) = evolve(&mut game) else {
        panic!("four-class Look selection")
    };
    assert_eq!(class_groups(&game).len(), 4);
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("four-class candidates")
    };
    let play = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Targets(vec![domain.members[2].handle, domain.members[3].handle]),
        )
        .unwrap();
    finish_order(&mut game, play);
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["field"], json!(["source", "witch", "dragon"]));
    assert_eq!(view["P1"]["deck"], json!(["royal2", "elf", "royal"]));
}
