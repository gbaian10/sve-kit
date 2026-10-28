use super::*;
use sve_engine::game::internal::InputDomain;
use sve_engine::ir::lowering::ExpandedLowerer;

fn tree() -> Value {
    let mut tree = fanfare::expanded::tree().clone();
    let mut entry = tree["cards"]["BP08-038"]["abilities"][0].clone();
    entry["fanfare"] = json!({"optional_cost":{"pp":2,"result":"paid"},
        "targets":[{"bind":"enemy","from":{"who":"opp","zone":"field","kind":"follower"},"n":1}],
        "do":[{"if":{"cond":{"paid":{"receipt":"paid"}},"then":[{"damage":{"to":"enemy","n":1}}]}}]});
    let meta = tree["cards"]["BP08-038"]["meta"].clone();
    tree["cards"] = json!({"unit-follower":{"meta":meta,"abilities":[entry]}});
    tree
}

#[test]
fn optional_trigger_declaration_precedes_targets_and_payment_is_atomic() {
    let programs = ExpandedLowerer::new().unwrap().lower(&tree()).unwrap();
    for (accept, pp) in [(false, 3_i64), (true, 3), (true, 2)] {
        let mut initial = setup(1, json!([]));
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"s","card":"unit-follower"}]);
        initial["players"]["P1"]["pp"] = json!({"current":pp,"max":pp});
        let mut game = Game::new(
            catalog(),
            &initial,
            &Value::Null,
            &Value::Null,
            "optional-trigger",
        )
        .unwrap();
        game.load_internal_card("unit-follower", &programs["unit-follower"])
            .unwrap();
        assert!(matches!(
            game.play_internal_card("s", PlayerId { seat: 1 }).unwrap(),
            CardPlay::Entered(_)
        ));
        let pending = game.internal_pending_choice().unwrap().unwrap();
        let CardPlay::Awaiting(declare) = game
            .answer_internal_card(pending.actor, pending.token, Answer::Pending(1))
            .unwrap()
        else {
            panic!("declaration");
        };
        assert_eq!(declare.domain, InputDomain::OptionalCost);
        let before = exact_authority(&game);
        game.answer_internal_card(
            PlayerId { seat: 2 },
            declare.token,
            Answer::OptionalCost(accept),
        )
        .unwrap_err();
        assert_eq!(before, exact_authority(&game));
        let CardPlay::Awaiting(target) = game
            .answer_internal_card(declare.actor, declare.token, Answer::OptionalCost(accept))
            .unwrap()
        else {
            panic!("target before payment");
        };
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
            pp - 1
        );
        let result =
            game.answer_internal_card(target.actor, target.token, Answer::Targets(vec![0]));
        if accept && pp == 2 {
            result.unwrap_err();
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["P1"]["pp"]["current"], 1);
            assert_eq!(state["objects"]["f"]["damage"], 0);
        } else {
            let result = resolved(result.unwrap());
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["P1"]["pp"]["current"],
                pp - 1 - if accept { 2 } else { 0 }
            );
            assert_eq!(state["objects"]["f"]["damage"], u8::from(accept));
            assert!(result.progress.receipts.values().any(|receipt| matches!(receipt, InstructionOutcome::ExchangePayment(payment) if payment.complete == Bool(accept))));
        }
    }
}
