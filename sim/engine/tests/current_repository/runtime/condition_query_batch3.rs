#![expect(
    clippy::panic,
    reason = "Unexpected questions invalidate these printed card scenarios."
)]
use super::*;
fn position(number: &str, zone: &str) -> Value {
    let mut state = draw_position();
    state["players"]["P1"]["zones"][zone] = json!([{"id":"source","card":number}]);
    state["players"]["P1"]["zones"]["deck"] =
        json!([{"id":"draw","card":"BP01-037"},{"id":"tail","card":"BP01-037"}]);
    state["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    state
}
fn fixture(state: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        state,
        &Value::Null,
        &Value::Null,
        "condition-batch3",
    )
    .unwrap()
}
fn finish(game: &mut Game, mut play: CardPlay, targets: &[&str]) -> CardPlay {
    let mut next = targets.iter();
    while let CardPlay::Awaiting(q) = &play {
        let answer = match &q.domain {
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            InputDomain::OptionalCost => Answer::OptionalCost(true),
            InputDomain::CostMaterials(d) => Answer::CostMaterials(
                d.members
                    .iter()
                    .take(usize::try_from(d.cardinality.minimum.0).unwrap())
                    .map(|m| m.handle)
                    .collect(),
            ),
            InputDomain::Targets(d) if d.members.is_empty() && d.cardinality.minimum == Nat(0) => {
                Answer::Targets(vec![])
            }
            InputDomain::Targets(d) => {
                let Some(wanted) = next.next() else {
                    panic!("each target has an independent printed expectation")
                };
                let handle = d
                    .members
                    .iter()
                    .find(|m| m.card.as_deref() == Some(*wanted))
                    .unwrap()
                    .handle;
                Answer::Targets(vec![handle])
            }
            InputDomain::Order(cards) => {
                Answer::Order(cards.iter().rev().map(|c| c.handle).collect())
            }
            InputDomain::Modes { modes, .. } => Answer::Modes(vec![modes[0]]),
            InputDomain::ResourceEntryPayment { .. }
            | InputDomain::Quick { .. }
            | InputDomain::BeforeDraw
            | InputDomain::Pending(_)
            | InputDomain::TriggerQuota(_)
            | InputDomain::Integer(_)
            | InputDomain::DamageReplacements(_)
            | InputDomain::DriveTrigger { .. }
            | InputDomain::Search(_)
            | InputDomain::SearchPartition(_)
            | InputDomain::Distribution(_)
            | InputDomain::SearchReplacement
            | InputDomain::TokenCapacity { .. } => panic!("unexpected printed question"),
        };
        play = game.answer_internal_card(q.actor, q.token, answer).unwrap();
    }
    assert!(
        next.next().is_none(),
        "every expected target question was asked"
    );
    play
}
fn trigger(game: &mut Game, number: &str, targets: &[&str]) -> CardPlay {
    let q = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &q.domain else {
        panic!("printed trigger")
    };
    let handle = options.iter().find(|o| o.card == number).unwrap().handle;
    let play = game
        .answer_internal_card(q.actor, q.token, Answer::Pending(handle))
        .unwrap();
    finish(game, play, targets)
}
fn enter(game: &mut Game, number: &str, targets: &[&str]) -> CardPlay {
    let play = game
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap();
    assert!(
        matches!(finish(game, play, &[]), CardPlay::Entered(_)),
        "real source enters before fanfare"
    );
    trigger(game, number, targets)
}

