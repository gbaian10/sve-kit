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

/// Discards `discarded`, picks the printed mode at `mode`, and targets the first member.
fn answer(game: &mut Game, mut step: CardPlay, discarded: &str, mode: usize) {
    while let CardPlay::Awaiting(q) = step {
        let answer = match &q.domain {
            InputDomain::CostMaterials(d) => Answer::CostMaterials(vec![
                d.members
                    .iter()
                    .find(|m| m.card.as_deref() == Some(discarded))
                    .unwrap()
                    .handle,
            ]),
            InputDomain::Modes { modes, .. } => Answer::Modes(vec![modes[mode]]),
            InputDomain::Targets(d) => Answer::Targets(vec![d.members[0].handle]),
            other => panic!("unexpected printed decision: {other:?}"),
        };
        step = game.answer_internal_card(q.actor, q.token, answer).unwrap();
    }
    finish_checks(game);
}

/// Resolves every pending trigger and returns how many there were.
fn triggers(game: &mut Game) -> usize {
    let mut resolved = 0_usize;
    while let Some(q) = game.internal_pending_choice().unwrap() {
        let InputDomain::Pending(options) = &q.domain else {
            panic!("printed pending ability");
        };
        let step = game
            .answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
            .unwrap();
        answer(game, step, "", 0);
        resolved = resolved.checked_add(1).unwrap();
    }
    resolved
}

fn activated() -> AbilityId {
    selection().current["BP09-006"]
        .program
        .card()
        .abilities
        .iter()
        .find(|ability| ability.node.kind == AbilityKind::Activated)
        .unwrap()
        .node
        .id
}

fn position(discarded: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP09-006"}]);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"discarded","card":discarded}]);
    initial["players"]["P1"]["zones"]["deck"] =
        json!([{"id":"drawn","card":"BP01-037"},{"id":"rest","card":"BP01-037"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    initial
}

#[test]
fn bp09_006_draws_once_when_it_discards_an_elf_spell_and_casts_either_mode() {
    for mode in [0, 1] {
        let mut game = game(&position("BP01-023"));
        let before = game.projection(View::Referee).unwrap();
        let step = game
            .activate_internal_card("source", activated(), actor())
            .unwrap();
        answer(&mut game, step, "BP01-023", mode);
        assert_eq!(triggers(&mut game), 1, "mode {mode}: one draw trigger");
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["P1"]["hand"], json!(["drawn"]), "mode {mode}");
        assert_eq!(view["P1"]["cemetery"], json!(["discarded"]), "mode {mode}");
        assert_eq!(view["objects"]["source"]["acted"], true, "mode {mode}");
        if mode == 0 {
            assert_eq!(view["objects"]["enemy"]["damage"], 4_u8);
            assert_eq!(view["P2"]["leader"]["life"], before["P2"]["leader"]["life"]);
        } else {
            assert_eq!(view["objects"]["enemy"]["damage"], 0_u8);
            assert_eq!(
                view["P2"]["leader"]["life"].as_u64().unwrap() + 2,
                before["P2"]["leader"]["life"].as_u64().unwrap()
            );
        }
    }
}

#[test]
fn bp09_006_does_not_draw_for_other_discarded_cards() {
    // A Witch spell and an Elf follower: each half of the Elf-spell filter must hold.
    for discarded in ["BP01-071", "BP01-021"] {
        let mut game = game(&position(discarded));
        let step = game
            .activate_internal_card("source", activated(), actor())
            .unwrap();
        answer(&mut game, step, discarded, 1);
        assert_eq!(triggers(&mut game), 0, "{discarded}");
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["P1"]["hand"], json!([]), "{discarded}");
        assert_eq!(view["P1"]["cemetery"], json!(["discarded"]), "{discarded}");
    }
}

#[test]
fn bp09_006_cannot_activate_without_a_card_to_discard() {
    let mut initial = position("BP01-023");
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    let mut game = game(&initial);
    let before = game.projection(View::Referee).unwrap();
    let outcome = game.activate_internal_card("source", activated(), actor());
    assert!(
        matches!(outcome, Err(EngineFailure::IllegalPlay(_))),
        "{outcome:?}"
    );
    assert_eq!(game.projection(View::Referee).unwrap(), before);
}
