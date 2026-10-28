#![expect(
    clippy::panic,
    reason = "Unexpected decisions indicate a broken game fixture."
)]
use super::*;

const fn actor() -> PlayerId {
    PlayerId { seat: 1 }
}

fn position(card: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    for seat in ["P1", "P2"] {
        initial["players"][seat]["zones"]["deck"] = json!([
            {"id":format!("{seat}-first"),"card":"BP01-037"},
            {"id":format!("{seat}-second"),"card":"BP01-037"},
            {"id":format!("{seat}-third"),"card":"BP01-037"}
        ]);
    }
    initial
}

fn field_position(card: &str) -> Value {
    let mut initial = position(card);
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":card}]);
    initial
}

fn finish(game: &mut Game, mut play: CardPlay, accept_optional: bool) {
    while let CardPlay::Awaiting(question) = play {
        let answer = if matches!(&question.domain, InputDomain::OptionalCost) {
            Answer::OptionalCost(accept_optional)
        } else if matches!(&question.domain, InputDomain::WardEntry { .. }) {
            Answer::WardEntry(false)
        } else if matches!(&question.domain, InputDomain::Modes { .. }) {
            Answer::Modes(vec![ModeId(1)])
        } else {
            let InputDomain::Targets(domain) = &question.domain else {
                panic!("expected target decision");
            };
            Answer::Targets(
                domain
                    .members
                    .iter()
                    .take(1)
                    .map(|member| member.handle)
                    .collect(),
            )
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
}

fn play(game: &mut Game) {
    let result = game.play_internal_card("source", actor()).unwrap();
    finish(game, result, true);
}

fn trigger(game: &mut Game) {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("expected pending trigger");
    };
    assert_eq!(options.len(), 1, "one card trigger is pending");
    let result = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap();
    finish(game, result, true);
}

fn pass_quick(game: &mut Game) {
    let question = game.internal_quick_choice().unwrap().unwrap();
    let result = game
        .answer_internal_card(question.actor, question.token, Answer::Quick(None))
        .unwrap();
    finish(game, result, true);
}

fn ward(card: &str) {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"attacker","card":"BP01-037"}]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"ward","card":card,"state":{"acted":true}}]);
    let mut game = game(&initial);
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"attack","attacker":"attacker","target":"P2.leader"})
        )
        .unwrap()
        .outcome,
        "cannot-attack",
        "rested ward protects the leader"
    );
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"attacker","target":"ward"}))
            .unwrap()
            .outcome,
        "resolved",
        "the ward remains attackable"
    );
}

fn immediate_attack(card: &str, keyword: &str, power: u8) {
    let mut initial = field_position(card);
    initial["players"]["P1"]["zones"]["field"][0]["state"] = json!({"entered_this_turn":true});
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-037","state":{"acted":true}}]);
    let mut game = game(&initial);
    let target = if keyword == "storm" {
        "P2.leader"
    } else {
        "enemy"
    };
    if keyword == "rush" {
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"attack","attacker":"source","target":"P2.leader"})
            )
            .unwrap()
            .outcome,
            "cannot-attack",
            "rush does not allow an immediate leader attack"
        );
    }
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":target}))
            .unwrap()
            .outcome,
        "resolved",
        "the printed keyword permits the immediate attack"
    );
    pass_quick(&mut game);
    if keyword == "storm" {
        assert_eq!(
            game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
            20_u8.checked_sub(power).unwrap(),
            "storm damages the opposing leader immediately"
        );
    } else if power >= 5 {
        assert_eq!(
            game.projection(View::Referee).unwrap()["P2"]["cemetery"],
            json!(["enemy"]),
            "the opposing follower is destroyed"
        );
    } else {
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
            power,
            "rush deals combat damage to its defender"
        );
    }
}

fn departed(card: &str) -> Game {
    let mut game = with_producer(
        &field_position(card),
        Action::Destroy {
            objects: subjects(),
        },
    );
    activate(&mut game, "P1");
    game.resume_internal_card().unwrap();
    trigger(&mut game);
    game
}

fn own_buff_target(game: &mut Game) {
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("expected a pending buff trigger");
    };
    assert_eq!(options.len(), 1, "only the own follower buff trigger is pending");
    let declaration = game
        .answer_internal_card(
            pending.actor,
            pending.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap();
    let CardPlay::Awaiting(choice) = declaration else {
        panic!("expected an own follower target");
    };
    let InputDomain::Targets(domain) = &choice.domain else {
        panic!("expected target domain");
    };
    assert_eq!(
        domain.members.len(),
        2,
        "both the source and the other own follower are legal"
    );
    assert!(
        domain.members.iter().all(|member| member.player == actor()),
        "opposing followers are excluded"
    );
    let target = domain
        .members
        .iter()
        .find(|member| member.card.as_deref() == Some("BP01-037"))
        .unwrap()
        .handle;
    let result = game
        .answer_internal_card(choice.actor, choice.token, Answer::Targets(vec![target]))
        .unwrap();
    finish(game, result, true);
    game.resume_internal_card().unwrap();
}

