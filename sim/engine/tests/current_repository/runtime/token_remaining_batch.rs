#![expect(
    clippy::panic,
    reason = "Unexpected questions invalidate these real-card scenarios."
)]
use super::event_cemetery::finish_checks;
use super::*;
use sve_engine::game::internal::SearchAnswer;

const fn actor() -> PlayerId {
    PlayerId { seat: 1 }
}

fn batch_game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "token-remaining-batch",
    )
    .unwrap()
}

fn finish(game: &mut Game, mut play: CardPlay, choices: &[usize], take: bool) -> CardPlay {
    let mut remaining_choices = choices.iter().copied();
    while let CardPlay::Awaiting(question) = play {
        let answer = if let InputDomain::Modes { modes, .. } = &question.domain {
            let selected = remaining_choices.next().unwrap_or(0);
            Answer::Modes(vec![modes[selected]])
        } else if let InputDomain::Targets(targets) = &question.domain {
            Answer::Targets(
                targets
                    .members
                    .iter()
                    .take(usize::from(take))
                    .map(|member| member.handle)
                    .collect(),
            )
        } else if let InputDomain::Search(search) = &question.domain {
            Answer::Search(SearchAnswer::Ungrouped(
                search
                    .cards
                    .iter()
                    .take(usize::from(take))
                    .map(|card| card.handle)
                    .collect(),
            ))
        } else if let InputDomain::CostMaterials(materials) = &question.domain {
            assert_eq!(
                materials.members.len(),
                1,
                "only the Machine hand card pays"
            );
            Answer::CostMaterials(vec![materials.members.first().unwrap().handle])
        } else if matches!(question.domain, InputDomain::WardEntry { .. }) {
            Answer::WardEntry(false)
        } else if matches!(question.domain, InputDomain::OptionalCost) {
            Answer::OptionalCost(true)
        } else {
            panic!("unexpected printed question");
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    play
}

fn enter(game: &mut Game) {
    let play = game.play_internal_card("source", actor()).unwrap();
    assert!(
        matches!(finish(game, play, &[], true), CardPlay::Entered(_)),
        "the real follower enters after payment"
    );
}

fn chosen_trigger(game: &mut Game, choices: &[usize]) {
    selected_trigger(game, choices, true);
}

fn selected_trigger(game: &mut Game, choices: &[usize], take: bool) {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("printed trigger");
    };
    assert_eq!(options.len(), 1, "one printed trigger is pending");
    let play = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options.first().unwrap().handle),
        )
        .unwrap();
    resolved(finish(game, play, choices, take));
}

fn trigger(game: &mut Game) {
    chosen_trigger(game, &[]);
}

fn attack_and_pass(game: &mut Game) {
    game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"enemy"}))
        .unwrap();
    let question = game.internal_quick_choice().unwrap().unwrap();
    game.answer_internal_card(question.actor, question.token, Answer::Quick(None))
        .unwrap();
}

#[test]
fn bp17_048_printings_put_the_optional_machine_and_always_create_product_machine_in_ex() {
    for number in ["BP17-048", "PR-454"] {
        for take in [false, true] {
            let mut initial = draw_position();
            initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":number}]);
            initial["players"]["P1"]["zones"]["deck"] = json!([
                {"id":"machine","card":"BP07-110"},{"id":"below","card":"BP01-037"}]);
            let mut game = batch_game(&initial);
            let play = game.play_internal_card("source", actor()).unwrap();
            resolved(finish(&mut game, play, &[], take));
            let state = game.projection(View::Referee).unwrap();
            let ex = state["P1"]["ex"].as_array().unwrap();
            assert_eq!(ex.len(), if take { 2_usize } else { 1 });
            assert!(
                ex.iter()
                    .any(|id| state["objects"][id.as_str().unwrap()]["card"] == "BP07-T01")
            );
            assert_eq!(ex.iter().any(|id| id == "machine"), take);
            assert_eq!(
                state["P1"]["deck"],
                if take {
                    json!(["below"])
                } else {
                    json!(["machine", "below"])
                }
            );
            assert_eq!(state["P2"]["ex"], json!([]));
        }
    }
}

