#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected questions invalidate the printed Octrice scenarios."
)]
use super::*;

const PLAYER: PlayerId = PlayerId { seat: 1 };

fn fixture(card: &str, grave: usize) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"thief","card":"BP05-029"},
        {"id":"other","card":"BP01-171"},
        {"id":"spare","card":"BP05-029"},
        {"id":"tail","card":"BP01-171"}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-080"}]);
    initial["players"]["P2"]["zones"]["cemetery"] = json!(
        (0..grave)
            .map(|n| json!({"id":format!("grave{n}"),"card":"BP01-171"}))
            .collect::<Vec<_>>()
    );
    initial
}

fn finish(game: &mut Game, mut play: CardPlay, modes: &[ModeId], take: bool) {
    while let CardPlay::Awaiting(question) = play {
        let answer = match &question.domain {
            InputDomain::Modes { .. } => Answer::Modes(modes.to_vec()),
            InputDomain::Targets(domain) => {
                if domain.members.iter().any(|member| member.player.seat == 2) {
                    assert_eq!(
                        domain.members.len(),
                        1,
                        "only the printed target or named cost material is eligible"
                    );
                } else {
                    assert!(
                        domain
                            .members
                            .iter()
                            .all(|member| member.card.as_deref() == Some("BP05-029")),
                        "only observed thieves can be selected"
                    );
                }
                Answer::Targets(
                    domain
                        .members
                        .iter()
                        .take(if domain.members.iter().any(|m| m.player.seat == 2) {
                            1
                        } else {
                            usize::from(take)
                        })
                        .map(|member| member.handle)
                        .collect(),
                )
            }
            InputDomain::Order(cards) => {
                Answer::Order(cards.iter().rev().map(|card| card.handle).collect())
            }
            InputDomain::CostMaterials(domain) => {
                assert_eq!(
                    domain.members.len(),
                    1,
                    "only the printed target or named cost material is eligible"
                );
                Answer::CostMaterials(vec![domain.members[0].handle])
            }
            _ => panic!("unexpected Octrice question"),
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}

fn activate(game: &mut Game, card: &str, line: u32, modes: &[ModeId]) {
    let ability = selection().current[card]
        .program
        .card()
        .abilities
        .iter()
        .find(|ability| ability.node.kind == AbilityKind::Activated && ability.source.line == line)
        .unwrap()
        .node
        .id;
    let play = game
        .activate_internal_card("source", ability, PLAYER)
        .unwrap();
    finish(game, play, modes, true);
}

#[test]
fn octrice_all_printings_create_the_complete_afterglow_token() {
    for card in ["BP15-020", "BP15-SL06", "SCS01-003"] {
        let mut game = game(&fixture(card, 0));
        let played = game.play_internal_card("source", PLAYER).unwrap();
        finish(&mut game, played, &[], true);
        let question = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = &question.domain else {
            panic!("Fanfare")
        };
        let fanfare = game
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap();
        finish(&mut game, fanfare, &[], true);
        let state = game.projection(View::Referee).unwrap();
        let token = state["P1"]["ex"][0].as_str().unwrap();
        assert_eq!(state["objects"][token]["name"], "空絶の残光");
        let token = token.to_owned();
        let token_play = game.play_internal_card(&token, PLAYER).unwrap();
        finish(&mut game, token_play, &[ModeId(2)], true);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["hand"],
            json!(["thief"])
        );
    }
}

#[test]
fn afterglow_all_printings_reveal_only_the_chosen_thief_and_order_the_remainder() {
    for card in ["BP15-PR10", "BP15-PR20"] {
        for take in [false, true] {
            let mut game = game(&fixture(card, 0));
            let play = game.play_internal_card("source", PLAYER).unwrap();
            finish(&mut game, play, &[ModeId(2)], take);
            let public = game.projection(View::P2).unwrap();
            let known = public["known_cards"].as_array().unwrap();
            assert_eq!(
                known.iter().any(|known_card| known_card["id"] == "thief"),
                take
            );
            for id in ["other", "spare", "tail"] {
                assert!(!known.iter().any(|known_card| known_card["id"] == id));
            }
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["P1"]["hand"],
                if take { json!(["thief"]) } else { json!([]) }
            );
            assert_eq!(
                state["P1"]["deck"],
                if take {
                    json!(["tail", "spare", "other"])
                } else {
                    json!(["tail", "spare", "other", "thief"])
                }
            );
        }
    }
}

