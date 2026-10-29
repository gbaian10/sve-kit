#![expect(
    clippy::panic,
    reason = "Unexpected choices invalidate these printed card scenarios."
)]
use super::*;

fn position(card: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-037","state":{"acted":true}}]);
    initial
}

fn finish(game: &mut Game, mut play: CardPlay, target_seat: u32) {
    while let CardPlay::Awaiting(question) = play {
        let answer = if matches!(&question.domain, InputDomain::WardEntry { .. }) {
            Answer::WardEntry(false)
        } else {
            let InputDomain::Targets(domain) = &question.domain else {
                panic!("formal target");
            };
            let chosen = domain
                .members
                .iter()
                .find(|member| member.player.seat == target_seat)
                .unwrap();
            Answer::Targets(vec![chosen.handle])
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
}

fn play(game: &mut Game, id: &str) {
    let entered = game.play_internal_card(id, PlayerId { seat: 1 }).unwrap();
    finish(game, entered, 2);
}

fn trigger(game: &mut Game, target_seat: u32) {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("printed Fanfare or End trigger");
    };
    assert_eq!(options.len(), 1, "only the intended source is pending");
    let fired = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap();
    finish(game, fired, target_seat);
}

fn end_quick(game: &mut Game) {
    let question = game.internal_quick_choice().unwrap().unwrap();
    game.answer_internal_card(question.actor, question.token, Answer::Quick(None))
        .unwrap();
}

fn storm_attacks_on_entry(card: &str, power: u8) {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"source","card":card,"state":{"entered_this_turn":true}}]);
    let mut game = game(&initial);
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"P2.leader"}))
            .unwrap()
            .outcome,
        "resolved",
        "printed Storm permits a leader attack on the entry turn"
    );
    end_quick(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
        20_u8.checked_sub(power).unwrap(),
        "the attack deals the printed power to the leader"
    );
}

fn rush_attacks_only_followers_on_entry(card: &str) {
    let mut initial = position(card);
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"source","card":card,"state":{"entered_this_turn":true}}]);
    let mut game = game(&initial);
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"P2.leader"}))
            .unwrap()
            .outcome,
        "cannot-attack",
        "Rush does not permit an entry-turn leader attack"
    );
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"enemy"}))
            .unwrap()
            .outcome,
        "resolved",
        "Rush permits an entry-turn follower attack"
    );
}

fn ward_protects_the_leader(card: &str) {
    let mut initial = draw_position();
    initial["turn"]["active"] = json!("P2");
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"source","card":card,"state":{"acted":true}}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"attacker","card":"BP01-037"}]);
    let mut game = game(&initial);
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"attack","attacker":"attacker","target":"P1.leader"})
        )
        .unwrap()
        .outcome,
        "cannot-attack",
        "rested Ward protects its leader"
    );
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"attacker","target":"source"}))
            .unwrap()
            .outcome,
        "resolved",
        "the attacker may attack the Ward follower instead"
    );
}

#[test]
fn bp01_004_counts_prior_plays_without_counting_itself() {
    for prior in [0_usize, 1, 2] {
        let mut initial = position("BP01-004");
        for n in 0..prior {
            initial["players"]["P1"]["zones"]["hand"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":format!("prior{n}"),"card":"BP01-171"}));
        }
        let mut game = game(&initial);
        for n in 0..prior {
            play(&mut game, &format!("prior{n}"));
            game.resume_internal_card().unwrap();
        }
        play(&mut game, "source");
        trigger(&mut game, 2);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["source"]["power"], 1_usize + prior);
        assert_eq!(state["objects"]["source"]["hp"], 1_u8);
    }
}

#[test]
fn bp01_095_captures_another_followers_power() {
    let mut initial = position("BP01-095");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"ally","card":"BP01-171"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    trigger(&mut game, 2);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["source"]["power"], 5_u8);
    assert_eq!(state["objects"]["source"]["hp"], 2_u8);
    assert_eq!(state["objects"]["ally"]["power"], 2_u8);
    assert_eq!(state["objects"]["enemy"]["power"], 5_u8);
}

fn crimson_position(card: &str, paid: bool) -> Game {
    let mut initial = position(card);
    if paid {
        initial["players"]["P1"]["zones"]["hand"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"payer","card":"BP01-118"}));
    }
    let mut game = game(&initial);
    if paid {
        play(&mut game, "payer");
        trigger(&mut game, 2);
        game.resume_internal_card().unwrap();
    }
    play(&mut game, "source");
    trigger(&mut game, 2);
    game.resume_internal_card().unwrap();
    game
}

