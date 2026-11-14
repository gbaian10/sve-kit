#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate the real-card scenario."
)]
use super::*;

const PLAYER: PlayerId = PlayerId { seat: 1 };

/// BP19-074 beside BP19-117 with `cemetery` cards already in the cemetery and one enemy.
fn position(cemetery: usize) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"istend","card":"BP19-074"},
        {"id":"guard","card":"BP19-117"}
    ]);
    initial["players"]["P1"]["zones"]["cemetery"] = json!(
        (0..cemetery)
            .map(|n| json!({"id":format!("c{n}"),"card":"BP01-037"}))
            .collect::<Vec<_>>()
    );
    initial["players"]["P1"]["zones"]["deck"] =
        json!([{"id":"d1","card":"BP01-037"}, {"id":"d2","card":"BP01-037"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    initial
}

fn settle(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}

/// Acts BP19-074, sends BP19-117 to the cemetery as the cost and destroys the enemy.
fn sacrifice(game: &mut Game) {
    sacrifice_with(game, "istend", 1);
}

/// `materials` other-named 八獄 followers can pay; the first one is sent.
fn sacrifice_with(game: &mut Game, source: &str, materials: usize) {
    let ability = game.internal_activated_sources(source).unwrap()[0];
    let mut step = game
        .activate_internal_card_instance(source, ability.instance, PLAYER)
        .unwrap();
    while let CardPlay::Awaiting(question) = step {
        let answer = match &question.domain {
            InputDomain::Targets(domain) => Answer::Targets(vec![domain.members[0].handle]),
            InputDomain::CostMaterials(domain) => {
                assert_eq!(
                    domain.members.len(),
                    materials,
                    "only other-named 八獄 followers pay"
                );
                Answer::CostMaterials(vec![domain.members[0].handle])
            }
            _ => panic!("unexpected decision: {:?}", question.domain),
        };
        step = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    settle(game);
}

#[test]
fn bp19_074_returns_a_cheap_eight_hells_follower_without_its_fanfare_from_ten_cards() {
    let mut game = game(&position(10));
    sacrifice(&mut game);
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("BP19-074 trigger: {:?}", question.domain);
    };
    assert_eq!(options.len(), 1, "one departure, one occurrence");
    assert_eq!(options[0].card, "BP19-074");
    let mut step = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap();
    // BP19-117 has Ward, so its owner picks the entry posture.
    while let CardPlay::Awaiting(posture) = step {
        let InputDomain::WardEntry { .. } = posture.domain else {
            panic!("unexpected decision: {:?}", posture.domain);
        };
        step = game
            .answer_internal_card(posture.actor, posture.token, Answer::WardEntry(false))
            .unwrap();
    }
    resolved(step);
    settle(&mut game);
    assert!(
        game.internal_pending_choice().unwrap().is_none(),
        "the returned follower's Fanfare does not trigger"
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["field"], json!(["istend", "guard"]));
    assert_eq!(state["P1"]["hand"], json!([]), "no Fanfare draw");
    assert_eq!(state["P1"]["deck"], json!(["d1", "d2"]));
    assert_eq!(
        state["P2"]["cemetery"],
        json!(["enemy"]),
        "the cost paid for a destroy"
    );
}

#[test]
fn bp19_074_does_not_trigger_when_the_departing_card_makes_the_tenth() {
    // Q2590: nine cards before the move do not satisfy 「墓場が10枚以上で」.
    let mut game = game(&position(9));
    sacrifice(&mut game);
    assert!(
        game.internal_pending_choice().unwrap().is_none(),
        "the gate reads the cemetery before the departure"
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["field"], json!(["istend"]));
    assert_eq!(state["P1"]["cemetery"].as_array().unwrap().len(), 10);
}

#[test]
fn bp19_074_fanfare_mills_the_top_two_cards() {
    let mut initial = position(0);
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"istend","card":"BP19-074"}]);
    let mut game = game(&initial);
    let entry = game.play_internal_card("istend", PLAYER).unwrap();
    assert!(
        matches!(entry, CardPlay::Entered(_)),
        "BP19-074 enters before its Fanfare"
    );
    settle(&mut game);
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("BP19-074 Fanfare: {:?}", question.domain);
    };
    resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap(),
    );
    settle(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["deck"], json!([]), "both deck cards left");
    assert_eq!(
        state["P1"]["cemetery"],
        json!(["d1", "d2"]),
        "top two milled"
    );
}

#[test]
fn bp19_074_a_closed_gate_spends_no_use_and_each_copy_observes_the_next_departure() {
    let mut initial = position(9);
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"istend","card":"BP19-074"},
        {"id":"twin","card":"BP19-074"},
        {"id":"guard","card":"BP19-117"},
        {"id":"guard2","card":"BP19-117"}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"enemy","card":"BP01-037"},
        {"id":"enemy2","card":"BP01-037"}
    ]);
    let mut game = game(&initial);
    // Nine cards: neither copy triggers, so neither spends its once-per-turn use.
    sacrifice_with(&mut game, "istend", 2);
    assert!(
        game.internal_pending_choice().unwrap().is_none(),
        "nine cards"
    );
    // The same-named copy cannot pay with the other BP19-074, only the remaining guard.
    sacrifice_with(&mut game, "twin", 1);
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("BP19-074 triggers: {:?}", question.domain);
    };
    assert_eq!(
        options.len(),
        2,
        "both copies observe the tenth-card departure"
    );
}
