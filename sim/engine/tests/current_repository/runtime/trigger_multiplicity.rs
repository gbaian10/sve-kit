#![expect(
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate printed extra-trigger scenarios."
)]
use super::*;

fn finish(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}

fn position(providers: usize) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!(
        (0..providers)
            .map(|i| json!({"id":format!("elrade-{i}"),"card":"BP19-092","state":{"acted":true}}))
            .collect::<Vec<_>>()
    );
    initial["players"]["P1"]["zones"]["deck"] = json!(
        (0_u8..10)
            .map(|i| json!({"id":format!("draw-{i}"),"card":"BP01-009"}))
            .collect::<Vec<_>>()
    );
    initial
}

#[test]
fn bp19_092_whole_card_adds_occurrences_and_preserves_conditional_modes() {
    let mut initial = position(2);
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-037","state":{"hp":4_u8,"max_hp":4_u8}}]);
    let mut game = game(&initial);
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    let mut resolved_count = 0_usize;
    while let Some(question) = game.internal_pending_choice().unwrap() {
        let InputDomain::Pending(options) = &question.domain else {
            panic!("pending");
        };
        assert!(
            options
                .iter()
                .all(|option| option.card == "BP19-092" && option.line == 2),
            "extra occurrences retain the printed ability"
        );
        let mut play = game
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap();
        while let CardPlay::Awaiting(choice) = play {
            let answer = match &choice.domain {
                InputDomain::Modes { modes, count, .. } => {
                    assert_eq!(
                        *count,
                        Nat(2),
                        "another Eight Prison follower enables up to two modes"
                    );
                    assert_eq!(
                        modes.contains(&ModeId(1)),
                        resolved_count == 0,
                        "the damage mode disappears after its only legal target leaves"
                    );
                    Answer::Modes(vec![
                        ModeId(if resolved_count == 0 { 1 } else { 2 }),
                        ModeId(3),
                    ])
                }
                InputDomain::Targets(domain) => Answer::Targets(vec![domain.members[0].handle]),
                _ => panic!("mode declaration"),
            };
            play = game
                .answer_internal_card(choice.actor, choice.token, answer)
                .unwrap();
        }
        resolved(play);
        resolved_count += 1;
        finish(&mut game);
    }
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(
        resolved_count, 6,
        "two live providers each add one occurrence to both original abilities"
    );
    assert_eq!(
        view["P2"]["cemetery"],
        json!(["enemy"]),
        "the printed four damage resolves before the later choices"
    );
    assert_eq!(
        view["P1"]["leader"]["life"], 15_u8,
        "five remaining heal modes resolve independently"
    );
    assert_eq!(
        view["P1"]["hand"].as_array().unwrap().len(),
        6,
        "every occurrence also draws once"
    );
    for id in ["elrade-0", "elrade-1"] {
        assert_eq!(
            view["objects"][id]["acted"], false,
            "the selected source stands even if it already stood"
        );
    }
}

#[test]
fn bp19_092_also_adds_own_ex_observer_at_opponents_end_with_separate_costs() {
    let mut initial = position(1);
    initial["turn"]["active"] = json!("P2");
    initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"nekhbet","card":"BP14-091"}]);
    let mut game = game(&initial);
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    let mut count = 0_usize;
    while let Some(question) = game.internal_pending_choice().unwrap() {
        let InputDomain::Pending(options) = &question.domain else {
            panic!("pending");
        };
        assert!(options.iter().all(|option| option.card == "BP14-091"));
        assert_eq!(question.actor, PlayerId { seat: 1 });
        let mut play = game
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap();
        while let CardPlay::Awaiting(choice) = play {
            assert_eq!(choice.domain, InputDomain::OptionalCost);
            play = game
                .answer_internal_card(choice.actor, choice.token, Answer::OptionalCost(true))
                .unwrap();
        }
        resolved(play);
        count += 1;
        finish(&mut game);
    }
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(count, 2);
    assert_eq!(view["P1"]["pp"]["current"], 6_u8);
    assert_eq!(view["P1"]["leader"]["life"], 24_u8);
    assert_eq!(view["P2"]["leader"]["life"], 16_u8);
    assert_eq!(view["P1"]["ex"], json!(["nekhbet"]));
}
