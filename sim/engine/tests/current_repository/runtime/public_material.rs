use super::*;
use sve_engine::game::internal::DecisionView;

fn game_with_rules(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "public-material",
    )
    .unwrap()
}

#[expect(clippy::panic, reason = "A different question is a scenario failure.")]
fn awaiting(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(question) = play else {
        panic!("expected play question");
    };
    question
}

fn fanfare(game: &mut Game) -> DecisionView {
    let mut play = game
        .play_internal_card("payer", PlayerId { seat: 1 })
        .unwrap();
    while let CardPlay::Awaiting(question) = play {
        assert!(
            matches!(question.domain, InputDomain::WardEntry { .. }),
            "only Ward posture can interrupt entry"
        );
        play = game
            .answer_internal_card(question.actor, question.token, Answer::WardEntry(false))
            .unwrap();
    }
    assert!(
        matches!(play, CardPlay::Entered(_)),
        "the follower enters before its fanfare"
    );
    let question = game.internal_pending_choice().unwrap().unwrap();
    awaiting(
        game.answer_internal_card(question.actor, question.token, Answer::Pending(1))
            .unwrap(),
    )
}

#[test]
fn bp05_013_declares_then_targets_then_pays_ex_material_before_ability_play() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"payer","card":"BP05-013"}]);
    initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"fee","card":"BP01-168"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    let baseline = game_with_rules(&initial);
    for accepted in [false, true] {
        let mut game = baseline.clone();
        let declaration = fanfare(&mut game);
        assert_eq!(declaration.domain, InputDomain::OptionalCost);
        let before = format!("{game:?}");
        game.answer_internal_card(
            PlayerId { seat: 2 },
            declaration.token,
            Answer::OptionalCost(false),
        )
        .unwrap_err();
        assert_eq!(format!("{game:?}"), before);
        let result = game
            .answer_internal_card(
                declaration.actor,
                declaration.token,
                Answer::OptionalCost(accepted),
            )
            .unwrap();
        if !accepted {
            assert_eq!(result, CardPlay::Cancelled);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["P1"]["ex"], json!(["fee"]));
            assert_eq!(state["objects"]["enemy"]["damage"], 0_u8);
            game.resume_internal_card().unwrap();
            assert!(game.internal_pending_choice().unwrap().is_none());
            continue;
        }
        let targets = awaiting(result);
        assert!(matches!(targets.domain, InputDomain::Targets(_)));
        let materials = awaiting(
            game.answer_internal_card(targets.actor, targets.token, Answer::Targets(vec![0]))
                .unwrap(),
        );
        assert!(matches!(materials.domain, InputDomain::CostMaterials(_)));
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["ex"],
            json!(["fee"])
        );
        let material_authority = format!("{game:?}");
        game.answer_internal_card(
            materials.actor,
            materials.token,
            Answer::CostMaterials(vec![]),
        )
        .unwrap_err();
        assert_eq!(format!("{game:?}"), material_authority);
        let activation = resolved(
            game.answer_internal_card(
                materials.actor,
                materials.token,
                Answer::CostMaterials(vec![0]),
            )
            .unwrap(),
        );
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["banish"], json!(["fee"]));
        assert_eq!(state["objects"]["enemy"]["damage"], 5_u8);
        let played = activation
            .progress
            .facts
            .iter()
            .position(|fact| matches!(fact, Fact::AbilityPlayed { .. }))
            .unwrap();
        let moved = activation
            .progress
            .facts
            .iter()
            .position(|fact| matches!(fact, Fact::Movement(_)))
            .unwrap();
        let damage = activation
            .progress
            .facts
            .iter()
            .position(|fact| matches!(fact, Fact::Damage(_)))
            .unwrap();
        assert!(moved < played && played < damage);
    }
}

