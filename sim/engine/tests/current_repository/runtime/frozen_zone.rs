#![expect(
    clippy::panic,
    reason = "Unexpected questions invalidate the printed card scenarios."
)]
use super::*;

fn finish(game: &mut Game, mut play: CardPlay) {
    while let CardPlay::Awaiting(question) = play {
        let InputDomain::Targets(domain) = &question.domain else {
            panic!("printed formal target")
        };
        let answer = Answer::Targets(vec![domain.members[0].handle]);
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    assert!(
        !matches!(play, CardPlay::Cancelled),
        "the printed play has a legal target and payment"
    );
}

fn position(card: &str, spells: usize, followers: usize, owner: &str) -> Value {
    let mut value = draw_position();
    value["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    value["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"top","card":"BP01-037"},{"id":"next","card":"BP01-037"},{"id":"last","card":"BP01-037"}]);
    value["players"][owner]["zones"]["cemetery"] = json!(
        (0..spells)
            .map(|i| json!({"id":format!("spell-{i}"),"card":"BP01-065"}))
            .chain((0..followers).map(|i| json!({"id":format!("follower-{i}"),"card":"BP01-037"})))
            .collect::<Vec<_>>()
    );
    value["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP04-117"}]);
    value
}

#[test]
fn bp01_065_draws_two_and_recovers_one_pp_only_at_own_spellchain_ten() {
    for (spells, followers, owner, allowed) in [
        (9_usize, 0_usize, "P1", false),
        (10, 0, "P1", true),
        (10, 0, "P2", false),
        (9, 1, "P1", false),
    ] {
        let mut game = game(&position("BP01-065", spells, followers, owner));
        let play = game
            .play_internal_card("source", PlayerId { seat: 1 })
            .unwrap();
        finish(&mut game, play);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P1"]["hand"],
            json!(["top", "next"]),
            "drawing two is unconditional"
        );
        assert_eq!(
            state["P1"]["pp"]["current"],
            if allowed { 8_u8 } else { 7_u8 },
            "only ten cemetery spells recover PP"
        );
    }
}

#[test]
fn bp01_071_and_alias_declare_the_target_before_replacing_damage_at_spellchain_ten() {
    for card in ["BP01-071", "SD03-012"] {
        for (spells, followers, owner, damage) in [
            (9_usize, 0_usize, "P1", 2_u8),
            (10, 0, "P1", 4),
            (9, 1, "P1", 2),
            (10, 0, "P2", 2),
        ] {
            let mut game = game(&position(card, spells, followers, owner));
            let play = game
                .play_internal_card("source", PlayerId { seat: 1 })
                .unwrap();
            assert!(
                matches!(play, CardPlay::Awaiting(_)),
                "the formal target is required below and above the threshold"
            );
            finish(&mut game, play);
            assert_eq!(
                game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
                damage,
                "the printed replacement reads only allied cemetery spells"
            );
        }
    }
}

#[test]
fn sd05_010_grants_storm_at_necrocharge_ten_including_nonspells() {
    for (spells, followers, owner, allowed) in [
        (9_usize, 0_usize, "P1", false),
        (9, 1, "P1", true),
        (0, 10, "P1", true),
        (0, 10, "P2", false),
    ] {
        let mut game = game(&position("SD05-010", spells, followers, owner));
        let entered = game
            .play_internal_card("source", PlayerId { seat: 1 })
            .unwrap();
        finish(&mut game, entered);
        let q = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = &q.domain else {
            panic!("printed fanfare")
        };
        assert_eq!(options.len(), 1, "one printed fanfare is pending");
        let triggered = game
            .answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
            .unwrap();
        finish(&mut game, triggered);
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["source"]["keywords"],
            if allowed {
                json!(["疾走"])
            } else {
                json!([])
            },
            "NC counts every allied cemetery card at resolution"
        );
    }
}
