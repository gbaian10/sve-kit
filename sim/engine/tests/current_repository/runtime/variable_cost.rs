#![expect(
    clippy::panic,
    reason = "Unexpected choices invalidate the printed cost scenarios."
)]
use super::*;

fn position(card: &str, zone: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"][zone] = json!([{"id":"source","card":card}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"drawn","card":"BP01-037"}]);
    initial
}

fn from_position(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "variable-printed-cost",
    )
    .unwrap()
}

fn finish(game: &mut Game, mut result: CardPlay) {
    while let CardPlay::Awaiting(question) = result {
        let answer = match &question.domain {
            InputDomain::Targets(domain) => {
                Answer::Targets(domain.members.iter().map(|member| member.handle).collect())
            }
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            InputDomain::TokenCapacity { choices } => {
                assert_eq!(
                    choices.len(),
                    1,
                    "the single printed token has one complete allocation"
                );
                Answer::TokenCapacity(0)
            }
            InputDomain::MealPayment { .. }
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
            | InputDomain::Modes { .. } => panic!("unexpected printed card decision"),
        };
        result = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}

fn play(game: &mut Game) {
    let result = game
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap();
    finish(game, result);
}

#[test]
fn bp18_013_counts_only_face_up_tokyo_evolution_cards_and_returns_real_followers() {
    for count in [0_usize, 2] {
        let mut initial = position("BP18-013", "hand");
        let mut forms = (0..count)
            .map(|i| json!({"id":format!("used-{i}"),"card":"BP18-072","face_up":true}))
            .collect::<Vec<_>>();
        forms.push(json!({"id":"unused","card":"BP18-072","face_up":false}));
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!(forms);
        initial["players"]["P1"]["zones"]["cemetery"] =
            json!([{"id":"friend-one","card":"BP18-024"},{"id":"friend-two","card":"BP18-024"}]);
        let mut game = from_position(&initial);
        play(&mut game);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["pp"]["current"], 5 + count);
        assert_eq!(state["P1"]["hand"], json!(["friend-one", "friend-two"]));
    }
}

#[test]
fn cp02_062_charges_for_both_fields_before_dealing_seven_to_every_follower() {
    let mut initial = position("CP02-062", "hand");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"own","card":"BP01-037"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    let mut game = from_position(&initial);
    play(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["pp"]["current"], 6_u8);
    assert_eq!(state["P1"]["field"], json!([]));
    assert_eq!(state["P2"]["field"], json!([]));
}

#[test]
fn bp10_113_discounts_only_from_ex_and_keeps_its_fanfare_damage() {
    for zone in ["hand", "ex"] {
        let mut initial = position("BP10-113", zone);
        initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
        let mut game = from_position(&initial);
        play(&mut game);
        let question = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = &question.domain else {
            panic!("printed Fanfare");
        };
        assert_eq!(options.len(), 1, "only the printed Fanfare is pending");
        let result = game
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap();
        finish(&mut game, result);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P1"]["pp"]["current"],
            if zone == "ex" { 9_u8 } else { 6_u8 }
        );
        assert_eq!(state["P2"]["field"], json!(["enemy"]));
        assert_eq!(state["objects"]["enemy"]["damage"], 4_u8);
    }
}

#[test]
fn bp19_073_keeps_its_ex_discount_draw_and_awakened_orca_count() {
    for zone in ["hand", "ex"] {
        for maximum in [6_u8, 7] {
            let mut initial = position("BP19-073", zone);
            initial["players"]["P1"]["pp"] = json!({"current":maximum,"max":maximum});
            let mut game = from_position(&initial);
            play(&mut game);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["P1"]["pp"]["current"],
                maximum - if zone == "ex" { 1 } else { 2 }
            );
            assert_eq!(state["P1"]["hand"], json!(["drawn"]));
            let field = state["P1"]["field"].as_array().unwrap();
            assert_eq!(field.len(), if maximum == 7 { 2 } else { 1 });
            for id in field {
                assert_eq!(state["objects"][id.as_str().unwrap()]["name"], "オルカ");
            }
        }
    }
}

#[test]
fn bp10_113_end_phase_buffs_only_its_other_friendly_followers() {
    let mut initial = position("BP10-113", "field");
    initial["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"ally","card":"BP01-037"}));
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    let mut game = from_position(&initial);
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("printed end phase trigger");
    };
    assert_eq!(
        options.len(),
        1,
        "only the printed end phase trigger is pending"
    );
    let result = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap();
    finish(&mut game, result);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["source"]["power"], 2_u8);
    assert_eq!(state["objects"]["ally"]["power"], 6_u8);
    assert_eq!(state["objects"]["enemy"]["power"], 5_u8);
}
