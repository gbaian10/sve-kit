#![expect(
    clippy::panic,
    reason = "Unexpected questions invalidate the printed scenarios."
)]
use super::*;
use sve_engine::game::internal::SearchAnswer;

fn position(card: &str, zone: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    initial["players"]["P1"]["zones"][zone] = json!([{"id":"source","card":card}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([
        {"id":"carrot0","card":"CSD01-032"},
        {"id":"carrot1","card":"CSD01-032"},
        {"id":"carrot2","card":"CSD01-032"}
    ]);
    initial["players"]["P1"]["zones"]["deck"] = json!(
        (0_u8..8_u8)
            .map(|i| json!({"id":format!("draw{i}"),"card":"BP01-171"}))
            .collect::<Vec<_>>()
    );
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP04-117","state":{"hp":20_u8,"max_hp":20_u8,"power":0_u8,"acted":true}}]);
    initial
}
fn from_position(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "meal-migration",
    )
    .unwrap()
}
fn finish(game: &mut Game, play: CardPlay) {
    finish_with_cost(game, play, true);
}
fn finish_with_cost(game: &mut Game, mut play: CardPlay, pay_cost: bool) {
    while let CardPlay::Awaiting(question) = play {
        let answer = match &question.domain {
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            InputDomain::OptionalCost => Answer::OptionalCost(pay_cost),
            InputDomain::CostMaterials(domain) => Answer::CostMaterials(
                domain
                    .members
                    .iter()
                    .take(usize::try_from(domain.cardinality.minimum.0).unwrap())
                    .map(|m| m.handle)
                    .collect(),
            ),
            InputDomain::Targets(domain) => Answer::Targets(
                domain
                    .members
                    .iter()
                    .find(|m| m.card.is_some())
                    .or_else(|| domain.members.first())
                    .map(|m| vec![m.handle])
                    .unwrap_or_default(),
            ),
            InputDomain::Search(domain) => Answer::Search(SearchAnswer::Ungrouped(
                domain
                    .cards
                    .first()
                    .map(|c| vec![c.handle])
                    .unwrap_or_default(),
            )),
            InputDomain::Order(cards) => Answer::Order(cards.iter().map(|c| c.handle).collect()),
            InputDomain::Quick { .. } => Answer::Quick(None),
            InputDomain::DriveTrigger { .. }
            | InputDomain::TokenCapacity { .. }
            | InputDomain::ResourceEntryPayment { .. }
            | InputDomain::BeforeDraw
            | InputDomain::Pending(_)
            | InputDomain::DamageReplacements(_)
            | InputDomain::TriggerQuota(_)
            | InputDomain::Integer(_)
            | InputDomain::SearchPartition(_)
            | InputDomain::Distribution(_)
            | InputDomain::SearchReplacement
            | InputDomain::Modes { .. } => {
                panic!("unexpected printed choice: {:?}", question.domain)
            }
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    assert!(
        matches!(
            play,
            CardPlay::Resolved(_) | CardPlay::Entered(_) | CardPlay::Cancelled
        ),
        "a printed procedure must resolve, enter, or cancel an unpayable or declined ability"
    );
}
fn settle(game: &mut Game) {
    settle_with_cost(game, true);
}
fn settle_with_cost(game: &mut Game, pay_cost: bool) {
    loop {
        if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
        let Some(question) = game.internal_pending_choice().unwrap() else {
            break;
        };
        let InputDomain::Pending(options) = &question.domain else {
            panic!("printed pending");
        };
        let play = game
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap();
        finish_with_cost(game, play, pay_cost);
    }
}
fn play(game: &mut Game, holder: &str) {
    let entered = game
        .play_internal_card(holder, PlayerId { seat: 1 })
        .unwrap();
    finish(game, entered);
    settle(game);
}
fn feed(game: &mut Game, card: &str) {
    meal_race::feed(game, card, 0, false);
    settle(game);
}
fn activate(game: &mut Game, card: &str) {
    let ability = selection_with_rules().current[card]
        .program
        .card()
        .abilities
        .iter()
        .find(|a| {
            a.node.kind == AbilityKind::Activated
                && matches!(a.node.source.origin, AbilityOrigin::Printed { .. })
        })
        .unwrap()
        .node
        .id;
    let play = game
        .activate_internal_card("source", ability, PlayerId { seat: 1 })
        .unwrap();
    finish(game, play);
    settle(game);
}
fn offering(game: &mut Game) {
    play(game, "offering");
}
fn with_offering(initial: &mut Value) {
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"offering","card":"BP01-140"}]);
}
fn zone<'view>(view: &'view Value, id: &str) -> &'view str {
    for seat in ["P1", "P2"] {
        for (zone, members) in view[seat].as_object().unwrap() {
            if members
                .as_array()
                .is_some_and(|members| members.contains(&json!(id)))
            {
                return zone;
            }
        }
    }
    panic!("physical card is absent from the referee's zones");
}

