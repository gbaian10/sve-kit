use super::*;

fn position(printing: &str, seat: &str, pp: u8) -> Value {
    let mut initial = draw_position();
    initial["turn"]["active"] = json!(seat);
    initial["players"][seat]["pp"]["current"] = json!(pp);
    initial["players"][seat]["leader"]["life"] = json!(17_u8);
    initial["players"][seat]["zones"]["ex"] = json!([{"id":"blast","card":printing}]);
    for player in ["P1", "P2"] {
        initial["players"][player]["zones"]["deck"] = json!([
            {"id":format!("{player}-top"),"card":"BP01-037"},
            {"id":format!("{player}-next"),"card":"BP01-037"}
        ]);
    }
    initial
}

#[test]
fn bp13_039_and_sl11_pay_three_hit_only_opposing_leaders_and_return_face_up() {
    for (seat, printing) in [(1_u32, "BP13-039"), (2, "BP13-SL11")] {
        let owner = if seat == 1 { "P1" } else { "P2" };
        let opponent = if seat == 1 { "P2" } else { "P1" };
        let mut game = game(&position(printing, owner, 3));
        let result = resolved(game.play_internal_card("blast", PlayerId { seat }).unwrap());
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state[owner]["pp"]["current"], 0_u8);
        assert_eq!(state[owner]["leader"]["life"], 17_u8);
        assert_eq!(state[opponent]["leader"]["life"], 12_u8);
        assert_eq!(state[owner]["ex"], json!([]));
        assert_eq!(state[owner]["cemetery"], json!([]));
        assert_eq!(state[owner]["resolution"], json!([]));
        assert_eq!(state[owner]["evolve_deck"], json!(["blast"]));
        assert_eq!(state["objects"]["blast"]["card"], printing);
        assert_eq!(state["objects"]["blast"]["face_up"], true);
        assert_eq!(state["objects"]["blast"]["generation"], 3_u8);
        assert!(
            result
                .progress
                .facts
                .iter()
                .any(|fact| matches!(fact, Fact::CardResolved { .. }))
        );
        game.resume_internal_card().unwrap();
        let before = format!("{game:?}");
        game.play_internal_card("blast", PlayerId { seat })
            .unwrap_err();
        assert_eq!(format!("{game:?}"), before);
    }
}

#[test]
fn bp13_039_cannot_spend_less_than_its_printed_cost() {
    let mut game = game(&position("BP13-039", "P1", 2));
    let before = format!("{game:?}");
    game.play_internal_card("blast", PlayerId { seat: 1 })
        .unwrap_err();
    assert_eq!(format!("{game:?}"), before);
}

#[test]
fn bp13_039_does_not_gain_quick_from_its_card_kind() {
    let mut initial = position("BP13-039", "P1", 3);
    initial["turn"]["active"] = json!("P2");
    let mut game = game(&initial);
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    let question = game.internal_quick_choice().unwrap().unwrap();
    let InputDomain::Quick { options, .. } = &question.domain else {
        panic!("Quick authority window");
    };
    assert_eq!(options.as_slice(), []);
}
