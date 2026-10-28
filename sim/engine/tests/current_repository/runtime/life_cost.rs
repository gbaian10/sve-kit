use super::*;
use sve_engine::game::internal::InstructionOutcome;

#[test]
fn bp08_084_pays_life_before_damage_and_stops_after_two_activations() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"payer","card":"BP08-084"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    let mut game = game(&initial);
    for remaining in [19_u8, 18] {
        let CardPlay::Awaiting(question) = game
            .activate_internal_card("payer", AbilityId(1), PlayerId { seat: 1 })
            .unwrap()
        else {
            panic!("play target");
        };
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
            remaining + 1
        );
        let result = resolved(
            game.answer_internal_card(question.actor, question.token, Answer::Targets(vec![0]))
                .unwrap(),
        );
        assert!(result.progress.receipts.values().any(|receipt| matches!(receipt,
            InstructionOutcome::Payment { quantity: Quantity { amount: Int(1), unit: QuantityUnit::Life }, subjects } if subjects.is_empty())));
        assert!(
            matches!(result.progress.facts.as_slice(), [Fact::LifeChange(life), Fact::AbilityPlayed { .. }, Fact::Damage(_), Fact::AbilityResolved { .. }] if life.delta == Int(-1))
        );
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["leader"]["life"], remaining);
        assert_eq!(state["P2"]["leader"]["life"], 20_u8);
        assert_eq!(state["P1"]["pp"]["current"], 10_u8);
        assert_eq!(state["objects"]["enemy"]["damage"], 20_u8 - remaining);
        game.resume_internal_card().unwrap();
    }
    let before = format!("{game:?}");
    game.activate_internal_card("payer", AbilityId(1), PlayerId { seat: 1 })
        .unwrap_err();
    assert!(
        format!("{game:?}") == before,
        "third activation cannot pay life"
    );
}
