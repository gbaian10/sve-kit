use super::*;
use sve_engine::game::internal::DecisionView;
use sve_engine::game::player::{
    Command, Input, MainActionAnswer, MainIntentKind, QuestionShape, SubmissionKind, Viewer,
};

fn position(pp: u8) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["pp"]["current"] = json!(pp);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP17-030"}]);
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"riord","card":"BP17-022","state":{"damage":1_u8}},
        {"id":"other","card":"BP17-035"},
        {"id":"not_assassin","card":"BP01-037"}
    ]);
    initial
}
fn build(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "play-target-cost",
    )
    .unwrap()
}
fn question(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(question) = play else {
        panic!("formal target question")
    };
    question
}
fn choose(game: &mut Game, question: &DecisionView, card: &str) -> sve_engine::Result<CardPlay> {
    let InputDomain::Targets(domain) = &question.domain else {
        return Err(EngineFailure::Invalid("expected object targets".into()));
    };
    let member = domain
        .members
        .iter()
        .find(|member| member.card.as_deref() == Some(card))
        .unwrap();
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::Targets(vec![member.handle]),
    )
}
#[test]
fn hidden_memory_prices_the_actual_selected_assassin_and_resets_its_ex_generation() {
    for (selected, pp, expected, moved) in [
        ("BP17-022", 0, 0_u8, "riord"),
        ("BP17-022", 2, 2_u8, "riord"),
        ("BP17-035", 2, 0_u8, "other"),
    ] {
        let mut game = build(&position(pp));
        let before = game.projection(View::Referee).unwrap();
        let play = game
            .play_internal_card("source", PlayerId { seat: 1 })
            .unwrap();
        let q = question(play);
        let InputDomain::Targets(domain) = &q.domain else {
            panic!("formal targets")
        };
        assert_eq!(domain.members.len(), 2);
        let unchanged = game.projection(View::Referee).unwrap();
        for answer in [
            vec![],
            vec![99],
            vec![domain.members[0].handle, domain.members[1].handle],
        ] {
            game.answer_internal_card(q.actor, q.token, Answer::Targets(answer))
                .unwrap_err();
            assert_eq!(game.projection(View::Referee).unwrap(), unchanged);
        }
        resolved(choose(&mut game, &q, selected).unwrap());
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["pp"]["current"], expected);
        assert_eq!(state["P1"]["ex"], json!([moved]));
        assert_eq!(
            state["objects"][moved]["generation"].as_u64().unwrap(),
            before["objects"][moved]["generation"].as_u64().unwrap() + 1
        );
        assert_eq!(state["objects"][moved]["damage"], 0_u8);
        assert_eq!(state["P1"]["cemetery"], json!(["source"]));
    }
}
#[test]
fn hidden_memory_full_ex_still_selects_and_pays_without_moving_the_target() {
    let mut initial = position(0);
    initial["players"]["P1"]["zones"]["ex"] = json!([
        {"id":"ex0","card":"BP01-037"},{"id":"ex1","card":"BP01-037"},{"id":"ex2","card":"BP01-037"},{"id":"ex3","card":"BP01-037"},{"id":"ex4","card":"BP01-037"}
    ]);
    let mut game = build(&initial);
    let play = game
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap();
    let q = question(play);
    resolved(choose(&mut game, &q, "BP17-022").unwrap());
    let state = game.projection(View::Referee).unwrap();
    assert!(
        state["P1"]["field"]
            .as_array()
            .unwrap()
            .contains(&json!("riord"))
    );
    assert_eq!(state["objects"]["riord"]["generation"], 0_u8);
    assert_eq!(state["objects"]["riord"]["damage"], 1_u8);
    assert_eq!(state["P1"]["pp"]["current"], 0_u8);
}
#[test]
fn hidden_memory_cannot_borrow_an_unselected_riord_for_payment() {
    let mut game = build(&position(0));
    let before = game.projection(View::Referee).unwrap();
    let play = game
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap();
    let q = question(play);
    choose(&mut game, &q, "BP17-035").unwrap_err();
    assert_eq!(game.projection(View::Referee).unwrap(), before);
    let retry_play = game
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap();
    let retry_question = question(retry_play);
    resolved(choose(&mut game, &retry_question, "BP17-022").unwrap());

    let mut initial = position(0);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"other","card":"BP17-035"}]);
    let mut paid_game = build(&initial);
    paid_game
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap_err();
    assert_eq!(
        paid_game.projection(View::Referee).unwrap()["P1"]["hand"],
        json!(["source"])
    );
}

