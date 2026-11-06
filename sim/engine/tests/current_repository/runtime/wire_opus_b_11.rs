#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate the printed scenarios."
)]
use super::*;
use sve_engine::game::internal::SearchAnswer;

const fn actor() -> PlayerId {
    PlayerId { seat: 1 }
}

fn finish_checks(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}

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

fn activated(number: &str) -> AbilityId {
    selection().current[number]
        .program
        .card()
        .abilities
        .iter()
        .find(|ability| ability.node.kind == AbilityKind::Activated)
        .unwrap()
        .node
        .id
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

#[test]
fn bp09_001_limits_the_original_cost_by_distinct_elf_spell_names_and_frees_the_pick() {
    // Two distinct names give X = 2, so the cost-2 spell counts itself and is offered.
    let cemetery = json!([
        {"id":"circle","card":"BP01-023"},
        {"id":"curse","card":"BP01-024"}
    ]);
    let mut game = game(&position("BP09-001", cemetery));
    let step = game
        .activate_internal_card("source", activated("BP09-001"), actor())
        .unwrap();
    let CardPlay::Awaiting(q) = &step else {
        panic!("the target is chosen");
    };
    let InputDomain::Targets(d) = &q.domain else {
        panic!("target question");
    };
    let mut offered: Vec<_> = d.members.iter().filter_map(|m| m.card.clone()).collect();
    offered.sort();
    offered.dedup();
    assert_eq!(offered, ["BP01-023", "BP01-024"]);
    answer(&mut game, step, "BP01-024");
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["ex"].as_array().unwrap().len(), 1);
    assert_eq!(view["P1"]["pp"]["current"], 9_u8);
    let placed = view["P1"]["ex"][0].as_str().unwrap().to_owned();
    let cast = game.play_internal_card(&placed, actor()).unwrap();
    answer(&mut game, cast, "BP01-037");
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        9_u8,
        "the placed spell costs 0"
    );
}

#[test]
fn bp09_001_counts_names_not_copies() {
    // Two copies of one name give X = 1, so the cost-2 spell is not a legal target.
    let cemetery = json!([
        {"id":"curse","card":"BP01-024"},
        {"id":"curse-2","card":"BP01-024"}
    ]);
    let mut game = game(&position("BP09-001", cemetery));
    let before = game.projection(View::Referee).unwrap()["P1"].clone();
    let outcome = game.activate_internal_card("source", activated("BP09-001"), actor());
    assert!(
        matches!(outcome, Err(_) | Ok(CardPlay::Cancelled)),
        "no legal target exists: {outcome:?}"
    );
    assert_eq!(game.projection(View::Referee).unwrap()["P1"], before);
}
