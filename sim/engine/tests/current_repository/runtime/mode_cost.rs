#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate printed mode payment scenarios."
)]
use super::*;

fn position(card: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    initial["players"]["P1"]["zones"]["deck"] =
        json!([{"id":"draw1","card":"BP01-171"},{"id":"draw2","card":"BP01-171"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    initial
}
fn game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "printed-mode-payment",
    )
    .unwrap()
}
fn answer(input: &InputDomain, modes: &[u64]) -> Answer {
    if matches!(input, InputDomain::OptionalCost) {
        Answer::OptionalCost(true)
    } else if matches!(input, InputDomain::Modes { .. }) {
        Answer::Modes(modes.iter().copied().map(ModeId).collect())
    } else if let InputDomain::Targets(domain) | InputDomain::CostMaterials(domain) = input {
        let handles = domain
            .members
            .iter()
            .take(usize::try_from(domain.cardinality.minimum.0).unwrap())
            .map(|member| member.handle)
            .collect();
        if matches!(input, InputDomain::CostMaterials(_)) {
            Answer::CostMaterials(handles)
        } else {
            Answer::Targets(handles)
        }
    } else if matches!(input, InputDomain::WardEntry { .. }) {
        Answer::WardEntry(false)
    } else if let InputDomain::Pending(options) = input {
        Answer::Pending(options[0].handle)
    } else {
        panic!("unexpected mode payment decision");
    }
}

fn finish(game: &mut Game, mut result: CardPlay, modes: &[u64]) -> Result<(), EngineFailure> {
    loop {
        while let CardPlay::Awaiting(question) = result {
            let answer = answer(&question.domain, modes);
            result = game.answer_internal_card(question.actor, question.token, answer)?;
        }
        if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card()?;
        }
        let Some(question) = game.internal_pending_choice()? else {
            return Ok(());
        };
        result = game.answer_internal_card(
            question.actor,
            question.token,
            answer(&question.domain, modes),
        )?;
    }
}
fn play(game: &mut Game, modes: &[u64]) {
    let result = game
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap();
    finish(game, result, modes).unwrap();
}
#[test]
fn bp16_081_pays_for_each_selected_fanfare_mode_before_destroy_and_buff() {
    let mut game = game(&position("BP16-081"));
    play(&mut game, &[1, 2]);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        state["P1"]["pp"]["current"], 6_i64,
        "bp16 081 pays for each selected fanfare mode before destroy and buff"
    );
    assert_eq!(
        state["P2"]["field"],
        json!([]),
        "bp16 081 pays for each selected fanfare mode before destroy and buff"
    );
    assert_eq!(
        state["objects"]["source"]["power"], 4_i64,
        "bp16 081 pays for each selected fanfare mode before destroy and buff"
    );
    assert_eq!(
        state["objects"]["source"]["hp"], 4_i64,
        "bp16 081 pays for each selected fanfare mode before destroy and buff"
    );
    assert!(
        state["objects"]["source"]["keywords"]
            .as_array()
            .unwrap()
            .contains(&json!("守護")),
        "bp16 081 pays for each selected fanfare mode before destroy and buff"
    );
}
#[test]
fn bp17_053_charges_only_for_the_damage_mode() {
    for mode in [1, 2] {
        let mut game = game(&position("BP17-053"));
        play(&mut game, &[mode]);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P1"]["pp"]["current"],
            if mode == 1 { 5_i64 } else { 3_i64 },
            "bp17 053 charges only for the damage mode"
        );
        assert_eq!(
            state["P1"]["leader"]["life"],
            if mode == 1 { 25_i64 } else { 20_i64 },
            "bp17 053 charges only for the damage mode"
        );
        assert_eq!(
            state["P2"]["field"],
            if mode == 1 {
                json!(["enemy"])
            } else {
                json!([])
            },
            "bp17 053 charges only for the damage mode"
        );
    }
}
#[test]
fn bp19_090_two_paid_modes_charge_two_pp_and_each_applies_its_own_life_loss() {
    let mut game = game(&position("BP19-090"));
    play(&mut game, &[1, 2]);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        state["P1"]["pp"]["current"], 8_i64,
        "bp19 090 two paid modes charge two pp and each applies its own life loss"
    );
    assert_eq!(
        state["P1"]["leader"]["life"], 18_i64,
        "bp19 090 two paid modes charge two pp and each applies its own life loss"
    );
    assert_eq!(
        state["objects"]["enemy"]["damage"], 2_i64,
        "bp19 090 two paid modes charge two pp and each applies its own life loss"
    );
    assert_eq!(
        state["P1"]["hand"],
        json!(["draw1"]),
        "bp19 090 two paid modes charge two pp and each applies its own life loss"
    );
}
#[test]
fn unpayable_combined_modes_commit_neither_payment_nor_body_effects() {
    let mut initial = position("BP19-090");
    initial["players"]["P1"]["pp"] = json!({"current":1_i64,"max":1_i64});
    let mut game = game(&initial);
    let result = game
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap();
    finish(&mut game, result, &[1, 2]).unwrap_err();
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        state["P1"]["pp"]["current"], 1_i64,
        "unpayable combined modes commit neither payment nor body effects"
    );
    assert_eq!(
        state["P1"]["leader"]["life"], 20_i64,
        "unpayable combined modes commit neither payment nor body effects"
    );
    assert_eq!(
        state["objects"]["enemy"]["damage"], 0_i64,
        "unpayable combined modes commit neither payment nor body effects"
    );
    assert_eq!(
        state["P1"]["deck"],
        json!(["draw1", "draw2"]),
        "unpayable combined modes commit neither payment nor body effects"
    );
}

#[test]
fn bp16_081_third_mode_draws_and_discards_after_paying_its_own_pp() {
    let mut game = game(&position("BP16-081"));
    play(&mut game, &[1, 3]);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        state["P1"]["pp"]["current"], 6_i64,
        "bp16 081 third mode draws and discards after paying its own pp"
    );
    assert_eq!(
        state["P2"]["field"],
        json!([]),
        "bp16 081 third mode draws and discards after paying its own pp"
    );
    assert_eq!(
        state["objects"]["source"]["power"], 3_i64,
        "bp16 081 third mode draws and discards after paying its own pp"
    );
    assert_eq!(
        state["objects"]["source"]["hp"], 3_i64,
        "bp16 081 third mode draws and discards after paying its own pp"
    );
    assert_eq!(
        state["P1"]["hand"].as_array().unwrap().len(),
        1_usize,
        "bp16 081 third mode draws and discards after paying its own pp"
    );
    assert_eq!(
        state["P1"]["cemetery"].as_array().unwrap().len(),
        1_usize,
        "bp16 081 third mode draws and discards after paying its own pp"
    );
}