#[test]
fn main_menu_checks_the_target_price_without_spending_or_revealing_the_spell() {
    let mut initial = position(0);
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"other","card":"BP17-035"},{"id":"riord","card":"BP17-022"}]);
    let mut game = build(&initial);
    game.advance_player_input();
    let snapshot = game.projection(View::Referee).unwrap();
    let Input::Question(main) = game.input(Viewer::P1) else {
        panic!("main question");
    };
    let QuestionShape::ActionWindow(actions) = &main.shape else {
        panic!("main action window");
    };
    let intent = actions.intents.iter().find(|intent| {
        matches!(
            &intent.kind,
            MainIntentKind::Play { card } if card.properties.card == "BP17-030"
        )
    }).unwrap();
    assert_eq!(game.projection(View::Referee).unwrap(), snapshot);
    assert_eq!(game.input(Viewer::P1), Input::Question(main.clone()));
    assert_eq!(
        game.submit_player(Viewer::P1, Command::MainAction {
            token: main.token,
            intent: intent.id.clone(),
            answer: MainActionAnswer::None,
        }).kind,
        SubmissionKind::Applied
    );

    let target = game.internal_decision(View::P1).unwrap();
    resolved(choose(&mut game, &target, "BP17-022").unwrap());
    assert_eq!(game.projection(View::Referee).unwrap()["P1"]["cemetery"], json!(["source"]));

    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"other","card":"BP17-035"}]);
    let mut only_expensive = build(&initial);
    only_expensive.advance_player_input();
    let Input::Question(unpaid_question) = only_expensive.input(Viewer::P1) else {
        panic!("main question");
    };
    let QuestionShape::ActionWindow(unpaid_actions) = unpaid_question.shape else {
        panic!("main action window");
    };
    assert!(!unpaid_actions.intents.iter().any(|choice| {
        matches!(
            &choice.kind,
            MainIntentKind::Play { card } if card.properties.card == "BP17-030"
        )
    }));
    assert!(matches!(
        only_expensive.play_internal_card("source", PlayerId { seat: 1 }),
        Err(EngineFailure::IllegalPlay(_))
    ));
}

#[test]
fn play_target_quote_wf_rejects_foreign_decisions_bindings_faces_and_private_targets() {
    let base = selection().current["BP17-030"].program.card().clone();
    for change in 0_u8..7 {
        let mut card = base.clone();
        let modifier = &mut card.card_play_costs[0].node.modifiers[0];
        let Expr::Live(read) = modifier.node.condition.as_mut().unwrap() else {
            panic!("target quote")
        };
        let BoolRead::CardPlayTarget {
            decision,
            selection,
            ..
        } = read.as_mut()
        else {
            panic!("target reader")
        };
        match change {
            0 => *decision = DecisionId(999),
            1 => *selection = Binding::new(999, ScopeId(999)),
            2 => modifier.source.face = "foreign-face".into(),
            3 => modifier.source.card_id = "foreign-card".into(),
            4 => {
                card.abilities[0].node.play.as_mut().unwrap().parameters[0].visibility =
                    Visibility::AuthorityOnly {}
            }
            5 => {
                let mut duplicate = card.abilities[0].clone();
                duplicate.node.id = AbilityId(999);
                card.abilities.push(duplicate);
            }
            _ => {
                let ChoiceDomain::Objects { constraint, .. } =
                    &mut card.abilities[0].node.play.as_mut().unwrap().parameters[0].domain
                else {
                    panic!("objects")
                };
                constraint.cardinality.maximum = Some(Nat(2));
            }
        }
        WellFormedCardProgram::check(card).unwrap_err();
    }
}
