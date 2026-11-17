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

/// Answers printed questions with every offered member, up to each bound.
fn answer(game: &mut Game, mut step: CardPlay) -> CardPlay {
    while let CardPlay::Awaiting(q) = step {
        let answer = match &q.domain {
            InputDomain::Targets(d) | InputDomain::CostMaterials(d) => {
                let most = d
                    .cardinality
                    .maximum
                    .map_or(d.members.len(), |n| usize::try_from(n.0).unwrap());
                let handles = d.members.iter().take(most).map(|m| m.handle).collect();
                if matches!(q.domain, InputDomain::Targets(_)) {
                    Answer::Targets(handles)
                } else {
                    Answer::CostMaterials(handles)
                }
            }
            other => panic!("unexpected printed decision: {other:?}"),
        };
        step = game.answer_internal_card(q.actor, q.token, answer).unwrap();
    }
    finish_checks(game);
    step
}

fn triggers(game: &mut Game) {
    while let Some(q) = game.internal_pending_choice().unwrap() {
        let InputDomain::Pending(options) = &q.domain else {
            panic!("printed pending ability");
        };
        let step = game
            .answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
            .unwrap();
        answer(game, step);
    }
}

#[test]
fn bp07_020_all_printings_cast_the_named_spell_from_the_cemetery_for_free() {
    for printing in ["BP07-020", "BP07-SL06"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":printing}]);
        initial["players"]["P1"]["zones"]["field"] = json!([]);
        initial["players"]["P1"]["zones"]["cemetery"] =
            // The other spell is first, so a broken name filter would pick it instead.
            json!([{"id":"other","card":"BP01-071"},{"id":"blade","card":"BP07-028"}]);
        initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
        let mut game = game(&initial);
        let step = game.play_internal_card("source", actor()).unwrap();
        answer(&mut game, step);
        let before = game.projection(View::Referee).unwrap()["P1"]["pp"]["current"].clone();
        triggers(&mut game);
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(
            view["P1"]["pp"]["current"], before,
            "{printing}: the spell is free"
        );
        // Misutorina is on the field, so the blade deals 6 instead of 2 and destroys the 4/4.
        assert_eq!(view["P2"]["field"], json!([]), "{printing}");
        assert_eq!(view["P2"]["cemetery"], json!(["enemy"]), "{printing}");
        assert_eq!(
            view["P1"]["cemetery"],
            json!(["other", "blade"]),
            "{printing}"
        );
    }
}
