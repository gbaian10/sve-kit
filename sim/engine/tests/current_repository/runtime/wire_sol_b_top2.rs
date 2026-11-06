#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate these printed card scenarios."
)]
use super::*;
use sve_engine::game::internal::SearchAnswer;

const fn actor() -> PlayerId {
    PlayerId { seat: 1 }
}

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

fn finish_mode(game: &mut Game, mut step: CardPlay, mode: ModeId) {
    while let CardPlay::Awaiting(q) = step {
        let answer = if let InputDomain::Targets(domain) = &q.domain {
            let mut members: Vec<_> = domain.members.iter().collect();
            members.sort_by_key(|member| {
                if member.card.as_deref() == Some("BP01-092") {
                    0_u8
                } else if member.card.as_deref() == Some("BP01-037") {
                    2
                } else {
                    3
                }
            });
            let limit = domain
                .cardinality
                .maximum
                .map_or(members.len(), |n| usize::try_from(n.0).unwrap());
            Answer::Targets(members.into_iter().take(limit).map(|m| m.handle).collect())
        } else if let InputDomain::CostMaterials(domain) = &q.domain {
            let members: Vec<_> = domain.members.iter().collect();
            assert!(
                members.iter().all(|member| member.player == actor()),
                "Only the controller can supply these cost materials"
            );
            let limit = domain
                .cardinality
                .maximum
                .map_or(members.len(), |n| usize::try_from(n.0).unwrap());
            Answer::CostMaterials(members.into_iter().take(limit).map(|m| m.handle).collect())
        } else if let InputDomain::Search(domain) = &q.domain {
            let limit = usize::try_from(
                domain
                    .cardinality
                    .maximum
                    .unwrap_or(domain.cardinality.minimum)
                    .0,
            )
            .unwrap();
            Answer::Search(SearchAnswer::Ungrouped(
                domain.cards.iter().take(limit).map(|c| c.handle).collect(),
            ))
        } else if let InputDomain::Order(cards) = &q.domain {
            Answer::Order(cards.iter().rev().map(|card| card.handle).collect())
        } else if matches!(&q.domain, InputDomain::Modes { .. }) {
            Answer::Modes(vec![mode])
        } else if matches!(&q.domain, InputDomain::WardEntry { .. }) {
            Answer::WardEntry(false)
        } else if matches!(&q.domain, InputDomain::TokenCapacity { .. }) {
            Answer::TokenCapacity(0)
        } else if matches!(&q.domain, InputDomain::OptionalCost) {
            Answer::OptionalCost(true)
        } else {
            panic!("unexpected decision")
        };
        step = game.answer_internal_card(q.actor, q.token, answer).unwrap();
    }
}

fn checks(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}

fn play(game: &mut Game, id: &str) {
    let step = game.play_internal_card(id, actor()).unwrap();
    finish(game, step);
    checks(game);
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

fn evolution_position(base: &str, form: &str) -> Value {
    let mut initial = position(base);
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":base}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
    initial
}

fn evolve_source(game: &mut Game, pp: u8) {
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":pp}})
        )
        .unwrap()
        .outcome,
        "resolved",
        "The printed evolution is legal in this position"
    );
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

fn keyword(state: &Value, object: &str, word: &str) -> bool {
    state["objects"][object]["keywords"]
        .as_array()
        .unwrap()
        .contains(&json!(word))
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
fn ecp02_028_uses_the_other_cute_bonus_and_requires_ten_cute_cards_for_its_activation() {
    let mut initial = position("ECP02-028");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"cute","card":"CP02-014"}]);
    initial["players"]["P1"]["zones"]["cemetery"] = json!(
        (0_u8..10)
            .map(|i| json!({"id":format!("grave-{i}"),"card":"CP02-014"}))
            .collect::<Vec<_>>()
    );
    let mut game = game(&initial);
    play(&mut game, "source");
    trigger(&mut game, "ECP02-028");
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["cemetery"],
        json!(["enemy"])
    );
    activate_source(&mut game, "ECP02-028");
    let state = game.projection(View::Referee).unwrap();
    assert!(keyword(&state, "source", "疾走"));
    assert_eq!(state["P1"]["ex"].as_array().unwrap().len(), 1_usize);
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

#[test]
fn bp14_005_damages_on_evolution_then_uses_its_evolved_power_for_the_end_phase_buff() {
    let mut initial = evolution_position("BP14-004", "BP14-005");
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-034"}]);
    let mut game = game(&initial);
    evolve_source(&mut game, 1);
    trigger(&mut game, "BP14-005");
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["cemetery"],
        json!(["enemy"])
    );
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    trigger(&mut game, "BP14-005");
    quick_pass(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["source"]["power"], 6_u8);
    assert_eq!(state["objects"]["source"]["hp"], 8_u8);
    assert_eq!(
        state["objects"]["source"]["counters"]["四季カウンター"],
        1_u8
    );
}

