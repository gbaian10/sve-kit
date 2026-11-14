#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate the printed Kagemitsu chain."
)]
use super::*;
fn finish(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}
fn resolve(game: &mut Game, mut play: CardPlay) {
    while let CardPlay::Awaiting(question) = play {
        let answer = match &question.domain {
            InputDomain::Integer(_) => Answer::Integer(Nat(1)),
            InputDomain::Targets(domain) => Answer::Targets(vec![
                domain
                    .members
                    .iter()
                    .find(|member| member.card.as_deref() == Some("BP15-022"))
                    .unwrap_or(&domain.members[0])
                    .handle,
            ]),
            InputDomain::Modes { modes, .. } => Answer::Modes(vec![modes[0]]),
            _ => panic!("optional return or evolution form"),
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    resolved(play);
    finish(game);
}
fn drain(game: &mut Game) {
    finish(game);
    while let Some(question) = game.internal_pending_choice().unwrap() {
        let InputDomain::Pending(options) = &question.domain else {
            panic!("native pending");
        };
        let option = options
            .iter()
            .find(|option| option.card == "BP15-021")
            .unwrap_or(&options[0]);
        let play = game
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(option.handle),
            )
            .unwrap();
        resolve(game, play);
    }
}
#[test]
fn bp15_021_whole_card_returns_by_lastword_then_observes_two_evolutions_from_ex() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"ally-one","card":"BP01-004"},{"id":"ally-two","card":"BP01-004"},
        {"id":"kagemitsu","card":"BP15-021"}
    ]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([
        {"id":"ally-form-one","card":"BP01-005"},{"id":"ally-form-two","card":"BP01-005"},
        {"id":"kagemitsu-form","card":"BP15-022"}
    ]);
    for player in ["P1", "P2"] {
        initial["players"][player]["zones"]["deck"] = json!(
            (0_u8..3)
                .map(|n| json!({"id":format!("{player}-draw-{n}"),"card":"BP01-009"}))
                .collect::<Vec<_>>()
        );
    }
    let mut game = with_producer(
        &initial,
        Action::Destroy {
            objects: damage_prevent::field_name("ロストサムライ・カゲミツ"),
        },
    );
    assert!(
        game.projection(View::Referee).unwrap()["objects"]["kagemitsu"]["keywords"]
            .as_array()
            .unwrap()
            .contains(&json!("突進")),
        "the unevolved native card retains Rush"
    );
    activate(&mut game, "P1");
    drain(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["ex"],
        json!(["kagemitsu"]),
        "the native Lastword returns the destroyed base card to EX"
    );
    let before = format!("{game:?}");
    let error = game
        .play_internal_card("kagemitsu", PlayerId { seat: 1 })
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("card play holder, permission or timing"),
        "the native EX play restriction reaches the existing permission gate"
    );
    assert!(
        format!("{game:?}") == before,
        "the rejected EX play spends no authority"
    );
    assert_eq!(game.internal_rule_action(&json!({"do":"evolve","source":"ally-one","evolve_card":"ally-form-one","pay":{"pp":0_u8}})).unwrap().outcome,"resolved","the first real own evolution supplies an EX occurrence");
    drain(&mut game);
    let first = game.projection(View::Referee).unwrap();
    assert_eq!(
        first["P1"]["ex"],
        json!(["kagemitsu"]),
        "one counter does not meet the return threshold"
    );
    assert_eq!(
        first["objects"]["kagemitsu"]["counters"]["戦意カウンター"], 1_u8,
        "the first evolution adds one EX counter"
    );
    attack_batch::end_turn(&mut game);
    attack_batch::end_turn(&mut game);
    assert_eq!(game.internal_rule_action(&json!({"do":"evolve","source":"ally-two","evolve_card":"ally-form-two","pay":{"pp":0_u8}})).unwrap().outcome,"resolved","the next own turn restores ordinary evolution");
    drain(&mut game);
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(
        after["objects"]["kagemitsu"]["evolved"], true,
        "the accepted return follows the new field generation into effect evolution"
    );
    assert_eq!(
        after["objects"]["kagemitsu"]["counters"]["戦意カウンター"], 2_u8,
        "EX-to-field carry preserves the two counters"
    );
    activate(&mut game, "P1");
    finish(&mut game);
    assert!(
        game.internal_pending_choice().unwrap().is_none(),
        "the evolved print has its own abilities and does not inherit the base Lastword"
    );
    assert!(
        game.projection(View::Referee).unwrap()["P1"]["cemetery"]
            .as_array()
            .unwrap()
            .contains(&json!("kagemitsu")),
        "the evolved print stays in the cemetery"
    );
}
