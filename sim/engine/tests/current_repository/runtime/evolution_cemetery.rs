#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate the printed card scenario."
)]
use super::*;
use sve_engine::game::Step;

const fn actor() -> PlayerId {
    PlayerId { seat: 1 }
}

fn position(count: usize) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP22-078"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP22-079"}]);
    initial["players"]["P1"]["zones"]["cemetery"] = json!(
        (0..count)
            .map(|i| json!({"id":format!("grave-{i}"),"card":"BP22-078"}))
            .collect::<Vec<_>>()
    );
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"top-1","card":"BP22-078"},
        {"id":"top-2","card":"BP22-078"},
        {"id":"remaining","card":"BP01-037"}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP04-117"}]);
    initial
}

fn game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "evolution-cemetery",
    )
    .unwrap()
}

fn enter(game: &mut Game) {
    let CardPlay::Entered(entry) = game.play_internal_card("source", actor()).unwrap() else {
        panic!("ordinary printed card play")
    };
    if entry.resume == ResumeStage::CheckTiming {
        game.resume_internal_card().unwrap();
    }
}

fn declare(game: &mut Game, card: &str) -> CardPlay {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("printed pending trigger")
    };
    assert_eq!(options.len(), 1, "only the printed trigger is pending");
    assert_eq!(
        options[0].card, card,
        "the actual printing supplies the effect"
    );
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::Pending(options[0].handle),
    )
    .unwrap()
}

fn evolve(game: &mut Game) -> Step {
    game.internal_rule_action(
        &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}}),
    )
    .unwrap()
}

#[test]
fn bp22_078_fanfare_mills_two_and_unlocks_the_printed_one_pp_evolution() {
    let mut game = game(&position(8));
    enter(&mut game);
    let entered = game.projection(View::Referee).unwrap();
    assert_eq!(
        entered["P1"]["pp"]["current"], 8_u8,
        "the normal play pays two PP"
    );
    assert_eq!(
        entered["P1"]["cemetery"].as_array().unwrap().len(),
        8,
        "the grave threshold has not yet been reached"
    );
    resolved(declare(&mut game, "BP22-078"));
    game.resume_internal_card().unwrap();
    let milled = game.projection(View::Referee).unwrap();
    assert_eq!(
        milled["P1"]["cemetery"].as_array().unwrap().len(),
        10,
        "both top cards enter the cemetery"
    );
    assert_eq!(
        milled["P1"]["deck"],
        json!(["remaining"]),
        "the printed two-card mill preserves the remaining deck"
    );
    assert_eq!(
        evolve(&mut game).outcome,
        "resolved",
        "the newly reached threshold is usable immediately"
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        state["P1"]["pp"]["current"], 7_u8,
        "the evolution costs exactly one additional PP"
    );
    assert_eq!(
        state["objects"]["source"]["power"], 3_u8,
        "the paired evolved printing supplies its stats"
    );
    assert_eq!(
        state["objects"]["source"]["hp"], 3_u8,
        "the paired evolved printing supplies its life"
    );
    assert_eq!(
        state["P1"]["cemetery"], milled["P1"]["cemetery"],
        "the ten cards qualify without being paid"
    );
}

#[test]
fn bp22_079_real_evolution_requires_a_target_then_deals_six_and_heals_two() {
    for enemy in [false, true] {
        let mut initial = position(10);
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP22-078"}]);
        if !enemy {
            initial["players"]["P2"]["zones"]["field"] = json!([]);
        }
        let mut game = game(&initial);
        assert_eq!(
            evolve(&mut game).outcome,
            "resolved",
            "the base really evolves into BP22-079"
        );
        let play = declare(&mut game, "BP22-079");
        if enemy {
            let CardPlay::Awaiting(target) = play else {
                panic!("required formal target")
            };
            let InputDomain::Targets(domain) = &target.domain else {
                panic!("enemy follower domain")
            };
            assert_eq!(
                domain.members.len(),
                1,
                "only the enemy follower is eligible"
            );
            assert_eq!(
                domain.members[0].player,
                PlayerId { seat: 2 },
                "the own evolved follower is not selectable"
            );
            resolved(
                game.answer_internal_card(
                    target.actor,
                    target.token,
                    Answer::Targets(vec![domain.members[0].handle]),
                )
                .unwrap(),
            );
        } else {
            assert!(
                matches!(play, CardPlay::Cancelled),
                "Q2847: no enemy follower means the ability cannot be played"
            );
        }
        game.resume_internal_card().unwrap();
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P1"]["leader"]["life"],
            if enemy { 12_u8 } else { 10 },
            "Q2847: healing requires successfully playing the targeted ability"
        );
        if enemy {
            assert_eq!(
                state["objects"]["enemy"]["damage"], 6_u8,
                "the printed effect deals six"
            );
        }
        assert_eq!(
            state["P1"]["pp"]["current"], 9_u8,
            "the effect adds no PP payment"
        );
    }
}

#[test]
fn bp22_078_rechecks_the_cemetery_after_a_previous_failed_attempt() {
    let mut initial = position(8);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"older","card":"BP22-078"}]);
    let mut game = game(&initial);
    let attempt = json!({"do":"evolve","source":"older","evolve_card":"form","pay":{"pp":1_u8}});
    let before = game.projection(View::Referee).unwrap();
    let refused = game.internal_rule_action(&attempt).unwrap();
    assert_eq!(
        refused.outcome, "cannot-evolve",
        "eight matching cards do not qualify"
    );
    assert!(
        refused.events.is_empty(),
        "a refused attempt reveals nothing"
    );
    assert_eq!(
        game.projection(View::Referee).unwrap(),
        before,
        "the failed attempt spends neither PP nor the form nor the evolution allowance"
    );
    enter(&mut game);
    resolved(declare(&mut game, "BP22-078"));
    game.resume_internal_card().unwrap();
    assert_eq!(
        game.internal_rule_action(&attempt).unwrap().outcome,
        "resolved",
        "the live count is checked again after the second base's Fanfare"
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        state["objects"]["older"]["power"], 3_u8,
        "the originally refused holder can now evolve"
    );
    assert_eq!(
        state["objects"]["source"]["power"], 2_u8,
        "playing another copy does not evolve it"
    );
}
