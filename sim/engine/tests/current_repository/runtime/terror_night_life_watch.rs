#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate the real-card scenario."
)]
use super::*;

/// BP17-074 in hand with its evolved form ready; one spare テラーナイト already waits in EX.
fn position() -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"urias","card":"BP17-074"}]);
    initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"spare","card":"BP17-T07"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP17-075"}]);
    initial["players"]["P1"]["zones"]["deck"] =
        json!([{"id":"d1","card":"BP01-037"}, {"id":"d2","card":"BP01-037"}]);
    initial
}

fn settle(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}

fn take_pending(game: &mut Game, card: &str, count: usize) -> CardPlay {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("pending occurrence: {:?}", question.domain);
    };
    assert_eq!(options.len(), count, "pending occurrences for {card}");
    assert!(
        options.iter().all(|option| option.card == card),
        "every occurrence belongs to {card}"
    );
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::Pending(options[0].handle),
    )
    .unwrap()
}

fn ex_tokens(game: &Game) -> Vec<String> {
    game.projection(View::Referee).unwrap()["P1"]["ex"]
        .as_array()
        .unwrap()
        .iter()
        .map(|id| id.as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn bp17_074_terror_night_watch_drains_once_per_own_leader_decrease_this_turn() {
    let mut game = game(&position());
    let entry = game
        .play_internal_card("urias", PlayerId { seat: 1 })
        .unwrap();
    assert!(
        matches!(entry, CardPlay::Entered(_)),
        "BP17-074 enters before its Fanfare"
    );
    settle(&mut game);
    resolved(take_pending(&mut game, "BP17-074", 1));
    settle(&mut game);
    let created: Vec<String> = ex_tokens(&game)
        .into_iter()
        .filter(|id| id != "spare")
        .collect();
    assert_eq!(created.len(), 1, "the Fanfare puts one テラーナイト into EX");
    let created = &created[0];
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"][created.as_str()]["name"],
        "テラーナイト"
    );

    // The first テラーナイト of the turn is playable; it only installs the watch.
    resolved(
        game.play_internal_card(created, PlayerId { seat: 1 })
            .unwrap(),
    );
    settle(&mut game);
    let installed = game.projection(View::Referee).unwrap();
    assert_eq!(installed["P1"]["pp"]["current"], 6_u8, "3 + 1 PP spent");
    assert_eq!(installed["P1"]["leader"]["life"], 20_u8);
    assert_eq!(installed["P2"]["leader"]["life"], 20_u8);

    // Another テラーナイト this turn is rejected before any cost is paid.
    game.play_internal_card("spare", PlayerId { seat: 1 })
        .unwrap_err();
    assert_eq!(
        game.projection(View::Referee).unwrap(),
        installed,
        "the rejected play leaves authority unchanged"
    );

    // Q2119: each of the evolved form's own-leader damage modes triggers the watch separately.
    let outcome = game
        .internal_rule_action(
            &json!({"do":"evolve","source":"urias","evolve_card":"form","pay":{"pp":1_u8}}),
        )
        .unwrap()
        .outcome;
    assert_eq!(outcome, "resolved");
    settle(&mut game);
    let CardPlay::Awaiting(modes) = take_pending(&mut game, "BP17-075", 1) else {
        panic!("mode declaration");
    };
    resolved(
        game.answer_internal_card(
            modes.actor,
            modes.token,
            Answer::Modes(vec![ModeId(2), ModeId(3)]),
        )
        .unwrap(),
    );
    settle(&mut game);
    let damaged = game.projection(View::Referee).unwrap();
    assert_eq!(damaged["P1"]["leader"]["life"], 18_u8, "two self-damage modes");
    resolved(take_pending(&mut game, "BP17-T07", 2));
    settle(&mut game);
    resolved(take_pending(&mut game, "BP17-T07", 1));
    settle(&mut game);
    assert!(
        game.internal_pending_choice().unwrap().is_none(),
        "both occurrences resolved"
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["leader"]["life"], 18_u8, "1 damage per decrease");
    assert_eq!(state["P1"]["leader"]["life"], 20_u8, "+1 per decrease");
    assert_eq!(state["P1"]["hand"], json!(["d1"]), "mode 3 draws once");
}
