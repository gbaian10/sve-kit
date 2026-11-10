#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate the printed top-deck cost scenarios."
)]
use super::*;

fn initial(card: &str, count: usize, fuel: bool) -> Value {
    let mut position = draw_position();
    position["players"]["P1"]["zones"]["field"] = json!([
        {"id":"source","card":card,"state":{"entered_this_turn":true}}
    ]);
    position["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-080","state":{"acted":true}}]);
    position["players"]["P1"]["zones"]["deck"] = json!(
        (0..count)
            .map(|n| json!({"id":format!("d{n}"),"card":"BP01-171"}))
            .collect::<Vec<_>>()
    );
    position["players"]["P1"]["zones"]["hand"] = if fuel {
        json!([{"id":"fuel","card":"BP01-171"}])
    } else {
        json!([])
    };
    position
}

fn finish(game: &mut Game, mut play: CardPlay, accept: bool) {
    while let CardPlay::Awaiting(question) = play {
        let answer = match &question.domain {
            InputDomain::OptionalCost => Answer::OptionalCost(accept),
            InputDomain::Targets(domain) => {
                let other = if question.actor.seat == 1 {
                    View::P2
                } else {
                    View::P1
                };
                assert!(
                    game.internal_decision(other).is_none(),
                    "only the discarding player sees their hand choice"
                );
                Answer::Targets(
                    domain
                        .members
                        .iter()
                        .take(1)
                        .map(|member| member.handle)
                        .collect(),
                )
            }
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            _ => panic!("a deterministic top-deck cost must not ask for materials"),
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}

fn settle(game: &mut Game, accept: bool) {
    loop {
        while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
        if let Some(question) = game.internal_pending_choice().unwrap() {
            let InputDomain::Pending(options) = &question.domain else {
                panic!("pending printed ability")
            };
            let play = game
                .answer_internal_card(
                    question.actor,
                    question.token,
                    Answer::Pending(options[0].handle),
                )
                .unwrap();
            finish(game, play, accept);
            continue;
        }
        let Some(question) = game.internal_quick_choice().unwrap() else {
            break;
        };
        let play = game
            .answer_internal_card(question.actor, question.token, Answer::Quick(None))
            .unwrap();
        finish(game, play, accept);
    }
}

fn attack(game: &mut Game, attacker: &str, target: &str, accept: bool) {
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":attacker,"target":target}))
            .unwrap()
            .outcome,
        "resolved",
        "the printed attacker must be able to declare this battle"
    );
    settle(game, accept);
}

#[test]
fn blade_elf_storm_attack_pays_exactly_three_top_cards_before_its_buff_and_combat() {
    for (count, accept) in [(2_usize, true), (3, true), (4, true), (3, false)] {
        let position = initial("BP13-015", count, false);
        let mut game = game(&position);
        attack(&mut game, "source", "P2.leader", accept);
        let state = game.projection(View::Referee).unwrap();
        let paid = accept && count >= 3;
        assert_eq!(
            state["objects"]["source"]["power"],
            if paid { 3_u8 } else { 2 }
        );
        assert_eq!(
            state["objects"]["source"]["hp"],
            if paid { 3_u8 } else { 2 }
        );
        assert_eq!(state["P2"]["leader"]["life"], if paid { 17_u8 } else { 18 });
        let consumed = if paid { 3 } else { 0 };
        assert_eq!(
            state["P1"]["banish"],
            json!((0..consumed).map(|n| format!("d{n}")).collect::<Vec<_>>())
        );
        assert_eq!(
            state["P1"]["deck"],
            json!(
                (consumed..count)
                    .map(|n| format!("d{n}"))
                    .collect::<Vec<_>>()
            )
        );
        assert_eq!(state["P1"]["pp"]["current"], 10_u8);
        if paid {
            let public = game.projection(View::P2).unwrap();
            for id in ["d0", "d1", "d2"] {
                assert_eq!(
                    public["objects"][id]["name"], "ゴブリン",
                    "paid top cards are face-up in banish"
                );
            }
        }
    }
}

#[test]
fn dark_alice_all_printings_rush_attack_forces_only_the_opponents_chosen_discard() {
    for card in ["BP03-073", "BP03-SL14", "BP03-U05", "PR-048"] {
        let mut position = initial(card, 0, true);
        position["players"]["P2"]["zones"]["hand"] = json!([
            {"id":"opp-one","card":"BP01-171"},{"id":"opp-two","card":"BP01-080"}
        ]);
        let mut game = game(&position);
        attack(&mut game, "source", "enemy", true);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["enemy"]["damage"], 6_u8);
        assert_eq!(
            state["P2"]["hand"].as_array().unwrap().len(),
            1,
            "the opponent chooses exactly one card"
        );
        assert_eq!(
            state["P2"]["cemetery"].as_array().unwrap().len(),
            1,
            "only the opponent's selected card is discarded"
        );
        assert_eq!(state["P1"]["hand"], json!(["fuel"]));
        assert_eq!(state["P1"]["cemetery"], json!(["source"]));
        assert_eq!(state["P1"]["banish"], json!([]));
    }
}

#[test]
fn dark_alice_all_printings_pay_full_ten_or_decline_the_entire_lastword() {
    for card in ["BP03-073", "BP03-SL14", "BP03-U05", "PR-048"] {
        for (count, accept, fuel) in [
            (10_usize, true, false),
            (9, true, true),
            (10, false, true),
            (11, true, true),
        ] {
            let mut position = initial(card, count, fuel);
            position["turn"]["active"] = json!("P2");
            position["players"]["P2"]["zones"]["field"][0]["state"]["acted"] = json!(false);
            position["players"]["P1"]["zones"]["field"][0]["state"]["acted"] = json!(true);
            let mut game = game(&position);
            attack(&mut game, "enemy", "source", accept);
            let state = game.projection(View::Referee).unwrap();
            let paid = accept && count >= 10;
            assert_eq!(
                state["P1"]["field"],
                if paid { json!(["source"]) } else { json!([]) }
            );
            assert_eq!(
                state["P1"]["hand"],
                if !paid && fuel {
                    json!(["fuel"])
                } else {
                    json!([])
                }
            );
            let grave = if paid && fuel {
                json!(["fuel"])
            } else if paid {
                json!([])
            } else {
                json!(["source"])
            };
            assert_eq!(state["P1"]["cemetery"], grave);
            let consumed = if paid { 10 } else { 0 };
            assert_eq!(
                state["P1"]["banish"],
                json!((0..consumed).map(|n| format!("d{n}")).collect::<Vec<_>>())
            );
            assert_eq!(
                state["P1"]["deck"],
                json!(
                    (consumed..count)
                        .map(|n| format!("d{n}"))
                        .collect::<Vec<_>>()
                )
            );
            assert_eq!(state["P1"]["pp"]["current"], 10_u8);
            if paid {
                assert_eq!(state["objects"]["source"]["generation"], 2_u8);
                assert_eq!(state["objects"]["source"]["damage"], 0_u8);
                assert_eq!(state["objects"]["source"]["hp"], 6_u8);
            }
        }
    }
}
