#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate the scenario."
)]
use super::*;
use sve_engine::game::internal::DecisionView;

fn begin(number: &str, available: bool) -> (Game, DecisionView) {
    let mut initial = draw_position();
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":number}]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"ally","card":"BP01-037"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"first","card":"BP01-037"}, {"id":"second","card":"BP01-037"},
        {"id":"third","card":"BP01-037"}
    ]);
    if available {
        initial["players"]["P2"]["zones"]["field"] = json!([
            {"id":"enemy-a","card":"BP01-037"}, {"id":"enemy-b","card":"BP01-037"}
        ]);
    }
    initial["players"]["P2"]["zones"]["ex"] = json!([{"id":"off-zone","card":"BP01-037"}]);
    let mut game = game(&initial);
    assert!(
        matches!(
            game.play_internal_card("source", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ),
        "the real follower enters before its fanfare"
    );
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("fanfare");
    };
    assert_eq!(options.len(), 1, "only the printed fanfare is pending");
    assert_eq!(options[0].card, number, "the real printing triggers");
    let CardPlay::Awaiting(question) = game
        .answer_internal_card(
            pending.actor,
            pending.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap()
    else {
        panic!("mode declaration");
    };
    assert!(
        matches!(&question.domain, InputDomain::Modes { .. }),
        "mode is declared first"
    );
    (game, question)
}

fn choose(game: &mut Game, question: &DecisionView, mode: u64) {
    let before = game.projection(View::Referee).unwrap();
    game.answer_internal_card(
        PlayerId { seat: 2 },
        question.token,
        Answer::Modes(vec![ModeId(mode)]),
    )
    .unwrap_err();
    assert_eq!(
        game.projection(View::Referee).unwrap(),
        before,
        "wrong actor preserves state"
    );
    let mut play = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Modes(vec![ModeId(mode)]),
        )
        .unwrap();
    if let CardPlay::Awaiting(target) = play {
        let InputDomain::Targets(domain) = &target.domain else {
            panic!("selected mode's target");
        };
        assert_eq!(
            domain.members.len(),
            2,
            "only enemy field followers are candidates"
        );
        let declared = game.projection(View::Referee).unwrap();
        assert_eq!(
            declared["objects"]["enemy-a"]["damage"], 0_u8,
            "target precedes damage"
        );
        assert_eq!(declared["P1"]["hand"], json!([]), "target precedes drawing");
        play = game
            .answer_internal_card(
                target.actor,
                target.token,
                Answer::Targets(vec![domain.members[0].handle]),
            )
            .unwrap();
    }
    resolved(play);
    game.resume_internal_card().unwrap();
    assert!(
        game.internal_pending_choice().unwrap().is_none(),
        "the fanfare finishes exactly once"
    );
}

fn mode_game(number: &str, mode: u64) -> Game {
    let (mut game, question) = begin(number, true);
    choose(&mut game, &question, mode);
    game
}

#[test]
fn bp20_081_and_alias_choose_enemy_sweep_or_heal_and_two_draws() {
    for number in ["BP20-081", "BP20-P41"] {
        for mode in [1_u64, 2] {
            let game = mode_game(number, mode);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["P1"]["pp"]["current"], 4_u8);
            assert_eq!(state["objects"]["ally"]["damage"], 0_u8);
            for enemy in ["enemy-a", "enemy-b"] {
                assert_eq!(
                    state["objects"][enemy]["damage"],
                    if mode == 1 { 4_u8 } else { 0 }
                );
            }
            assert_eq!(
                state["P1"]["leader"]["life"],
                if mode == 1 { 10_u8 } else { 14 }
            );
            assert_eq!(
                state["P1"]["hand"],
                if mode == 1 {
                    json!([])
                } else {
                    json!(["first", "second"])
                }
            );
            assert_eq!(
                state["P1"]["deck"],
                if mode == 1 {
                    json!(["first", "second", "third"])
                } else {
                    json!(["third"])
                }
            );
        }
    }
}

#[test]
fn bp20_085_and_alias_choose_target_leader_damage_or_healing() {
    for number in ["BP20-085", "PR-554"] {
        for mode in [1_u64, 2, 3] {
            let game = mode_game(number, mode);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["P1"]["pp"]["current"], 8_u8);
            assert_eq!(
                state["P1"]["leader"]["life"],
                if mode == 3 { 12_u8 } else { 10 }
            );
            assert_eq!(
                state["P2"]["leader"]["life"],
                if mode == 2 { 18_u8 } else { 20 }
            );
            let mut damage =
                ["enemy-a", "enemy-b"].map(|id| state["objects"][id]["damage"].as_u64().unwrap());
            damage.sort_unstable();
            assert_eq!(damage, if mode == 1 { [0_u64, 1] } else { [0, 0] });
            assert_eq!(state["objects"]["ally"]["damage"], 0_u8);
            assert_eq!(state["P2"]["ex"], json!(["off-zone"]));
            assert_eq!(state["P1"]["hand"], json!([]));
        }
        let (mut game, question) = begin(number, false);
        let InputDomain::Modes { modes, .. } = &question.domain else {
            panic!("modes");
        };
        assert_eq!(modes, &vec![ModeId(2), ModeId(3)]);
        let before = game.projection(View::Referee).unwrap();
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Modes(vec![ModeId(1)]),
        )
        .unwrap_err();
        assert_eq!(game.projection(View::Referee).unwrap(), before);
        choose(&mut game, &question, 3);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
            12_u8
        );
    }
}

#[test]
fn bp14_114_chooses_one_lethal_target_or_three_damage_to_enemy_followers() {
    for mode in [1_u64, 2] {
        let game = mode_game("BP14-114", mode);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["pp"]["current"], 4_u8);
        assert_eq!(state["objects"]["ally"]["damage"], 0_u8);
        if mode == 1 {
            assert_eq!(state["P2"]["field"].as_array().unwrap().len(), 1);
            assert_eq!(state["P2"]["cemetery"].as_array().unwrap().len(), 1);
        } else {
            for id in ["enemy-a", "enemy-b"] {
                assert_eq!(state["objects"][id]["damage"], 3_u8);
            }
        }
        assert_eq!(state["P1"]["hand"], json!([]));
        assert_eq!(state["P1"]["leader"]["life"], 10_u8);
    }
}
