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
                    .find(|m| m.card.as_deref() == Some(chosen))
                    .or_else(|| d.members.first())
                    .map(|m| m.handle)
                    .into_iter()
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

fn ernesta_end_phase(printing: &str, partner: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"source","card":printing},
        {"id":"partner","card":partner}
    ]);
    let mut game = game(&initial);
    assert_eq!(
        game.internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap()
            .outcome,
        "resolved",
        "{printing}: the end phase starts"
    );
    drain(&mut game, "");
    game.projection(View::Referee).unwrap()
}

#[test]
fn bp10_035_all_printings_ping_leaders_only_beside_an_other_named_merchant() {
    for printing in ["BP10-035", "BP10-P08"] {
        let merchant = ernesta_end_phase(printing, "BP09-033");
        assert_eq!(merchant["P2"]["leader"]["life"], 18_u8, "{printing}");
        // Another 武器商人・エルネスタ shares the name, so neither copy finds a partner.
        let twin = ernesta_end_phase(printing, "BP10-035");
        assert_eq!(twin["P2"]["leader"]["life"], 20_u8, "{printing}");
    }
}

#[test]
fn bp10_035_fanfare_sharpens_another_royal_follower() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP10-035"}]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"ally","card":"BP01-037"}]);
    let mut game = game(&initial);
    let before = game.projection(View::Referee).unwrap();
    let entered = game.play_internal_card("source", actor()).unwrap();
    assert!(matches!(entered, CardPlay::Entered(_)), "the merchant enters");
    assert_eq!(drain(&mut game, "BP01-037"), 1);
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(
        view["objects"]["ally"]["power"].as_u64().unwrap(),
        before["objects"]["ally"]["power"].as_u64().unwrap().saturating_add(1)
    );
    assert!(
        view["objects"]["ally"]["keywords"]
            .as_array()
            .unwrap()
            .contains(&json!("指定攻撃")),
        "{}",
        view["objects"]["ally"]
    );
}

#[test]
fn bp15_106_discards_a_ward_follower_to_draw() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"source","card":"BP15-106"},
        {"id":"plain","card":"BP01-037"},
        {"id":"guard","card":"BP01-088"}
    ]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"top","card":"BP01-037"}]);
    let mut game = game(&initial);
    // 【守護】 asks whether it enters acted; it enters standing.
    let entered = game.play_internal_card("source", actor()).unwrap();
    answer(&mut game, entered, "");
    assert_eq!(drain(&mut game, "BP01-088"), 1);
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["cemetery"], json!(["guard"]));
    assert_eq!(view["P1"]["hand"], json!(["plain", "top"]));
}

#[test]
fn bp15_106_cannot_pay_without_a_ward_follower_in_hand() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"source","card":"BP15-106"},
        {"id":"plain","card":"BP01-037"}
    ]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"top","card":"BP01-037"}]);
    let mut game = game(&initial);
    // 【守護】 asks whether it enters acted; it enters standing.
    let entered = game.play_internal_card("source", actor()).unwrap();
    answer(&mut game, entered, "");
    drain(&mut game, "");
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["hand"], json!(["plain"]));
    assert_eq!(view["P1"]["cemetery"], json!([]));
}
