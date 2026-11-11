#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate the X payment scenarios."
)]
use super::*;
use alloc::collections::BTreeMap;
use sve_engine::game::internal::{DecisionView, InputDomain};
use sve_engine::game::player::Viewer;
use sve_engine::game::player::observation::{CostLine, PendingCost, PpQuote};

fn tree(head: &str, body: Value) -> Value {
    let mut tree = fanfare::expanded::tree().clone();
    let prototype = tree["cards"]["BP01-041"].clone();
    let mut entry = prototype["abilities"][0].clone();
    entry.as_object_mut().unwrap().remove("activate");
    entry[head] = body;
    tree["cards"] = json!({"unit-follower":{"meta":prototype["meta"],"abilities":[entry]}});
    tree
}
fn program(body: Value) -> WellFormedCardProgram {
    lowerer()
        .lower(&tree("activate", body))
        .unwrap()
        .remove("unit-follower")
        .unwrap()
}
fn board(program: &WellFormedCardProgram, pp: u8) -> Game {
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["pp"] = json!({"current":pp,"max":pp});
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
    let mut game = Game::new(catalog(), &initial, &Value::Null, &Value::Null, "X-PP").unwrap();
    game.load_internal_card("unit-follower", program).unwrap();
    game
}
fn awaiting(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(question) = play else {
        panic!("expected X input");
    };
    question
}
fn start(game: &mut Game, program: &WellFormedCardProgram) -> DecisionView {
    awaiting(
        game.activate_internal_card(
            "s",
            program.card().abilities[0].node.id,
            PlayerId { seat: 1 },
        )
        .unwrap(),
    )
}
fn bounds(question: &DecisionView, maximum: u64) {
    assert_eq!(
        question.domain,
        InputDomain::Integer(Cardinality {
            minimum: Nat(0),
            maximum: Some(Nat(maximum))
        }),
        "X is capped by the complete PP payment, before targets"
    );
}

#[test]
fn x_pp_counts_repeated_payments_and_fixed_fees_and_uses_one_immutable_answer() {
    let program = program(
        json!({"x":{},"cost":{"steps":[{"pp":2},{"pp":"x"},{"pp":"x"},{"act_self":true}]},"do":[{"buff":{"on":"this","power":"x","hp":"x"}}]}),
    );
    let mut game = board(&program, 7);
    let question = start(&mut game, &program);
    bounds(&question, 2);
    let cost = cost_observation::active(&game);
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
    resolved(
        game.answer_internal_card(question.actor, question.token, Answer::Integer(Nat(1)))
            .unwrap(),
    );
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
fn x_pp_zero_is_legal_and_does_not_read_maximum_pp_as_the_answer() {
    let program = program(
        json!({"x":{},"cost":{"pp":"x"},"do":[{"buff":{"on":"this","power":"x","hp":"x"}}]}),
    );
    let mut game = board(&program, 0);
    let question = start(&mut game, &program);
    bounds(&question, 0);
    resolved(
        game.answer_internal_card(question.actor, question.token, Answer::Integer(Nat(0)))
            .unwrap(),
    );
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["pp"]["current"], 0_u8);
    assert_eq!(view["objects"]["s"]["power"], 2_u8);
    assert_eq!(view["objects"]["s"]["hp"], 3_u8);
}

#[test]
fn x_pp_binds_before_cost_filtered_targets_and_impossible_targets_restore_the_whole_play() {
    let program = program(
        json!({"x":{},"cost":{"steps":[{"pp":"x"},{"act_self":true}]},
        "targets":[{"bind":"enemy","from":{"who":"opp","zone":"field","kind":"follower","orig_cost_le":"x"},"n":1}],
        "do":[{"damage":{"to":"enemy","n":"x"}}]}),
    );
    let mut rejected = board(&program, 3);
    let origin = rejected.clone();
    let question = start(&mut rejected, &program);
    rejected
        .answer_internal_card(question.actor, question.token, Answer::Integer(Nat(0)))
        .unwrap_err();
    transactions::assert_restored(&rejected, &origin);
    let mut game = board(&program, 3);
    let question = start(&mut game, &program);
    bounds(&question, 3);
    let target = awaiting(
        game.answer_internal_card(question.actor, question.token, Answer::Integer(Nat(1)))
            .unwrap(),
    );
    assert!(
        matches!(&target.domain, InputDomain::Targets(domain) if domain.members.len()==1),
        "target cost limits see the chosen X"
    );
    let quoted = cost_observation::active(&game);
    assert_eq!(quoted.total_pp, Some(1));
    assert_eq!(quoted.pending, []);
    let before = exact_authority(&game);
    game.answer_internal_card(target.actor, target.token, Answer::Targets(vec![999]))
        .unwrap_err();
    assert_eq!(exact_authority(&game), before);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        3_u8,
        "target selection precedes every payment"
    );
    resolved(
        game.answer_internal_card(target.actor, target.token, Answer::Targets(vec![0]))
            .unwrap(),
    );
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["pp"]["current"], 2_u8);
    assert_eq!(view["objects"]["f"]["hp"], 2_u8);
    assert_eq!(view["objects"]["s"]["acted"], true);
}

