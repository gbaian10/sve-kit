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

fn most(cardinality: &Cardinality, available: usize) -> usize {
    cardinality
        .maximum
        .map_or(available, |n| usize::try_from(n.0).unwrap())
        .min(available)
}

/// Answers printed questions with every offered member, up to each bound.
fn answer(game: &mut Game, mut step: CardPlay) -> CardPlay {
    while let CardPlay::Awaiting(q) = step {
        let answer = match &q.domain {
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            InputDomain::OptionalCost => Answer::OptionalCost(true),
            InputDomain::Targets(d) | InputDomain::CostMaterials(d) => {
                let handles = d
                    .members
                    .iter()
                    .take(most(&d.cardinality, d.members.len()))
                    .map(|m| m.handle)
                    .collect();
                if matches!(q.domain, InputDomain::Targets(_)) {
                    Answer::Targets(handles)
                } else {
                    Answer::CostMaterials(handles)
                }
            }
            InputDomain::Search(d) => Answer::Search(d.groups.as_ref().map_or_else(
                || {
                    SearchAnswer::Ungrouped(
                        d.cards
                            .iter()
                            .take(most(&d.cardinality, d.cards.len()))
                            .map(|c| c.handle)
                            .collect(),
                    )
                },
                |groups| {
                    SearchAnswer::Grouped(
                        groups
                            .iter()
                            .map(|g| {
                                g.handles
                                    .iter()
                                    .take(most(&g.cardinality, g.handles.len()))
                                    .copied()
                                    .collect()
                            })
                            .collect(),
                    )
                },
            )),
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

fn position(printing: &str, fodder: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] =
        json!([{"id":"source","card":printing},{"id":"fodder","card":fodder}]);
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"ward-two","card":"BP01-046"},
        {"id":"ward-one","card":"BP01-146"},
        {"id":"plain-two","card":"BP01-068"}
    ]);
    initial
}

#[test]
fn bp22_095_all_printings_discard_a_ward_follower_to_field_a_two_and_a_one_cost_ward() {
    for printing in ["BP22-095", "BP22-SL25"] {
        let mut game = game(&position(printing, "BP01-046"));
        let step = game.play_internal_card("source", actor()).unwrap();
        answer(&mut game, step);
        triggers(&mut game);
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["P1"]["cemetery"], json!(["fodder"]), "{printing}");
        let mut field: Vec<_> = view["P1"]["field"].as_array().unwrap().clone();
        field.sort_by_key(|id| id.as_str().map(str::to_owned));
        assert_eq!(
            field,
            [json!("source"), json!("ward-one"), json!("ward-two")],
            "{printing}"
        );
        assert_eq!(view["P1"]["deck"], json!(["plain-two"]), "{printing}");
    }
}

#[test]
fn bp22_095_cannot_pay_with_a_follower_without_ward() {
    let mut game = game(&position("BP22-095", "BP01-068"));
    let step = game.play_internal_card("source", actor()).unwrap();
    answer(&mut game, step);
    triggers(&mut game);
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["hand"], json!(["fodder"]));
    assert_eq!(view["P1"]["field"], json!(["source"]));
    assert_eq!(view["P1"]["deck"].as_array().unwrap().len(), 3);
}
