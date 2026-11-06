#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate the printed scenarios."
)]
use super::*;
use sve_engine::EngineFailure;
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
            InputDomain::Targets(d) => Answer::Targets(
                d.members
                    .iter()
                    .take(most(&d.cardinality, d.members.len()))
                    .map(|m| m.handle)
                    .collect(),
            ),
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

#[test]
fn bp10_063_all_printings_search_an_armed_follower_and_may_put_it_into_play() {
    for printing in ["BP10-063", "BP10-P13", "PR-251"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":printing}]);
        initial["players"]["P1"]["zones"]["field"] = json!([]);
        initial["players"]["P1"]["zones"]["deck"] = json!([
            {"id":"leva","card":"BP03-056"},
            {"id":"plain","card":"BP01-037"}
        ]);
        let mut game = game(&initial);
        let step = game.play_internal_card("source", actor()).unwrap();
        answer(&mut game, step);
        triggers(&mut game);
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["P1"]["field"], json!(["leva"]), "{printing}");
        assert_eq!(view["P1"]["deck"], json!(["plain"]), "{printing}");
    }
}

#[test]
fn bp10_063_destroys_leva_and_two_weapons_to_put_a_dual_mode_advance_into_play() {
    for weapons in [1_u8, 2] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        let mut field = vec![json!({"id":"leva","card":"BP03-056"})];
        field.extend((0..weapons).map(|i| json!({"id":format!("weapon{i}"),"card":"BP03-T04"})));
        initial["players"]["P1"]["zones"]["field"] = Value::Array(field);
        initial["players"]["P1"]["zones"]["cemetery"] = json!([{"id":"source","card":"BP10-063"}]);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([
            {"id":"beta","card":"BP10-058"},
            {"id":"leva-form","card":"BP03-057"}
        ]);
        let mut game = game(&initial);
        if weapons == 1 {
            // Q1314: it can be played only when Leva and two weapons can be chosen.
            let before = game.projection(View::Referee).unwrap();
            let mut outcome = game.activate_internal_card("source", activated("BP10-063"), actor());
            while let Ok(CardPlay::Awaiting(q)) = &outcome {
                let InputDomain::Targets(d) = &q.domain else {
                    panic!("only the formal targets are asked: {:?}", q.domain);
                };
                let handles = d.members.iter().map(|m| m.handle).collect();
                outcome = game.answer_internal_card(q.actor, q.token, Answer::Targets(handles));
            }
            assert!(
                matches!(
                    outcome,
                    Ok(CardPlay::Cancelled) | Err(EngineFailure::IllegalPlay(_))
                ),
                "the declaration fails: {outcome:?}"
            );
            let after = game.projection(View::Referee).unwrap();
            assert_eq!(after["P1"], before["P1"]);
            assert_eq!(after["P2"], before["P2"]);
            assert!(game.internal_pending_choice().unwrap().is_none());
            assert_eq!(game.internal_card_stage(), None);
            continue;
        }
        let step = game.activate_internal_card("source", activated("BP10-063"), actor());
        let mut step = step.unwrap();
        // The private Advance pick offers only the name containing デュアルモード.
        while let CardPlay::Awaiting(q) = &step {
            if let InputDomain::Targets(d) = &q.domain
                && d.members.iter().any(|m| m.card.as_deref() == Some("BP10-058"))
            {
                let offered: Vec<_> = d.members.iter().filter_map(|m| m.card.as_deref()).collect();
                assert_eq!(offered, ["BP10-058"]);
            }
            step = answer_once(&mut game, step);
        }
        finish_checks(&mut game);
        triggers(&mut game);
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["P1"]["field"], json!(["beta"]));
        assert_eq!(view["P1"]["banish"], json!(["source"]));
        assert_eq!(view["P1"]["evolve_deck"], json!(["leva-form"]));
    }
}

/// Answers exactly one printed question with every offered member.
fn answer_once(game: &mut Game, step: CardPlay) -> CardPlay {
    let CardPlay::Awaiting(q) = step else {
        return step;
    };
    let answer = match &q.domain {
        InputDomain::WardEntry { .. } => Answer::WardEntry(false),
        InputDomain::Targets(d) => Answer::Targets(
            d.members
                .iter()
                .take(most(&d.cardinality, d.members.len()))
                .map(|m| m.handle)
                .collect(),
        ),
        other => panic!("unexpected printed decision: {other:?}"),
    };
    game.answer_internal_card(q.actor, q.token, answer).unwrap()
}