#[test]
fn bp03_017_puts_a_fairy_tale_from_the_look_into_ex_then_pays_itself_for_a_counter() {
    let mut initial = position("BP03-017");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"ally","card":"BP03-101"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"picked","card":"BP03-101"},{"id":"rest-a","card":"BP01-171"},{"id":"rest-b","card":"BP01-171"},{"id":"rest-c","card":"BP01-171"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    trigger(&mut game, "BP03-017");
    activate_source(&mut game, "BP03-017");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["ex"], json!(["picked"]));
    assert_eq!(state["P1"]["deck"], json!(["rest-c", "rest-b", "rest-a"]));
    assert_eq!(state["objects"]["ally"]["counters"]["童話カウンター"], 1_u8);
    assert_eq!(state["P1"]["cemetery"], json!(["source"]));
}

#[test]
fn bp13_083_revives_a_dead_follower_of_original_cost_at_most_three_after_a_legal_evolution() {
    let mut initial = evolution_position("BP13-082", "BP13-083");
    initial["players"]["P1"]["zones"]["cemetery"] = json!([{"id":"spirit","card":"BP01-117"}]);
    let mut game = game(&initial);
    evolve_source(&mut game, 1);
    trigger(&mut game, "BP13-083");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["cemetery"], json!([]));
    assert_eq!(state["P1"]["field"], json!(["source", "spirit"]));
}

#[test]
fn bp12_006_stands_and_draws_only_on_its_first_attack_and_prevents_combat_damage_on_its_turn() {
    let mut initial = position("BP12-006");
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037","state":{"acted":true}},{"id":"second","card":"BP01-037","state":{"acted":true}}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    for target in ["enemy", "second"] {
        assert_eq!(
            game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":target}))
                .unwrap()
                .outcome,
            "resolved",
            "Rush permits these follower attacks"
        );
        if target == "enemy" {
            trigger(&mut game, "BP12-006");
        } else {
            assert!(
                game.internal_pending_choice().unwrap().is_none(),
                "The second attack must not trigger again"
            );
        }
        quick_pass(&mut game);
    }
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["P1-first"]));
    assert_eq!(state["objects"]["source"]["hp"], 6_u8);
    assert_eq!(state["objects"]["source"]["acted"], true);
}

#[test]
fn bp13_022_observes_a_real_soldier_entering_on_its_controllers_turn() {
    let mut initial = position("BP13-022");
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP13-022"}]);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"soldier","card":"BP01-034"}]);
    let mut game = game(&initial);
    play(&mut game, "soldier");
    trigger(&mut game, "BP13-022");
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        11_u8
    );
}

#[test]
fn bp13_023_creates_three_soldiers_on_evolution_and_observes_each_entry() {
    let mut initial = evolution_position("BP13-022", "BP13-023");
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    let mut game = game(&initial);
    evolve_source(&mut game, 1);
    trigger(&mut game, "BP13-023");
    for _ in 0_u8..3 {
        trigger(&mut game, "BP13-023");
    }
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["field"].as_array().unwrap().len(), 4_usize);
    assert_eq!(state["P1"]["leader"]["life"], 13_u8);
}

#[test]
fn bp14_002_waits_for_three_real_plays_then_banishes_only_its_own_other_banquet_to_search() {
    let mut initial = position("BP14-002");
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"source","card":"BP14-002"},{"id":"fuel","card":"BP14-001"}]);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"played-a","card":"BP01-034"},{"id":"played-b","card":"BP01-034"},{"id":"played-c","card":"BP01-034"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"found","card":"BP01-034"}]);
    let mut game = game(&initial);
    for id in ["played-a", "played-b", "played-c"] {
        play(&mut game, id);
    }
    activate_source(&mut game, "BP14-002");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["banish"], json!(["fuel"]));
    assert_eq!(state["P1"]["deck"], json!([]));
    assert_eq!(
        state["P1"]["field"],
        json!(["source", "played-a", "played-b", "played-c", "found"])
    );
    assert_eq!(state["P2"]["field"], json!(["enemy"]));
}

#[test]
fn ecp01_023_discards_its_own_uma_for_damage_healing_and_two_cards() {
    let mut initial = position("ECP01-023");
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    initial["players"]["P1"]["zones"]["hand"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"fuel","card":"ECP01-034"}));
    let mut game = game(&initial);
    play(&mut game, "source");
    trigger(&mut game, "ECP01-023");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["cemetery"], json!(["fuel"]));
    assert_eq!(state["P1"]["leader"]["life"], 12_u8);
    assert_eq!(state["P2"]["leader"]["life"], 18_u8);
    assert_eq!(state["P1"]["hand"], json!(["P1-first", "P1-second"]));
}
