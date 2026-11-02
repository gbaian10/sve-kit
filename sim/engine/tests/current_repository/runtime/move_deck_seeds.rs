#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate the printed deck movement scenarios."
)]
use super::*;
use sve_engine::game::internal::DecisionView;

fn game(state: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        state,
        &Value::Null,
        &Value::Null,
        "move-deck",
    )
    .unwrap()
}

fn awaiting(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(q) = play else {
        panic!("printed target")
    };
    q
}

fn trigger(game: &mut Game, number: &str) -> DecisionView {
    let q = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &q.domain else {
        panic!("printed trigger")
    };
    let handle = options.iter().find(|o| o.card == number).unwrap().handle;
    awaiting(
        game.answer_internal_card(q.actor, q.token, Answer::Pending(handle))
            .unwrap(),
    )
}

#[test]
fn suzuka_evolution_returns_an_opposing_follower_to_its_owners_deck_but_erases_a_token() {
    for (token, card) in [(false, "BP01-037"), (true, "BP01-T03")] {
        let mut state = draw_position();
        state["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"CP01-001"}]);
        state["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"CP01-002"}]);
        state["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":card}]);
        state["players"]["P2"]["zones"]["deck"] = json!([{"id":"tail","card":"BP01-037"}]);
        let mut game = game(&state);
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":2_u8}}),
        )
        .unwrap();
        let q = trigger(&mut game, "CP01-002");
        let InputDomain::Targets(domain) = &q.domain else {
            panic!("opposing follower target")
        };
        assert_eq!(domain.members.len(), 1);
        let result = resolved(
            game.answer_internal_card(
                q.actor,
                q.token,
                Answer::Targets(vec![domain.members[0].handle]),
            )
            .unwrap(),
        );
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["P2"]["field"], json!([]));
        if token {
            assert_eq!(view["P2"]["deck"], json!(["tail"]));
            assert!(view["objects"].get("enemy").is_none());
            assert!(result.progress.facts.iter().any(|fact| matches!(fact,
            Fact::Movement(movement) if movement.movement.actual_to.is_some_and(|to| to.kind == ZoneKind::Deck)
                && movement.movement.actual_position.is_none())));
        } else {
            assert_eq!(view["P2"]["deck"], json!(["enemy", "tail"]));
            assert_eq!(view["objects"]["enemy"]["generation"], 1_u8);
        }
        assert_eq!(view["P1"]["deck"], json!([]));
    }
}

#[test]
fn silent_tom_puts_one_chosen_cemetery_card_third_from_the_top() {
    let mut state = draw_position();
    state["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"CP03-108"}]);
    state["players"]["P1"]["zones"]["cemetery"] = json!([{"id":"returned","card":"BP01-034"}]);
    state["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"one","card":"BP01-037"}, {"id":"two","card":"BP01-037"},
        {"id":"three","card":"BP01-037"}
    ]);
    let mut game = game(&state);
    assert!(
        matches!(
            game.play_internal_card("source", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ),
        "the printed follower enters before its fanfare"
    );
    let q = trigger(&mut game, "CP03-108");
    let InputDomain::Targets(domain) = &q.domain else {
        panic!("cemetery target")
    };
    assert_eq!(domain.cardinality.minimum, Nat(0));
    assert_eq!(domain.members.len(), 1);
    let result = resolved(
        game.answer_internal_card(
            q.actor,
            q.token,
            Answer::Targets(vec![domain.members[0].handle]),
        )
        .unwrap(),
    );
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["cemetery"], json!([]));
    assert_eq!(
        view["P1"]["deck"],
        json!(["one", "two", "returned", "three"])
    );
    assert!(result.progress.facts.iter().any(|fact| matches!(fact, Fact::Movement(movement)
        if matches!(movement.movement.requested_position, Some(Position::Index { index }) if index.0.get() == 3)
        && movement.movement.requested_position == movement.movement.actual_position)));
}

fn whale_fanfare(number: &str) {
    for from_ex in [false, true] {
        let mut state = draw_position();
        state["players"]["P1"]["zones"][if from_ex { "ex" } else { "hand" }] =
            json!([{"id":"source","card":number}]);
        let mut game = game(&state);
        let play = game
            .play_internal_card("source", PlayerId { seat: 1 })
            .unwrap();
        let CardPlay::Awaiting(q) = play else {
            panic!("printed Ward entry")
        };
        assert!(
            matches!(q.domain, InputDomain::WardEntry { .. }),
            "the real source has Ward"
        );
        assert!(
            matches!(
                game.answer_internal_card(q.actor, q.token, Answer::WardEntry(false))
                    .unwrap(),
                CardPlay::Entered(_)
            ),
            "the real source enters before its fanfare"
        );
        let pending = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = &pending.domain else {
            panic!("printed fanfare")
        };
        let option = options.iter().find(|o| o.card == number).unwrap();
        resolved(
            game.answer_internal_card(pending.actor, pending.token, Answer::Pending(option.handle))
                .unwrap(),
        );
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
            if from_ex { 9_u8 } else { 5_u8 },
            "only EX play refunds four PP after the printed five-PP payment"
        );
    }
}

fn whale_lastword(number: &str) {
    for bottom in [false, true] {
        let mut state = draw_position();
        state["players"]["P1"]["zones"]["field"] =
            json!([{"id":"source","card":number,"owner":"P2"}]);
        state["players"]["P1"]["zones"]["deck"] = json!([{"id":"own-tail","card":"BP01-037"}]);
        state["players"]["P2"]["zones"]["deck"] = json!([
            {"id":"first","card":"BP01-037"},{"id":"second","card":"BP01-037"},{"id":"third","card":"BP01-037"}
        ]);
        let mut game = with_producer(
            &state,
            Action::Destroy {
                objects: subjects(),
            },
        );
        resolved(
            game.activate_internal_card("producer", AbilityId(1), PlayerId { seat: 1 })
                .unwrap(),
        );
        game.resume_internal_card().unwrap();
        let q = trigger(&mut game, number);
        assert_eq!(
            q.actor,
            PlayerId { seat: 1 },
            "the last controller chooses even though the owner is P2"
        );
        let InputDomain::Modes { modes, .. } = &q.domain else {
            panic!("second-or-bottom resolution choice")
        };
        assert_eq!(
            modes.len(),
            2,
            "the controller chooses exactly between second and bottom"
        );
        assert_eq!(
            game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
            18_u8,
            "damage precedes the placement choice"
        );
        resolved(
            game.answer_internal_card(
                q.actor,
                q.token,
                Answer::Modes(vec![modes[usize::from(bottom)]]),
            )
            .unwrap(),
        );
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(
            view["P2"]["deck"],
            if bottom {
                json!(["first", "second", "third", "source"])
            } else {
                json!(["first", "source", "second", "third"])
            },
            "the chosen fixed position applies to the owner deck"
        );
        assert_eq!(
            view["P1"]["deck"],
            json!(["own-tail"]),
            "the last controller does not receive the foreign card"
        );
        assert_eq!(
            view["P2"]["cemetery"],
            json!([]),
            "the departed source leaves its owner cemetery"
        );
        assert_eq!(
            view["objects"]["source"]["generation"], 2_u8,
            "both physical zone changes advance the source generation"
        );
    }
}

#[test]
fn eternal_whale_recovers_pp_only_from_ex_and_its_last_controller_chooses_the_owners_deck_position()
{
    for number in ["BP10-062", "PR-616"] {
        whale_fanfare(number);
        whale_lastword(number);
    }
}
