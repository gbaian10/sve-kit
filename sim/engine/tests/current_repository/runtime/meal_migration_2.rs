#![expect(
    clippy::panic,
    reason = "Unexpected questions invalidate the printed scenarios."
)]
use super::*;
use sve_engine::game::internal::SearchAnswer;
use sve_engine::ir::AbilityId;

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
            | InputDomain::MealPayment { .. }
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
fn printed_activation(card: &str) -> AbilityId {
    selection_with_rules().current[card]
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
        .id
}
fn activate(game: &mut Game, card: &str) {
    let play = game
        .activate_internal_card("source", printed_activation(card), PlayerId { seat: 1 })
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
fn with_ally(initial: &mut Value) {
    let field = initial["players"]["P1"]["zones"]["field"].as_array_mut();
    if let Some(field) = field {
        field.push(json!({"id":"ally","card":"BP04-117"}));
    } else {
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"ally","card":"BP04-117"}]);
    }
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

fn assert_gained_stats(before: &Value, after: &Value) {
    for stat in ["power", "hp"] {
        assert_eq!(
            after["objects"]["source"][stat].as_i64().unwrap(),
            before["objects"]["source"][stat]
                .as_i64()
                .unwrap()
                .saturating_add(1),
            "the raced follower gains one point in {stat}"
        );
    }
}

#[test]
fn cp01_040_fanfare_takes_the_observed_uma_into_hand() {
    let mut initial = position("CP01-040", "hand");
    initial["players"]["P1"]["zones"]["deck"][0] = json!({"id":"uma","card":"CP01-022"});
    let mut game = from_position(&initial);
    play(&mut game, "source");
    assert_eq!(
        zone(&game.projection(View::Referee).unwrap(), "uma"),
        "hand"
    );
}
#[test]
fn cp01_046_race_adds_stats_and_deals_two_damage() {
    let mut game = from_position(&position("CP01-046", "field"));
    let before = game.projection(View::Referee).unwrap();
    feed(&mut game, "CP01-046");
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["objects"]["enemy"]["damage"], 2_u8);
    assert_gained_stats(&before, &view);
}
#[test]
fn cp01_048_discards_for_two_damage_to_the_enemy_leader() {
    let mut initial = position("CP01-048", "hand");
    initial["players"]["P1"]["zones"]["hand"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"fodder","card":"BP01-171"}));
    let mut game = from_position(&initial);
    play(&mut game, "source");
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P2"]["leader"]["life"], 18_u8);
    assert_eq!(zone(&view, "fodder"), "cemetery");
}
#[test]
fn cp01_049_race_buffs_itself_and_another_allied_follower() {
    let mut initial = position("CP01-049", "field");
    with_ally(&mut initial);
    let mut game = from_position(&initial);
    let before = game.projection(View::Referee).unwrap();
    feed(&mut game, "CP01-049");
    let view = game.projection(View::Referee).unwrap();
    for id in ["source", "ally"] {
        assert_eq!(
            view["objects"][id]["power"].as_i64().unwrap(),
            before["objects"][id]["power"]
                .as_i64()
                .unwrap()
                .saturating_add(1)
        );
        assert_eq!(
            view["objects"][id]["hp"].as_i64().unwrap(),
            before["objects"][id]["hp"]
                .as_i64()
                .unwrap()
                .saturating_add(1)
        );
    }
    assert_eq!(
        view["objects"]["enemy"]["power"],
        before["objects"]["enemy"]["power"]
    );
    assert_eq!(
        view["objects"]["enemy"]["hp"],
        before["objects"]["enemy"]["hp"]
    );
}
#[test]
fn cp01_050_fanfare_damages_every_enemy_follower() {
    let mut initial = position("CP01-050", "hand");
    initial["players"]["P2"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"enemy2","card":"BP04-117","state":{"hp":20_u8,"max_hp":20_u8}}));
    let mut game = from_position(&initial);
    play(&mut game, "source");
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["objects"]["enemy"]["damage"], 2_u8);
    assert_eq!(view["objects"]["enemy2"]["damage"], 2_u8);
    assert_eq!(view["P2"]["leader"]["life"], 20_u8);
    assert_eq!(view["objects"]["source"]["damage"], 0_u8);
}
#[test]
fn cp01_051_fanfare_recovers_five_life() {
    for life in [10_u8, 19_u8] {
        let mut initial = position("CP01-051", "hand");
        initial["players"]["P1"]["leader"]["life"] = json!(life);
        let mut game = from_position(&initial);
        play(&mut game, "source");
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["P1"]["leader"]["life"], life.saturating_add(5_u8));
        assert_eq!(view["P2"]["leader"]["life"], 20_u8);
    }
}

