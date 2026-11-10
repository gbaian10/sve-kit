#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected choices invalidate the printed repeat scenarios."
)]
use super::*;
use sve_engine::game::internal::SearchAnswer;

const PLAYER: PlayerId = PlayerId { seat: 1 };

fn initial(card: &str, occupied: usize, deck: &[&str]) -> Value {
    let mut state = draw_position();
    state["players"]["P1"]["leader"]["life"] = json!(10_u8);
    state["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    state["players"]["P1"]["zones"]["ex"] = json!(
        (0..occupied)
            .map(|n| json!({"id":format!("ex{n}"),"card":"BP01-171"}))
            .collect::<Vec<_>>()
    );
    state["players"]["P1"]["zones"]["deck"] = json!(
        deck.iter()
            .enumerate()
            .map(|(n, number)| json!({"id":format!("d{n}"),"card":number}))
            .collect::<Vec<_>>()
    );
    state
}

fn finish(game: &mut Game, mut play: CardPlay, mode: ModeId) {
    while let CardPlay::Awaiting(question) = play {
        let answer = match &question.domain {
            InputDomain::Modes { .. } => Answer::Modes(vec![mode]),
            InputDomain::OptionalCost => Answer::OptionalCost(true),
            InputDomain::Integer(_) => Answer::Integer(Nat(1)),
            InputDomain::Search(domain) => {
                assert_eq!(
                    domain.cards.len(),
                    1,
                    "only the named dependency is offered"
                );
                Answer::Search(SearchAnswer::Ungrouped(
                    domain.cards.iter().map(|card| card.handle).collect(),
                ))
            }
            InputDomain::Targets(domain) => {
                assert!(
                    domain.members.len() <= 1,
                    "the printed target or named search has at most one candidate"
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
            InputDomain::CostMaterials(domain) => {
                assert_eq!(
                    domain.members.len(),
                    1,
                    "only the eligible hand fuel is offered"
                );
                Answer::CostMaterials(domain.members.iter().map(|member| member.handle).collect())
            }
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            _ => panic!("unexpected repeat decision"),
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    assert!(
        matches!(play, CardPlay::Resolved(_) | CardPlay::Entered(_)),
        "the printed action finishes"
    );
}

fn settle(game: &mut Game, mode: ModeId) {
    loop {
        while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
        let Some(question) = game.internal_pending_choice().unwrap() else {
            break;
        };
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
        finish(game, play, mode);
    }
}

#[test]
fn disdrestan_all_printings_pay_a_marine_card_and_repeat_only_into_available_ex_slots() {
    for card in ["BP17-057", "BP17-SL15"] {
        for occupied in [0_usize, 3, 5] {
            let mut position = initial(card, occupied, &["BP01-171"; 4]);
            position["players"]["P1"]["zones"]["hand"]
                .as_array_mut()
                .unwrap()
                .extend([
                    json!({"id":"fuel","card":"BP17-057"}),
                    json!({"id":"wrong","card":"BP01-171"}),
                ]);
            let mut game = game(&position);
            let play = game.play_internal_card("source", PLAYER).unwrap();
            finish(&mut game, play, ModeId(1));
            settle(&mut game, ModeId(1));
            let state = game.projection(View::Referee).unwrap();
            let moved = 3.min(5 - occupied);
            assert_eq!(
                state["P1"]["ex"].as_array().unwrap().len(),
                occupied + moved
            );
            assert_eq!(
                state["P1"]["deck"],
                json!((moved..4).map(|n| format!("d{n}")).collect::<Vec<_>>())
            );
            assert_eq!(state["P1"]["hand"], json!(["wrong"]));
            assert_eq!(state["P1"]["cemetery"], json!(["fuel"]));
            assert_eq!(state["P1"]["pp"]["current"], 5_u8);
        }
    }
}

#[test]
fn cetus_all_printings_read_only_each_iterations_successful_beast_arrival() {
    for card in ["BP17-004", "BP17-SL04"] {
        for (deck, occupied, heal) in [
            (["BP17-004", "BP17-004"], 0_usize, 4_u8),
            (["BP01-171", "BP17-004"], 0, 2),
            (["BP17-004", "BP17-004"], 4, 2),
            (["BP17-004", "BP17-004"], 5, 0),
        ] {
            let position = initial(card, occupied, &deck);
            let mut game = game(&position);
            let play = game.play_internal_card("source", PLAYER).unwrap();
            finish(&mut game, play, ModeId(1));
            settle(&mut game, ModeId(1));
            let state = game.projection(View::Referee).unwrap();
            let moved = 2.min(5 - occupied);
            assert_eq!(state["P1"]["leader"]["life"], 10 + heal);
            assert_eq!(
                state["P1"]["deck"],
                json!((moved..2).map(|n| format!("d{n}")).collect::<Vec<_>>())
            );
            assert_eq!(
                state["P1"]["ex"].as_array().unwrap().len(),
                occupied + moved
            );
            assert_eq!(
                state["P1"]["pp"]["current"], 6_u8,
                "playing Cetus itself does not trigger its PP recovery (Q2220)"
            );
        }
    }
}

#[test]
fn cetus_recovers_pp_for_only_the_first_own_beast_play_in_the_turn() {
    let mut position = initial("BP17-004", 0, &["BP01-171"; 4]);
    position["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP17-004"}]);
    position["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"wrong","card":"BP01-171"}, {"id":"one","card":"BP17-095"},{"id":"two","card":"BP17-095"}
    ]);
    let mut game = game(&position);
    for (id, pp) in [("wrong", 9_u8), ("one", 9), ("two", 8)] {
        let play = game.play_internal_card(id, PLAYER).unwrap();
        finish(&mut game, play, ModeId(1));
        settle(&mut game, ModeId(1));
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
            pp
        );
    }
}

#[test]
fn king_halo_all_printings_can_choose_the_repeat_mode_even_with_full_ex() {
    for card in ["CP01-045", "CP01-P33"] {
        for occupied in [0_usize, 3, 5] {
            let position = initial(card, occupied, &["BP01-171"; 4]);
            let mut game = game(&position);
            let play = game.play_internal_card("source", PLAYER).unwrap();
            finish(&mut game, play, ModeId(1));
            settle(&mut game, ModeId(1));
            let state = game.projection(View::Referee).unwrap();
            let moved = 3.min(5 - occupied);
            assert_eq!(
                state["P1"]["ex"].as_array().unwrap().len(),
                occupied + moved
            );
            assert_eq!(
                state["P1"]["deck"],
                json!((moved..4).map(|n| format!("d{n}")).collect::<Vec<_>>())
            );
            assert_eq!(state["P1"]["pp"]["current"], 4_u8);
        }
    }
}

#[test]
fn king_halos_other_mode_searches_the_named_complete_dependency_and_triggers_its_fanfare() {
    let mut position = initial("CP01-045", 0, &["BP01-171", "CP01-036", "BP01-171"]);
    position["players"]["P1"]["zones"]["hand"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"fuel","card":"BP01-171"}));
    position["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-080"}]);
    let mut game = game(&position);
    let play = game.play_internal_card("source", PLAYER).unwrap();
    finish(&mut game, play, ModeId(2));
    settle(&mut game, ModeId(2));
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["field"], json!(["source", "d1"]));
    assert_eq!(state["objects"]["enemy"]["damage"], 4_u8);
    assert_eq!(state["P1"]["hand"], json!([]));
    assert_eq!(
        state["P1"]["deck"].as_array().unwrap().len(),
        2,
        "only the named dependency leaves the deck"
    );
    assert_eq!(state["P1"]["pp"]["current"], 4_u8);
}
