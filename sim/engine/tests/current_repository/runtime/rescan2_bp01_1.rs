#![expect(
    clippy::panic,
    reason = "Unexpected choices fail the printed card scenarios."
)]
use super::*;

fn position(card: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"drawn","card":"BP01-037"}, {"id":"next","card":"BP01-037"}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    initial
}

fn finish(game: &mut Game, mut play: CardPlay, leader: bool) {
    while let CardPlay::Awaiting(question) = play {
        let answer = if matches!(&question.domain, InputDomain::WardEntry { .. }) {
            Answer::WardEntry(false)
        } else {
            let InputDomain::Targets(domain) = &question.domain else {
                panic!("expected a damage target");
            };
            let member = domain
                .members
                .iter()
                .find(|member| member.card.is_none() == leader)
                .unwrap();
            Answer::Targets(vec![member.handle])
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
}

fn play(game: &mut Game, leader: bool) {
    let entered = game
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap();
    finish(game, entered, leader);
}

fn trigger(game: &mut Game) {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("pending trigger");
    };
    assert_eq!(options.len(), 1, "only this card's Fanfare is pending");
    let fired = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap();
    finish(game, fired, false);
}

#[test]
fn bp01_017_combo_replaces_one_damage_with_three() {
    for prior in [0_usize, 2] {
        let mut initial = position("BP01-017");
        for n in 0..prior {
            initial["players"]["P1"]["zones"]["hand"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":format!("prior{n}"),"card":"BP01-171"}));
        }
        let mut game = game(&initial);
        for n in 0..prior {
            let entered = game
                .play_internal_card(&format!("prior{n}"), PlayerId { seat: 1 })
                .unwrap();
            finish(&mut game, entered, false);
            game.resume_internal_card().unwrap();
        }
        play(&mut game, false);
        trigger(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
            if prior == 2 { 3_u8 } else { 1_u8 }
        );
    }
}

#[test]
fn bp01_098_awakening_checks_maximum_pp() {
    for maximum in [6_u8, 7] {
        let mut initial = position("BP01-098");
        initial["players"]["P1"]["pp"] = json!({"current":1_u8,"max":maximum});
        let mut game = game(&initial);
        play(&mut game, false);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["objects"]["enemy"]["damage"],
            if maximum == 7 { 4_u8 } else { 2_u8 }
        );
        assert_eq!(state["P1"]["pp"]["current"], 0_u8);
    }
}

#[test]
fn bp01_007_draws_before_counting_hand_for_damage() {
    for leader in [false, true] {
        let mut initial = position("BP01-007");
        initial["players"]["P1"]["zones"]["hand"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"kept","card":"BP01-037"}));
        let mut game = game(&initial);
        play(&mut game, leader);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["hand"], json!(["kept", "drawn"]));
        assert_eq!(
            state["objects"]["enemy"]["damage"],
            if leader { 0_u8 } else { 2_u8 }
        );
        assert_eq!(
            state["P2"]["leader"]["life"],
            if leader { 18_u8 } else { 20_u8 }
        );
    }
}

#[test]
fn bp01_t01_spell_token_damages_draws_and_disappears() {
    for leader in [false, true] {
        let mut initial = position("BP01-T01");
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"source","card":"BP01-T01"}]);
        let mut game = game(&initial);
        play(&mut game, leader);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["hand"], json!(["drawn"]));
        assert_eq!(state["P1"]["ex"], json!([]));
        assert_eq!(state["P1"]["cemetery"], json!([]));
        assert_eq!(
            state["objects"]["enemy"]["damage"],
            if leader { 0_u8 } else { 3_u8 }
        );
        assert_eq!(
            state["P2"]["leader"]["life"],
            if leader { 17_u8 } else { 20_u8 }
        );
    }
}

#[test]
fn bp01_164_fanfare_hits_only_other_friendly_followers() {
    let mut initial = position("BP01-164");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"ally","card":"BP01-037"}]);
    let mut game = game(&initial);
    play(&mut game, false);
    trigger(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["ally"]["damage"], 2_u8);
    assert_eq!(state["objects"]["source"]["damage"], 0_u8);
    assert_eq!(state["objects"]["enemy"]["damage"], 0_u8);
}

#[test]
fn bp01_089_storm_hits_both_fields_without_hitting_leaders() {
    let mut initial = position("BP01-089");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"ally","card":"BP01-037"}]);
    let mut game = game(&initial);
    play(&mut game, false);
    game.resume_internal_card().unwrap();
    let state = game.projection(View::Referee).unwrap();
    let cemetery = state["P1"]["cemetery"].as_array().unwrap();
    assert_eq!(cemetery.len(), 2);
    assert!(cemetery.contains(&json!("ally")));
    assert!(cemetery.contains(&json!("source")));
    assert_eq!(state["P2"]["cemetery"], json!(["enemy"]));
    assert_eq!(state["P1"]["leader"]["life"], 20_u8);
    assert_eq!(state["P2"]["leader"]["life"], 20_u8);
}

#[test]
fn bp01_125_damages_self_then_chosen_enemy() {
    for leader in [false, true] {
        let mut game = game(&position("BP01-125"));
        play(&mut game, leader);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["leader"]["life"], 18_u8);
        assert_eq!(
            state["P2"]["leader"]["life"],
            if leader { 17_u8 } else { 20_u8 }
        );
        assert_eq!(
            state["objects"]["enemy"]["damage"],
            if leader { 0_u8 } else { 3_u8 }
        );
    }
}

#[test]
fn bp01_143_heals_only_when_an_amulet_is_present() {
    for amulet in [false, true] {
        let mut initial = position("BP01-143");
        initial["players"]["P1"]["zones"]["field"] = if amulet {
            json!([{"id":"amulet","card":"BP19-108"}])
        } else {
            json!([{"id":"ally","card":"BP01-037"}])
        };
        let mut game = game(&initial);
        play(&mut game, false);
        trigger(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
            if amulet { 22_u8 } else { 20_u8 }
        );
    }
}
