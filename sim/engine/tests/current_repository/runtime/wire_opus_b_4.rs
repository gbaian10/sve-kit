#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate the printed scenarios."
)]
use super::*;
use sve_engine::game::internal::SearchAnswer;

fn position() -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"drawn","card":"BP01-037"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-012"}]);
    initial
}

fn finish_checks(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}

fn limit(cardinality: &Cardinality, available: usize) -> usize {
    cardinality
        .maximum
        .map_or(available, |most| usize::try_from(most.0).unwrap())
        .min(available)
}

/// Answers printed questions with every offered member, up to each bound.
fn answer(game: &mut Game, mut step: CardPlay) -> CardPlay {
    while let CardPlay::Awaiting(q) = step {
        let answer = match &q.domain {
            InputDomain::OptionalCost => Answer::OptionalCost(true),
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            InputDomain::Targets(d) | InputDomain::CostMaterials(d) => {
                let handles = d
                    .members
                    .iter()
                    .take(limit(&d.cardinality, d.members.len()))
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
                            .take(limit(&d.cardinality, d.cards.len()))
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
                                    .take(limit(&g.cardinality, g.handles.len()))
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

#[test]
fn bp12_070_all_printings_buff_only_original_cost_one_followers_on_field_and_ex() {
    for printing in ["BP12-070", "BP12-SL19"] {
        let mut initial = position();
        initial["players"]["P1"]["zones"]["field"] = json!([
            {"id":"source","card":"BP12-069"},
            {"id":"one","card":"BP01-012"},
            {"id":"two","card":"BP01-002"}
        ]);
        initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"ex-one","card":"BP01-146"}]);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":printing}]);
        let mut game = game(&initial);
        let before = game.projection(View::Referee).unwrap();
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}}),
        )
        .unwrap();
        triggers(&mut game);
        let after = game.projection(View::Referee).unwrap();
        let power = |view: &Value, id: &str| view["objects"][id]["power"].as_u64().unwrap();
        assert_eq!(power(&after, "one"), power(&before, "one") + 1, "{printing}");
        assert_eq!(power(&after, "ex-one"), power(&before, "ex-one") + 1, "{printing}");
        assert_eq!(power(&after, "two"), power(&before, "two"), "{printing}");
        assert_eq!(power(&after, "enemy"), power(&before, "enemy"), "{printing}");
    }
}