#[test]
fn cp01_052_fanfare_recovers_ten_life() {
    for life in [10_u8, 19_u8] {
        let mut initial = position("CP01-052", "hand");
        initial["players"]["P1"]["leader"]["life"] = json!(life);
        let mut game = from_position(&initial);
        play(&mut game, "source");
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["P1"]["leader"]["life"], life.saturating_add(10_u8));
        assert_eq!(view["P2"]["leader"]["life"], 20_u8);
    }
}

#[test]
fn cp01_053_necro_ten_controls_the_minus_four_effect() {
    for count in [9_u8, 10] {
        let mut initial = position("CP01-053", "hand");
        initial["players"]["P1"]["zones"]["cemetery"] = json!(
            (0_u8..count)
                .map(|i| json!({"id":format!("dead{i}"),"card":"BP01-171"}))
                .collect::<Vec<_>>()
        );
        initial["players"]["P2"]["zones"]["field"][0]["state"]["power"] = json!(6_u8);
        let mut game = from_position(&initial);
        play(&mut game, "source");
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(
            view["objects"]["enemy"]["hp"],
            if count == 10_u8 { 16_u8 } else { 20_u8 }
        );
        assert_eq!(
            view["objects"]["enemy"]["power"],
            if count == 10_u8 { 2_u8 } else { 6_u8 }
        );
        assert_eq!(
            view["P1"]["cemetery"].as_array().unwrap().len(),
            usize::from(count)
        );
    }
}
#[test]
fn cp01_058_race_buffs_heals_and_mills_two() {
    let mut game = from_position(&position("CP01-058", "field"));
    let before = game.projection(View::Referee).unwrap();
    feed(&mut game, "CP01-058");
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["leader"]["life"], 12_u8);
    assert_eq!(zone(&view, "draw0"), "cemetery");
    assert_eq!(zone(&view, "draw1"), "cemetery");
    assert_gained_stats(&before, &view);
}
#[test]
fn cp01_061_activation_pays_acts_damages_and_mills() {
    let mut game = from_position(&position("CP01-061", "field"));
    feed(&mut game, "CP01-061");
    activate(&mut game, "CP01-061");
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["objects"]["enemy"]["damage"], 1_u8);
    assert_eq!(view["objects"]["source"]["acted"], true);
    assert_eq!(view["P1"]["pp"]["current"], 8_u8);
    assert_eq!(zone(&view, "draw0"), "cemetery");
}
#[test]
fn cp01_063_race_returns_an_uma_card_from_cemetery() {
    let mut initial = position("CP01-063", "field");
    initial["players"]["P1"]["zones"]["cemetery"] = json!([
        {"id":"non-uma","card":"BP01-171"}, {"id":"uma","card":"CP01-022"}]);
    initial["players"]["P2"]["zones"]["cemetery"] = json!([{"id":"foreign-uma","card":"CP01-022"}]);
    let mut game = from_position(&initial);
    let before = game.projection(View::Referee).unwrap();
    feed(&mut game, "CP01-063");
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(zone(&view, "uma"), "hand");
    assert_eq!(view["P1"]["cemetery"], json!(["non-uma"]));
    assert_eq!(view["P2"]["cemetery"], json!(["foreign-uma"]));
    assert_gained_stats(&before, &view);
}

