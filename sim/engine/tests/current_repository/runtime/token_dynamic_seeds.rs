#![expect(
    clippy::panic,
    reason = "Unexpected questions invalidate these real-card scenarios."
)]
use super::*;

const fn actor() -> PlayerId {
    PlayerId { seat: 1 }
}

fn finish(game: &mut Game, mut play: CardPlay, mode: usize, expect_targets: bool) -> CardPlay {
    while let CardPlay::Awaiting(question) = play {
        let answer = if let InputDomain::Modes { modes, .. } = &question.domain {
            Answer::Modes(vec![modes[mode]])
        } else if let InputDomain::Targets(targets) = &question.domain {
            assert!(expect_targets, "an inactive Combo must not ask for targets");
            Answer::Targets(
                targets
                    .members
                    .iter()
                    .take(1)
                    .map(|member| member.handle)
                    .collect(),
            )
        } else if matches!(question.domain, InputDomain::WardEntry { .. }) {
            Answer::WardEntry(false)
        } else {
            panic!("unexpected printed question");
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    play
}

fn trigger(game: &mut Game, expect_targets: bool) {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("fanfare trigger");
    };
    let play = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options.first().unwrap().handle),
        )
        .unwrap();
    assert!(
        matches!(finish(game, play, 0, expect_targets), CardPlay::Resolved(_)),
        "the printed fanfare resolves"
    );
}

#[test]
fn bloodfang_and_yes_my_dark_count_every_opponent_field_card_and_allow_zero() {
    for number in ["BP02-081", "BP02-082"] {
        for populated in [false, true] {
            let mut initial = draw_position();
            initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":number}]);
            initial["players"]["P1"]["zones"]["field"] = json!([{"id":"own","card":"BP01-037"}]);
            if populated {
                initial["players"]["P2"]["zones"]["field"] = json!([
                    {"id":"enemy","card":"BP01-037"},{"id":"amulet","card":"BP07-T03"}]);
            }
            let mut game = game(&initial);
            let play = game.play_internal_card("source", actor()).unwrap();
            resolved(finish(&mut game, play, 0, false));
            let state = game.projection(View::Referee).unwrap();
            let field = state["P1"]["field"].as_array().unwrap();
            assert_eq!(field.len(), if populated { 3_usize } else { 1 });
            for id in field
                .iter()
                .filter_map(Value::as_str)
                .filter(|id| *id != "own")
            {
                assert_eq!(state["objects"][id]["card"], "BP01-T15");
                assert_eq!(state["objects"][id]["power"], 1_u8);
                assert_eq!(state["objects"][id]["hp"], 1_u8);
            }
            assert_eq!(
                state["P2"]["field"].as_array().unwrap().len(),
                if populated { 2_usize } else { 0 }
            );
        }
    }
}

#[test]
fn either_spell_can_choose_the_other_mode_and_buff_only_own_forest_bats() {
    for number in ["BP02-081", "BP02-082"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":number}]);
        initial["players"]["P1"]["zones"]["field"] = json!([
            {"id":"bat","card":"BP01-T15"},{"id":"other","card":"BP01-037"}]);
        initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemybat","card":"BP01-T15"}]);
        let mut game = game(&initial);
        let play = game.play_internal_card("source", actor()).unwrap();
        resolved(finish(&mut game, play, 1, false));
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["field"].as_array().unwrap().len(), 2);
        assert_eq!(state["objects"]["bat"]["power"], 2_u8);
        assert_eq!(state["objects"]["bat"]["hp"], 2_u8);
        assert_eq!(state["objects"]["other"]["power"], 5_u8);
        assert_eq!(state["objects"]["enemybat"]["power"], 1_u8);
    }
}

#[test]
fn noble_fairy_destroys_then_creates_for_opponent_only_with_combo_three() {
    for prior in [1_usize, 2] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP01-014"}]);
        initial["players"]["P1"]["zones"]["ex"] = json!([
            {"id":"first","card":"BP07-T02"},{"id":"second","card":"BP07-T02"}]);
        initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
        let mut game = game(&initial);
        for repair in ["first", "second"].iter().take(prior) {
            resolved(game.play_internal_card(repair, actor()).unwrap());
            game.resume_internal_card().unwrap();
        }
        let play = game.play_internal_card("source", actor()).unwrap();
        assert!(matches!(
            finish(&mut game, play, 0, false),
            CardPlay::Entered(_)
        ));
        trigger(&mut game, prior == 2);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["field"], json!(["source"]));
        let enemies = state["P2"]["field"].as_array().unwrap();
        assert_eq!(enemies.len(), 1);
        if prior == 2 {
            assert_eq!(state["P2"]["cemetery"], json!(["enemy"]));
            let fairy = enemies.first().unwrap().as_str().unwrap();
            assert_ne!(fairy, "enemy");
            assert_eq!(state["objects"][fairy]["card"], "BP01-T03");
            assert_eq!(state["objects"][fairy]["power"], 1_u8);
        } else {
            assert_eq!(enemies.as_slice(), &[json!("enemy")]);
            assert_eq!(state["P2"]["cemetery"], json!([]));
        }
    }
}

#[test]
fn dynamic_creation_fills_only_available_field_slots_even_against_a_full_opponent_field() {
    for number in ["BP02-081", "BP02-082"] {
        for own_count in [4_usize, 5] {
            let mut initial = draw_position();
            initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":number}]);
            initial["players"]["P1"]["zones"]["field"] = Value::Array(
                (0..own_count)
                    .map(|index| json!({"id":format!("own{index}"),"card":"BP01-037"}))
                    .collect(),
            );
            initial["players"]["P2"]["zones"]["field"] = Value::Array(
                (0..5_usize)
                    .map(|index| json!({"id":format!("enemy{index}"),"card":"BP01-037"}))
                    .collect(),
            );
            let mut game = game(&initial);
            let play = game.play_internal_card("source", actor()).unwrap();
            resolved(finish(&mut game, play, 0, false));
            let state = game.projection(View::Referee).unwrap();
            let field = state["P1"]["field"].as_array().unwrap();
            assert_eq!(field.len(), 5);
            assert_eq!(
                field
                    .iter()
                    .filter(|id| state["objects"][id.as_str().unwrap()]["card"] == "BP01-T15")
                    .count(),
                5_usize - own_count
            );
            assert_eq!(state["P2"]["field"].as_array().unwrap().len(), 5);
            assert_eq!(state["P1"]["cemetery"], json!(["source"]));
        }
    }
}