#[test]
fn x_pp_spell_uses_the_discounted_intrinsic_quote_and_declining_does_not_ask_for_x() {
    let mut tree = tree(
        "spell",
        json!({"x":{},"optional_cost":{"pp":"x","result":"paid"},"do":[{"if":{"cond":{"paid":{"receipt":"paid"}},"then":[{"life":{"who":"self","delta":"x"}}]}}]}),
    );
    let mut printed = tree["cards"]["unit-follower"].take();
    let mut header = printed["abilities"][0].clone();
    header.as_object_mut().unwrap().remove("spell");
    header["line"] = json!(2_u8);
    header["play"] = json!({"cost_delta":-2_i64});
    printed["abilities"].as_array_mut().unwrap().push(header);
    tree["cards"] = json!({"unit-spell":printed});
    let program = lowerer()
        .lower_with_printed_costs(&tree, &BTreeMap::from([("unit-spell".into(), Nat(3))]))
        .unwrap()
        .remove("unit-spell")
        .unwrap();
    for accepted in [true, false] {
        let mut initial = setup(1, json!([]));
        initial["players"]["P1"]["pp"] = json!({"current":5_u8,"max":5_u8});
        let mut game = Game::new(
            catalog_with_cost(&json!("3")),
            &initial,
            &Value::Null,
            &Value::Null,
            "X-PP-spell",
        )
        .unwrap();
        game.load_internal_card("unit-spell", &program).unwrap();
        game.load_internal_card("unit-follower", &card(vec![]))
            .unwrap();
        let preview = cost_observation::preview(&game, Viewer::P1);
        assert_eq!(preview.cost.total_pp, None);
        assert_eq!(preview.cost.pending, [PendingCost::Optional]);
        let declare = awaiting(game.play_internal_card("s", PlayerId { seat: 1 }).unwrap());
        assert_eq!(declare.domain, InputDomain::OptionalCost);
        let play = game
            .answer_internal_card(declare.actor, declare.token, Answer::OptionalCost(accepted))
            .unwrap();
        let play = if accepted {
            let question = awaiting(play);
            bounds(&question, 4);
            let quote = cost_observation::active(&game);
            assert_eq!(quote.total_pp, None);
            assert_eq!(quote.pending, [PendingCost::X]);
            game.answer_internal_card(question.actor, question.token, Answer::Integer(Nat(4)))
                .unwrap()
        } else {
            play
        };
        resolved(play);
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(
            view["P1"]["pp"]["current"],
            if accepted { 0_u8 } else { 4 },
            "X reserves the quoted intrinsic cost exactly once"
        );
        assert_eq!(
            view["P1"]["leader"]["life"],
            if accepted { 24_u8 } else { 20 }
        );
    }
}

#[test]
fn x_pp_rejects_undeclared_unused_bounded_and_shadowed_declarations() {
    for body in [
        json!({"cost":{"pp":"x"},"do":[]}),
        json!({"x":{},"cost":{"pp":1},"do":[]}),
        json!({"x":{"min":1},"cost":{"pp":"x"},"do":[]}),
        json!({"x":{"max":3},"cost":{"pp":"x"},"do":[]}),
        json!({"x":{"from_payment":"paid"},"cost":{"pp":"x"},"do":[]}),
        json!({"x":[],"cost":{"pp":"x"},"do":[]}),
        json!({"x":{},"define":{"x":1},"cost":{"pp":"x"},"do":[]}),
        json!({"x":{},"cost":{"pp":"y"},"do":[]}),
        json!({"x":{},"cost":{"pp":"x"},"do":[{"choice":{"n":1,"options":[[{"draw":{"n":1}}],[{"life":{"who":"self","delta":1}}]]}}]}),
        json!({"x":{},"cost":{"pp":"x"},"targets":[{"bind":"x","from":{"who":"opp","zone":"field"},"n":1}],"do":[]}),
    ] {
        lowerer().lower(&tree("activate", body)).unwrap_err();
    }
    let quick = lowerer()
        .lower(&tree(
            "activate",
            json!({"x":{},"quick":true,"cost":{"pp":"x"},"do":[]}),
        ))
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    let mut quick_game = board(&card(vec![]), 3);
    quick_game
        .load_internal_card("unit-follower", &quick)
        .unwrap_err();
    let valid = program(json!({"x":{},"cost":{"pp":"x"},"do":[]}));
    let mut raw = valid.card().clone();
    let ChoiceDomain::Integer { cardinality, .. } =
        &mut raw.abilities[0].node.play.as_mut().unwrap().parameters[0].domain
    else {
        panic!("X integer");
    };
    cardinality.maximum = Some(Nat(100));
    let forged = WellFormedCardProgram::check(raw).unwrap();
    let mut game = Game::new(
        catalog(),
        &setup(1, json!([])),
        &Value::Null,
        &Value::Null,
        "forged-X",
    )
    .unwrap();
    game.load_internal_card("unit-follower", &forged)
        .unwrap_err();
}

#[test]
fn x_pp_unaffordable_fixed_part_cannot_be_hidden_in_the_x_range() {
    let program =
        program(json!({"x":{},"cost":{"steps":[{"pp":4},{"pp":"x"},{"act_self":true}]},"do":[]}));
    let mut game = board(&program, 3);
    let origin = game.clone();
    game.activate_internal_card(
        "s",
        program.card().abilities[0].node.id,
        PlayerId { seat: 1 },
    )
    .unwrap_err();
    transactions::assert_restored(&game, &origin);
}
