#![expect(
    clippy::panic,
    reason = "Unexpected questions invalidate the complete printed unlimited evolution scenarios."
)]
use super::*;

fn position() -> Value {
    let mut initial = draw_position();
    initial["turn"]["elapsed_turns"] = json!({"P1":7_u8,"P2":6_u8});
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["sep"] = json!(1_u8);
    initial
}

fn board(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "unlimited-current",
    )
    .unwrap()
}

fn settle(game: &mut Game) {
    loop {
        while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
        let Some(q) = game.internal_pending_choice().unwrap() else {
            return;
        };
        let InputDomain::Pending(options) = &q.domain else {
            panic!("printed evolution observers");
        };
        resolved(
            game.answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
                .unwrap(),
        );
    }
}

fn evolve(game: &mut Game, source: &str, form: &str, pay: &Value) {
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":source,"evolve_card":form,"pay":pay})
        )
        .unwrap()
        .outcome,
        "resolved",
        "the actual printed intrinsic evolution is legal"
    );
    settle(game);
}

#[test]
fn all_pruner_printings_keep_the_aura_after_evolution_and_heal_for_normal_and_super_evolution() {
    for (base, form) in [("BP18-005", "BP18-006"), ("BP18-P01", "BP18-P02")] {
        let mut initial = position();
        initial["players"]["P1"]["zones"]["field"] = json!([
            {"id":"s","card":base},{"id":"u","card":base}
        ]);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([
            {"id":"e","card":form},{"id":"e2","card":form}
        ]);
        let mut game = board(&initial);
        evolve(&mut game, "s", "e", &json!({"pp":0_u8}));
        evolve(&mut game, "u", "e2", &json!({"pp":0_u8,"sep":1_u8}));
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(
            view["P1"]["leader"]["life"], 14_u8,
            "both printed observers heal once for each ordinary or super evolution (Q2303)"
        );
        assert_eq!(
            view["P1"]["pp"]["current"], 10_u8,
            "both actual zero-PP evolutions preserve PP"
        );
        assert_eq!(
            view["P1"]["sep"], 0_u8,
            "only the second evolution consumes SEP"
        );
        assert_eq!(view["objects"]["s"]["evolved_with"], "e");
        assert_eq!(view["objects"]["u"]["evolved_with"], "e2");
    }
}

#[test]
fn an_evolved_pruner_alone_allows_each_separate_one_pp_evolution_to_use_its_own_ep() {
    let mut initial = position();
    initial["players"]["P1"]["ep"] = json!(2_u8);
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"provider","card":"BP18-005"},
        {"id":"s","card":"BP16-038"},{"id":"u","card":"BP16-038"}
    ]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([
        {"id":"provider-form","card":"BP18-006"},
        {"id":"e","card":"BP16-039"},{"id":"e2","card":"BP16-SL11"}
    ]);
    let mut game = board(&initial);
    evolve(&mut game, "provider", "provider-form", &json!({"pp":0_u8}));
    for (source, form) in [("s", "e"), ("u", "e2")] {
        evolve(&mut game, source, form, &json!({"pp":0_u8,"ep":1_u8}));
    }
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(
        view["objects"]["provider"]["evolved_with"], "provider-form",
        "the evolved face is the only unlimited evolution provider"
    );
    assert_eq!(
        view["P1"]["ep"], 0_u8,
        "each distinct evolution can spend one EP (Q2301)"
    );
    assert_eq!(
        view["P1"]["pp"]["current"], 10_u8,
        "each EP replaces that play's one PP"
    );
    assert_eq!(
        view["P1"]["leader"]["life"], 13_u8,
        "the evolved source alone heals for its own evolution and both later plays"
    );
}