#[test]
fn bp22_093_counts_its_own_printed_ward_and_two_other_guardians() {
    for guards in [1_usize, 2] {
        let mut s = position("BP22-093", "hand");
        s["players"]["P1"]["zones"]["field"] = json!(
            (0..guards)
                .map(|i| json!({"id":format!("ward-{i}"),"card":"BP01-086"}))
                .collect::<Vec<_>>()
        );
        let mut g = fixture(&s);
        resolved(enter(&mut g, "BP22-093", &["BP01-037"]));
        let v = g.projection(View::Referee).unwrap();
        assert_eq!(
            v["objects"]["enemy"]["damage"],
            if guards == 2 { 2_u8 } else { 0 }
        );
        assert_eq!(v["P1"]["pp"]["current"], 8_u8);
    }
}
#[test]
fn bp22_p68_can_grant_storm_to_two_guardians_but_never_a_plain_follower() {
    let mut initial = position("BP22-P68", "ex");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"ward1","card":"BP01-086"},{"id":"BP01-037","card":"BP01-037"},{"id":"ward2","card":"BP01-086"}]);
    let mut game = fixture(&initial);
    let play = game
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap();
    let CardPlay::Awaiting(choice) = play else {
        panic!("formal ward choices")
    };
    let InputDomain::Targets(domain) = &choice.domain else {
        panic!("ward followers")
    };
    assert_eq!(domain.members.len(), 2);
    assert_eq!(domain.cardinality.maximum, Some(Nat(2)));
    assert!(
        domain
            .members
            .iter()
            .all(|m| m.card.as_deref() == Some("BP01-086"))
    );
    resolved(
        game.answer_internal_card(
            choice.actor,
            choice.token,
            Answer::Targets(domain.members.iter().map(|m| m.handle).collect()),
        )
        .unwrap(),
    );
    let view = game.projection(View::Referee).unwrap();
    for id in ["ward1", "ward2"] {
        assert!(
            view["objects"][id]["keywords"]
                .as_array()
                .unwrap()
                .contains(&json!("疾走"))
        );
    }
    assert!(
        !view["objects"]["BP01-037"]["keywords"]
            .as_array()
            .unwrap()
            .contains(&json!("疾走"))
    );
}
#[test]
fn bp22_094_real_super_evolution_deals_two_and_creates_its_real_command() {
    let mut s = position("BP22-093", "field");
    s["turn"]["elapsed_turns"] = json!({"P1":7_u8,"P2":6_u8});
    s["players"]["P1"]["sep"] = json!(1_u8);
    s["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP22-094"}]);
    let mut g = fixture(&s);
    assert_eq!(g.internal_rule_action(&json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8,"sep":1_u8}})).unwrap().outcome,"resolved");
    for line in [2_u64, 3] {
        let q = g.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = &q.domain else {
            panic!("evolution heads")
        };
        let handle = options.iter().find(|o| o.line == line).unwrap().handle;
        let p = g
            .answer_internal_card(q.actor, q.token, Answer::Pending(handle))
            .unwrap();
        resolved(finish(
            &mut g,
            p,
            if line == 2 { &["BP01-037"] } else { &[] },
        ));
        g.resume_internal_card().unwrap();
    }
    let v = g.projection(View::Referee).unwrap();
    assert_eq!(v["objects"]["enemy"]["damage"], 2_u8);
    assert_eq!(v["P1"]["sep"], 0_u8);
    let token = v["P1"]["ex"][0].as_str().unwrap();
    assert_eq!(v["objects"][token]["card"], "BP22-P68");
}
#[test]
fn bp15_p31_replaces_rush_with_storm_at_ten_opponent_cemetery_cards() {
    for count in [9_usize, 10] {
        let mut s = position("BP15-P31", "ex");
        s["players"]["P1"]["zones"]["field"] = json!([{"id":"ally","card":"BP11-019"}]);
        s["players"]["P2"]["zones"]["cemetery"] = json!(
            (0..count)
                .map(|i| json!({"id":format!("grave-{i}"),"card":"BP01-037"}))
                .collect::<Vec<_>>()
        );
        let mut g = fixture(&s);
        let p = g
            .play_internal_card("source", PlayerId { seat: 1 })
            .unwrap();
        resolved(finish(&mut g, p, &["BP11-019"]));
        let v = g.projection(View::Referee).unwrap();
        let ks = v["objects"]["ally"]["keywords"].as_array().unwrap();
        assert!(ks.contains(&json!(if count == 10 { "疾走" } else { "突進" })));
        assert!(!ks.contains(&json!(if count == 10 { "突進" } else { "疾走" })));
    }
}
#[test]
fn bp20_086_observes_nightmare_omen_cards_and_then_sacrifices_for_rush() {
    let mut s = position("BP20-086", "hand");
    s["players"]["P1"]["zones"]["field"] = json!([{"id":"ally","card":"BP01-037"}]);
    s["players"]["P1"]["zones"]["deck"] = json!([{"id":"omen","card":"BP15-076"},{"id":"other","card":"BP01-109"},{"id":"plain-card","card":"BP15-014"},{"id":"tail","card":"BP01-037"}]);
    let mut g = fixture(&s);
    let entry_play = g
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap();
    assert!(matches!(
        finish(&mut g, entry_play, &[]),
        CardPlay::Entered(_)
    ));
    let pending_choice = g.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(pending_options) = &pending_choice.domain else {
        panic!("the real amulet fanfare")
    };
    let handle = pending_options
        .iter()
        .find(|option| option.card == "BP20-086")
        .unwrap()
        .handle;
    let CardPlay::Awaiting(look_choice) = g
        .answer_internal_card(
            pending_choice.actor,
            pending_choice.token,
            Answer::Pending(handle),
        )
        .unwrap()
    else {
        panic!("the private Omen observation")
    };
    let InputDomain::Targets(look_targets) = &look_choice.domain else {
        panic!("private observed cards")
    };
    assert_eq!(
        look_targets
            .members
            .iter()
            .map(|member| member.card.as_deref())
            .collect::<Vec<_>>(),
        vec![Some("BP15-076")],
        "exclude a non-Omen Nightmare card and a non-Nightmare Omen card independently"
    );
    assert_eq!(look_targets.cardinality.minimum, Nat(0));
    assert_eq!(look_targets.cardinality.maximum, Some(Nat(1)));
    assert!(g.internal_decision(View::P2).is_none());
    let observation_play = g
        .answer_internal_card(
            look_choice.actor,
            look_choice.token,
            Answer::Targets(vec![look_targets.members[0].handle]),
        )
        .unwrap();
    resolved(finish(&mut g, observation_play, &[]));
    let look_view = g.projection(View::Referee).unwrap();
    assert_eq!(look_view["P1"]["hand"], json!(["omen"]));
    assert_eq!(
        look_view["P1"]["deck"],
        json!(["tail", "plain-card", "other"])
    );
    g.resume_internal_card().unwrap();
    let p = g
        .activate_internal_card("source", AbilityId(4), PlayerId { seat: 1 })
        .unwrap();
    resolved(finish(&mut g, p, &["BP01-037"]));
    let activation_view = g.projection(View::Referee).unwrap();
    assert_eq!(activation_view["P1"]["pp"]["current"], 8_u8);
    assert_eq!(activation_view["P1"]["cemetery"], json!(["source"]));
    assert!(
        activation_view["objects"]["ally"]["keywords"]
            .as_array()
            .unwrap()
            .contains(&json!("突進"))
    );
}
