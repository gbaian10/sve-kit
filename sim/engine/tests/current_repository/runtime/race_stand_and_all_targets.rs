use super::*;

fn position(card: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    for seat in ["P1", "P2"] {
        initial["players"][seat]["zones"]["deck"] = json!([
            {"id":format!("{seat}-first"),"card":"BP01-171"},
            {"id":format!("{seat}-second"),"card":"BP01-171"}
        ]);
    }
    initial
}

fn finish(game: &mut Game, step: CardPlay) {
    finish_mode(game, step, ModeId(1));
}

fn trigger(game: &mut Game, card: &str) {
    trigger_mode(game, card, ModeId(1));
}

fn trigger_mode(game: &mut Game, card: &str, mode: ModeId) {
    let q = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &q.domain else {
        panic!("expected a pending ability")
    };
    let chosen = options.iter().find(|option| option.card == card).unwrap();
    let step = game
        .answer_internal_card(q.actor, q.token, Answer::Pending(chosen.handle))
        .unwrap();
    finish_mode(game, step, mode);
    checks(game);
}

fn activate_source(game: &mut Game, card: &str) {
    let id = selection().current[card]
        .program
        .card()
        .abilities
        .iter()
        .find(|ability| {
            ability.node.kind == AbilityKind::Activated
                && !matches!(
                    &ability.node.source.origin,
                    AbilityOrigin::Keyword {
                        keyword: KeywordId::Evolution | KeywordId::Meal | KeywordId::Ride,
                        ..
                    }
                )
        })
        .unwrap()
        .node
        .id;
    let step = game.activate_internal_card("source", id, actor()).unwrap();
    finish(game, step);
    checks(game);
}

fn quick_pass(game: &mut Game) {
    for _ in 0_u8..4 {
        let Some(q) = game.internal_quick_choice().unwrap() else {
            return;
        };
        game.answer_internal_card(q.actor, q.token, Answer::Quick(None))
            .unwrap();
    }
    panic!("The quick window must finish");
}

#[test]
fn ecp01_030_discards_an_expensive_uma_as_a_cost_for_damage_and_an_extra_draw() {
    let mut initial = position("ECP01-030");
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"fuel","card":"ECP01-010"}]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"ECP01-030"}]);
    let mut game = game(&initial);
    activate_source(&mut game, "ECP01-030");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["cemetery"], json!(["enemy"]));
    assert_eq!(state["P1"]["cemetery"], json!(["fuel"]));
    assert_eq!(state["P1"]["hand"], json!(["P1-first"]));
    assert_eq!(state["objects"]["source"]["acted"], true);
}

#[test]
fn bp14_004_cannot_attack_without_four_counters_and_uses_its_current_power_at_end_phase() {
    let mut initial = position("BP14-004");
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP14-004"}]);
    let mut game = game(&initial);
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"P2.leader"}))
            .unwrap()
            .outcome,
        "cannot-attack",
        "Fewer than four season counters prohibit attacking any opponent"
    );
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    trigger(&mut game, "BP14-004");
    quick_pass(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["power"], 7_u8);
    assert_eq!(state["objects"]["enemy"]["hp"], 7_u8);
    assert_eq!(
        state["objects"]["source"]["counters"]["四季カウンター"],
        1_u8
    );
}