#[test]
fn bp01_112_crimson_buffs_and_grants_rush() {
    for paid in [false, true] {
        let mut game = crimson_position("BP01-112", paid);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["objects"]["source"]["power"],
            if paid { 4_u8 } else { 3_u8 }
        );
        assert_eq!(
            state["objects"]["source"]["hp"],
            if paid { 3_u8 } else { 2_u8 }
        );
        assert_eq!(
            game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"enemy"}))
                .unwrap()
                .outcome,
            if paid { "resolved" } else { "cannot-attack" }
        );
    }
}

#[test]
fn bp01_113_crimson_grants_storm() {
    for paid in [false, true] {
        let mut game = crimson_position("BP01-113", paid);
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"attack","attacker":"source","target":"P2.leader"})
            )
            .unwrap()
            .outcome,
            if paid { "resolved" } else { "cannot-attack" }
        );
        if paid {
            end_quick(&mut game);
            assert_eq!(
                game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
                14_u8
            );
        }
    }
}

#[test]
fn bp01_151_buffs_another_ally_and_grants_assail() {
    let mut initial = position("BP01-151");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"ally","card":"BP01-171"}]);
    initial["players"]["P2"]["zones"]["field"][0]["state"]["acted"] = json!(false);
    let mut game = game(&initial);
    play(&mut game, "source");
    trigger(&mut game, 1);
    game.resume_internal_card().unwrap();
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["ally"]["power"], 6_u8);
    assert_eq!(state["objects"]["ally"]["hp"], 5_u8);
    assert_eq!(state["objects"]["source"]["power"], 4_u8);
    assert_eq!(state["objects"]["source"]["hp"], 3_u8);
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"ally","target":"enemy"}))
            .unwrap()
            .outcome,
        "resolved"
    );
}

#[test]
fn bp01_025_buffs_a_fairy_then_pays_to_return_itself() {
    let mut initial = position("BP01-025");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"fairy","card":"BP01-T03"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    trigger(&mut game, 1);
    game.resume_internal_card().unwrap();
    let ability = selection().current["BP01-025"]
        .program
        .card()
        .abilities
        .iter()
        .find(|entry| entry.node.kind == AbilityKind::Activated)
        .unwrap()
        .node
        .id;
    let activated = game
        .activate_internal_card("source", ability, PlayerId { seat: 1 })
        .unwrap();
    finish(&mut game, activated, 1);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["fairy"]["power"], 2_u8);
    assert_eq!(state["objects"]["fairy"]["hp"], 1_u8);
    assert_eq!(state["P1"]["hand"], json!(["source"]));
    assert_eq!(state["P1"]["field"], json!(["fairy"]));
    assert_eq!(state["P1"]["pp"]["current"], 8_u8);
}

#[test]
fn bp01_048_storm_attacks_on_entry() {
    storm_attacks_on_entry("BP01-048", 3);
}
#[test]
fn bp01_158_storm_attacks_on_entry() {
    storm_attacks_on_entry("BP01-158", 6);
}
#[test]
fn bp01_t16_storm_attacks_on_entry() {
    storm_attacks_on_entry("BP01-T16", 2);
}
#[test]
fn bp01_t08_rush_attacks_followers_on_entry() {
    rush_attacks_only_followers_on_entry("BP01-T08");
}
#[test]
fn bp01_t17_rush_attacks_followers_on_entry() {
    rush_attacks_only_followers_on_entry("BP01-T17");
}
#[test]
fn bp01_146_ward_protects_the_leader() {
    ward_protects_the_leader("BP01-146");
}
#[test]
fn bp01_t09_ward_protects_the_leader() {
    ward_protects_the_leader("BP01-T09");
}

#[test]
fn bp01_096_intimidate_blocks_attacks_but_leaves_leader_exposed() {
    for strong in [false, true] {
        let mut initial = draw_position();
        initial["turn"]["active"] = json!("P2");
        initial["players"]["P1"]["zones"]["field"] =
            json!([{"id":"source","card":"BP01-096","state":{"acted":true}}]);
        initial["players"]["P2"]["zones"]["field"] =
            json!([{"id":"attacker","card":if strong { "BP01-037" } else { "BP01-118" }}]);
        let mut game = game(&initial);
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"attack","attacker":"attacker","target":"source"})
            )
            .unwrap()
            .outcome,
            "cannot-attack"
        );
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"attack","attacker":"attacker","target":"P1.leader"})
            )
            .unwrap()
            .outcome,
            "resolved"
        );
    }
}

#[test]
fn bp01_163_assail_attacks_standing_enemy() {
    let mut initial = position("BP01-163");
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP01-163"}]);
    initial["players"]["P2"]["zones"]["field"][0]["state"]["acted"] = json!(false);
    let mut game = game(&initial);
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"enemy"}))
            .unwrap()
            .outcome,
        "resolved"
    );
}
