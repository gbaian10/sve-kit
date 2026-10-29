#![expect(
    clippy::panic,
    reason = "An unexpected decision invalidates the real card scenario."
)]
use super::*;
use sve_engine::game::internal::DecisionView;

const fn controller() -> PlayerId {
    PlayerId { seat: 1 }
}

fn game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "bp22-bases",
    )
    .unwrap()
}

fn position(card: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"ally","card":"BP01-037"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"first","card":"BP01-037"},
        {"id":"second","card":"BP01-037"},
        {"id":"third","card":"BP01-037"}
    ]);
    initial
}

fn awaiting(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(question) = play else {
        panic!("expected the printed ability's next decision");
    };
    question
}

fn pending(game: &mut Game) -> DecisionView {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("expected the real card's pending ability");
    };
    assert_eq!(options.len(), 1, "only the printed trigger is pending");
    awaiting(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap(),
    )
}

fn opponent_target(question: &DecisionView) -> u64 {
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("expected an opposing follower target");
    };
    assert_eq!(domain.members.len(), 1, "the own follower is not a target");
    assert_eq!(
        domain.members[0].player,
        PlayerId { seat: 2 },
        "the target belongs to the opponent"
    );
    domain.members[0].handle
}

fn declare_fanfare(print: &str, accepted: bool) -> (Game, CardPlay) {
    let mut initial = position(print);
    initial["players"]["P1"]["zones"]["ex"] = json!([
        {"id":"fee-fairy","card":"BP01-T03"},
        {"id":"fee-wisp","card":"BP01-T02"},
        {"id":"wrong-token","card":"BP22-P69"},
        {"id":"ordinary","card":"BP01-037"}
    ]);
    initial["players"]["P2"]["zones"]["ex"] =
        json!([{"id":"foreign-fairy","card":"BP01-T03"}]);
    let mut game = game(&initial);
    assert!(
        matches!(
            game.play_internal_card("source", controller()).unwrap(),
            CardPlay::Entered(_)
        ),
        "the real follower enters before its fanfare"
    );
    let optional = pending(&mut game);
    assert_eq!(
        optional.domain,
        InputDomain::OptionalCost,
        "the controller can decline the printed material cost"
    );
    let declared = game
        .answer_internal_card(
            optional.actor,
            optional.token,
            Answer::OptionalCost(accepted),
        )
        .unwrap();
    (game, declared)
}

fn assert_paid_fanfare(game: &mut Game, declared: CardPlay) {
    let target = awaiting(declared);
    let handle = opponent_target(&target);
    let materials = awaiting(
        game.answer_internal_card(
            target.actor,
            target.token,
            Answer::Targets(vec![handle]),
        )
        .unwrap(),
    );
    let InputDomain::CostMaterials(domain) = &materials.domain else {
        panic!("expected the public EX material choice");
    };
    assert_eq!(
        domain.members.len(),
        2,
        "only the controller's Fairy and Fairy Wisp are Fairy token materials"
    );
    let handles = domain
        .members
        .iter()
        .map(|member| member.handle)
        .collect::<Vec<_>>();
    assert!(
        game.answer_internal_card(
            materials.actor,
            materials.token,
            Answer::CostMaterials(vec![handles[0]]),
        )
        .is_err(),
        "one Fairy token cannot pay a two-card cost"
    );
    let rejected = game.projection(View::Referee).unwrap();
    assert_eq!(rejected["P1"]["ex"].as_array().unwrap().len(), 4, "an incomplete payment leaves all EX cards intact");
    assert_eq!(rejected["objects"]["enemy"]["damage"], 0_u8, "no damage occurs before full payment");
    let paid = game
        .answer_internal_card(
            materials.actor,
            materials.token,
            Answer::CostMaterials(handles),
        )
        .unwrap();
    let CardPlay::Resolved(activation) = paid else {
        panic!("the printed body must resolve after payment");
    };
    assert_paid_result(game, &activation.progress.facts);
}

