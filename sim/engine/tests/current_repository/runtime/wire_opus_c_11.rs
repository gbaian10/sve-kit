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

/// Answers printed questions: targets prefer `chosen` (else the first member).
fn answer(game: &mut Game, mut step: CardPlay, chosen: &str) {
    while let CardPlay::Awaiting(q) = step {
        let reply = match &q.domain {
            InputDomain::Targets(d) => Answer::Targets(
                d.members
                    .iter()
                    .find(|m| m.card.as_deref() == Some(chosen))
                    .or_else(|| d.members.first())
                    .map(|m| m.handle)
                    .into_iter()
                    .collect(),
            ),
            InputDomain::CostMaterials(d) => Answer::CostMaterials(
                d.members
                    .iter()
                    .take(usize::try_from(d.cardinality.minimum.0).unwrap())
                    .map(|m| m.handle)
                    .collect(),
            ),
            InputDomain::OptionalCost => Answer::OptionalCost(true),
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            other => panic!("unexpected printed decision: {other:?}"),
        };
        step = game.answer_internal_card(q.actor, q.token, reply).unwrap();
    }
    finish_checks(game);
}

/// Resolves every pending printed trigger in offer order; returns how many resolved.
fn drain(game: &mut Game, chosen: &str) -> usize {
    let mut resolved = 0_usize;
    finish_checks(game);
    while let Some(q) = game.internal_pending_choice().unwrap() {
        let InputDomain::Pending(options) = &q.domain else {
            panic!("printed pending ability");
        };
        let step = game
            .answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
            .unwrap();
        answer(game, step, chosen);
        resolved = resolved.saturating_add(1);
    }
    resolved
}

/// Opens the next pending trigger and returns the offered hand cards of its first choice.
fn offered_choice(game: &mut Game) -> (CardPlay, Vec<String>) {
    finish_checks(game);
    let q = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &q.domain else {
        panic!("printed trigger");
    };
    let step = game
        .answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
        .unwrap();
    let CardPlay::Awaiting(pick) = &step else {
        panic!("hand choice");
    };
    let InputDomain::Targets(d) = &pick.domain else {
        panic!("hand choice: {:?}", pick.domain);
    };
    let mut offered: Vec<_> = d.members.iter().filter_map(|m| m.card.clone()).collect();
    offered.sort();
    (step, offered)
}

fn board(hand: &Value, field: &Value) -> Game {
    let mut initial = draw_position();
    initial["players"]["P1"]["pp"] = json!({"current":10_u8,"max":10_u8});
    initial["players"]["P1"]["zones"]["hand"] = hand.clone();
    initial["players"]["P1"]["zones"]["field"] = field.clone();
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    game(&initial)
}

#[test]
fn bp21_008_all_printings_put_a_cheap_academy_or_beast_follower_and_it_triggers_line_1() {
    for printing in ["BP21-008", "BP21-P03"] {
        let mut game = board(
            &json!([
                {"id":"source","card":printing},
                {"id":"elza","card":"BP08-042"},
                {"id":"frog","card":"BP04-094"},
                {"id":"big","card":"BP16-037"},
                {"id":"rhino","card":"BP01-004"}
            ]),
            &json!([]),
        );
        let entered = game.play_internal_card("source", actor()).unwrap();
        assert!(matches!(entered, CardPlay::Entered(_)), "{printing}: the dwarf enters");
        let (step, offered) = offered_choice(&mut game);
        // The cost-5 academy follower and the insect follower are not offered.
        assert_eq!(offered, ["BP04-094", "BP08-042"], "{printing}");
        answer(&mut game, step, "BP04-094");
        // The entering beast triggers line 1 on the enemy follower and its leader.
        assert_eq!(drain(&mut game, "BP01-037"), 1, "{printing}");
        let view = game.projection(View::Referee).unwrap();
        assert!(
            view["P1"]["field"].as_array().unwrap().contains(&json!("frog")),
            "{printing}: {}",
            view["P1"]
        );
        assert_eq!(view["P2"]["cemetery"], json!(["enemy"]), "{printing}");
        assert_eq!(view["P2"]["leader"]["life"], 19_u8, "{printing}");
    }
}

#[test]
fn bp21_008_ignores_followers_without_either_trait() {
    let mut game = board(
        &json!([
            {"id":"rhino","card":"BP01-004"},
            {"id":"elza","card":"BP08-042"}
        ]),
        &json!([{"id":"source","card":"BP21-008"}]),
    );
    let insect = game.play_internal_card("rhino", actor()).unwrap();
    answer(&mut game, insect, "");
    // Only リノセウス's own Fanfare resolves.
    assert_eq!(drain(&mut game, ""), 1);
    let quiet = game.projection(View::Referee).unwrap();
    assert_eq!(quiet["P2"]["leader"]["life"], 20_u8);
    assert_eq!(quiet["objects"]["enemy"]["damage"], 0_u8);
    let academy = game.play_internal_card("elza", actor()).unwrap();
    assert!(matches!(academy, CardPlay::Entered(_)), "the academy follower enters");
    assert_eq!(drain(&mut game, "BP01-037"), 1);
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P2"]["cemetery"], json!(["enemy"]));
    assert_eq!(view["P2"]["leader"]["life"], 19_u8);
}