#[test]
fn bp15_111_fanfare_damages_and_lastword_heals() {
    for print in ["BP15-111", "EBD04-020"] {
        let mut initial = position(print);
        initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
        let mut game = game(&initial);
        play(&mut game);
        trigger(&mut game);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["enemy"]["damage"], 3_u8);
        assert_eq!(state["P1"]["leader"]["life"], 20_u8);
        let death = departed(print);
        let after = death.projection(View::Referee).unwrap();
        assert_eq!(after["P1"]["leader"]["life"], 21_u8);
        assert_eq!(after["P2"]["leader"]["life"], 20_u8);
    }
}

#[test]
fn bp15_125_rush_ward_and_fanfare_deal_printed_damage() {
    ward("BP15-125");
    immediate_attack("BP15-125", "rush", 7);
    let mut initial = position("BP15-125");
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    let mut game = game(&initial);
    play(&mut game);
    trigger(&mut game);
    game.resume_internal_card().unwrap();
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["cemetery"],
        json!(["enemy"])
    );
}

#[test]
fn bp15_018_fanfare_draws_one() {
    let mut game = game(&position("BP15-018"));
    play(&mut game);
    trigger(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["P1-first"]));
    assert_eq!(state["P1"]["deck"], json!(["P1-second", "P1-third"]));
}

#[test]
fn bp15_107_ward_protects_its_leader() {
    ward("BP15-107");
}

#[test]
fn bp15_032_optional_fanfare_buffs_only_after_paying_three() {
    ward("BP15-032");
    for accepted in [false, true] {
        let mut game = game(&position("BP15-032"));
        play(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
            9_u8
        );
        let pending = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = &pending.domain else {
            panic!("expected fanfare trigger");
        };
        assert_eq!(options.len(), 1, "only the own follower buff trigger is pending");
        let declaration = game
            .answer_internal_card(
                pending.actor,
                pending.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap();
        let CardPlay::Awaiting(payment) = declaration else {
            panic!("expected optional PP choice");
        };
        assert_eq!(payment.domain, InputDomain::OptionalCost);
        let result = game
            .answer_internal_card(payment.actor, payment.token, Answer::OptionalCost(accepted))
            .unwrap();
        finish(&mut game, result, accepted);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P1"]["pp"]["current"],
            if accepted { 6_u8 } else { 9_u8 }
        );
        assert_eq!(
            state["objects"]["source"]["power"],
            if accepted { 3_u8 } else { 1_u8 }
        );
        assert_eq!(
            state["objects"]["source"]["hp"],
            if accepted { 5_u8 } else { 3_u8 }
        );
    }
}

#[test]
fn bp15_115_ward_and_fanfare_buff_another_own_follower() {
    for print in ["BP15-115", "BP15-P25"] {
        ward(print);
        let mut initial = position(print);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"ally","card":"BP01-037"}]);
        initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
        let mut game = game(&initial);
        play(&mut game);
        own_buff_target(&mut game);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["ally"]["power"], 6_u8);
        assert_eq!(state["objects"]["ally"]["hp"], 6_u8);
        assert_eq!(state["objects"]["enemy"]["power"], 5_u8);
        assert_eq!(state["objects"]["source"]["power"], 0_u8);
    }
}

#[test]
fn bp15_116_real_evolution_buffs_only_own_follower() {
    for (base, form) in [("BP15-115", "BP15-116"), ("BP15-P25", "BP15-P26")] {
        ward(form);
        let mut initial = field_position(base);
        initial["players"]["P1"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"ally","card":"BP01-037"}));
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
        initial["players"]["P1"]["pp"]["current"] = json!(1_u8);
        initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
        let mut game = game(&initial);
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
            )
            .unwrap()
            .outcome,
            "resolved"
        );
        own_buff_target(&mut game);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["pp"]["current"], 0_u8);
        assert_eq!(state["objects"]["ally"]["power"], 6_u8);
        assert_eq!(state["objects"]["ally"]["hp"], 6_u8);
        assert_eq!(state["objects"]["source"]["power"], 1_u8);
        assert_eq!(state["objects"]["enemy"]["power"], 5_u8);
    }
}
