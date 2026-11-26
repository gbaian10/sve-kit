use super::*;
use sve_engine::game::internal::DecisionView;

fn game(initial: &Value, facts: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        facts,
        &Value::Null,
        "look-pairs",
    )
    .unwrap()
}

fn awaiting(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(question) = play else {
        panic!("printed decision")
    };
    question
}

fn targets(game: &mut Game, question: &DecisionView, numbers: &[&str]) -> CardPlay {
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("observed targets")
    };
    let handles = domain
        .members
        .iter()
        .filter(|member| member.card.as_deref().is_some_and(|n| numbers.contains(&n)))
        .map(|member| member.handle)
        .collect();
    game.answer_internal_card(question.actor, question.token, Answer::Targets(handles))
        .unwrap()
}

fn order(game: &mut Game, play: CardPlay, numbers: &[&str]) -> CardPlay {
    let question = awaiting(play);
    let InputDomain::Order(cards) = &question.domain else {
        panic!("private order")
    };
    assert!(
        game.internal_decision(View::P2).is_none(),
        "only the Look viewer receives the ordering question"
    );
    let handles = numbers
        .iter()
        .map(|number| {
            cards
                .iter()
                .find(|card| card.card == *number)
                .unwrap()
                .handle
        })
        .collect();
    game.answer_internal_card(question.actor, question.token, Answer::Order(handles))
        .unwrap()
}

fn dark_cat(initial: &Value) -> (Game, DecisionView) {
    let mut game = game(initial, &Value::Null);
    let ability = selection_with_rules().current["CP03-116"]
        .program
        .card()
        .abilities
        .iter()
        .find(|a| {
            matches!(
                a.node.source.origin,
                AbilityOrigin::Keyword {
                    keyword: KeywordId::Ride,
                    ..
                }
            )
        })
        .unwrap()
        .node
        .id;
    let payment = awaiting(
        game.activate_internal_card("source", ability, PlayerId { seat: 1 })
            .unwrap(),
    );
    assert!(
        matches!(payment.domain, InputDomain::ResourceEntryPayment { .. }),
        "Ride requires its DrivePoint payment"
    );
    resolved(
        game.answer_internal_card(
            payment.actor,
            payment.token,
            Answer::ResourceEntryPayment {
                materials: vec![0],
                use_ep: false,
            },
        )
        .unwrap(),
    );
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("Drive gained")
    };
    let question = awaiting(
        game.answer_internal_card(
            pending.actor,
            pending.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap(),
    );
    (game, question)
}

#[test]
fn dark_cat_rides_takes_a_revealed_oracle_and_splits_only_the_remaining_look() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"CP03-116"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"point","card":"CSD03a-017"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"top","card":"BP01-171"},
        {"id":"oracle","card":"CP03-121"},
        {"id":"bottom","card":"BP01-037"},
        {"id":"unseen","card":"CP03-116"}
    ]);
    let (mut game, question) = dark_cat(&initial);
    let question = awaiting(targets(&mut game, &question, &["CP03-121"]));
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("remaining subset")
    };
    assert_eq!(domain.members.len(), 2);
    let top = targets(&mut game, &question, &["BP01-171"]);
    let bottom = order(&mut game, top, &["BP01-171"]);
    resolved(order(&mut game, bottom, &["BP01-037"]));
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["hand"], json!(["oracle"]));
    assert_eq!(view["P1"]["deck"], json!(["top", "unseen", "bottom"]));
    assert_eq!(view["P1"]["pp"]["current"], 9_u8);
    assert_eq!(view["objects"]["source"]["power"], 3_u8);
    assert_eq!(view["objects"]["source"]["hp"], 3_u8);
    let opponent = game.projection(View::P2).unwrap();
    let known = opponent["known_cards"].as_array().unwrap();
    assert!(
        known.iter().any(|card| card["id"] == "oracle"),
        "the selected Oracle is publicly revealed"
    );
    for hidden in ["top", "bottom", "unseen"] {
        assert!(
            !known.iter().any(|card| card["id"] == hidden),
            "the remaining card {hidden} stays private"
        );
    }
}

#[test]
fn luck_bird_orders_an_arbitrary_top_subset_and_the_bottom_remainder_privately() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"CP03-123"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"one","card":"BP01-171"},
        {"id":"two","card":"BP01-037"},
        {"id":"three","card":"BP01-009"},
        {"id":"tail","card":"BP01-010"}
    ]);
    let mut game = game(&initial, &Value::Null);
    let question = awaiting(
        game.play_internal_card("source", PlayerId { seat: 1 })
            .unwrap(),
    );
    let question = awaiting(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Modes(vec![ModeId(1)]),
        )
        .unwrap(),
    );
    let top = targets(&mut game, &question, &["BP01-171", "BP01-009"]);
    let bottom = order(&mut game, top, &["BP01-009", "BP01-171"]);
    resolved(order(&mut game, bottom, &["BP01-037"]));
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["deck"],
        json!(["three", "one", "tail", "two"])
    );
    let opponent = game.projection(View::P2).unwrap();
    let known = opponent["known_cards"].as_array().unwrap();
    assert!(
        !known
            .iter()
            .any(|card| ["one", "two", "three", "tail"].contains(&card["id"].as_str().unwrap())),
        "private top and bottom orders reveal no card to the opponent"
    );
}

#[test]
fn luck_bird_requires_five_trigger_followers_before_returning_drawing_and_refunding_pp() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"CP03-123"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"old","card":"BP01-171"}]);
    initial["players"]["P1"]["zones"]["cemetery"] = json!([
        {"id":"a","card":"CP03-121"},{"id":"b","card":"CP03-121"},
        {"id":"c","card":"CP03-121"},{"id":"d","card":"CP03-100"},
        {"id":"e","card":"CP03-100"},{"id":"wrong","card":"CP03-117"}
    ]);
    let facts = json!({
        "CP03-121":{"verified":true,"trigger_icon":"heal"},
        "CP03-100":{"verified":true,"trigger_icon":"heal"},
        "CP03-117":{"verified":true,"trigger_icon":"none"}
    });
    let mut game = game(&initial, &facts);
    let question = awaiting(
        game.play_internal_card("source", PlayerId { seat: 1 })
            .unwrap(),
    );
    let question = awaiting(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Modes(vec![ModeId(2)]),
        )
        .unwrap(),
    );
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("formal cemetery targets")
    };
    assert_eq!(domain.members.len(), 5);
    assert_eq!(domain.cardinality.minimum, Nat(5));
    resolved(targets(&mut game, &question, &["CP03-121", "CP03-100"]));
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["pp"]["current"], 10_u8);
    assert_eq!(view["P1"]["hand"].as_array().unwrap().len(), 1);
    assert_eq!(view["P1"]["deck"].as_array().unwrap().len(), 5);
    assert_eq!(view["P1"]["cemetery"], json!(["wrong", "source"]));

    initial["players"]["P1"]["zones"]["cemetery"]
        .as_array_mut()
        .unwrap()
        .remove(0);
    let mut insufficient = self::game(&initial, &facts);
    let unavailable = awaiting(
        insufficient
            .play_internal_card("source", PlayerId { seat: 1 })
            .unwrap(),
    );
    let before = insufficient.projection(View::Referee).unwrap();
    insufficient
        .answer_internal_card(
            unavailable.actor,
            unavailable.token,
            Answer::Modes(vec![ModeId(2)]),
        )
        .unwrap_err();
    assert_eq!(insufficient.projection(View::Referee).unwrap(), before);
}
