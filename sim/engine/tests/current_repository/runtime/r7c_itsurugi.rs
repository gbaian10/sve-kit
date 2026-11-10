#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate the printed Itsurugi scenarios."
)]
use super::*;

const PLAYER: PlayerId = PlayerId { seat: 1 };

fn initial(card: &str, zone: &str) -> Value {
    let mut state = draw_position();
    state["players"]["P1"]["zones"]["hand"] = json!([]);
    state["players"]["P1"]["zones"][zone] = json!([{"id":"source","card":card}]);
    state["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"top","card":"BP01-171"},{"id":"tail","card":"BP01-171"}
    ]);
    state["players"]["P2"]["zones"]["field"] = json!([
        {"id":"enemy","card":"BP01-080"}
    ]);
    state
}

fn finish(game: &mut Game, mut play: CardPlay, accept: bool) {
    while let CardPlay::Awaiting(question) = play {
        let answer = match &question.domain {
            InputDomain::Integer(_) => Answer::Integer(Nat(u64::from(accept))),
            InputDomain::Targets(domain) => {
                assert!(
                    domain.members.len() <= 1,
                    "the printed choice accepts at most one card"
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
            InputDomain::ResourceEntryPayment {
                materials,
                ep_allowed,
            } => {
                assert!(
                    materials.members.is_empty(),
                    "Advance has no Meal or Ride material"
                );
                assert!(!ep_allowed, "the fixture supplies no EP");
                Answer::ResourceEntryPayment {
                    materials: vec![],
                    use_ep: false,
                }
            }
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            _ => panic!("unexpected Itsurugi decision"),
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    assert!(
        matches!(play, CardPlay::Resolved(_) | CardPlay::Entered(_)),
        "the complete printed action resolves or enters"
    );
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}

fn fanfare(game: &mut Game) {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("Fanfare")
    };
    assert_eq!(options.len(), 1, "only the printed Fanfare is pending");
    let play = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap();
    finish(game, play, true);
}

#[test]
fn itsurugi_all_printings_create_and_play_the_complete_young_wolf_token() {
    for card in ["BP14-070", "BP14-SL14", "BP21-PR12", "BP21-PR18"] {
        let mut position = initial(card, "hand");
        position["players"]["P1"]["zones"]["hand"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"fuel","card":"BP01-171"}));
        let mut game = game(&position);
        let played = game.play_internal_card("source", PLAYER).unwrap();
        finish(&mut game, played, true);
        fanfare(&mut game);
        let entered = game.projection(View::Referee).unwrap();
        let token = entered["P1"]["ex"][0].as_str().unwrap().to_owned();
        assert_eq!(entered["objects"][&token]["name"], "幼狼奮闘");
        let token_play = game.play_internal_card(&token, PLAYER).unwrap();
        finish(&mut game, token_play, true);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["enemy"]["damage"], 1_u8);
        assert_eq!(state["P1"]["hand"], json!([]));
        assert!(
            state["P1"]["cemetery"]
                .as_array()
                .unwrap()
                .contains(&json!("fuel")),
            "the accepted discard moves the hand card to the cemetery"
        );
        assert!(
            state["P1"]["cemetery"]
                .as_array()
                .unwrap()
                .contains(&json!("top")),
            "successful discard mills the top card"
        );
        assert_eq!(state["P1"]["deck"], json!(["tail"]));
        assert_eq!(state["P1"]["pp"]["current"], 8_u8);
    }
}

#[test]
fn young_wolf_all_printings_mill_only_after_an_actual_discard() {
    for card in ["BP14-P32", "BP14-T05"] {
        for (fuel, accept) in [(true, true), (true, false), (false, true), (false, false)] {
            let mut position = initial(card, "ex");
            if fuel {
                position["players"]["P1"]["zones"]["hand"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!({"id":"fuel","card":"BP01-171"}));
            }
            let mut game = game(&position);
            let play = game.play_internal_card("source", PLAYER).unwrap();
            finish(&mut game, play, accept);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["objects"]["enemy"]["damage"], 1_u8);
            assert_eq!(
                state["P1"]["deck"],
                if fuel && accept {
                    json!(["tail"])
                } else {
                    json!(["top", "tail"])
                }
            );
            assert_eq!(
                state["P1"]["hand"],
                if fuel && !accept {
                    json!(["fuel"])
                } else {
                    json!([])
                }
            );
            assert_eq!(
                state["P1"]["cemetery"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("top")),
                fuel && accept
            );
        }
    }
}

#[test]
fn young_wolf_cannot_play_without_a_formal_enemy_target() {
    let mut position = initial("BP14-T05", "ex");
    position["players"]["P2"]["zones"]["field"] = json!([]);
    position["players"]["P1"]["zones"]["hand"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"fuel","card":"BP01-171"}));
    let mut game = game(&position);
    let before = game.projection(View::Referee).unwrap();
    game.play_internal_card("source", PLAYER).unwrap_err();
    assert_eq!(game.projection(View::Referee).unwrap(), before);
}

#[test]
fn itsurugi_advance_checks_each_graveyard_threshold_and_uses_the_named_resource() {
    for (grave, count, allowed) in [
        ("BP14-070", 4_usize, false),
        ("BP14-070", 5, true),
        ("BP01-112", 9, false),
        ("BP01-112", 10, true),
    ] {
        let mut position = initial("BP14-070", "hand");
        position["players"]["P1"]["zones"]["hand"] = json!([]);
        position["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP14-070"}]);
        position["players"]["P1"]["zones"]["cemetery"] = json!(
            (0..count)
                .map(|n| json!({"id":format!("grave{n}"),"card":grave}))
                .collect::<Vec<_>>()
        );
        position["players"]["P1"]["zones"]["evolve_deck"] = json!([
            {"id":"incoming","card":"BP14-071"},{"id":"decoy","card":"BP14-019"}
        ]);
        let mut game = game(&position);
        let ability = selection().current["BP14-070"]
            .program
            .card()
            .abilities
            .iter()
            .find(|ability| ability.node.kind == AbilityKind::Activated)
            .unwrap()
            .node
            .id;
        if allowed {
            let play = game
                .activate_internal_card("source", ability, PLAYER)
                .unwrap();
            finish(&mut game, play, true);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["P1"]["banish"], json!(["source"]));
            assert_eq!(state["P1"]["field"], json!(["incoming"]));
            assert_eq!(state["P1"]["pp"]["current"], 6_u8);
            assert_eq!(state["P1"]["evolve_deck"], json!(["decoy"]));
        } else {
            let before = game.projection(View::Referee).unwrap();
            game.activate_internal_card("source", ability, PLAYER)
                .unwrap_err();
            assert_eq!(game.projection(View::Referee).unwrap(), before);
        }
    }
}