fn assert_paid_result(game: &mut Game, facts: &[Fact]) {
    assert_eq!(
        facts.iter().filter(|fact| matches!(fact,
            Fact::Movement(movement) if movement.method == MoveMethod::Banish
                && movement.from.kind == ZoneKind::Ex
                && movement.movement.actual_to.as_ref().is_some_and(|zone| zone.kind == ZoneKind::Banish)
        )).count(),
        2,
        "both EX token cards were banished as the printed cost"
    );
    let before_checks = game.projection(View::Referee).unwrap();
    assert_eq!(before_checks["objects"]["enemy"]["damage"], 5_u8, "the printed body deals five damage before check timing");
    game.resume_internal_card().unwrap();
    let completed = game.projection(View::Referee).unwrap();
    // Tokens cease after leaving permitted zones (CR 9.1.4.4).
    assert_eq!(completed["P1"]["banish"], json!([]), "paid tokens cease instead of remaining in banish");
    assert!(
        completed["objects"].get("fee-fairy").is_none(),
        "the paid Fairy token ceased"
    );
    assert!(
        completed["objects"].get("fee-wisp").is_none(),
        "Fairy Wisp also satisfies the Fairy token cost and ceased"
    );
    assert_eq!(completed["P1"]["ex"], json!(["wrong-token", "ordinary"]), "nonmatching own EX cards remain");
    assert_eq!(completed["P2"]["ex"], json!(["foreign-fairy"]), "the opposing EX Fairy cannot pay this cost");
    assert_eq!(completed["P2"]["cemetery"], json!(["enemy"]), "five damage destroys the opposing follower");
    assert_eq!(completed["P1"]["hand"], json!(["first"]), "the paid body draws the top card");
    assert_eq!(completed["P1"]["deck"], json!(["second", "third"]), "the body draws exactly one card");
    assert_eq!(completed["P1"]["pp"]["current"], 5_u8, "the fanfare adds no PP cost to the five-PP play");
}

#[test]
fn bp22_013_pays_two_fairy_tokens_from_ex_before_damage_and_draw() {
    for print in ["BP22-013", "BP22-P04"] {
        let (game, declared) = declare_fanfare(print, false);
        assert_eq!(
            declared,
            CardPlay::Cancelled,
            "declining cancels the ability"
        );
        let declined = game.projection(View::Referee).unwrap();
        assert_eq!(declined["P1"]["ex"].as_array().unwrap().len(), 4);
        assert_eq!(declined["objects"]["enemy"]["damage"], 0_u8);
        assert_eq!(declined["P1"]["hand"], json!([]));
        assert_eq!(declined["P1"]["pp"]["current"], 5_u8);
        let (mut accepted_game, accepted_declaration) = declare_fanfare(print, true);
        assert_paid_fanfare(&mut accepted_game, accepted_declaration);
    }
}

#[test]
fn bp22_013_to_014_evolution_pays_one_pp_then_destroys_only_an_opposing_follower() {
    for (base, form) in [("BP22-013", "BP22-014"), ("BP22-P04", "BP22-P05")] {
        let mut initial = position(base);
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] = json!([
            {"id":"source","card":base}, {"id":"ally","card":"BP01-037"}
        ]);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
        initial["players"]["P1"]["pp"]["current"] = json!(0_u8);
        let mut denied = game(&initial);
        if let Ok(step) = denied.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":0_u8}}),
        ) {
            assert_ne!(
                step.outcome, "resolved",
                "insufficient PP cannot evolve the base"
            );
        }
        let unchanged = denied.projection(View::Referee).unwrap();
        assert_eq!(unchanged["P1"]["pp"]["current"], 0_u8);
        assert_eq!(unchanged["objects"]["source"]["power"], 3_u8);
        initial["players"]["P1"]["pp"]["current"] = json!(1_u8);
        let mut evolved = game(&initial);
        let progress = evolved
            .internal_rule_action(
                &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}}),
            )
            .unwrap();
        assert_eq!(
            progress.outcome, "resolved",
            "the printed one-PP evolution succeeds"
        );
        let target = pending(&mut evolved);
        let handle = opponent_target(&target);
        let resolved = evolved
            .answer_internal_card(target.actor, target.token, Answer::Targets(vec![handle]))
            .unwrap();
        assert!(
            matches!(resolved, CardPlay::Resolved(_)),
            "the real evolved ability resolves"
        );
        let state = evolved.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["pp"]["current"], 0_u8);
        assert_eq!(state["objects"]["source"]["power"], 4_u8);
        assert_eq!(state["objects"]["source"]["hp"], 4_u8);
        assert_eq!(state["P2"]["cemetery"], json!(["enemy"]));
        assert!(
            state["P1"]["field"]
                .as_array()
                .unwrap()
                .contains(&json!("ally")),
            "an own follower is untouched"
        );
    }
}