#[test]
fn bp07_116_printings_offer_both_token_bodies_and_pay_machine_discard_for_lastword() {
    for number in ["BP07-116", "PR-206", "PR-339"] {
        for (mode, expected) in [(0_usize, "BP07-T01"), (1, "BP07-T02")] {
            let mut initial = draw_position();
            initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":number}]);
            let mut game = batch_game(&initial);
            enter(&mut game);
            chosen_trigger(&mut game, &[mode]);
            let state = game.projection(View::Referee).unwrap();
            let ex = state["P1"]["ex"].as_array().unwrap();
            assert_eq!(ex.len(), 1);
            assert_eq!(
                state["objects"][ex.first().unwrap().as_str().unwrap()]["card"],
                expected
            );
        }
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":number}]);
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"payment","card":"BP07-110"}]);
        initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"drawn","card":"BP01-037"}]);
        initial["players"]["P2"]["zones"]["field"] =
            json!([{"id":"enemy","card":"BP01-037","state":{"acted":true}}]);
        let mut game = batch_game(&initial);
        attack_and_pass(&mut game);
        trigger(&mut game);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["hand"], json!(["drawn"]));
        assert_eq!(state["P1"]["cemetery"], json!(["source", "payment"]));
        assert_eq!(state["P1"]["deck"], json!([]));
    }
}

#[test]
fn bp12_069_printings_choose_field_and_ex_tokens_independently() {
    for number in ["BP12-069", "BP12-SL18"] {
        for field_choice in [0_usize, 1] {
            for ex_choice in [0_usize, 1] {
                let mut initial = draw_position();
                initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":number}]);
                let mut game = batch_game(&initial);
                enter(&mut game);
                chosen_trigger(&mut game, &[field_choice, ex_choice]);
                let state = game.projection(View::Referee).unwrap();
                let field = state["P1"]["field"].as_array().unwrap();
                assert_eq!(field.len(), 2);
                let created = field
                    .iter()
                    .find(|id| *id != "source")
                    .unwrap()
                    .as_str()
                    .unwrap();
                assert_eq!(
                    state["objects"][created]["card"],
                    if field_choice == 0 {
                        "BP07-T01"
                    } else {
                        "BP01-T15"
                    }
                );
                let ex = state["P1"]["ex"].as_array().unwrap();
                assert_eq!(ex.len(), 1);
                assert_eq!(
                    state["objects"][ex.first().unwrap().as_str().unwrap()]["card"],
                    if ex_choice == 0 {
                        "BP07-T01"
                    } else {
                        "BP01-T15"
                    }
                );
            }
        }
    }
}

#[test]
fn karen_printings_leave_a_playable_magic_item_in_ex_after_battle_destruction() {
    for number in ["ECP02-016", "ECP02-SL10"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":number}]);
        initial["players"]["P2"]["zones"]["field"] =
            json!([{"id":"enemy","card":"BP01-037","state":{"acted":true}}]);
        let mut game = batch_game(&initial);
        attack_and_pass(&mut game);
        trigger(&mut game);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["cemetery"], json!(["source"]));
        let items = state["P1"]["ex"].as_array().unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(
            state["objects"][items.first().unwrap().as_str().unwrap()]["card"],
            "CP02-T01"
        );
        assert_eq!(state["P2"]["ex"], json!([]));
    }
}

#[test]
fn dragosnake_printings_pay_lastword_and_find_a_real_same_name_follower_on_field() {
    for number in ["BP22-070", "BP22-P38"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["pp"]["current"] = json!(2_u8);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":number}]);
        initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"replacement","card":"BP22-P38"}]);
        initial["players"]["P2"]["zones"]["field"] =
            json!([{"id":"enemy","card":"BP01-037","state":{"acted":true}}]);
        let mut game = batch_game(&initial);
        attack_and_pass(&mut game);
        trigger(&mut game);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["cemetery"], json!(["source"]));
        assert_eq!(state["P1"]["field"], json!(["replacement"]));
        assert_eq!(state["P1"]["deck"], json!([]));
        assert_eq!(state["P1"]["ex"], json!([]));
        assert_eq!(state["P1"]["pp"]["current"], 0_u8);
        assert_eq!(state["objects"]["replacement"]["card"], "BP22-P38");
        assert_eq!(state["objects"]["replacement"]["keywords"], json!(["突進"]));
    }
}

