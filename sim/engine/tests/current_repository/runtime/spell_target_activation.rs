#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate the printed scenarios."
)]
use super::*;
use sve_engine::game::internal::SearchAnswer;

/// Answers printed questions, preferring members showing `wanted`.
fn answer(game: &mut Game, mut step: CardPlay, wanted: &str) -> CardPlay {
    while let CardPlay::Awaiting(q) = step {
        let answer = match &q.domain {
            InputDomain::Targets(d) => {
                let member = d
                    .members
                    .iter()
                    .find(|m| m.card.as_deref() == Some(wanted))
                    .or_else(|| d.members.first())
                    .unwrap();
                Answer::Targets(vec![member.handle])
            }
            InputDomain::Search(d) => Answer::Search(SearchAnswer::Ungrouped(
                d.cards.iter().take(1).map(|c| c.handle).collect(),
            )),
            other => panic!("unexpected printed decision: {other:?}"),
        };
        step = game.answer_internal_card(q.actor, q.token, answer).unwrap();
    }
    finish_checks(game);
    step
}

fn position(printing: &str, cemetery: Value) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":printing}]);
    initial["players"]["P1"]["zones"]["cemetery"] = cemetery;
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    initial
}

#[test]
fn bp09_001_all_printings_search_an_elf_spell() {
    for printing in ["BP09-001", "BP09-SL01", "BP09-U01"] {
        let mut initial = position(printing, json!([]));
        initial["players"]["P1"]["zones"]["field"] = json!([]);
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":printing}]);
        initial["players"]["P1"]["zones"]["deck"] = json!([
            {"id":"elf-spell","card":"BP01-023"},
            {"id":"plain","card":"BP01-037"}
        ]);
        let mut game = game(&initial);
        let played = game.play_internal_card("source", actor()).unwrap();
        answer(&mut game, played, "");
        let q = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = &q.domain else {
            panic!("fanfare");
        };
        let step = game
            .answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
            .unwrap();
        answer(&mut game, step, "");
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["P1"]["hand"], json!(["elf-spell"]), "{printing}");
    }
}
