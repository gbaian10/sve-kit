#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate these counter scenarios."
)]
use super::*;

const PLAYER: PlayerId = PlayerId { seat: 1 };

fn position(number: &str, state: &Value) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"source","card":number,"state":state}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    initial
}

fn definition(number: &str, line: u32) -> AbilityId {
    selection().current[number]
        .program
        .card()
        .abilities
        .iter()
        .find(|ability| ability.source.line == line)
        .unwrap()
        .node
        .id
}

fn finish(game: &mut Game, mut play: CardPlay) {
    while let CardPlay::Awaiting(question) = play {
        let answer = match &question.domain {
            InputDomain::Targets(domain) => Answer::Targets(vec![domain.members[0].handle]),
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            _ => panic!("unexpected counter decision"),
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    assert!(
        !matches!(play, CardPlay::Cancelled),
        "the printed effect must complete"
    );
}

fn trigger(game: &mut Game) {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("pending")
    };
    assert_eq!(options.len(), 1, "exactly one printed trigger is pending");
    let play = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap();
    finish(game, play);
    game.resume_internal_card().unwrap();
}

fn reject_payment(game: &mut Game, number: &str) {
    match game.activate_internal_card("source", definition(number, 2), PLAYER) {
        Err(error) => assert!(
            matches!(error, EngineFailure::IllegalPlay(_)),
            "missing counters make payment illegal"
        ),
        Ok(CardPlay::Awaiting(question)) => {
            assert!(
                matches!(question.domain, InputDomain::Targets(_)),
                "only formal target selection can precede payment"
            );
            game.answer_internal_card(question.actor, question.token, Answer::Targets(vec![0]))
                .unwrap_err();
        }
        Ok(_) => panic!("insufficient counters cannot resolve"),
    }
}

#[test]
fn lila_and_printing_add_then_spend_a_fairy_tale_counter_to_destroy() {
    for number in ["BP03-004", "BP03-U01"] {
        let mut game = game(&position(number, &json!({})));
        let play = game
            .activate_internal_card("source", definition(number, 1), PLAYER)
            .unwrap();
        finish(&mut game, play);
        game.resume_internal_card().unwrap();
        let placed = game.projection(View::Referee).unwrap();
        assert_eq!(
            placed["objects"]["source"]["counters"]["童話カウンター"],
            1_u8
        );
        assert_eq!(placed["objects"]["source"]["acted"], true);
        let removal = game
            .activate_internal_card("source", definition(number, 2), PLAYER)
            .unwrap();
        finish(&mut game, removal);
        let after_payment = game.projection(View::Referee).unwrap();
        assert_eq!(
            after_payment["objects"]["source"]["counters"]["童話カウンター"],
            0_u8
        );
        assert_eq!(after_payment["P1"]["pp"]["current"], 9_u8);
        assert_eq!(after_payment["P2"]["cemetery"], json!(["enemy"]));
    }
}

#[test]
fn maisy_and_printings_require_another_fairy_tale_follower_before_placing_a_counter() {
    for number in ["BP03-022", "BP03-P06", "PR-063"] {
        for companion in [false, true] {
            let mut initial = position(number, &json!({}));
            initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":number}]);
            initial["players"]["P1"]["zones"]["field"] = if companion {
                json!([{"id":"ally","card":"BP03-004"}])
            } else {
                json!([])
            };
            let mut game = game(&initial);
            let play = game.play_internal_card("source", PLAYER).unwrap();
            finish(&mut game, play);
            trigger(&mut game);
            let before = game.projection(View::Referee).unwrap();
            if companion {
                let attempt = game
                    .activate_internal_card("source", definition(number, 2), PLAYER)
                    .unwrap();
                finish(&mut game, attempt);
                let state = game.projection(View::Referee).unwrap();
                assert_eq!(
                    state["objects"]["source"]["counters"]["童話カウンター"],
                    0_u8
                );
                assert_eq!(state["P2"]["cemetery"], json!(["enemy"]));
                assert_eq!(state["objects"]["source"]["acted"], true);
            } else {
                reject_payment(&mut game, number);
                let after = game.projection(View::Referee).unwrap();
                assert_eq!(after["P1"]["pp"], before["P1"]["pp"]);
                assert_eq!(after["objects"]["source"], before["objects"]["source"]);
                assert_eq!(after["P2"]["field"], before["P2"]["field"]);
            }
        }
    }
}

#[test]
fn lila_cannot_pay_a_different_or_missing_counter() {
    for counters in [json!({}), json!({"加虐カウンター":3_u8})] {
        let mut game = game(&position("BP03-004", &json!({"counters":counters})));
        let before = game.projection(View::Referee).unwrap();
        reject_payment(&mut game, "BP03-004");
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["pp"],
            before["P1"]["pp"]
        );
        assert_eq!(
            game.projection(View::Referee).unwrap()["P2"]["field"],
            before["P2"]["field"]
        );
    }
}

#[test]
fn chamber_places_two_counters_then_retires_only_when_the_last_counter_is_paid() {
    let number = "BP19-091";
    let mut initial = position(number, &json!({}));
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":number}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"drawn","card":"BP01-037"}]);
    let mut game = game(&initial);
    let play = game.play_internal_card("source", PLAYER).unwrap();
    finish(&mut game, play);
    trigger(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["hand"],
        json!(["drawn"])
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["source"]["counters"]["加虐カウンター"],
        2_u8
    );
    let first_activation = game
        .activate_internal_card("source", definition(number, 2), PLAYER)
        .unwrap();
    finish(&mut game, first_activation);
    game.resume_internal_card().unwrap();
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        state["objects"]["source"]["counters"]["加虐カウンター"],
        1_u8
    );
    assert_eq!(state["P1"]["field"], json!(["source"]));
    assert_eq!(state["P1"]["leader"]["life"], 19_u8);
}

#[test]
fn chamber_retires_after_the_last_counter_is_paid() {
    let number = "BP19-091";
    let mut initial = position(number, &json!({"counters":{"加虐カウンター":1_u8}}));
    initial["players"]["P1"]["leader"]["life"] = json!(19_u8);
    let mut game = game(&initial);
    let play = game
        .activate_internal_card("source", definition(number, 2), PLAYER)
        .unwrap();
    finish(&mut game, play);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["leader"]["life"], 18_u8);
    assert_eq!(state["P1"]["cemetery"], json!(["source"]));
}