#[test]
fn cp01_043_attack_adds_one_power_after_meal() {
    let mut game = from_position(&position("CP01-043", "field"));
    feed(&mut game, "CP01-043");
    let before = game.projection(View::Referee).unwrap();
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"P2.leader"}))
            .unwrap()
            .outcome,
        "resolved"
    );
    settle(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["source"]["power"]
            .as_i64()
            .unwrap(),
        before["objects"]["source"]["power"]
            .as_i64()
            .unwrap()
            .saturating_add(1)
    );
}
#[test]
fn cp01_074_discards_an_amulet_to_gain_power_and_rush() {
    let mut initial = position("CP01-074", "hand");
    initial["players"]["P1"]["zones"]["hand"]
        .as_array_mut()
        .unwrap()
        .extend([json!({"id":"non-amulet","card":"CP01-022"}), json!({"id":"amulet","card":"BP09-102"})]);
    let mut game = from_position(&initial);
    play(&mut game, "source");
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(zone(&view, "amulet"), "cemetery");
    assert_eq!(zone(&view, "non-amulet"), "hand");
    assert_eq!(view["objects"]["source"]["power"], 4_u8);
    assert!(
        view["objects"]["source"]["keywords"]
            .as_array()
            .unwrap()
            .contains(&json!("突進"))
    );
}
#[test]
fn cp01_075_race_adds_stats_and_two_life() {
    let mut game = from_position(&position("CP01-075", "field"));
    let before = game.projection(View::Referee).unwrap();
    feed(&mut game, "CP01-075");
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["leader"]["life"], 12_u8);
    assert_eq!(
        view["objects"]["source"]["power"].as_i64().unwrap(),
        before["objects"]["source"]["power"]
            .as_i64()
            .unwrap()
            .saturating_add(1)
    );
}
#[test]
fn cp01_078_fanfare_banishes_a_follower_and_draws() {
    let mut game = from_position(&position("CP01-078", "hand"));
    play(&mut game, "source");
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(zone(&view, "enemy"), "banish");
    assert_eq!(zone(&view, "draw0"), "hand");
}
#[test]
fn ecp01_007_attack_draws_and_only_discards_with_three_or_four_uma() {
    for allies in [2_u8, 3, 4, 5] {
        let mut initial = position("ECP01-007", "field");
        for i in 1..allies {
            initial["players"]["P1"]["zones"]["field"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":format!("uma{i}"),"card":"CP01-022"}));
        }
        let mut game = from_position(&initial);
        feed(&mut game, "ECP01-007");
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"attack","attacker":"source","target":"P2.leader"})
            )
            .unwrap()
            .outcome,
            "resolved"
        );
        settle(&mut game);
        assert_eq!(
            zone(&game.projection(View::Referee).unwrap(), "draw0"),
            match allies { 3 | 4 => "cemetery", 5 => "hand", _ => "deck" }
        );
    }
}
#[test]
fn ecp01_008_two_and_three_races_pay_their_own_costs_and_resolve_each_trigger() {
    for (times, index) in [(2_i64, 1_usize), (3, 2)] {
        let mut game = from_position(&position("ECP01-008", "field"));
        let before = game.projection(View::Referee).unwrap();
        meal_race::feed(&mut game, "ECP01-008", index, false);
        settle(&mut game);
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["objects"]["enemy"]["damage"], times * 3);
        for stat in ["power", "hp"] {
            assert_eq!(view["objects"]["source"][stat].as_i64().unwrap(), before["objects"]["source"][stat].as_i64().unwrap() + times);
        }
        assert_eq!(view["P1"]["pp"]["current"], 10 - times);
    }
}
#[test]
fn ecp01_015_acts_four_other_uma_to_gain_five_power() {
    let mut initial = position("ECP01-015", "field");
    for i in 0_u8..4 {
        initial["players"]["P1"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":format!("uma{i}"),"card":"CP01-022"}));
    }
    let mut game = from_position(&initial);
    let before = game.projection(View::Referee).unwrap();
    activate(&mut game, "ECP01-015");
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(
        view["objects"]["source"]["power"].as_i64().unwrap(),
        before["objects"]["source"]["power"]
            .as_i64()
            .unwrap()
            .saturating_add(5)
    );
    for i in 0_u8..4 {
        assert_eq!(view["objects"][format!("uma{i}")]["acted"], true);
    }
    assert_eq!(view["objects"]["source"]["acted"], false);
}
#[test]
fn ecp01_024_unrelated_uma_does_not_reduce_the_three_pp_cost() {
    let mut initial = position("ECP01-024", "hand");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"uma","card":"CP01-022"}]);
    let mut game = from_position(&initial);
    play(&mut game, "source");
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        7_u8
    );
}
#[test]
fn ecp01_028_awakening_controls_both_damage_and_draw_in_the_paragraph() {
    for maximum in [6_u8, 7, 10] {
        let mut initial = position("ECP01-028", "hand");
        initial["players"]["P1"]["pp"] = json!({"current":maximum,"max":maximum});
        let mut game = from_position(&initial);
        play(&mut game, "source");
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(
            view["objects"]["enemy"]["damage"],
            if maximum >= 7_u8 { 4_u8 } else { 0_u8 }
        );
        assert_eq!(
            view["P1"]["hand"].as_array().unwrap().len(),
            usize::from(maximum >= 7)
        );
    }
}
#[test]
fn ecp01_033_counts_used_face_up_carrots_only() {
    let mut initial = position("ECP01-033", "hand");
    initial["players"]["P1"]["zones"]["evolve_deck"][0]["face_up"] = json!(true);
    initial["players"]["P1"]["zones"]["evolve_deck"][1]["face_up"] = json!(true);
    initial["players"]["P1"]["zones"]["evolve_deck"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"other-used","card":"CP01-015","face_up":true}));
    let mut game = from_position(&initial);
    play(&mut game, "source");
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
        2_u8
    );
}
#[test]
fn ecp01_035_race_buffs_itself_and_searches_seiun_sky_into_play() {
    let mut initial = position("ECP01-035", "field");
    initial["players"]["P1"]["zones"]["deck"][1] = json!({"id":"sky","card":"CP01-043"});
    let mut game = from_position(&initial);
    let before = game.projection(View::Referee).unwrap();
    feed(&mut game, "ECP01-035");
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(zone(&view, "sky"), "field");
    assert_eq!(
        view["objects"]["source"]["power"].as_i64().unwrap(),
        before["objects"]["source"]["power"]
            .as_i64()
            .unwrap()
            .saturating_add(1)
    );
}
#[test]
fn ecp01_042_lastword_discards_an_uma_to_draw_one() {
    let mut initial = position("ECP01-042", "field");
    with_offering(&mut initial);
    initial["players"]["P1"]["zones"]["hand"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"uma","card":"CP01-022"}));
    let mut game = from_position(&initial);
    offering(&mut game);
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(zone(&view, "uma"), "cemetery");
    assert_eq!(zone(&view, "draw1"), "hand");
}
#[test]
fn ecp01_046_recovers_life_only_when_behind_the_enemy_leader() {
    for own_life in [10_u8, 20] {
        let mut initial = position("ECP01-046", "hand");
        initial["players"]["P1"]["leader"]["life"] = json!(own_life);
        let mut game = from_position(&initial);
        play(&mut game, "source");
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
            if own_life == 10_u8 { 12_u8 } else { 20_u8 }
        );
    }
}

