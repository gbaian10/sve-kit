#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate these game scenarios."
)]
use super::*;

const fn actor() -> PlayerId {
    PlayerId { seat: 1 }
}

/// P2 has `deck` hidden cards and `grave` cards already in its cemetery.
fn position(card: &str, deck: usize, grave: usize) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    initial["players"]["P1"]["zones"]["deck"] =
        json!([{"id":"mine0","card":"BP01-171"},{"id":"mine1","card":"BP01-171"}]);
    initial["players"]["P2"]["zones"]["deck"] = json!(
        (0..deck)
            .map(|n| json!({"id":format!("top{n}"),"card":"BP01-171"}))
            .collect::<Vec<_>>()
    );
    initial["players"]["P2"]["zones"]["cemetery"] = json!(
        (0..grave)
            .map(|n| json!({"id":format!("old{n}"),"card":"BP01-171"}))
            .collect::<Vec<_>>()
    );
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-080","state":{"acted":true}}]);
    initial
}

fn finish(game: &mut Game, mut play: CardPlay) {
    while let CardPlay::Awaiting(question) = play {
        let answer = if matches!(&question.domain, InputDomain::WardEntry { .. }) {
            Answer::WardEntry(false)
        } else if let InputDomain::Targets(domain) = &question.domain {
            Answer::Targets(vec![domain.members[0].handle])
        } else {
            panic!("unexpected decision")
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
}

fn settle(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}

fn triggers(game: &mut Game) {
    settle(game);
    while let Some(question) = game.internal_pending_choice().unwrap() {
        let InputDomain::Pending(options) = &question.domain else {
            panic!("pending ability expected")
        };
        let step = game
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap();
        finish(game, step);
        settle(game);
    }
}

fn play(game: &mut Game, id: &str) {
    let step = game.play_internal_card(id, actor()).unwrap();
    finish(game, step);
    triggers(game);
}

fn attack(game: &mut Game, attacker: &str) {
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":attacker,"target":"P2.leader"}))
            .unwrap()
            .outcome,
        "resolved",
        "the attack is declared"
    );
    triggers(game);
}

fn ids(state: &Value, seat: &str, zone: &str) -> Vec<String> {
    state[seat][zone]
        .as_array()
        .unwrap()
        .iter()
        .map(|id| id.as_str().unwrap().to_owned())
        .collect()
}

/// P2's top `milled` cards are now in P2's cemetery and the rest of its deck keeps its order.
fn assert_milled(state: &Value, deck: usize, grave: usize, milled: usize) {
    let cemetery = ids(state, "P2", "cemetery");
    assert_eq!(
        cemetery.len(),
        grave.checked_add(milled).unwrap(),
        "only P2's cemetery grows"
    );
    assert!(
        (0..milled).all(|n| cemetery.contains(&format!("top{n}"))),
        "the former top cards reach their owner's cemetery"
    );
    assert_eq!(
        ids(state, "P2", "deck"),
        (milled..deck)
            .map(|n| format!("top{n}"))
            .collect::<Vec<_>>(),
        "the rest of P2's deck keeps its order"
    );
}

fn keyword(state: &Value, object: &str, keyword: &str) -> bool {
    state["objects"][object]["keywords"]
        .as_array()
        .unwrap()
        .contains(&json!(keyword))
}

#[test]
fn bp05_021_fanfare_mills_two_then_draws_at_ten_opponent_graves() {
    let mut game = game(&position("BP05-021", 3, 8));
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_milled(&state, 3, 8, 2);
    assert_eq!(ids(&state, "P1", "hand"), vec!["mine0".to_owned()]);
}

#[test]
fn bp05_025_storm_attack_mills_one_and_gains_two_power_at_ten() {
    let mut game = game(&position("BP05-025", 2, 9));
    play(&mut game, "source");
    attack(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_milled(&state, 2, 9, 1);
    assert_eq!(
        state["objects"]["source"]["power"], 5_u8,
        "printed 3 plus 2"
    );
}

#[test]
fn bp05_033_deals_three_then_two_more_to_the_same_target_at_ten() {
    let mut game = game(&position("BP05-033", 2, 9));
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_milled(&state, 2, 9, 1);
    assert_eq!(state["objects"]["enemy"]["damage"], 5_u8);
    assert_eq!(ids(&state, "P1", "cemetery"), vec!["source".to_owned()]);
}

#[test]
fn bp08_030_puts_own_top_card_into_ex_at_ten_opponent_graves() {
    let mut game = game(&position("BP08-030", 2, 9));
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_milled(&state, 2, 9, 1);
    assert_eq!(ids(&state, "P1", "ex"), vec!["mine0".to_owned()]);
    assert_eq!(ids(&state, "P1", "deck"), vec!["mine1".to_owned()]);
}

#[test]
fn bp15_035_makes_a_dagger_mills_one_and_gains_assail_at_ten() {
    let mut game = game(&position("BP15-035", 2, 9));
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_milled(&state, 2, 9, 1);
    let ex = ids(&state, "P1", "ex");
    assert_eq!(ex.len(), 1, "one token enters EX");
    assert_eq!(state["objects"][ex[0].as_str()]["name"], "黄金の小剣");
    assert!(keyword(&state, "source", "指定攻撃"));
}

#[test]
fn bp12_023_thief_attack_damages_the_chosen_follower_and_mills_its_owner() {
    let mut initial = position("BP12-023", 2, 0);
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP12-023"}]);
    let mut game = game(&initial);
    attack(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["damage"], 4_u8);
    assert_milled(&state, 2, 0, 1);
}
