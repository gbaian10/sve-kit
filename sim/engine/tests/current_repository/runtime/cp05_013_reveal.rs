
use super::*;

fn fanfare(game: &mut Game) -> CardPlay {
    assert!(
        matches!(
            game.play_internal_card("source", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ),
        "the card enters before its Fanfare offers payment"
    );
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("expected CP05-013's printed Fanfare");
    };
    assert_eq!(options.len(), 1, "only the printed Fanfare is pending");
    assert_eq!(
        options[0].card, "CP05-013",
        "the played printing owns the pending ability"
    );
    assert_eq!(
        options[0].line, 2,
        "the Fanfare comes from printed line two"
    );
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::Pending(options[0].handle),
    )
    .unwrap()
}

#[test]
fn cp05_013_reveals_three_fire_cards_before_creating_a_token_and_healing() {
    let mut initial = draw_position();
    initial["players"]["P1"]["leader"]["life"] = json!(15_u8);
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"source","card":"CP05-013"},
        {"id":"fire1","card":"CP05-044"},
        {"id":"fire2","card":"CP05-051"},
        {"id":"fire3","card":"CP05-P09"},
        {"id":"other","card":"BP01-037"}
    ]);
    let mut game = game(&initial);
    let CardPlay::Awaiting(payment) = fanfare(&mut game) else {
        panic!("the printed Fanfare offers its reveal cost");
    };
    assert_eq!(payment.domain, InputDomain::OptionalCost);
    let CardPlay::Awaiting(materials) = game
        .answer_internal_card(payment.actor, payment.token, Answer::OptionalCost(true))
        .unwrap()
    else {
        panic!("reveal cost must ask for private hand materials");
    };
    let InputDomain::CostMaterials(domain) = &materials.domain else {
        panic!("expected hand materials");
    };
    let cards = domain
        .members
        .iter()
        .map(|member| member.card.as_deref().unwrap())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        cards,
        BTreeSet::from(["CP05-044", "CP05-051", "CP05-P09"]),
        "both Fire followers and Fire amulets are eligible, regardless of printing"
    );
    assert!(game.internal_decision(View::P2).is_none());

    let handles = domain
        .members
        .iter()
        .map(|member| member.handle)
        .collect::<Vec<_>>();
    game.answer_internal_card(
        materials.actor,
        materials.token,
        Answer::CostMaterials(handles[..2].to_vec()),
    )
    .unwrap_err();
    let before_payment = game.projection(View::P2).unwrap();
    assert!(
        before_payment["known_cards"]
            .as_array()
            .unwrap()
            .iter()
            .all(|card| !["fire1", "fire2", "fire3", "other"]
                .contains(&card["id"].as_str().unwrap()))
    );
    assert_eq!(before_payment["P1"]["leader"]["life"], 15_u8);
    assert_eq!(before_payment["P1"]["ex"], json!([]));

    resolved(
        game.answer_internal_card(
            materials.actor,
            materials.token,
            Answer::CostMaterials(handles),
        )
        .unwrap(),
    );
    game.resume_internal_card().unwrap();
    let owner = game.projection(View::Referee).unwrap();
    assert_eq!(owner["P1"]["leader"]["life"], 17_u8);
    assert_eq!(
        owner["P1"]["hand"],
        json!(["fire1", "fire2", "fire3", "other"])
    );
    let [token] = owner["P1"]["ex"].as_array().unwrap().as_slice() else {
        panic!("the printed Fanfare creates one elemental token");
    };
    assert_eq!(
        owner["objects"][token.as_str().unwrap()]["card"],
        "CP05-T09"
    );

    let after_resolution = game.projection(View::P2).unwrap();
    let known = after_resolution["known_cards"].as_array().unwrap();
    for id in ["fire1", "fire2", "fire3"] {
        assert!(known.iter().any(|card| card["id"] == id));
        assert!(after_resolution["objects"][id].is_null());
    }
    assert!(known.iter().all(|card| card["id"] != "other"));
}
