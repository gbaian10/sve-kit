use super::*;
use sve_engine::game::{Game, Step, View, internal::DecisionView};

fn missing_base_ability(reason: &str) -> bool {
    reason.contains("unloaded whole card ECP01-001/0")
        && reason.contains("query evolution")
        && reason.contains("current ability definitions unavailable")
}

fn evolve_position(seat: &str) -> Value {
    let mut initial = draw_position();
    initial["turn"]["active"] = json!(seat);
    initial["turn"]["elapsed_turns"] = json!({"P1":7_u8,"P2":6_u8});
    initial["players"][seat]["zones"]["field"] = json!([{"id":"base","card":"ECP01-001"}]);
    initial["players"][seat]["zones"]["evolve_deck"] =
        json!([{"id":"resource","card":"ECP01-002"}]);
    // The supported spell cards isolate the evolve pair from unrelated context gates.
    for owner in ["P1", "P2"] {
        initial["players"][owner]["zones"]["hand"] =
            json!([{"id":format!("{owner}-helper"),"card":"CP02-068"}]);
        initial["players"][owner]["zones"]["deck"] = json!([
            {"id":format!("{owner}-d1"),"card":"CP02-068"},
            {"id":format!("{owner}-d2"),"card":"CP02-068"}
        ]);
    }
    initial["players"][seat]["pp"]["current"] = json!(1_u8);
    initial
}

fn projections(game: &Game) -> Result<Vec<Value>, EngineFailure> {
    [View::P1, View::P2, View::Referee]
        .into_iter()
        .map(|view| game.projection(view))
        .collect()
}

fn other_seat(seat: &str) -> &str {
    if seat == "P1" { "P2" } else { "P1" }
}

/// The blocked diagnostic is the deliverable while the gate is closed: exit 0 never means executed.
#[expect(
    clippy::print_stderr,
    reason = "A passing blocked test must still name the gate it recorded."
)]
fn record(seat: &str, message: &str) {
    eprintln!("ECP01-002 {seat} {message}");
}

fn assert_pre_evolve_gate(
    game: &mut Game,
    stage_before: Option<ResumeStage>,
    seat: &str,
    reason: &str,
) {
    assert!(
        missing_base_ability(reason),
        "only the missing base ability may block"
    );
    assert_eq!(
        game.internal_card_stage(),
        stage_before,
        "a blocked evolve leaves the card stage untouched"
    );
    assert!(
        game.internal_pending_choice().unwrap().is_none(),
        "a blocked evolve opens no pending choice"
    );
    assert!(
        game.internal_quick_choice().unwrap().is_none(),
        "a blocked evolve opens no quick choice"
    );
    record(
        seat,
        &format!("blocked before evolve events by the base ability query: {reason}"),
    );
}

fn assert_evolve_events(step: &Step, seat: &str) {
    assert_eq!(step.outcome, "resolved", "the evolve action resolves");
    for kind in ["公開", "移動", "進化"] {
        assert_eq!(
            step.events
                .iter()
                .filter(|event| event["kind"] == kind)
                .count(),
            1,
            "expected exactly one {kind} event"
        );
    }
    let at = |kind: &str| {
        step.events
            .iter()
            .position(|event| event["kind"] == kind)
            .unwrap()
    };
    let (reveal, movement, evolution) = (at("公開"), at("移動"), at("進化"));
    assert!(
        reveal < movement && movement < evolution,
        "reveal, then move, then evolve"
    );
    assert_eq!(
        step.events[reveal]["object"], "resource",
        "resource is revealed"
    );
    assert_eq!(step.events[reveal]["to"], "all", "the reveal is public");
    assert_eq!(
        step.events[movement]["from"],
        format!("{seat}.evolve_deck"),
        "the resource leaves the owner's evolve deck"
    );
    assert_eq!(
        step.events[movement]["to"],
        format!("{seat}.evolution"),
        "the resource moves to the evolution zone"
    );
    assert_eq!(step.events[evolution]["object"], "base", "base evolves");
    assert_eq!(
        step.events[evolution]["with"], "resource",
        "base evolves with the resource"
    );
}

fn assert_evolved_states(states: &[Value], seat: &str, opponent: &str) {
    for state in states {
        assert_eq!(
            state[seat]["evolve_deck"],
            json!([]),
            "evolve deck is spent"
        );
        assert_eq!(
            state[seat]["evolution"],
            json!(["resource"]),
            "resource sits in the evolution zone"
        );
        assert_eq!(state[seat]["pp"]["current"], 0_u8, "1 PP is paid");
        assert_eq!(state["objects"]["base"]["card"], "ECP01-001", "base card");
        assert_eq!(
            state["objects"]["base"]["evolved_with"], "resource",
            "base records its resource"
        );
        assert_eq!(
            state["objects"]["base"]["generation"], 0_u8,
            "base generation"
        );
        assert_eq!(
            state["objects"]["resource"]["card"], "ECP01-002",
            "resource card"
        );
        assert_eq!(
            state["objects"]["resource"]["generation"], 1_u8,
            "resource generation"
        );
        assert_eq!(state["objects"]["base"]["power"], 3_u8, "printed power");
        assert_eq!(state["objects"]["base"]["hp"], 3_u8, "printed hp");
        assert_eq!(
            projected_zone_count(&state[seat]["hand"]),
            1_u64,
            "no draw before the ability resolves"
        );
        assert_eq!(
            projected_zone_count(&state[seat]["deck"]),
            2_u64,
            "owner deck untouched before the draw"
        );
        assert_eq!(
            projected_zone_count(&state[opponent]["hand"]),
            1_u64,
            "opponent hand untouched"
        );
        assert_eq!(
            projected_zone_count(&state[opponent]["deck"]),
            2_u64,
            "opponent deck untouched"
        );
    }
}

