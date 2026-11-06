#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate the printed scenarios."
)]
use super::*;
use sve_engine::game::internal::SearchAnswer;

const fn actor() -> PlayerId {
    PlayerId { seat: 1 }
}

fn position(card: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":card}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"drawn","card":"BP01-037"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    initial
}

fn finish_checks(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}

/// Answers each printed question, preferring the member showing `chosen`.
fn answer(game: &mut Game, mut step: CardPlay, chosen: &str) -> CardPlay {
    while let CardPlay::Awaiting(q) = step {
        let InputDomain::Targets(d) = &q.domain else {
            panic!("unexpected printed decision: {:?}", q.domain);
        };
        let member = d
            .members
            .iter()
            .find(|m| m.card.as_deref() == Some(chosen))
            .or_else(|| d.members.first())
            .unwrap();
        step = game
            .answer_internal_card(q.actor, q.token, Answer::Targets(vec![member.handle]))
            .unwrap();
    }
    finish_checks(game);
    step
}

fn offered(step: &CardPlay) -> Vec<String> {
    let CardPlay::Awaiting(q) = step else {
        panic!("a choice is open");
    };
    let InputDomain::Targets(d) = &q.domain else {
        panic!("target-like choice");
    };
    let mut cards: Vec<_> = d.members.iter().filter_map(|m| m.card.clone()).collect();
    cards.sort();
    cards
}

/// The last activated ability; CP01-035's first one is its {食事}.
fn activated(number: &str) -> AbilityId {
    selection().current[number]
        .program
        .card()
        .abilities
        .iter()
        .rev()
        .find(|ability| ability.node.kind == AbilityKind::Activated)
        .unwrap()
        .node
        .id
}

fn pp(game: &Game) -> Value {
    game.projection(View::Referee).unwrap()["P1"]["pp"]["current"].clone()
}

fn play_spell(game: &mut Game, id: &str) {
    let step = game.play_internal_card(id, actor()).unwrap();
    let step = answer(game, step, "BP01-037");
    assert!(
        !matches!(step, CardPlay::Cancelled),
        "the discounted spell resolves"
    );
}

#[test]
fn cp01_035_all_printings_set_only_the_placed_spell_to_zero_this_turn() {
    for printing in ["CP01-035", "CP01-P26"] {
        let mut initial = position(printing);
        initial["players"]["P1"]["zones"]["hand"] = json!([
            {"id":"cheap","card":"BP01-071"},
            {"id":"other","card":"BP01-024"},
            {"id":"costly","card":"BP01-065"}
        ]);
        let mut game = game(&initial);
        let step = game
            .activate_internal_card("source", activated(printing), actor())
            .unwrap();
        // Current cost 3 is not offered; the two cheap spells are.
        assert_eq!(offered(&step), ["BP01-024", "BP01-071"], "{printing}");
        answer(&mut game, step, "BP01-071");
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["P1"]["ex"], json!(["cheap"]), "{printing}");
        assert_eq!(view["objects"]["source"]["acted"], true, "{printing}");
        play_spell(&mut game, "cheap");
        assert_eq!(pp(&game), 10_u8, "{printing}");
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
            2_u8
        );
        // The unplaced hand spell keeps its printed cost.
        play_spell(&mut game, "other");
        assert_eq!(pp(&game), 8_u8, "{printing}");
    }
}

fn pending_first(game: &mut Game) -> Option<CardPlay> {
    let q = game.internal_pending_choice().unwrap()?;
    let InputDomain::Pending(options) = &q.domain else {
        panic!("printed pending ability");
    };
    Some(
        game.answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
            .unwrap(),
    )
}

/// Ends the active turn, answering this card's search and the opponent's Quick window.
fn end_turn(game: &mut Game, wanted: &str) {
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    while let Some(mut step) = pending_first(game) {
        while let CardPlay::Awaiting(q) = step {
            let InputDomain::Search(d) = &q.domain else {
                panic!("unexpected printed decision: {:?}", q.domain);
            };
            let picks = d
                .cards
                .iter()
                .filter(|c| c.card == wanted)
                .map(|c| c.handle)
                .collect();
            step = game
                .answer_internal_card(
                    q.actor,
                    q.token,
                    Answer::Search(SearchAnswer::Ungrouped(picks)),
                )
                .unwrap();
        }
        finish_checks(game);
    }
    let q = game.internal_quick_choice().unwrap().unwrap();
    game.answer_internal_card(q.actor, q.token, Answer::Quick(None))
        .unwrap();
    finish_checks(game);
}

fn fillers(seat: &str) -> Vec<Value> {
    (0_u8..4)
        .map(|i| json!({"id":format!("{seat}-filler{i}"),"card":"BP01-037"}))
        .collect()
}

#[test]
fn bp02_091_all_printings_leave_at_four_prayers_and_set_the_found_card_to_zero() {
    for printing in ["BP02-091", "BP02-SL18", "EBD04-008"] {
        for prayers in [2_u8, 3] {
            let mut initial = position(printing);
            initial["players"]["P1"]["leader"]["life"] = json!(5_u8);
            initial["players"]["P1"]["zones"]["field"][0]["state"] =
                json!({"counters":{"祈りカウンター":prayers}});
            let mut deck = vec![
                json!({"id":"found","card":"BP01-065"}),
                json!({"id":"left","card":"BP01-024"}),
            ];
            deck.extend(fillers("P1"));
            initial["players"]["P1"]["zones"]["deck"] = Value::Array(deck);
            initial["players"]["P2"]["zones"]["deck"] = Value::Array(fillers("P2"));
            let mut game = game(&initial);
            end_turn(&mut game, "BP01-065");
            let view = game.projection(View::Referee).unwrap();
            if prayers == 2 {
                // Three prayers are not enough: it stays and nothing else happens.
                assert_eq!(view["P1"]["field"], json!(["source"]), "{printing}");
                assert_eq!(view["P1"]["leader"]["life"], 5_u8, "{printing}");
                assert_eq!(view["P1"]["ex"], json!([]), "{printing}");
                continue;
            }
            assert_eq!(view["P1"]["field"], json!([]), "{printing}");
            assert_eq!(view["P1"]["cemetery"], json!(["source"]), "{printing}");
            assert_eq!(view["P1"]["leader"]["life"], 15_u8, "{printing}");
            assert_eq!(view["P1"]["ex"], json!(["found"]), "{printing}");
            // Without "this turn" the set cost still applies on a later own turn.
            end_turn(&mut game, "");
            let before = pp(&game);
            play_spell(&mut game, "found");
            assert_eq!(pp(&game), before, "{printing}");
        }
    }
}
