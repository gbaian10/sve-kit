use super::event_cemetery::finish_checks;
use super::event_subject_capture::{position, prepared_program};
use super::union_burst_batch1::{finish, pending, question, ub};
use super::*;

fn reader_tree(who: Option<&str>) -> Value {
    let mut tree = expanded::tree().clone();
    let mut card = tree["cards"]["BP01-041"].clone();
    let mut query = json!({"event":"ub_activated","window":"this_turn"});
    if let Some(who) = who {
        query["who"] = json!(who);
        query["exclude"] = json!("this_execution");
    }
    card["abilities"][0]["activate"] =
        json!({"cost":{"pp":0_u64},"do":[{"life":{"who":"self","delta":{"history_count":query}}}]});
    tree["cards"] = json!({"producer":card});
    tree
}

fn reader(who: Option<&str>) -> WellFormedCardProgram {
    lowerer()
        .lower(&reader_tree(who))
        .unwrap()
        .remove("producer")
        .unwrap()
}

fn initial(card: &str, prior: usize, field: bool) -> Value {
    let mut initial = position(card, !field);
    initial["players"]["P1"]["construction"] = json!("title");
    initial["players"]["P1"]["title_code"] = json!("princess_connect_re_dive");
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    initial["players"]["P1"]["zones"]["deck"] =
        json!([{"id":"drawn","card":"subject-draw"},{"id":"spare","card":"subject-draw"}]);
    for i in 0..prior {
        if initial["players"]["P1"]["zones"]["hand"].is_null() {
            initial["players"]["P1"]["zones"]["hand"] = json!([]);
        }
        initial["players"]["P1"]["zones"]["hand"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":format!("prior{i}"),"card":"CP04-089"}));
    }
    initial
}

fn fanfare(game: &mut Game, id: &str, card: &str) -> CardPlay {
    finish_checks(game);
    assert!(
        matches!(
            game.play_internal_card(id, PlayerId { seat: 1 }).unwrap(),
            CardPlay::Entered(_)
        ),
        "the follower enters before its separate UB fanfare"
    );
    let play = pending(game, card);
    finish(game, play)
}
fn prime(game: &mut Game, prior: usize) {
    for i in 0..prior {
        ub(&resolved(fanfare(game, &format!("prior{i}"), "CP04-089")));
    }
    finish_checks(game);
}
fn probe(game: &mut Game) {
    finish_checks(game);
    let play = game
        .activate_internal_card(
            "producer",
            reader(None).card().abilities[0].node.id,
            PlayerId { seat: 1 },
        )
        .unwrap();
    resolved(play);
}

#[test]
fn eriko_all_printings_exclude_this_play_from_the_two_prior_ub_threshold() {
    for card in ["CP04-089", "CP04-P69", "PR-507"] {
        for prior in 0..=2 {
            let mut game = prepared_program(&initial(card, prior, false), &reader(Some("self")));
            prime(&mut game, prior);
            ub(&resolved(fanfare(&mut game, "source", card)));
            assert_eq!(
                game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
                prior + if prior == 2 { 3 } else { 1 }
            );
            assert_eq!(
                game.projection(View::Referee).unwrap()["semantic_state"]["counters_this_turn"]["P1"]
                    ["ub_activated"],
                prior + 1
            );
            probe(&mut game);
            assert_eq!(
                game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
                11 + prior
            );
        }
    }
}

#[test]
fn precia_all_printings_draw_only_after_two_prior_ubs_and_count_other_followers() {
    for card in ["CP04-043", "CP04-P31"] {
        for prior in 0..=2 {
            let mut game = prepared_program(&initial(card, prior, false), &reader(None));
            prime(&mut game, prior);
            ub(&resolved(fanfare(&mut game, "source", card)));
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["objects"]["enemy"]["damage"], 2 * prior + 1);
            assert_eq!(
                state["P1"]["hand"],
                if prior == 2 {
                    json!(["drawn"])
                } else {
                    json!([])
                }
            );
        }
    }
}

