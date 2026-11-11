use super::*;
use alloc::collections::BTreeMap;
use sve_engine::game::internal::DecisionView;

#[expect(
    clippy::panic,
    reason = "The printed exchange must request the specified input."
)]
fn awaiting(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(question) = play else {
        panic!("expected an exchange question")
    };
    question
}

#[test]
fn prophetess_exchanges_one_card_of_each_original_cost_for_free_play() {
    let fuel = [
        "BP01-012", "BP01-004", "BP01-014", "BP01-022", "BP01-028", "BP01-104", "BP01-007",
        "BP02-052", "BP01-061", "BP08-072", "BP01-023", "BP04-056",
    ];
    for printing in ["BP08-037", "BP08-SL09", "BP08-U03"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["pp"] = json!({"current":0_u8,"max":10_u8});
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":printing}]);
        initial["players"]["P1"]["zones"]["cemetery"] = json!(
            fuel.iter()
                .enumerate()
                .map(|(index, card)| json!({"id":format!("fuel{index}"),"card":card}))
                .collect::<Vec<_>>()
        );
        let mut game = game(&initial);
        let question = awaiting(
            game.play_internal_card("source", PlayerId { seat: 1 })
                .unwrap(),
        );
        assert_eq!(question.domain, InputDomain::OptionalCost);
        let question = awaiting(
            game.answer_internal_card(question.actor, question.token, Answer::OptionalCost(true))
                .unwrap(),
        );
        let InputDomain::CostMaterials(domain) = &question.domain else {
            panic!("expected material input")
        };
        assert_eq!(domain.members.len(), fuel.len() - 1);
        assert!(
            domain
                .members
                .iter()
                .all(|member| member.card.as_deref() != Some("BP04-056"))
        );
        let packet = game.projection(View::P1).unwrap();
        assert_eq!(packet["awaiting"]["domain"]["distinct_by"], "orig_cost");
        assert!(
            packet["awaiting"]["domain"]["members"]
                .as_array()
                .unwrap()
                .iter()
                .all(|member| member["orig_cost"].is_i64())
        );
        let mut by_cost = BTreeMap::new();
        for (&handle, &cost) in domain.original_costs.as_ref().unwrap() {
            by_cost.entry(cost).or_insert_with(Vec::new).push(handle);
        }
        assert_eq!(
            by_cost.keys().copied().collect::<Vec<_>>(),
            (1..=10).collect::<Vec<_>>()
        );
        let selected: Vec<_> = by_cost.values().map(|handles| handles[0]).collect();
        let mut duplicate_cost = selected.clone();
        duplicate_cost[9] = by_cost[&1][1];
        let before = game.projection(View::Referee).unwrap();
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::CostMaterials(duplicate_cost),
        )
        .unwrap_err();
        assert_eq!(game.projection(View::Referee).unwrap(), before);
        let mut play = game
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::CostMaterials(selected),
            )
            .unwrap();
        if let CardPlay::Awaiting(ward) = play {
            assert!(matches!(ward.domain, InputDomain::WardEntry { .. }));
            play = game
                .answer_internal_card(ward.actor, ward.token, Answer::WardEntry(false))
                .unwrap();
        }
        assert!(matches!(play, CardPlay::Entered(_)));
        let after = game.projection(View::Referee).unwrap();
        assert_eq!(after["P1"]["field"], json!(["source"]));
        assert_eq!(after["P1"]["banish"].as_array().unwrap().len(), 10);
        assert_eq!(after["P1"]["cemetery"].as_array().unwrap().len(), 2);
        assert_eq!(after["P1"]["pp"]["current"], 0_u8);
    }
}
