#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected questions invalidate the printed hand departure scenarios."
)]
use super::*;
use sve_engine::game::internal::DecisionView;

fn checks(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}

fn pending(game: &mut Game, printing: &str, event: Option<&str>) -> CardPlay {
    checks(game);
    let q = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &q.domain else {
        panic!("pending")
    };
    assert_eq!(
        options.len(),
        1,
        "one occurrence, despite two typed event alternatives"
    );
    assert_eq!(
        q.actor,
        PlayerId { seat: 1 },
        "the hand source controls its occurrence"
    );
    assert_eq!(
        options[0].card, printing,
        "the pending source retains its printing"
    );
    if let Some(event) = event {
        assert_eq!(
            options[0].event, event,
            "the pending source retains its hand event"
        );
    }
    game.answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
        .unwrap()
}

fn question(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(q) = play else {
        panic!("printed decision")
    };
    q
}

fn finish(game: &mut Game, mut play: CardPlay, selected: Option<&str>) {
    while let CardPlay::Awaiting(q) = play {
        let answer = match &q.domain {
            InputDomain::Targets(domain) => Answer::Targets(match selected {
                Some(printing) => vec![
                    domain
                        .members
                        .iter()
                        .find(|entry| entry.card.as_deref() == Some(printing))
                        .unwrap()
                        .handle,
                ],
                None if domain.cardinality.minimum.0 == 0 => vec![],
                None => vec![domain.members[0].handle],
            }),
            InputDomain::Integer(_) => Answer::Integer(Nat(1)),
            InputDomain::Order(cards) => {
                Answer::Order(cards.iter().rev().map(|card| card.handle).collect())
            }
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            other => panic!("unexpected printed decision {other:?}"),
        };
        play = game.answer_internal_card(q.actor, q.token, answer).unwrap();
    }
    if !matches!(play, CardPlay::Entered(_)) {
        resolved(play);
    }
    checks(game);
}

fn position(printing: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"fuel","card":printing}]);
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"ally","card":"BP14-074"},{"id":"wrong-class","card":"BP14-060"}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP14-074"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"d0","card":"BP01-171"},{"id":"d1","card":"BP01-171"},
        {"id":"d2","card":"BP01-171"},{"id":"d3","card":"BP01-171"}
    ]);
    initial["players"]["P2"]["zones"]["deck"] = json!([{"id":"opponent-draw","card":"BP01-171"}]);
    initial
}

fn game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "printed-hand-banish",
    )
    .unwrap()
}

fn produce(printing: &str, banish: bool) -> Game {
    let producer = if banish { "BP18-041" } else { "BP02-052" };
    let mut initial = position(printing);
    initial["players"]["P1"]["zones"]["hand"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"producer","card":producer}));
    let mut game = game(&initial);
    let enter = game
        .play_internal_card("producer", PlayerId { seat: 1 })
        .unwrap();
    finish(&mut game, enter, None);
    let body = pending(&mut game, producer, None);
    finish(&mut game, body, banish.then_some(printing));
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"][if banish { "banish" } else { "cemetery" }],
        json!(["fuel"]),
        "the producer commits the selected hand action before its trigger runs"
    );
    game
}

#[test]
fn ansage_both_printings_follow_their_real_discard_and_banish_into_ex() {
    for printing in ["BP14-074", "PR-367"] {
        for banish in [false, true] {
            let mut game = produce(printing, banish);
            let body = pending(
                &mut game,
                printing,
                Some(if banish { "Banish" } else { "Discard" }),
            );
            finish(&mut game, body, None);
            let view = game.projection(View::Referee).unwrap();
            assert_eq!(view["P1"]["ex"], json!(["fuel"]));
            assert_eq!(
                view["P1"][if banish { "banish" } else { "cemetery" }],
                json!([])
            );
            assert!(game.internal_pending_choice().unwrap().is_none());
        }
    }
}

#[test]
fn will_both_printings_grant_bane_only_to_a_formal_own_nightmare_target_after_either_hand_action() {
    for printing in ["BP14-081", "PR-379"] {
        for banish in [false, true] {
            let mut game = produce(printing, banish);
            let q = question(pending(
                &mut game,
                printing,
                Some(if banish { "Banish" } else { "Discard" }),
            ));
            let InputDomain::Targets(domain) = &q.domain else {
                panic!("formal own Nightmare target")
            };
            assert_eq!(domain.members.len(), 1);
            assert_eq!(domain.members[0].card.as_deref(), Some("BP14-074"));
            let body = game
                .answer_internal_card(
                    q.actor,
                    q.token,
                    Answer::Targets(vec![domain.members[0].handle]),
                )
                .unwrap();
            finish(&mut game, body, None);
            let view = game.projection(View::Referee).unwrap();
            assert!(
                view["objects"]["ally"]["keywords"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("必殺"))
            );
            for id in ["enemy", "wrong-class"] {
                assert!(
                    !view["objects"][id]["keywords"]
                        .as_array()
                        .unwrap()
                        .contains(&json!("必殺"))
                );
            }
            assert!(game.internal_pending_choice().unwrap().is_none());
        }
    }
}

