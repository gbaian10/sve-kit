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
                if member.card.as_deref() == Some("BP12-082") {
                    0_u8
                } else if member.card.as_deref() == Some("BP01-092") {
                    1
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
        if game.internal_pending_choice().unwrap().is_some() {
            return;
        }
        let Some(q) = game.internal_quick_choice().unwrap() else {
            return;
        };
        game.answer_internal_card(q.actor, q.token, Answer::Quick(None))
            .unwrap();
    }
    panic!("The Quick window must finish");
}

#[test]
fn bp03_013_stops_the_chosen_enemy_from_attacking_during_its_next_turn() {
    let mut game = game(&position("BP03-013"));
    play(&mut game, "source");
    trigger(&mut game, "BP03-013");
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    quick_pass(&mut game);
    for target in ["P1.leader", "source"] {
        assert_eq!(
            game.internal_rule_action(&json!({"do":"attack","attacker":"enemy","target":target}))
                .unwrap()
                .outcome,
            "cannot-attack",
            "The next opposing turn prohibits attacks on both legal recipient kinds"
        );
    }
}

#[test]
fn bp16_108_prevents_retaliation_damage_on_its_controllers_turn() {
    let mut initial = position("BP16-108");
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP16-108"}]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-037","state":{"acted":true}}]);
    let mut game = game(&initial);
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"enemy"}))
            .unwrap()
            .outcome,
        "resolved",
        "The source can attack this acted follower"
    );
    quick_pass(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["source"]["hp"], 6_u8);
    assert_eq!(state["objects"]["enemy"]["damage"], 4_u8);
    assert_eq!(state["P1"]["field"], json!(["source"]));
}

#[test]
fn ecp02_sp06_uses_the_other_cute_bonus_and_creates_a_magic_item_after_ten_cute_cards() {
    let mut initial = position("ECP02-SP06");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"cute","card":"CP02-014"}]);
    initial["players"]["P1"]["zones"]["cemetery"] = json!(
        (0_u8..10)
            .map(|i| json!({"id":format!("grave-{i}"),"card":"CP02-014"}))
            .collect::<Vec<_>>()
    );
    let mut game = game(&initial);
    play(&mut game, "source");
    trigger(&mut game, "ECP02-SP06");
    activate_source(&mut game, "ECP02-SP06");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["cemetery"], json!(["enemy"]));
    assert!(keyword(&state, "source", "疾走"));
    assert_eq!(state["P1"]["ex"].as_array().unwrap().len(), 1_usize);
}

#[test]
fn bp02_058_damages_only_on_the_first_real_dragon_entry_on_its_controllers_turn() {
    let mut initial = position("BP02-058");
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP02-058"}]);
    initial["players"]["P1"]["zones"]["ex"] =
        json!([{"id":"dragon1","card":"BP13-T10"},{"id":"dragon2","card":"BP13-T10"}]);
    let mut game = game(&initial);
    play(&mut game, "dragon1");
    trigger(&mut game, "BP02-058");
    play(&mut game, "dragon2");
    assert!(
        game.internal_pending_choice().unwrap().is_none(),
        "Only the first Dragon entry triggers this turn"
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["cemetery"], json!(["enemy"]));
    assert_eq!(state["P2"]["leader"]["life"], 20_u8);
    assert_eq!(
        state["P1"]["field"],
        json!(["source", "dragon1", "dragon2"])
    );
}

#[test]
fn bp02_t08_banishes_itself_at_its_controllers_next_main_phase() {
    let mut initial = position("BP02-T08");
    initial["turn"]["active"] = json!("P2");
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP02-T08"}]);
    let mut game = game(&initial);
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    quick_pass(&mut game);
    trigger(&mut game, "BP02-T08");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["field"], json!([]));
    assert_eq!(state["P1"]["banish"], json!([]));
    assert!(
        state["objects"]["source"].is_null(),
        "A banished token ceases to exist"
    );
}

#[test]
fn bp06_068_creates_a_dragon_in_each_players_ex_area() {
    let mut game = game(&position("BP06-068"));
    play(&mut game, "source");
    trigger(&mut game, "BP06-068");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["ex"].as_array().unwrap().len(), 1_usize);
    assert_eq!(state["P2"]["ex"].as_array().unwrap().len(), 1_usize);
    assert_eq!(state["P1"]["pp"]["current"], 6_u8);
}

#[test]
fn bp06_096_enters_one_five_cost_and_one_small_amulet_from_the_same_seven_card_look() {
    let mut initial = position("BP06-096");
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"high","card":"BP01-141"},{"id":"low","card":"BP01-041"},{"id":"a","card":"BP01-116"},{"id":"b","card":"BP01-116"},{"id":"c","card":"BP01-116"},{"id":"d","card":"BP01-116"},{"id":"e","card":"BP01-116"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["field"], json!(["high", "low"]));
    assert_eq!(state["P1"]["deck"], json!(["e", "d", "c", "b", "a"]));
}

