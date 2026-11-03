#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected questions invalidate the printed Ride scenarios."
)]
use super::*;
use sve_engine::game::internal::SearchAnswer;

fn position(card: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P2"]["leader"]["life"] = json!(100_u8);
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"source","card":card,"state":{"entered_this_turn":true}},
        {"id":"a","card":"CP03-013"},
        {"id":"b","card":"CP03-013"},
        {"id":"c","card":"CP03-013"}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"enemy","card":"BP01-037","state":{"hp":40_u8,"max_hp":40_u8}}
    ]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"point","card":"CSD03a-017"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([]);
    initial
}

fn answer(domain: &InputDomain) -> Answer {
    match domain {
        InputDomain::Targets(d) => Answer::Targets(
            d.members
                .iter()
                .take(
                    d.cardinality
                        .maximum
                        .map_or(d.members.len(), |n| usize::try_from(n.0).unwrap()),
                )
                .map(|m| m.handle)
                .collect(),
        ),
        InputDomain::Search(d) => Answer::Search(SearchAnswer::Ungrouped(
            d.cards
                .iter()
                .take(
                    d.cardinality
                        .maximum
                        .map_or(d.cards.len(), |n| usize::try_from(n.0).unwrap()),
                )
                .map(|c| c.handle)
                .collect(),
        )),
        InputDomain::Order(cards) => Answer::Order(cards.iter().map(|c| c.handle).collect()),
        _ => panic!("unexpected Ride question"),
    }
}

fn finish(game: &mut Game, mut play: CardPlay) {
    while let CardPlay::Awaiting(q) = play {
        play = game
            .answer_internal_card(q.actor, q.token, answer(&q.domain))
            .unwrap();
    }
}

fn settle(game: &mut Game) {
    loop {
        if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
        let Some(q) = game.internal_pending_choice().unwrap() else {
            break;
        };
        let InputDomain::Pending(options) = &q.domain else {
            panic!("pending")
        };
        let play = game
            .answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
            .unwrap();
        finish(game, play);
    }
}

fn ride(game: &mut Game, card: &str) {
    let id = selection_with_rules().current[card]
        .program
        .card()
        .abilities
        .iter()
        .find(|a| {
            matches!(
                a.node.source.origin,
                AbilityOrigin::Keyword {
                    keyword: KeywordId::Ride,
                    ..
                }
            )
        })
        .unwrap()
        .node
        .id;
    let CardPlay::Awaiting(q) = game
        .activate_internal_card("source", id, PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("Ride payment")
    };
    let play = game
        .answer_internal_card(
            q.actor,
            q.token,
            Answer::ResourceEntryPayment {
                materials: vec![0],
                use_ep: false,
            },
        )
        .unwrap();
    finish(game, play);
    settle(game);
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(
        view["P1"]["drive"],
        json!(["point"]),
        "Ride associates a DrivePoint"
    );
    assert_eq!(view["P1"]["pp"]["current"], 9_u8, "Ride pays one PP");
}

fn game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "ride-batch2",
    )
    .unwrap()
}

fn attack(game: &mut Game, source: &str) {
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":source,"target":"P2.leader"}))
            .unwrap()
            .outcome,
        "resolved",
        "a real declaration records attack history"
    );
    settle(game);
    if let Some(q) = game.internal_quick_choice().unwrap() {
        let play = game
            .answer_internal_card(q.actor, q.token, Answer::Quick(None))
            .unwrap();
        finish(game, play);
        settle(game);
    }
}

#[test]
fn cp03_009_all_printings_grant_storm_and_damage_only_on_the_third_aqua_attack() {
    for card in ["CP03-009", "CP03-P05"] {
        for earlier in [0_usize, 2, 3] {
            let mut board = game(&position(card));
            ride(&mut board, card);
            for source in ["a", "b", "c"].iter().take(earlier) {
                attack(&mut board, source);
            }
            attack(&mut board, "source");
            let view = board.projection(View::Referee).unwrap();
            assert_eq!(
                view["objects"]["source"]["power"], 2_u8,
                "Drive-time buffs the holder"
            );
            assert_eq!(
                view["objects"]["enemy"]["hp"],
                if earlier == 2 { 38_u8 } else { 40_u8 },
                "the granted ability checks exactly attack three, not three or more"
            );
            assert!(
                view["P2"]["leader"]["life"].as_u64().unwrap() < 100,
                "the granted Storm permits attacking the leader"
            );
        }
    }
}

#[test]
fn cp03_019_all_printings_observe_other_aqua_attacks_but_damage_only_on_the_third() {
    for card in ["CP03-019", "CP03-P13"] {
        let mut board = game(&position(card));
        ride(&mut board, card);
        assert_eq!(
            board.projection(View::Referee).unwrap()["objects"]["enemy"]["hp"],
            38_u8,
            "Drive-time deals two before attack history"
        );
        for (source, expected) in [("a", 38_u8), ("b", 38), ("c", 36)] {
            attack(&mut board, source);
            assert_eq!(
                board.projection(View::Referee).unwrap()["objects"]["enemy"]["hp"],
                expected,
                "the observer counts other own Aqua Force attacks"
            );
        }
        assert_eq!(
            board.projection(View::Referee).unwrap()["objects"]["source"]["power"],
            3_u8,
            "Drive-time also buffs the observer"
        );
    }
}

#[test]
fn cp03_073_all_printings_reveal_one_or_two_kagero_followers_and_bottom_the_remainder() {
    for card in ["CP03-073", "CP03-P50"] {
        for maximum in [6_u8, 7] {
            let mut initial = position(card);
            initial["players"]["P1"]["pp"] = json!({"current":maximum,"max":maximum});
            initial["players"]["P1"]["zones"]["deck"] = json!([
                {"id":"first","card":"CP03-063"},{"id":"second","card":"CP03-068"},{"id":"tail","card":"BP01-009"}
            ]);
            let mut board = game(&initial);
            let id = selection_with_rules().current[card]
                .program
                .card()
                .abilities[0]
                .node
                .id;
            let CardPlay::Awaiting(q) = board
                .activate_internal_card("source", id, PlayerId { seat: 1 })
                .unwrap()
            else {
                panic!("Ride payment")
            };
            let play = board
                .answer_internal_card(
                    q.actor,
                    q.token,
                    Answer::ResourceEntryPayment {
                        materials: vec![0],
                        use_ep: false,
                    },
                )
                .unwrap();
            finish(&mut board, play);
            settle(&mut board);
            let view = board.projection(View::Referee).unwrap();
            assert_eq!(
                view["P1"]["hand"],
                if maximum == 7 {
                    json!(["first", "second"])
                } else {
                    json!(["first"])
                },
                "Awaken raises the optional reveal cap from one to two"
            );
            assert_eq!(
                view["P1"]["deck"],
                if maximum == 7 {
                    json!(["tail"])
                } else {
                    json!(["tail", "second"])
                },
                "unselected observed cards go below the unobserved deck"
            );
            assert_eq!(
                view["objects"]["source"]["power"], 3_u8,
                "Drive-time buffs before observing"
            );
        }
    }
}