#[test]
fn will_both_printings_revive_banquet_followers_and_buff_only_an_arriving_ansage() {
    for printing in ["BP14-081", "PR-379"] {
        for (revived, full) in [("BP14-074", false), ("BP14-060", false), ("BP14-074", true)] {
            let mut initial = position(printing);
            initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":printing}]);
            initial["players"]["P1"]["zones"]["evolve_deck"] =
                json!([{"id":"form","card":"BP14-075"}]);
            initial["players"]["P1"]["zones"]["cemetery"] = json!([
                {"id":"revived","card":revived},{"id":"wrong-trait","card":"BP01-171"}
            ]);
            initial["players"]["P1"]["zones"]["field"] = if full {
                json!([{"filler":5_u8}])
            } else {
                json!([])
            };
            let mut game = game(&initial);
            let targets = question(
                game.play_internal_card("source", PlayerId { seat: 1 })
                    .unwrap(),
            );
            let InputDomain::Targets(domain) = &targets.domain else {
                panic!("formal cemetery target")
            };
            assert_eq!(domain.members.len(), 1);
            let body = game
                .answer_internal_card(
                    targets.actor,
                    targets.token,
                    Answer::Targets(vec![domain.members[0].handle]),
                )
                .unwrap();
            finish(&mut game, body, None);
            let view = game.projection(View::Referee).unwrap();
            assert!(
                view["P1"][if full { "cemetery" } else { "field" }]
                    .as_array()
                    .unwrap()
                    .contains(&json!("revived"))
            );
            assert_eq!(
                view["objects"]["revived"]["power"],
                if revived == "BP14-074" && !full {
                    3_u8
                } else {
                    2_u8
                }
            );
            assert_eq!(
                view["objects"]["revived"]["keywords"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("疾走")),
                revived == "BP14-074" && !full
            );
            assert!(game.internal_pending_choice().unwrap().is_none());
            if revived == "BP14-074" && !full {
                assert_eq!(game.internal_rule_action(
                    &json!({"do":"evolve","source":"revived","evolve_card":"form","pay":{"pp":1_u8}})
                ).unwrap().outcome, "resolved");
                let evolved_body = pending(&mut game, "BP14-075", None);
                finish(&mut game, evolved_body, None);
                let evolved_view = game.projection(View::Referee).unwrap();
                assert_eq!(
                    evolved_view["objects"]["revived"]["power"], 4_u8,
                    "the arriving grant survives evolution"
                );
                assert_eq!(evolved_view["objects"]["revived"]["hp"], 3_u8);
                assert!(
                    evolved_view["objects"]["revived"]["keywords"]
                        .as_array()
                        .unwrap()
                        .contains(&json!("疾走"))
                );
                assert_eq!(
                    game.internal_rule_action(
                        &json!({"do":"attack","attacker":"revived","target":"P2.leader"})
                    )
                    .unwrap()
                    .outcome,
                    "resolved"
                );
                let quick = game.internal_quick_choice().unwrap().unwrap();
                game.answer_internal_card(quick.actor, quick.token, Answer::Quick(None))
                    .unwrap();
                assert_eq!(
                    game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
                    16_u8
                );
            }
        }
    }
}

#[test]
fn ansage_both_printings_pay_one_to_evolve_into_the_admitted_real_form() {
    for printing in ["BP14-074", "PR-367"] {
        let mut initial = position(printing);
        initial["players"]["P1"]["pp"] = json!({"current":1_u8,"max":1_u8});
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":printing}]);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP14-075"}]);
        let mut game = game(&initial);
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
            )
            .unwrap()
            .outcome,
            "resolved"
        );
        let body = pending(&mut game, "BP14-075", None);
        finish(&mut game, body, None);
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["P1"]["pp"]["current"], 0_u8);
        assert_eq!(view["objects"]["source"]["card"], printing);
        assert_eq!(view["objects"]["source"]["evolved"], true);
        assert_eq!(view["objects"]["source"]["evolved_with"], "form");
        assert_eq!(projected_zone_count(&view["P1"]["deck"]), 4);
    }
}

#[test]
fn all_four_printings_trigger_from_the_rule_hand_limit_discard() {
    for printing in ["BP14-074", "PR-367", "BP14-081", "PR-379"] {
        let mut initial = position(printing);
        for i in 0_u8..7 {
            initial["players"]["P1"]["zones"]["hand"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":format!("keep{i}"),"card":"BP01-171"}));
        }
        let mut game = game(&initial);
        game.internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap();
        checks(&mut game);
        while let Some(q) = game.internal_quick_choice().unwrap() {
            game.answer_internal_card(q.actor, q.token, Answer::Quick(None))
                .unwrap();
            checks(&mut game);
        }
        game.internal_rule_action(&json!({"do":"end-discard","select":["fuel"]}))
            .unwrap();
        let body = pending(&mut game, printing, Some("Discard"));
        finish(&mut game, body, Some("BP14-074"));
        let view = game.projection(View::Referee).unwrap();
        if matches!(printing, "BP14-074" | "PR-367") {
            assert_eq!(view["P1"]["ex"], json!(["fuel"]), "Q1922");
        } else {
            assert!(
                view["objects"]["ally"]["keywords"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("必殺")),
                "Q1930"
            );
        }
        assert!(game.internal_pending_choice().unwrap().is_none());
    }
}