#[test]
fn sd06_006_creates_a_tiger_on_entry_then_pays_itself_and_two_pp_for_another() {
    let mut game = game(&position("SD06-006"));
    play(&mut game, "source");
    trigger(&mut game, "SD06-006");
    activate_source(&mut game, "SD06-006");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["cemetery"], json!(["source"]));
    assert_eq!(state["P1"]["pp"]["current"], 4_u8);
    assert_eq!(state["P1"]["field"].as_array().unwrap().len(), 2_usize);
    for tiger in state["P1"]["field"].as_array().unwrap() {
        let id = tiger.as_str().unwrap();
        assert_eq!(state["objects"][id]["power"], 4_u8);
        assert!(keyword(&state, id, "突進"));
    }
}

#[test]
fn sd06_009_adds_one_to_its_retaliation_damage_on_the_opposing_turn() {
    let mut initial = position("SD06-009");
    initial["turn"]["active"] = json!("P2");
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"source","card":"SD06-009","state":{"acted":true}}]);
    let mut game = game(&initial);
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"enemy","target":"source"}))
            .unwrap()
            .outcome,
        "resolved",
        "The opposing Sage can attack this acted Ward"
    );
    quick_pass(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["damage"], 3_u8);
    assert_eq!(state["P1"]["cemetery"], json!(["source"]));
}

#[test]
fn sd06_010_evolves_legally_and_adds_two_to_its_opposing_turn_retaliation() {
    let mut initial = evolution_position("SD06-009", "SD06-010");
    initial["players"]["P1"]["zones"]["field"][0]["state"] = json!({"acted":true});
    let mut game = game(&initial);
    evolve_source(&mut game, 1);
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    quick_pass(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["turn"]["active"],
        "P2"
    );
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"enemy","target":"source"}))
            .unwrap()
            .outcome,
        "resolved",
        "The Sage attacks the evolved Ward on the opposing turn"
    );
    quick_pass(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["cemetery"], json!(["source"]));
    assert_eq!(state["P2"]["cemetery"], json!(["enemy"]));
}

#[test]
fn sd06_011_uses_a_friendly_amulet_on_entry_to_gain_health() {
    let mut initial = position("SD06-011");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"amulet","card":"BP01-041"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    trigger(&mut game, "SD06-011");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["source"]["hp"], 4_u8);
    assert!(keyword(&state, "source", "守護"));
}

#[test]
fn sd04_005_uses_awakening_on_entry_for_four_damage() {
    let mut game = game(&position("SD04-005"));
    play(&mut game, "source");
    trigger(&mut game, "SD04-005");
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
        4_u8
    );
}

#[test]
fn sd04_006_gains_two_power_and_health_on_its_awakened_entry() {
    let mut game = game(&position("SD04-006"));
    play(&mut game, "source");
    trigger(&mut game, "SD04-006");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["source"]["power"], 6_u8);
    assert_eq!(state["objects"]["source"]["hp"], 7_u8);
    assert!(keyword(&state, "source", "守護"));
}

#[test]
fn sd04_013_gains_two_power_after_a_legal_awakened_evolution() {
    let mut game = game(&evolution_position("SD04-012", "SD04-013"));
    evolve_source(&mut game, 1);
    trigger(&mut game, "SD04-013");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["source"]["power"], 5_u8);
    assert_eq!(state["objects"]["source"]["hp"], 3_u8);
}

#[test]
fn sd01_006_fills_the_field_then_puts_the_remaining_fairies_in_ex() {
    let mut initial = position("SD01-006");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"ally1","card":"BP01-034"},{"id":"ally2","card":"BP01-034"},{"id":"ally3","card":"BP01-034"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    trigger(&mut game, "SD01-006");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["field"].as_array().unwrap().len(), 5_usize);
    assert_eq!(state["P1"]["ex"].as_array().unwrap().len(), 2_usize);
    let fairy = state["P1"]["field"][4].as_str().unwrap();
    assert_eq!(state["objects"][fairy]["power"], 1_u8);
    assert_eq!(state["objects"][fairy]["hp"], 1_u8);
}

#[test]
fn bp12_082_revives_with_ennea_then_its_granted_lastword_banishes_the_grave_generation() {
    let mut initial = position("BP12-082");
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"kill","card":"BP01-116"}]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"ennea","card":"BP07-072"}]);
    initial["players"]["P1"]["zones"]["cemetery"] = json!([{"id":"source","card":"BP12-082"}]);
    let mut game = game(&initial);
    activate_source(&mut game, "BP12-082");
    let revived = game.projection(View::Referee).unwrap();
    assert_eq!(revived["P1"]["field"], json!(["ennea", "source"]));
    assert_eq!(revived["objects"]["source"]["power"], 2_u8);
    assert_eq!(revived["P1"]["pp"]["current"], 9_u8);
    play(&mut game, "kill");
    trigger(&mut game, "BP12-082");
    let departed = game.projection(View::Referee).unwrap();
    assert_eq!(departed["P1"]["cemetery"], json!(["kill"]));
    assert_eq!(departed["P1"]["banish"], json!(["source"]));
    assert_eq!(departed["P1"]["field"], json!(["ennea"]));
}
