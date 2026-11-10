#![expect(
    clippy::panic,
    reason = "Unexpected choices invalidate these card scenarios."
)]
use super::*;
use sve_engine::game::internal::DecisionView;

fn trigger(game: &mut Game, card: &str) -> CardPlay {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = question.domain else {
        panic!("trigger")
    };
    let option = options.iter().find(|option| option.card == card).unwrap();
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::Pending(option.handle),
    )
    .unwrap()
}

fn gravekeeper(field_count: usize, cards: Value) -> Game {
    let mut initial = draw_position();
    let mut field = vec![json!({"id":"source","card":"BP20-083"})];
    field.extend((1..field_count).map(|index| json!({"id":format!("f{index}"),"card":"BP01-037"})));
    initial["players"]["P1"]["zones"]["field"] = json!(field);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP20-084"}]);
    initial["players"]["P1"]["zones"]["cemetery"] = cards;
    let mut game = game(&initial);
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":5_u8}})
        )
        .unwrap()
        .outcome,
        "resolved",
        "the printed evolution resolves before its trigger is played"
    );
    game
}

fn handle(question: &DecisionView, card: &str) -> u64 {
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("targets")
    };
    domain
        .members
        .iter()
        .find(|member| member.card.as_deref() == Some(card))
        .unwrap()
        .handle
}

#[test]
fn bp20_084_requires_distinct_groups_before_choosing_the_last_field_slot() {
    let mut game = gravekeeper(
        4,
        json!([
            {"id":"high","card":"BP01-037"},
            {"id":"high2","card":"BP01-037"},
            {"id":"low","card":"BP01-034"},
            {"id":"nightmare","card":"BP20-083"}
        ]),
    );
    let CardPlay::Awaiting(question) = trigger(&mut game, "BP20-084") else {
        panic!("targets")
    };
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("targets")
    };
    assert_eq!(domain.cardinality.minimum, Nat(2));
    assert_eq!(domain.members.len(), 3);
    let highs = domain
        .members
        .iter()
        .filter(|member| member.card.as_deref() == Some("BP01-037"))
        .map(|member| member.handle)
        .collect::<Vec<_>>();
    let high = highs[0];
    let low = handle(&question, "BP01-034");
    // Two cost-5 followers only satisfy the wider group, so they cannot fill both groups.
    for invalid in [vec![low], vec![low, low], highs] {
        game.answer_internal_card(question.actor, question.token, Answer::Targets(invalid))
            .unwrap_err();
        assert_eq!(
            game.internal_decision(View::P1).unwrap().token,
            question.token
        );
    }
    let CardPlay::Awaiting(capacity) = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Targets(vec![low, high]),
        )
        .unwrap()
    else {
        panic!("capacity")
    };
    let InputDomain::SearchPartition(capacity_domain) = &capacity.domain else {
        panic!("capacity domain")
    };
    let chosen = capacity_domain
        .cards
        .iter()
        .find(|card| card.card == "BP01-034")
        .unwrap()
        .handle;
    resolved(
        game.answer_internal_card(
            capacity.actor,
            capacity.token,
            Answer::SearchPartition(vec![chosen]),
        )
        .unwrap(),
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        state["P1"]["cemetery"],
        json!(["high", "high2", "nightmare"])
    );
    assert_eq!(state["P1"]["field"].as_array().unwrap().len(), 5);
    assert!(
        state["P1"]["field"]
            .as_array()
            .unwrap()
            .contains(&json!("low"))
    );
}

#[test]
fn bp20_084_cannot_play_when_only_one_group_can_be_filled() {
    let mut game = gravekeeper(
        1,
        json!([
            {"id":"high1","card":"BP01-037"},{"id":"high2","card":"BP01-037"}
        ]),
    );
    assert!(matches!(
        trigger(&mut game, "BP20-084"),
        CardPlay::Cancelled
    ));
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["cemetery"],
        json!(["high1", "high2"])
    );
}

#[test]
fn ecp02_029_and_alias_reanimate_one_of_each_original_cost_and_heal() {
    for card in ["ECP02-029", "ECP02-SL16"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
        initial["players"]["P1"]["zones"]["cemetery"] = json!([
            {"id":"one","card":"CSD02c-004"},{"id":"two","card":"CSD02c-007"}
        ]);
        let mut game = game(&initial);
        let CardPlay::Awaiting(question) = game
            .play_internal_card("source", PlayerId { seat: 1 })
            .unwrap()
        else {
            panic!("spell targets")
        };
        let InputDomain::Targets(domain) = &question.domain else {
            panic!("spell target domain")
        };
        assert_eq!(domain.cardinality.minimum, Nat(0));
        assert_eq!(domain.members.len(), 2);
        let handles = domain.members.iter().map(|member| member.handle).collect();
        resolved(
            game.answer_internal_card(question.actor, question.token, Answer::Targets(handles))
                .unwrap(),
        );
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["field"], json!(["two", "one"]));
        assert_eq!(state["P1"]["leader"]["life"], 12_u8);
    }
}