#[test]
fn bp06_017_can_decline_without_hunters_but_still_requires_a_target_and_pays_the_spell() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"spell","card":"BP06-017"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    let mut game = game(&initial);
    let pp = game.projection(View::Referee).unwrap()["P1"]["pp"]["current"]
        .as_i64()
        .unwrap();
    let declaration = awaiting(
        game.play_internal_card("spell", PlayerId { seat: 1 })
            .unwrap(),
    );
    assert_eq!(declaration.domain, InputDomain::OptionalCost);
    let target = awaiting(
        game.answer_internal_card(
            declaration.actor,
            declaration.token,
            Answer::OptionalCost(false),
        )
        .unwrap(),
    );
    assert!(matches!(target.domain, InputDomain::Targets(_)));
    let result = resolved(
        game.answer_internal_card(target.actor, target.token, Answer::Targets(vec![0]))
            .unwrap(),
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["cemetery"], json!(["spell"]));
    assert_eq!(state["P1"]["pp"]["current"], pp - 1);
    assert_eq!(state["objects"]["enemy"]["damage"], 0_u8);
    assert!(
        result
            .progress
            .facts
            .iter()
            .any(|fact| matches!(fact, Fact::CardPlayed { .. }))
    );
}

#[test]
fn bp15_013_moves_only_a_token_to_ex_and_full_ex_cancels_without_payment() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"payer","card":"BP15-013"}]);
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"token","card":"BP01-T02"}, {"id":"ordinary","card":"BP01-037"}
    ]);
    for capacity in [4_usize, 5] {
        initial["players"]["P1"]["zones"]["ex"] = Value::Array(
            (0..capacity)
                .map(|n| json!({"id":format!("ex{n}"),"card":"BP01-T02"}))
                .collect(),
        );
        let mut game = game(&initial);
        let declaration = fanfare(&mut game);
        let materials = awaiting(
            game.answer_internal_card(
                declaration.actor,
                declaration.token,
                Answer::OptionalCost(true),
            )
            .unwrap(),
        );
        let InputDomain::CostMaterials(domain) = &materials.domain else {
            panic!("material question")
        };
        assert_eq!(domain.members.len(), 1);
        let before = game.projection(View::Referee).unwrap();
        let result = game
            .answer_internal_card(
                materials.actor,
                materials.token,
                Answer::CostMaterials(vec![0]),
            )
            .unwrap();
        let state = game.projection(View::Referee).unwrap();
        if capacity == 5 {
            assert_eq!(result, CardPlay::Cancelled);
            assert_eq!(state["P1"]["field"], before["P1"]["field"]);
            assert_eq!(state["P1"]["ex"], before["P1"]["ex"]);
            assert_eq!(
                state["P1"]["leader"]["life"],
                before["P1"]["leader"]["life"]
            );
        } else {
            resolved(result);
            assert_eq!(state["P1"]["ex"].as_array().unwrap().len(), 5);
            assert!(
                state["P1"]["ex"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("token"))
            );
            assert_eq!(
                state["P1"]["leader"]["life"].as_i64().unwrap(),
                before["P1"]["leader"]["life"].as_i64().unwrap() + 2
            );
        }
    }
}

#[test]
fn bp01_002_can_pay_by_returning_a_token_which_is_then_erased_by_rules() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"payer","card":"BP01-002"}]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"token","card":"BP01-T02"}]);
    let mut game = game(&initial);
    let declaration = fanfare(&mut game);
    let before = game.projection(View::Referee).unwrap();
    let materials = awaiting(
        game.answer_internal_card(
            declaration.actor,
            declaration.token,
            Answer::OptionalCost(true),
        )
        .unwrap(),
    );
    let InputDomain::CostMaterials(domain) = &materials.domain else {
        panic!("material question")
    };
    assert_eq!(domain.members.len(), 1);
    resolved(
        game.answer_internal_card(
            materials.actor,
            materials.token,
            Answer::CostMaterials(vec![0]),
        )
        .unwrap(),
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!([]));
    assert_eq!(state["P1"]["field"], json!(["payer"]));
    assert_eq!(state["P1"]["cemetery"], json!([]));
    assert_eq!(state["P1"]["banish"], json!([]));
    assert_eq!(
        state["objects"]["payer"]["power"].as_i64().unwrap(),
        before["objects"]["payer"]["power"].as_i64().unwrap() + 1
    );
}
