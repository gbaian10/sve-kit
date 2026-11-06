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

fn position() -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    initial["players"]["P1"]["zones"]["deck"] = json!([]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    initial
}

fn finish_checks(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}

/// Picks up to `most` handles, preferring members whose card number is in `wanted`.
fn prefer(members: &[(u64, Option<String>)], wanted: &[&str], most: usize) -> Vec<u64> {
    let mut picks: Vec<u64> = Vec::new();
    for card in wanted {
        if let Some((handle, _)) = members
            .iter()
            .find(|(h, c)| c.as_deref() == Some(*card) && !picks.contains(h))
        {
            picks.push(*handle);
        }
    }
    picks.truncate(most);
    picks
}

fn most(cardinality: &Cardinality, available: usize) -> usize {
    cardinality
        .maximum
        .map_or(available, |n| usize::try_from(n.0).unwrap())
        .min(available)
}

/// Answers printed questions with the `wanted` cards (repeat a number to take two copies).
fn answer(game: &mut Game, mut step: CardPlay, wanted: &[&str]) -> CardPlay {
    while let CardPlay::Awaiting(q) = step {
        let answer = match &q.domain {
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            InputDomain::Targets(d) => {
                let members: Vec<_> = d.members.iter().map(|m| (m.handle, m.card.clone())).collect();
                Answer::Targets(prefer(&members, wanted, most(&d.cardinality, members.len())))
            }
            InputDomain::Order(cards) => Answer::Order(cards.iter().map(|c| c.handle).collect()),
            InputDomain::Search(d) => {
                let members: Vec<_> = d
                    .cards
                    .iter()
                    .map(|c| (c.handle, Some(c.card.clone())))
                    .collect();
                Answer::Search(d.groups.as_ref().map_or_else(
                    || {
                        SearchAnswer::Ungrouped(prefer(
                            &members,
                            wanted,
                            most(&d.cardinality, members.len()),
                        ))
                    },
                    |groups| {
                        SearchAnswer::Grouped(
                            groups
                                .iter()
                                .map(|g| {
                                    let inside: Vec<_> = members
                                        .iter()
                                        .filter(|(h, _)| g.handles.contains(h))
                                        .cloned()
                                        .collect();
                                    prefer(&inside, wanted, most(&g.cardinality, inside.len()))
                                })
                                .collect(),
                        )
                    },
                ))
            }
            other => panic!("unexpected printed decision: {other:?}"),
        };
        step = game.answer_internal_card(q.actor, q.token, answer).unwrap();
    }
    finish_checks(game);
    step
}

fn triggers(game: &mut Game, wanted: &[&str]) {
    while let Some(q) = game.internal_pending_choice().unwrap() {
        let InputDomain::Pending(options) = &q.domain else {
            panic!("printed pending ability");
        };
        let step = game
            .answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
            .unwrap();
        answer(game, step, wanted);
    }
}

fn pp(game: &Game) -> u64 {
    game.projection(View::Referee).unwrap()["P1"]["pp"]["current"]
        .as_u64()
        .unwrap()
}

/// Plays `id` from EX and asserts that it resolved, left EX and cost nothing.
fn plays_free(game: &mut Game, id: &str, label: &str) {
    let before = pp(game);
    let step = game.play_internal_card(id, actor()).unwrap();
    let step = answer(game, step, &["BP01-037"]);
    assert!(
        matches!(step, CardPlay::Entered(_) | CardPlay::Resolved(_)),
        "{label}: {id} is played"
    );
    assert!(
        !ex(game).as_array().unwrap().contains(&json!(id)),
        "{label}: {id} left EX"
    );
    assert_eq!(pp(game), before, "{label}: {id} costs 0");
}

fn ex(game: &Game) -> Value {
    game.projection(View::Referee).unwrap()["P1"]["ex"].clone()
}

fn evolve(game: &mut Game, wanted: &[&str]) {
    game.internal_rule_action(
        &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}}),
    )
    .unwrap();
    triggers(game, wanted);
}

#[test]
fn bp17_021_all_printings_search_two_cheap_nature_spells_for_free() {
    for printing in ["BP17-021", "BP17-SL07"] {
        let mut initial = position();
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP17-020"}]);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":printing}]);
        initial["players"]["P1"]["zones"]["deck"] = json!([
            {"id":"tree-a","card":"BP07-017"},
            {"id":"tree-b","card":"BP07-017"},
            {"id":"costly","card":"BP07-024"}
        ]);
        let mut game = game(&initial);
        evolve(&mut game, &["BP07-017", "BP07-017"]);
        assert_eq!(ex(&game), json!(["tree-a", "tree-b"]), "{printing}");
        plays_free(&mut game, "tree-a", printing);
    }
}

#[test]
fn bp09_037_all_printings_search_earth_sigils_within_four_for_free() {
    for printing in ["BP09-037", "BP09-SL09", "BP09-U03"] {
        let mut initial = position();
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":printing}]);
        initial["players"]["P1"]["zones"]["deck"] = json!([
            {"id":"draw","card":"BP01-075"},
            {"id":"heal","card":"BP03-048"},
            {"id":"costly","card":"BP01-066"},
            {"id":"spare","card":"BP01-037"}
        ]);
        let mut game = game(&initial);
        let step = game.play_internal_card("source", actor()).unwrap();
        answer(&mut game, step, &[]);
        triggers(&mut game, &["BP01-075", "BP03-048"]);
        assert_eq!(ex(&game), json!(["draw", "heal"]), "{printing}");
        plays_free(&mut game, "heal", printing);
        triggers(&mut game, &[]);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
            23_u8,
            "{printing}"
        );
    }
}

#[test]
fn bp16_096_all_printings_search_other_wilderness_cards_for_free_and_add_a_bike() {
    for printing in ["BP16-096", "BP16-SL26"] {
        let mut initial = position();
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":printing}]);
        initial["players"]["P1"]["zones"]["deck"] = json!([
            {"id":"rider","card":"BP11-019"},
            {"id":"biker","card":"BP11-027"},
            {"id":"same","card":"BP16-096"}
        ]);
        let mut game = game(&initial);
        let step = game.play_internal_card("source", actor()).unwrap();
        answer(&mut game, step, &[]);
        // The same-named copy is preferred first, so an unfiltered search would pick it.
        triggers(&mut game, &["BP16-096", "BP11-019", "BP11-027"]);
        let view = game.projection(View::Referee).unwrap();
        let mut placed: Vec<_> = view["P1"]["ex"].as_array().unwrap().clone();
        placed.sort_by_key(|id| id.as_str().map(str::to_owned));
        assert_eq!(placed, [json!("biker"), json!("rider")], "{printing}");
        assert_eq!(view["P1"]["field"].as_array().unwrap().len(), 2, "{printing}");
        plays_free(&mut game, "rider", printing);
    }
}
