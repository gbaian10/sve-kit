#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate the printed scenarios."
)]
use super::*;

const fn actor() -> PlayerId {
    PlayerId { seat: 1 }
}

fn finish_checks(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}

/// Answers printed questions: targets take the first member; 【守護】 enters standing.
fn answer(game: &mut Game, step: CardPlay) {
    answer_capacity(game, step, &[]);
}

/// Like `answer`, and a full EX keeps exactly the `kept` token names (Q2146).
fn answer_capacity(game: &mut Game, mut step: CardPlay, kept: &[&str]) {
    while let CardPlay::Awaiting(q) = step {
        let reply = match &q.domain {
            InputDomain::TokenCapacity { choices } => Answer::TokenCapacity(
                u64::try_from(
                    choices
                        .iter()
                        .position(|choice| choice == kept)
                        .unwrap_or_else(|| panic!("{kept:?} among {choices:?}")),
                )
                .unwrap(),
            ),
            InputDomain::Targets(d) => {
                Answer::Targets(d.members.first().map(|m| m.handle).into_iter().collect())
            }
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            other => panic!("unexpected printed decision: {other:?}"),
        };
        step = game.answer_internal_card(q.actor, q.token, reply).unwrap();
    }
    finish_checks(game);
}

/// Resolves every pending printed trigger in offer order; returns how many resolved.
fn drain(game: &mut Game) -> usize {
    let mut resolved = 0_usize;
    finish_checks(game);
    while let Some(q) = game.internal_pending_choice().unwrap() {
        let InputDomain::Pending(options) = &q.domain else {
            panic!("printed pending ability");
        };
        let step = game
            .answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
            .unwrap();
        answer(game, step);
        resolved = resolved.saturating_add(1);
    }
    resolved
}

#[test]
fn bp16_p35_all_printings_set_only_the_enemy_leader_to_one() {
    for printing in ["BP16-P35", "BP16-T09", "EBD04-T05"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["pp"] = json!({"current":10_u8,"max":10_u8});
        initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"astaroth","card":printing}]);
        initial["players"]["P1"]["leader"]["life"] = json!(5_u8);
        let mut game = game(&initial);
        let cast = game.play_internal_card("astaroth", actor()).unwrap();
        answer(&mut game, cast);
        drain(&mut game);
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["P2"]["leader"]["life"], 1_u8, "{printing}");
        assert_eq!(view["P1"]["leader"]["life"], 5_u8, "{printing}");
        assert_eq!(view["P1"]["pp"]["current"], 0_u8, "{printing}");
    }
}
