use super::*;
use sve_engine::game::internal::{Answer, InputDomain};

fn position(different_name: bool, cost_limit: u64) -> Game {
    let head = json!({"activate":{"cost":{"pp":3},"targets":[{
        "bind":"picked","from":{"who":"self","zone":"cemetery"},
        "n":2,"distinct_by":"name","cost_sum_le":cost_limit
    }],"do":[{"life":{"who":"self","delta":1}}]}});
    let programs = lowerer()
        .lower(&same_name::tree(&head, "unit-amulet"))
        .unwrap();
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"caster","card":"unit-amulet"}]);
    initial["players"]["P1"]["zones"]["cemetery"] = json!(
        (0..12)
            .map(|index| json!({"id":format!("grave-{index}"),"card":
            if index == 11 && different_name {"unit-spell"} else {"unit-follower"}}))
            .collect::<Vec<_>>()
    );
    initial["players"]["P1"]["pp"] = json!({"current":5,"max":5});
    initial["players"]["P1"]["leader"]["life"] = json!(10);
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "large-pair",
    )
    .unwrap();
    game.load_internal_card("unit-amulet", &programs["unit-amulet"])
        .unwrap();
    game.load_internal_card("unit-follower", &card(vec![]))
        .unwrap();
    game.load_internal_card("unit-spell", &card(vec![]))
        .unwrap();
    game
}

#[test]
fn cemetery_pair_capacity_finds_a_compatible_name_beyond_the_first_ten_candidates() {
    let mut game = position(true, 2);
    let CardPlay::Awaiting(question) = game
        .activate_internal_card("caster", AbilityId(1), PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("formal pair")
    };
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("formal pair")
    };
    assert_eq!(domain.members.len(), 12);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        5
    );
    let pair = ["unit-follower", "unit-spell"]
        .iter()
        .map(|number| {
            domain
                .members
                .iter()
                .find(|member| member.card.as_deref() == Some(*number))
                .unwrap()
                .handle
        })
        .collect();
    resolved(
        game.answer_internal_card(question.actor, question.token, Answer::Targets(pair))
            .unwrap(),
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["pp"]["current"], 2);
    assert_eq!(state["P1"]["leader"]["life"], 11);
}

#[test]
fn cemetery_pair_capacity_rejects_name_or_cost_shortages_before_payment_atomically() {
    for (different_name, cost_limit) in [(false, 2), (true, 1)] {
        let mut game = position(different_name, cost_limit);
        let before = exact_authority(&game);
        assert!(matches!(
            game.activate_internal_card("caster", AbilityId(1), PlayerId { seat: 1 })
                .unwrap_err(),
            sve_engine::EngineFailure::IllegalPlay(_)
        ));
        assert_eq!(exact_authority(&game), before);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
            5
        );
    }
}