#[test]
fn optional_meal_card_costs_can_be_declined() {
    let mut initial = position("CP01-074", "hand");
    initial["players"]["P1"]["zones"]["hand"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"amulet","card":"BP09-102"}));
    let mut game = from_position(&initial);
    let before = game.projection(View::Referee).unwrap();
    let entered = game
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap();
    finish_with_cost(&mut game, entered, false);
    settle_with_cost(&mut game, false);
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(zone(&view, "amulet"), "hand");
    assert_eq!(
        view["objects"]["source"]["power"],
        before["objects"]["source"]["power"]
    );
    assert!(
        !view["objects"]["source"]["keywords"]
            .as_array()
            .unwrap()
            .contains(&json!("突進")),
        "declining the amulet cost cannot grant Rush"
    );
}
#[test]
fn ecp01_042_declining_the_lastword_keeps_the_uma_and_does_not_draw_again() {
    let mut initial = position("ECP01-042", "field");
    with_offering(&mut initial);
    initial["players"]["P1"]["zones"]["hand"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"fodder","card":"CP01-022"}));
    let mut game = from_position(&initial);
    let entered = game
        .play_internal_card("offering", PlayerId { seat: 1 })
        .unwrap();
    finish_with_cost(&mut game, entered, false);
    settle_with_cost(&mut game, false);
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(zone(&view, "source"), "cemetery");
    assert_eq!(zone(&view, "fodder"), "hand");
    assert_eq!(zone(&view, "draw1"), "deck");
    assert_eq!(view["P1"]["hand"].as_array().unwrap().len(), 2_usize);
}

#[test]
fn ecp01_028_without_an_enemy_follower_enters_but_cannot_draw_from_fanfare() {
    let mut initial = position("ECP01-028", "hand");
    initial["players"]["P2"]["zones"]["field"] = json!([]);
    let mut game = from_position(&initial);
    play(&mut game, "source");
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(zone(&view, "source"), "field");
    assert_eq!(zone(&view, "draw0"), "deck");
    assert_eq!(view["P1"]["hand"].as_array().unwrap().len(), 0_usize);
    assert_eq!(view["P1"]["pp"]["current"], 7_u8);
}
