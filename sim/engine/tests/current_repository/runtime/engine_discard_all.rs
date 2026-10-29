#![expect(
    clippy::panic,
    reason = "An unexpected pending domain means the card did not reach its printed trigger."
)]

use super::*;

const fn actor() -> PlayerId {
    PlayerId { seat: 1 }
}

fn play_and_resolve_fanfare(game: &mut Game, printing: &str) {
    assert!(matches!(
        game.play_internal_card("source", actor()).unwrap(),
        CardPlay::Entered(_)
    ), "the played card should enter before its Fanfare resolves");
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("expected the played card's Fanfare");
    };
    assert_eq!(options.len(), 1, "one printed Fanfare should be pending");
    assert_eq!(options[0].card, printing, "the pending trigger uses this printing");
    assert!(matches!(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap(),
        CardPlay::Resolved(_)
    ), "the selected Fanfare should resolve");
    game.resume_internal_card().unwrap();
}

#[test]
fn bp02_052_and_printed_alias_damage_by_hand_count_then_discard_the_whole_hand() {
    for printing in ["BP02-052", "BP02-SL10"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["hand"] = json!([
            {"id":"source","card":printing},
            {"id":"discard-a","card":"BP01-037"},
            {"id":"discard-b","card":"BP01-037"}
        ]);
        initial["players"]["P2"]["zones"]["field"] = json!([
            {"id":"enemy-a","card":"BP01-037"},
            {"id":"enemy-b","card":"BP01-037"}
        ]);
        let mut game = game(&initial);

        play_and_resolve_fanfare(&mut game, printing);

        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P2"]["leader"]["life"], 18_u8, "damage equals the two-card hand");
        for enemy in ["enemy-a", "enemy-b"] {
            assert_eq!(state["objects"][enemy]["damage"], 2_u8, "each enemy follower takes hand-count damage");
        }
        assert_eq!(state["P2"]["field"], json!(["enemy-a", "enemy-b"]), "damage does not destroy by itself");
        assert_eq!(state["P2"]["cemetery"], json!([]), "enemy followers remain out of the cemetery");
        assert_eq!(state["P1"]["hand"], json!([]), "all remaining cards leave hand");
        assert_eq!(state["P1"]["cemetery"], json!(["discard-a", "discard-b"]), "the whole hand is discarded");
    }
}

#[test]
fn bp02_053_evolved_printings_draw_three() {
    for printing in ["BP02-053", "BP02-SL11", "BP02-U04"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"base","card":"BP02-052"}]);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":printing}]);
        initial["players"]["P1"]["zones"]["deck"] = json!([
            {"id":"draw-a","card":"BP01-037"},
            {"id":"draw-b","card":"BP01-037"},
            {"id":"draw-c","card":"BP01-037"}
        ]);
        let mut game = game(&initial);

        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"evolve","source":"base","evolve_card":"form","pay":{"pp":1_i32}})
            )
            .unwrap()
            .outcome,
            "resolved"
        );
        let question = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = &question.domain else {
            panic!("expected the evolved card's trigger");
        };
        assert_eq!(options.len(), 1, "one evolve-time trigger should be pending");
        assert_eq!(options[0].card, printing, "the evolved trigger uses this printing");
        assert!(matches!(
            game.answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap(),
            CardPlay::Resolved(_)
        ), "the evolved trigger should resolve");
        game.resume_internal_card().unwrap();

        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["hand"], json!(["draw-a", "draw-b", "draw-c"]), "each evolved printing draws three");
        assert_eq!(state["P1"]["deck"], json!([]), "the three-card deck is exhausted");
    }
}
