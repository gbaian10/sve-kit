use super::{board_fixtures, payment_fixtures};
use crate::battle::costs::project_costs;
use crate::game::Game;
use crate::game::View;
use crate::game::internal::{Answer, CardPlay, InputDomain, InstructionOutcome};
use crate::game::player::{Viewer, observation as native};
use crate::ir::lowering::ExpandedLowerer;
use crate::ir::{Bool, Cardinality, Nat, PlayerId};
use crate::ir::{WellFormedCardProgram, payment_expanded_tree};
use crate::{EngineFailure, Result as EngineResult};
use board_fixtures::exact_authority;
use native::{CostGroupKind, CostLine, PendingCost, PpQuote};
use serde_json::{Value, json};
use sve_battle_contract::{Int as WireInt, observation as wire};

fn optional_board(program: &WellFormedCardProgram, pp: i64) -> EngineResult<Game> {
    let mut initial = board_fixtures::setup(1, json!([]));
    *initial
        .pointer_mut("/players/P1/pp")
        .ok_or_else(optional_fixture_error)? = json!({"current":pp,"max":pp});
    *initial
        .pointer_mut("/players/P1/zones/hand")
        .ok_or_else(optional_fixture_error)? = json!([{"id":"s","card":"unit-follower"}]);
    let mut game = Game::new(
        board_fixtures::catalog()?,
        &initial,
        &Value::Null,
        &Value::Null,
        "optional-trigger",
    )?;
    game.load_internal_card("unit-follower", program)?;
    Ok(game)
}

fn optional_fixture_error() -> EngineFailure {
    EngineFailure::Invalid("incomplete optional payment fixture".into())
}

fn active(game: &Game) -> Option<native::CostSummary> {
    assert!(
        game.observe(Viewer::P2).ok()?.costs.current_play.is_none(),
        "the opponent cannot inspect a private play's cost state"
    );
    Some(game.observe(Viewer::P1).ok()?.costs.current_play?.cost)
}

fn optional(cost: &native::CostSummary) -> Option<&CostGroupKind> {
    cost.groups.iter().find_map(|group| {
        matches!(&group.kind, CostGroupKind::Optional { .. }).then_some(&group.kind)
    })
}

fn assert_wire_optional(game: &Game, accepted: Option<bool>) -> Option<()> {
    let projected = project_costs(&game.observe(Viewer::P1).ok()?)
        .ok()?
        .current_play?
        .cost;
    assert_eq!(
        projected.total_pp.map(WireInt::get),
        accepted.map(|accept| if accept { 2 } else { 0 }),
        "A declaration changes the quote before any optional payment commits"
    );
    if accepted.is_none() {
        assert_eq!(
            projected.pending,
            [wire::PendingCost::Optional],
            "Unanswered acceptance remains explicitly pending"
        );
    }
    assert!(
        projected.groups.iter().any(|group| group.role
            == wire::CostGroupRole::Optional {
                required: false,
                accepted,
            }),
        "The wire quote preserves optional declaration status"
    );
    Some(())
}

