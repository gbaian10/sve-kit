use super::*;
use sve_engine::game::internal::InputDomain;

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
    let programs = lowerer().lower(&tree()).unwrap();
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

#[test]
fn optional_declaration_rejects_activation_and_exchange() {
    let mut activation = tree();
    let entry = &mut activation["cards"]["unit-follower"]["abilities"][0];
    let body = entry.as_object_mut().unwrap().remove("fanfare").unwrap();
    entry["activate"] = body;
    entry["activate"]["cost"] = json!({"pp":0});
    lowerer().lower(&activation).unwrap_err();
    let mut tree = tree();
    let mut exchange = tree["cards"]["unit-follower"]["abilities"][0].clone();
    exchange.as_object_mut().unwrap().remove("fanfare");
    exchange["play"] = json!({"optional_cost":{"to_cemetery":{"sel":{"who":"self","zone":"field","kind":"follower"},"n":1}},"optional_cost_delta":-1});
    tree["cards"]["unit-follower"]["abilities"]
        .as_array_mut()
        .unwrap()
        .push(exchange);
    lowerer().lower(&tree).unwrap_err();
}
