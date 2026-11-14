use super::*;
use sve_engine::game::internal::SearchAnswer;

fn finish(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}

#[expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate the native evolution-search fixture."
)]
fn resolve_evolved_search(game: &mut Game) {
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("evolution trigger");
    };
    let CardPlay::Awaiting(search) = game
        .answer_internal_card(
            pending.actor,
            pending.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap()
    else {
        panic!("evolved native search");
    };
    let InputDomain::Search(domain) = &search.domain else {
        panic!("evolved search domain");
    };
    assert!(
        domain.cards.is_empty(),
        "the synthetic deck has no named native search candidate"
    );
    resolved(
        game.answer_internal_card(
            search.actor,
            search.token,
            Answer::Search(SearchAnswer::Ungrouped(vec![])),
        )
        .unwrap(),
    );
    finish(game);
}

#[test]
fn bp15_058_whole_card_installs_awakened_tax_for_the_next_opponent_turn() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP15-058"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP15-059"}]);
    initial["players"]["P2"]["zones"]["hand"] = json!([{"id":"taxed","card":"BP01-009"}]);
    for seat in ["P1", "P2"] {
        initial["players"][seat]["zones"]["deck"] = json!([
            {"id":format!("draw-{seat}"),"card":"BP01-009"}
        ]);
    }
    let mut game = Game::from_selected_effects(
        selection_with_rules().clone(),
        &initial,
        &Value::Null,
        &Value::Null,
        "native-next-turn-tax",
    )
    .unwrap();
    assert!(
        matches!(
            game.play_internal_card("source", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ),
        "Filene enters normally"
    );
    finish(&mut game);
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("Fanfare");
    };
    resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap(),
    );
    finish(&mut game);
    assert!(
        game.projection(View::Referee).unwrap()["objects"]["source"]["keywords"]
            .as_array()
            .unwrap()
            .iter()
            .any(|keyword| keyword == "必殺"),
        "the native Bane remains present"
    );
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
        )
        .unwrap()
        .outcome,
        "resolved",
        "the printed intrinsic evolution remains usable"
    );
    finish(&mut game);
    resolve_evolved_search(&mut game);
    attack_batch::end_turn(&mut game);
    let before = game.projection(View::Referee).unwrap()["P2"]["pp"]["current"]
        .as_i64()
        .unwrap();
    assert!(
        matches!(
            game.play_internal_card("taxed", PlayerId { seat: 2 })
                .unwrap(),
            CardPlay::Entered(_)
        ),
        "the taxed native follower is playable"
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["pp"]["current"],
        before - 4,
        "the printed three PP card costs four in the affected opponent turn"
    );
}