#[test]
fn cp01_064_lastword_mills_two_after_the_offering_draw() {
    let mut initial = position("CP01-064", "field");
    with_offering(&mut initial);
    let mut game = from_position(&initial);
    offering(&mut game);
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(zone(&view, "draw0"), "hand");
    assert_eq!(zone(&view, "draw1"), "cemetery");
    assert_eq!(zone(&view, "draw2"), "cemetery");
}
#[test]
fn cp01_065_fanfare_destroys_the_opposing_amulet() {
    let mut initial = position("CP01-065", "hand");
    initial["players"]["P2"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"amulet","card":"BP09-102"}));
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"own-amulet","card":"BP09-102"}]);
    let mut game = from_position(&initial);
    play(&mut game, "source");
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(zone(&view, "amulet"), "cemetery");
    assert_eq!(zone(&view, "enemy"), "field");
    assert_eq!(zone(&view, "own-amulet"), "field");
}
#[test]
fn cp01_066_fanfare_sends_own_amulet_to_cemetery_for_three_damage() {
    let mut initial = position("CP01-066", "hand");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"amulet","card":"BP09-102"}]);
    let mut game = from_position(&initial);
    play(&mut game, "source");
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(zone(&view, "amulet"), "cemetery");
    assert_eq!(view["objects"]["enemy"]["damage"], 3_u8);
}

#[test]
fn optional_costs_do_not_resolve_when_declined_or_without_materials() {
    for card in ["CP01-048", "CP01-066"] {
        for has_material in [false, true] {
            let mut initial = position(card, "hand");
            if card == "CP01-066" {
                initial["players"]["P2"]["zones"]["field"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!({"id":"foreign-amulet","card":"BP09-102"}));
            }
            if has_material {
                if card == "CP01-048" {
                    initial["players"]["P1"]["zones"]["hand"]
                        .as_array_mut()
                        .unwrap()
                        .push(json!({"id":"fodder","card":"BP01-171"}));
                } else {
                    initial["players"]["P1"]["zones"]["field"] =
                        json!([{"id":"own-amulet","card":"BP09-102"}]);
                }
            }
            let mut game = from_position(&initial);
            let entered = game
                .play_internal_card("source", PlayerId { seat: 1 })
                .unwrap();
            finish_with_cost(&mut game, entered, !has_material);
            settle_with_cost(&mut game, !has_material);
            let view = game.projection(View::Referee).unwrap();
            assert_eq!(zone(&view, "source"), "field");
            assert_eq!(view["P2"]["leader"]["life"], 20_u8);
            assert_eq!(view["objects"]["enemy"]["damage"], 0_u8);
            if card == "CP01-066" {
                assert_eq!(zone(&view, "foreign-amulet"), "field");
            }
            if has_material {
                if card == "CP01-048" {
                    assert_eq!(zone(&view, "fodder"), "hand");
                } else {
                    assert_eq!(zone(&view, "own-amulet"), "field");
                }
            }
        }
    }
}

#[test]
fn cp01_040_leaves_a_non_uma_card_on_top_without_adding_it_to_hand() {
    let mut game = from_position(&position("CP01-040", "hand"));
    play(&mut game, "source");
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(zone(&view, "draw0"), "deck");
    assert_eq!(view["P1"]["hand"], json!([]));
}
#[test]
fn cp01_061_cannot_activate_or_mill_without_an_opposing_follower() {
    let mut initial = position("CP01-061", "field");
    initial["players"]["P2"]["zones"]["field"] = json!([]);
    let mut game = from_position(&initial);
    let before = game.projection(View::Referee).unwrap();
    assert!(
        game.activate_internal_card(
            "source",
            printed_activation("CP01-061"),
            PlayerId { seat: 1 }
        )
        .is_err(),
        "the required opposing follower must exist before payment or milling"
    );
    assert_eq!(game.projection(View::Referee).unwrap(), before);
}