#[expect(
    clippy::panic,
    reason = "An unexpected view failure after the evolve event is a broken scenario."
)]
fn report_post_evolve_gate(game: &Game, seat: &str, reason: &str) {
    match projections(game) {
        Ok(states) => assert_evolved_states(&states, seat, other_seat(seat)),
        Err(EngineFailure::Unsupported(view_reason))
            if view_reason.contains("unloaded whole card")
                || view_reason == "current projection requires unavailable abilities" =>
        {
            record(
                seat,
                &format!("projection stopped at resource admission: {view_reason}"),
            );
        }
        Err(error) => panic!("unexpected view failure after evolve event: {error:?}"),
    }
    record(
        seat,
        &format!("evolve event resolved; resource ability blocked: {reason}"),
    );
}

fn assert_evolve_draw_views(game: &Game, seat: &str, opponent: &str, opponent_view: View) {
    for view in [View::P1, View::P2, View::Referee] {
        let state = game.projection(view).unwrap();
        assert_eq!(
            projected_zone_count(&state[seat]["hand"]),
            2_u64,
            "owner hand gains one"
        );
        assert_eq!(
            projected_zone_count(&state[seat]["deck"]),
            1_u64,
            "owner deck loses one"
        );
        assert_eq!(
            projected_zone_count(&state[opponent]["hand"]),
            1_u64,
            "opponent hand unchanged"
        );
        assert_eq!(
            projected_zone_count(&state[opponent]["deck"]),
            2_u64,
            "opponent deck unchanged"
        );
        if view == opponent_view {
            assert!(
                !state[seat]["hand"]
                    .to_string()
                    .contains(&format!("{seat}-d1")),
                "the opponent never sees the drawn card"
            );
        }
        if view == View::Referee {
            let hand = state[seat]["hand"].as_array().unwrap();
            assert!(
                hand.contains(&json!(format!("{seat}-helper"))),
                "the helper stays in hand"
            );
            assert!(
                hand.contains(&json!(format!("{seat}-d1"))),
                "the top card is drawn"
            );
            assert_eq!(
                state[seat]["deck"],
                json!([format!("{seat}-d2")]),
                "the second card stays on the deck"
            );
            assert_eq!(
                state[opponent]["deck"],
                json!([format!("{opponent}-d1"), format!("{opponent}-d2")]),
                "the opponent deck keeps its order"
            );
        }
    }
}

#[expect(
    clippy::panic,
    reason = "A missing evolved view or non-pending domain means the scenario is wrong."
)]
fn resolve_evolve_draw(
    game: &mut Game,
    seat: &str,
    opponent: &str,
    opponent_view: View,
    question: &DecisionView,
) {
    let states =
        projections(game).unwrap_or_else(|error| panic!("evolved views unavailable: {error:?}"));
    assert_evolved_states(&states, seat, opponent);
    let InputDomain::Pending(options) = &question.domain else {
        panic!("expected the ECP01-002 evolve ability pending")
    };
    assert_eq!(options.len(), 1, "exactly the resource's Evolve ability");
    assert_eq!(
        options[0].card, "ECP01-002",
        "the resource owns the ability"
    );
    assert_eq!(options[0].line, 1, "printed line");
    assert!(options[0].event.contains("Evolve"), "evolve-time trigger");
    assert_eq!(
        question.actor.seat,
        if seat == "P1" { 1 } else { 2 },
        "the controller chooses"
    );
    let result = resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap(),
    );
    assert!(
        result
            .progress
            .facts
            .iter()
            .any(|fact| matches!(fact, Fact::Draw(_))),
        "the ability resolves with a draw fact"
    );
    assert_evolve_draw_views(game, seat, opponent, opponent_view);
}

#[expect(
    clippy::panic,
    reason = "Any gate other than the recorded one, or a missing trigger, is a scenario failure."
)]
fn run_real_evolution(seat: &str) {
    let opponent = other_seat(seat);
    let opponent_view = if seat == "P1" { View::P2 } else { View::P1 };
    let mut game = game(&evolve_position(seat));
    let stage_before = game.internal_card_stage();
    let step = match game.internal_rule_action(&json!({
        "do":"evolve","source":"base","evolve_card":"resource","pay":{"pp":1_u8}
    })) {
        Ok(step) => step,
        Err(EngineFailure::Unsupported(reason)) if missing_base_ability(&reason) => {
            assert_pre_evolve_gate(&mut game, stage_before, seat, &reason);
            return;
        }
        Err(EngineFailure::Unsupported(reason)) => panic!(
            "the real evolve producer must pass its runtime gate before checking resource abilities: {reason}"
        ),
        Err(error) => panic!("unexpected evolve failure: {error:?}"),
    };
    assert_evolve_events(&step, seat);
    match game.internal_pending_choice() {
        Err(EngineFailure::Unsupported(reason))
            if reason.contains("evolution resource printed abilities are not admitted")
                || reason.contains("unloaded whole card") =>
        {
            report_post_evolve_gate(&game, seat, &reason);
        }
        Ok(Some(question)) => {
            resolve_evolve_draw(&mut game, seat, opponent, opponent_view, &question);
        }
        Ok(None) => panic!("ECP01-002 evolve-time draw did not trigger"),
        Err(error) => panic!("unexpected pending-choice failure: {error:?}"),
    }
}

#[test]
fn ecp01_002_real_evolution_triggers_or_records_the_context_gate() {
    // CR 5.16.1.1.1-.3, 12.2.1-.4: use the printed Evolve relation ECP01-001 -> ECP01-002.
    for seat in ["P1", "P2"] {
        run_real_evolution(seat);
    }
}