#[test]
fn rei_all_printings_keep_rush_and_replace_the_attack_buff_after_two_prior_ubs() {
    for card in ["CP04-080", "CP04-P60", "PCS01-033", "PR-499"] {
        for prior in 0..=2 {
            let mut game = prepared_program(&initial(card, prior, true), &reader(None));
            prime(&mut game, prior);
            game.internal_rule_action(
                &json!({"do":"attack","attacker":"source","target":"P2.leader"}),
            )
            .unwrap();
            let play = pending(&mut game, card);
            ub(&resolved(finish(&mut game, play)));
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["objects"]["source"]["power"],
                if prior == 2 { 3_u8 } else { 2_u8 }
            );
            assert!(
                state["objects"]["source"]["keywords"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("突進"))
            );
        }
    }
}

#[test]
fn targetless_fanfare_is_cancelled_without_counting_and_other_controller_is_filtered() {
    for card in ["CP04-043", "CP04-089"] {
        let mut initial = initial(card, 1, false);
        initial["players"]["P2"]["zones"]["field"] = json!([]);
        let mut game = prepared_program(&initial, &reader(Some("self")));
        assert_eq!(
            fanfare(&mut game, "prior0", "CP04-089"),
            CardPlay::Cancelled
        );
        assert_eq!(fanfare(&mut game, "source", card), CardPlay::Cancelled);
        probe(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
            10_u8
        );
    }
    for who in [None, Some("self"), Some("opp")] {
        let mut game = prepared_program(&initial("CP04-089", 2, false), &reader(who));
        prime(&mut game, 2);
        probe(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
            if who == Some("opp") { 10_u8 } else { 12_u8 }
        );
    }
}

#[test]
fn a_second_posture_payment_failure_does_not_record_another_ub() {
    let mut game = prepared_program(&initial("CP04-015", 0, true), &reader(Some("self")));
    let definition = selection_with_rules().current["CP04-015"]
        .program
        .card()
        .abilities[0]
        .node
        .id;
    let play = game
        .activate_internal_card("source", definition, PlayerId { seat: 1 })
        .unwrap();
    ub(&resolved(finish(&mut game, play)));
    finish_checks(&mut game);
    let before = game.projection(View::Referee).unwrap();
    let question = question(
        game.activate_internal_card("source", definition, PlayerId { seat: 1 })
            .unwrap(),
    );
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("the formal target precedes the failed posture payment");
    };
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::Targets(vec![domain.members[0].handle]),
    )
    .unwrap_err();
    assert_eq!(game.projection(View::Referee).unwrap(), before);
    probe(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        11_u8
    );
}

fn end(game: &mut Game) {
    finish_checks(game);
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    let window = game.internal_quick_choice().unwrap().unwrap();
    game.answer_internal_card(window.actor, window.token, Answer::Quick(None))
        .unwrap();
}

#[test]
fn committed_ub_history_is_cleared_at_the_next_turn() {
    let mut initial = initial("CP04-089", 2, false);
    initial["players"]["P2"]["zones"]["deck"] =
        json!([{"id":"opponent_draw","card":"subject-draw"}]);
    let mut game = prepared_program(&initial, &reader(Some("self")));
    prime(&mut game, 2);
    probe(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        12_u8
    );
    end(&mut game);
    end(&mut game);
    probe(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        12_u8
    );
    let before_damage = game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"]
        .as_u64()
        .unwrap();
    ub(&resolved(fanfare(&mut game, "source", "CP04-089")));
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
        before_damage + 1
    );
}

#[test]
fn explicit_prior_ub_counters_feed_the_real_seed_and_controller_queries() {
    for (who, gain) in [(Some("self"), 3_u8), (Some("opp"), 3_u8), (None, 6_u8)] {
        let mut initial = initial("CP04-089", 0, false);
        initial["semantic_state"]["counters_this_turn"] =
            json!({"P1":{"ub_activated":2_u8},"P2":{"ub_activated":3_u8}});
        let mut tree = reader_tree(who);
        if who == Some("opp") {
            tree["cards"]["producer"]["abilities"][0]["activate"]["ub"] = json!(true);
        }
        let program = lowerer().lower(&tree).unwrap().remove("producer").unwrap();
        let mut game = prepared_program(&initial, &program);
        ub(&resolved(fanfare(&mut game, "source", "CP04-089")));
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
            3_u8
        );
        probe(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
            10_u8 + gain
        );
        if who == Some("opp") {
            assert_eq!(
                game.projection(View::Referee).unwrap()["semantic_state"]["counters_this_turn"]["P1"]
                    ["ub_activated"],
                4_u8
            );
        }
    }
}