#[test]
fn x_pp_counts_repeated_payments_and_fixed_fees_and_uses_one_immutable_answer() {
    let expanded = payment_expanded_tree();
    let document = payment_fixtures::x_document(expanded, "activate",
        json!({"x":{},"cost":{"steps":[{"pp":2_i32},{"pp":"x"},{"pp":"x"},{"act_self":true}]},"do":[{"buff":{"on":"this","power":"x","hp":"x"}}]}),
    ).unwrap();
    let program = ExpandedLowerer::new()
        .unwrap()
        .lower(&document)
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    let mut game = payment_fixtures::x_board(&program, 7).unwrap();
    let CardPlay::Awaiting(question) = game
        .activate_internal_card(
            "s",
            program.card().abilities[0].node.id,
            PlayerId { seat: 1 },
        )
        .unwrap()
    else {
        panic!("expected X input");
    };
    assert_eq!(
        question.domain,
        InputDomain::Integer(Cardinality {
            minimum: Nat(0),
            maximum: Some(Nat(2))
        }),
        "X is capped by the complete PP payment, before targets"
    );
    let cost = active(&game).unwrap();
    let projected = project_costs(&game.observe(Viewer::P1).unwrap()).unwrap();
    let play = projected.current_play.unwrap();
    assert!(
        play.source.is_some(),
        "a holder still on the field is named"
    );
    let quoted = play.cost;
    assert_eq!(quoted.total_pp, None);
    assert_eq!(quoted.pending, [wire::PendingCost::X]);
    assert!(quoted.groups[0].lines.iter().any(|line| matches!(line,
        wire::CostLine::Pp { quote: wire::PpQuote::Variable { fixed, copies, x:None } }
        if fixed.get() == 2 && copies.get() == 2)));
    assert!(
        project_costs(&game.observe(Viewer::P2).unwrap())
            .unwrap()
            .current_play
            .is_none()
    );
    assert_eq!(cost.total_pp, None);
    assert_eq!(cost.pending, [PendingCost::X]);
    assert!(
        cost.groups[0]
            .lines
            .contains(&CostLine::Pp(PpQuote::Variable {
                fixed: 2,
                copies: 2,
                x: None
            })),
        "the quote explains fixed PP and the repeated X payments"
    );
    let before = exact_authority(&game);
    for (actor, value) in [
        (PlayerId { seat: 2 }, 1),
        (question.actor, 3),
        (question.actor, u64::MAX),
    ] {
        game.answer_internal_card(actor, question.token, Answer::Integer(Nat(value)))
            .unwrap_err();
        assert_eq!(
            exact_authority(&game),
            before,
            "invalid X keeps the original question and unspent resources"
        );
    }
    let mut stale = question.token;
    stale.epoch = stale.epoch.saturating_add(1);
    game.answer_internal_card(question.actor, stale, Answer::Integer(Nat(1)))
        .unwrap_err();
    assert_eq!(exact_authority(&game), before);
    assert!(matches!(
        game.answer_internal_card(question.actor, question.token, Answer::Integer(Nat(1)))
            .unwrap(),
        CardPlay::Resolved(_)
    ));
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(
        view["P1"]["pp"]["current"], 3_u8,
        "two fixed PP and two copies of the same X are paid"
    );
    assert_eq!(view["objects"]["s"]["power"], 3_u8);
    assert_eq!(view["objects"]["s"]["hp"], 4_u8);
    assert_eq!(view["objects"]["s"]["acted"], true);
}

#[test]
fn optional_trigger_declaration_precedes_targets_and_payment_is_atomic() {
    let expanded = payment_expanded_tree();
    let document = payment_fixtures::optional_document(expanded).unwrap();
    let programs = ExpandedLowerer::new().unwrap().lower(&document).unwrap();
    for (accept, pp) in [(false, 3_i64), (true, 3), (true, 2)] {
        let mut game = optional_board(&programs["unit-follower"], pp).unwrap();
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
        let quote = active(&game).unwrap();
        assert_wire_optional(&game, None).unwrap();
        assert_eq!(quote.total_pp, None);
        assert_eq!(quote.pending, [PendingCost::Optional]);
        assert_eq!(
            optional(&quote).unwrap(),
            &CostGroupKind::Optional {
                required: false,
                accepted: None
            }
        );
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
        let declared_quote = active(&game).unwrap();
        assert_wire_optional(&game, Some(accept)).unwrap();
        assert_eq!(declared_quote.total_pp, Some(if accept { 2 } else { 0 }));
        assert_eq!(
            optional(&declared_quote).unwrap(),
            &CostGroupKind::Optional {
                required: false,
                accepted: Some(accept)
            }
        );
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
            pp - 1
        );
        let result =
            game.answer_internal_card(target.actor, target.token, Answer::Targets(vec![0]));
        if accept && pp == 2 {
            result.unwrap_err();
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["P1"]["pp"]["current"], 1_i32);
            assert_eq!(state["objects"]["f"]["damage"], 0_i32);
        } else {
            let CardPlay::Resolved(result) = result.unwrap() else {
                panic!("expected complete ability");
            };
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