#[test]
fn zombie_dog_printings_buff_at_exactly_two_grave_copies_and_always_damage_the_enemy_leader() {
    for number in ["BP22-080", "BP22-P42"] {
        for count in [1_usize, 2, 3] {
            let mut initial = draw_position();
            initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":number}]);
            initial["players"]["P1"]["zones"]["cemetery"] = Value::Array(
                (0..count)
                    .map(|index| json!({"id":format!("grave{index}"),"card":"BP22-080"}))
                    .collect(),
            );
            let mut game = batch_game(&initial);
            game.internal_rule_action(
                &json!({"do":"attack","attacker":"source","target":"P2.leader"}),
            )
            .unwrap();
            trigger(&mut game);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["objects"]["source"]["power"],
                if count == 2 { 5_u8 } else { 3_u8 }
            );
            assert_eq!(
                state["objects"]["source"]["hp"],
                if count == 2 { 4_u8 } else { 2_u8 }
            );
            assert_eq!(state["P2"]["leader"]["life"], 18_u8);
            assert_eq!(state["P1"]["ex"], json!([]));
        }
    }
}

#[test]
fn karen_fanfare_adds_the_chosen_cool_card_and_reveals_it_to_the_opponent() {
    for number in ["ECP02-016", "ECP02-SL10"] {
        for take in [false, true] {
            let mut initial = draw_position();
            initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":number}]);
            initial["players"]["P1"]["zones"]["deck"] = json!([
            {"id":"found","card":"ECP02-SL10"},{"id":"below","card":"BP01-037"}]);
            let mut game = batch_game(&initial);
            enter(&mut game);
            selected_trigger(&mut game, &[], take);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["P1"]["hand"],
                if take { json!(["found"]) } else { json!([]) }
            );
            assert_eq!(
                state["P1"]["deck"],
                if take {
                    json!(["below"])
                } else {
                    json!(["found", "below"])
                }
            );
            let public = game.projection(View::P2).unwrap();
            let known = public["known_cards"].as_array().unwrap();
            assert_eq!(known.iter().any(|card| card["id"] == "found"), take);
            assert!(!known.iter().any(|card| card["id"] == "below"));
        }
    }
}

#[test]
fn optional_lastword_costs_can_be_declined_without_discarding_drawing_or_searching() {
    for number in ["BP07-116", "PR-206", "PR-339", "BP22-070", "BP22-P38"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["pp"]["current"] = json!(3_u8);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":number}]);
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"payment","card":"BP07-110"}]);
        initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"drawn","card":"BP22-P38"}]);
        initial["players"]["P2"]["zones"]["field"] =
            json!([{"id":"enemy","card":"BP01-037","state":{"acted":true}}]);
        let mut game = batch_game(&initial);
        attack_and_pass(&mut game);
        let question = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = &question.domain else {
            panic!("printed lastword");
        };
        let CardPlay::Awaiting(payment) = game
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(options.first().unwrap().handle),
            )
            .unwrap()
        else {
            panic!("optional printed payment");
        };
        assert!(matches!(payment.domain, InputDomain::OptionalCost));
        assert_eq!(
            game.answer_internal_card(payment.actor, payment.token, Answer::OptionalCost(false),)
                .unwrap(),
            CardPlay::Cancelled
        );
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["pp"]["current"], 3_u8);
        assert_eq!(state["P1"]["hand"], json!(["payment"]));
        assert_eq!(state["P1"]["deck"], json!(["drawn"]));
        assert_eq!(state["P1"]["cemetery"], json!(["source"]));
        assert_eq!(state["P1"]["field"], json!([]));
        assert_eq!(state["P1"]["ex"], json!([]));
        finish_checks(&mut game);
        assert!(game.internal_pending_choice().unwrap().is_none());
    }
}

#[test]
fn zombie_dog_lastword_finds_a_real_same_name_card_in_ex_after_battle_destruction() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP22-080"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"replacement","card":"BP22-P42"}]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-037","state":{"acted":true}}]);
    let mut game = batch_game(&initial);
    game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"enemy"}))
        .unwrap();
    trigger(&mut game);
    game.resume_internal_card().unwrap();
    let question = game.internal_quick_choice().unwrap().unwrap();
    game.answer_internal_card(question.actor, question.token, Answer::Quick(None))
        .unwrap();
    trigger(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["cemetery"], json!(["source"]));
    assert_eq!(state["P1"]["ex"], json!(["replacement"]));
    assert_eq!(state["P1"]["deck"], json!([]));
    assert_eq!(state["objects"]["replacement"]["card"], "BP22-P42");
    assert_eq!(state["P2"]["leader"]["life"], 18_u8);
}
