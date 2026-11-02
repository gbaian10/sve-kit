#![expect(
    clippy::panic,
    reason = "Unexpected input invalidates printed payment scenarios."
)]
use super::*;

fn position(card: &str, form: &str) -> Value {
    let mut initial = draw_position();
    initial["turn"]["elapsed_turns"]["P1"] = json!(7_u8);
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
    initial
}

fn game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "source-evolution-cost",
    )
    .unwrap()
}

fn finish(game: &mut Game, mut play: CardPlay) {
    loop {
        while let CardPlay::Awaiting(question) = play {
            let answer = match &question.domain {
                InputDomain::Targets(domain) => {
                    Answer::Targets(domain.members.iter().map(|member| member.handle).collect())
                }
                InputDomain::WardEntry { .. } => Answer::WardEntry(false),
                InputDomain::ResourceEntryPayment { .. }
                | InputDomain::Quick { .. }
                | InputDomain::OptionalCost
                | InputDomain::CostMaterials(_)
                | InputDomain::BeforeDraw
                | InputDomain::Pending(_)
                | InputDomain::TriggerQuota(_)
                | InputDomain::Integer(_)
                | InputDomain::Order(_)
                | InputDomain::DamageReplacements(_)
                | InputDomain::DriveTrigger { .. }
                | InputDomain::Search(_)
                | InputDomain::SearchPartition(_)
                | InputDomain::Distribution(_)
                | InputDomain::SearchReplacement
                | InputDomain::Modes { .. }
                | InputDomain::TokenCapacity { .. } => panic!("unexpected payment input"),
            };
            play = game
                .answer_internal_card(question.actor, question.token, answer)
                .unwrap();
        }
        if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
        let Some(question) = game.internal_pending_choice().unwrap() else {
            break;
        };
        let InputDomain::Pending(options) = &question.domain else {
            panic!("printed trigger");
        };
        play = game
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap();
    }
}

fn play(game: &mut Game, id: &str) {
    let result = game.play_internal_card(id, PlayerId { seat: 1 }).unwrap();
    finish(game, result);
}

fn evolve(game: &mut Game, payment: u8) {
    let before = game.projection(View::Referee).unwrap()["P1"]["pp"]["current"]
        .as_u64()
        .unwrap();
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":payment}})
        )
        .unwrap()
        .outcome,
        "resolved",
        "the printed evolution payment must resolve"
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["pp"]["current"], before.checked_sub(u64::from(payment)).unwrap(), "only the declared evolution payment is deducted");
    assert_eq!(state["objects"]["source"]["evolved_with"], "form", "successful evolution attaches the selected form");
}

#[test]
fn bp08_089_three_amulets_change_real_evolution_payment_and_two_do_not() {
    for count in [2_u8, 3] {
        let mut initial = position("BP08-089", "BP08-090");
        initial["players"]["P1"]["zones"]["field"] = json!(
            (0..count)
                .map(|i| json!({"id":format!("tree{i}"),"card":"BP07-T03"}))
                .collect::<Vec<_>>()
        );
        let mut game = game(&initial);
        play(&mut game, "source");
        evolve(&mut game, if count == 3 { 0 } else { 2 });
    }
}

#[test]
fn sd01_009_combo_three_sets_source_cost_to_zero_without_changing_another_copy() {
    for count in [0_u8, 2] {
        let mut initial = position("SD01-009", "SD01-010");
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"other","card":"SD01-009"}]);
        for i in 0..count {
            initial["players"]["P1"]["zones"]["hand"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":format!("prior{i}"),"card":"BP01-171"}));
        }
        let mut game = game(&initial);
        for i in 0..count {
            play(&mut game, &format!("prior{i}"));
        }
        play(&mut game, "source");
        let mut other = game.clone();
        let before = other.projection(View::Referee).unwrap()["P1"]["pp"]["current"]
            .as_u64()
            .unwrap();
        assert_eq!(
            other
                .internal_rule_action(
                    &json!({"do":"evolve","source":"other","evolve_card":"form","pay":{"pp":2_u8}})
                )
                .unwrap()
                .outcome,
            "resolved"
        );
        assert_eq!(
            other.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
            before - 2
        );
        evolve(&mut game, if count == 2 { 0 } else { 2 });
    }
}