#[test]
fn afterglow_damage_and_mode_limit_follow_the_opponents_cemetery_threshold() {
    for (grave, damage) in [(9_usize, 3_u8), (10, 8)] {
        let mut game = game(&fixture("BP15-PR10", grave));
        let play = game.play_internal_card("source", PLAYER).unwrap();
        if grave == 9 {
            let CardPlay::Awaiting(question) = &play else {
                panic!("mode")
            };
            game.answer_internal_card(
                question.actor,
                question.token,
                Answer::Modes(vec![ModeId(1), ModeId(2)]),
            )
            .unwrap_err();
        }
        let modes = if grave == 10 {
            vec![ModeId(1), ModeId(2)]
        } else {
            vec![ModeId(1)]
        };
        finish(&mut game, play, &modes, true);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["enemy"]["damage"], damage);
        assert_eq!(
            state["P1"]["hand"].as_array().unwrap().len(),
            usize::from(grave == 10)
        );
    }
    let mut initial = fixture("BP15-PR10", 10);
    initial["players"]["P2"]["zones"]["field"] = json!([]);
    let mut game = game(&initial);
    let CardPlay::Awaiting(question) = game.play_internal_card("source", PLAYER).unwrap() else {
        panic!("mode")
    };
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::Modes(vec![ModeId(1)]),
    )
    .unwrap_err();
    let play = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Modes(vec![ModeId(2)]),
        )
        .unwrap();
    finish(&mut game, play, &[], false);
}

#[test]
fn octrice_consumes_both_discounts_on_one_treasure_and_rejects_a_third_activation() {
    for card in ["BP15-020", "BP15-SL06", "SCS01-003"] {
        let mut initial = fixture(card, 0);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":card}]);
        initial["players"]["P1"]["zones"]["hand"] = json!([
            {"id":"treasure","card":"BP15-PR10"},
            {"id":"second-treasure","card":"BP15-PR10"}
        ]);
        let mut game = game(&initial);
        activate(&mut game, card, 2, &[]);
        activate(&mut game, card, 2, &[]);
        let ability = selection().current[card]
            .program
            .card()
            .abilities
            .iter()
            .find(|a| a.node.kind == AbilityKind::Activated && a.source.line == 2)
            .unwrap()
            .node
            .id;
        game.activate_internal_card("source", ability, PLAYER)
            .unwrap_err();
        let play = game.play_internal_card("treasure", PLAYER).unwrap();
        finish(&mut game, play, &[ModeId(2)], false);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
            10_u8
        );
        let second_play = game.play_internal_card("second-treasure", PLAYER).unwrap();
        finish(&mut game, second_play, &[ModeId(2)], false);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
            8_u8
        );
    }
}

#[test]
fn octrice_banishes_the_named_old_card_to_create_each_golden_token() {
    for (mode, name) in [(1_u64, "黄金の小剣"), (2, "黄金の杯"), (3, "黄金の靴")] {
        let mut initial = fixture("BP15-020", 0);
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP15-020"}]);
        initial["players"]["P1"]["zones"]["cemetery"] = json!([
            {"id":"old","card":"BP05-018"},{"id":"wrong","card":"BP01-171"}
        ]);
        let mut game = game(&initial);
        activate(&mut game, "BP15-020", 3, &[ModeId(mode)]);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["banish"], json!(["old"]));
        assert_eq!(state["P1"]["cemetery"], json!(["wrong"]));
        assert_eq!(
            state["objects"][state["P1"]["ex"][0].as_str().unwrap()]["name"],
            name
        );
    }
}
