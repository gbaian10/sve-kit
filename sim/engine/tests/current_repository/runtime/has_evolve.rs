#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate these card scenarios."
)]
use super::*;
use sve_engine::game::internal::SearchAnswer;

fn game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "has-evolve",
    )
    .unwrap()
}

fn pending(game: &mut Game, card: &str, line: u64) -> CardPlay {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("pending ability")
    };
    let option = options
        .iter()
        .find(|option| option.card == card && option.line == line)
        .unwrap();
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::Pending(option.handle),
    )
    .unwrap()
}

#[test]
fn maid_leader_searches_only_followers_with_printed_evolution() {
    for form in ["BP01-035", "BP01-P08"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP01-034"}]);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
        initial["players"]["P1"]["zones"]["deck"] = json!([
            {"id":"eligible","card":"BP02-118"}, {"id":"plain","card":"BP01-037"}
        ]);
        let mut game = game(&initial);
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":2_u8}})
            )
            .unwrap()
            .outcome,
            "resolved"
        );
        let CardPlay::Awaiting(question) = pending(&mut game, form, 1) else {
            panic!("search")
        };
        let InputDomain::Search(domain) = &question.domain else {
            panic!("search domain")
        };
        assert_eq!(domain.cards.len(), 1);
        assert_eq!(domain.cards[0].card, "BP02-118");
        let handle = domain.cards[0].handle;
        resolved(
            game.answer_internal_card(
                question.actor,
                question.token,
                Answer::Search(SearchAnswer::Ungrouped(vec![handle])),
            )
            .unwrap(),
        );
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["hand"], json!(["eligible"]));
        assert_eq!(state["P1"]["deck_count"], 1_u8);
    }
}

#[test]
fn maid_leader_can_search_when_no_follower_has_evolution() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP01-034"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP01-035"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"plain","card":"BP01-037"}]);
    let mut game = game(&initial);
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":2_u8}})
        )
        .unwrap()
        .outcome,
        "resolved"
    );
    let CardPlay::Awaiting(question) = pending(&mut game, "BP01-035", 1) else {
        panic!("search")
    };
    let InputDomain::Search(domain) = &question.domain else {
        panic!("search domain")
    };
    assert_eq!(domain.cards.len(), 0);
    resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Search(SearchAnswer::Ungrouped(vec![])),
        )
        .unwrap(),
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!([]));
    assert_eq!(state["P1"]["deck_count"], 1_u8);
}

#[test]
fn detective_returns_only_a_follower_with_printed_evolution() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"detective","card":"BP13-115"}]);
    initial["players"]["P1"]["zones"]["cemetery"] = json!([
        {"id":"eligible","card":"BP02-118"}, {"id":"plain","card":"BP01-037"}
    ]);
    let mut game = game(&initial);
    assert!(matches!(
        game.play_internal_card("detective", PlayerId { seat: 1 })
            .unwrap(),
        CardPlay::Entered(_)
    ));
    let CardPlay::Awaiting(question) = pending(&mut game, "BP13-115", 2) else {
        panic!("targets")
    };
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("target domain")
    };
    assert_eq!(domain.members.len(), 1);
    assert_eq!(domain.members[0].card.as_deref(), Some("BP02-118"));
    let handle = domain.members[0].handle;
    resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Targets(vec![handle]),
        )
        .unwrap(),
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["eligible"]));
    assert_eq!(state["P1"]["cemetery"], json!(["plain"]));
}

#[test]
fn field_target_uses_the_same_printed_evolution_filter() {
    use alloc::collections::BTreeMap;
    let (mut snapshot, keywords, mut input) = inputs().clone();
    let number = "synthetic-has-evolve";
    snapshot.push_str(
        &json!({"number":number,"faces":[{
            "name":"Synthetic filter","card_class":"ニュートラル","card_type":"スペル",
            "cost":"1","power":"-","hp":"-","traits":[],"text":null,"sections":[]
        }]})
        .to_string(),
    );
    snapshot.push('\n');
    input["documents"].as_array_mut().unwrap().push(json!(["synthetic-has-evolve.yaml",
        "format: 1\nkind: effect_set\ncards:\n  synthetic-has-evolve:\n    status: complete\n    abilities: []\n"]));
    let mut tree = expanded::spell_tree().clone();
    let mut card = tree["cards"]["SD03-005"].clone();
    card["abilities"][0]["spell"] = json!({
        "targets":[{"bind":"target","from":{"who":"opp","zone":"field","kind":"follower","has_evolve":true},"n":1_u8}],
        "do":[{"destroy":"target"}]
    });
    tree["cards"] = json!({number:card});
    let program = lowerer()
        .lower_with_printed_costs(&tree, &BTreeMap::from([(number.into(), Nat(1))]))
        .unwrap()
        .remove(number)
        .unwrap();
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"spell","card":number}]);
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"eligible","card":"BP02-118"},{"id":"plain","card":"BP01-037"}
    ]);
    let mut game = game_from_selection(&initial, &snapshot, &keywords, &input);
    game.load_internal_card(number, &program).unwrap();
    let CardPlay::Awaiting(question) = game
        .play_internal_card("spell", PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("targets")
    };
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("target domain")
    };
    assert_eq!(domain.members.len(), 1);
    assert_eq!(domain.members[0].card.as_deref(), Some("BP02-118"));
    let handle = domain.members[0].handle;
    resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Targets(vec![handle]),
        )
        .unwrap(),
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["field"], json!(["plain"]));
    assert_eq!(state["P2"]["cemetery"], json!(["eligible"]));
}

#[test]
fn detective_gains_rush_when_an_allied_follower_evolves() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"detective","card":"BP13-115"},{"id":"source","card":"BP02-118"}
    ]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP02-119"}]);
    let mut game = game(&initial);
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":3_u8}})
        )
        .unwrap()
        .outcome,
        "resolved"
    );
    resolved(pending(&mut game, "BP13-115", 1));
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["detective"]["keywords"],
        json!(["突進"])
    );
}

#[test]
fn detective_does_not_gain_rush_when_an_opponent_evolves() {
    let mut initial = draw_position();
    initial["turn"]["active"] = json!("P2");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"detective","card":"BP13-115"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"source","card":"BP02-118"}]);
    initial["players"]["P2"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP02-119"}]);
    let mut game = game(&initial);
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":3_u8}})
        )
        .unwrap()
        .outcome,
        "resolved"
    );
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
    assert!(game.internal_pending_choice().unwrap().is_none());
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["detective"]["keywords"],
        json!([])
    );
}
