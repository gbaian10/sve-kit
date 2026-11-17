#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate the printed scenarios."
)]
use super::*;

/// Answers printed questions: targets prefer `chosen` (else the first member), orders keep the offer.
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
            InputDomain::Order(cards) => Answer::Order(cards.iter().map(|c| c.handle).collect()),
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

fn evolve(game: &mut Game, sep: u8) {
    let outcome = game
        .internal_rule_action(&json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8,"sep":sep}}))
        .unwrap();
    assert_eq!(
        outcome.outcome, "resolved",
        "the printed evolution is legal"
    );
}

fn azurfleet(form: &str, sep: u8) -> Game {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"source","card":"BP20-057"},
        {"id":"ally","card":"BP01-037"}
    ]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
    initial["players"]["P1"]["sep"] = json!(sep);
    initial["turn"]["elapsed_turns"] = json!({"P1":7_u8,"P2":6_u8});
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    let mut game = game(&initial);
    evolve(&mut game, sep);
    game
}

#[test]
fn bp20_058_evolution_pings_every_follower_and_its_own_ability_damage_pings_enemies_again() {
    for form in ["BP20-058", "BP20-SL18"] {
        let mut game = azurfleet(form, 0);
        // Evolution damages every follower including itself; that own-turn ability damage triggers line 2.
        assert_eq!(drain(&mut game, ""), 2, "{form}");
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["objects"]["enemy"]["damage"], 2_u8, "{form}");
        assert_eq!(view["objects"]["ally"]["damage"], 1_u8, "{form}");
        assert_eq!(view["objects"]["source"]["damage"], 1_u8, "{form}");
        assert_eq!(view["P2"]["leader"]["life"], 20_u8, "{form}");
    }
}
