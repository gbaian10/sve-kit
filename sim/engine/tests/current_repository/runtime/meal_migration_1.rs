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
            | InputDomain::MealPayment { .. }
            | InputDomain::BeforeDraw
            | InputDomain::Pending(_)
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

#[test]
fn cp01_001_meal_keeps_storm_and_spends_a_resource() {
    for card in ["CP01-001", "CP01-SL01", "CP01-SP01", "CSD01-001"] {
        let mut game = from_position(&position(card, "field"));
        feed(&mut game, card);
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["P1"]["pp"]["current"], 9_u8);
        assert_eq!(zone(&view, "carrot0"), "race");
        assert!(
            view["objects"]["source"]["keywords"]
                .as_array()
                .unwrap()
                .contains(&json!("疾走"))
        );
    }
}
#[test]
fn cp01_007_race_buffs_before_returning_an_opposing_follower() {
    let mut game = from_position(&position("CP01-007", "field"));
    let before = game.projection(View::Referee).unwrap();
    feed(&mut game, "CP01-007");
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(zone(&view, "enemy"), "hand");
    assert_eq!(
        view["objects"]["source"]["power"].as_i64().unwrap(),
        before["objects"]["source"]["power"]
            .as_i64()
            .unwrap()
            .saturating_add(1)
    );
    assert_eq!(
        view["objects"]["source"]["hp"].as_i64().unwrap(),
        before["objects"]["source"]["hp"]
            .as_i64()
            .unwrap()
            .saturating_add(1)
    );
}
#[test]
fn cp01_009_return_cost_recovers_two_life() {
    let mut initial = position("CP01-009", "hand");
    with_ally(&mut initial);
    let mut game = from_position(&initial);
    play(&mut game, "source");
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(zone(&view, "ally"), "hand");
    assert_eq!(zone(&view, "enemy"), "field");
    assert_eq!(view["P1"]["leader"]["life"], 12_u8);
}
#[test]
fn cp01_010_return_cost_buffs_the_ward_follower() {
    let mut initial = position("CP01-010", "hand");
    with_ally(&mut initial);
    let mut game = from_position(&initial);
    let before = game.projection(View::Referee).unwrap();
    play(&mut game, "source");
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(zone(&view, "ally"), "hand");
    assert_eq!(zone(&view, "enemy"), "field");
    assert_eq!(
        view["objects"]["source"]["power"].as_i64().unwrap(),
        before["objects"]["source"]["power"]
            .as_i64()
            .unwrap()
            .saturating_add(1)
    );
}
#[test]
fn cp01_011_fanfare_returns_another_allied_card() {
    let mut initial = position("CP01-011", "hand");
    with_ally(&mut initial);
    let mut game = from_position(&initial);
    play(&mut game, "source");
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(zone(&view, "ally"), "hand");
    assert_eq!(zone(&view, "enemy"), "field");
    assert_eq!(zone(&view, "source"), "field");
}
#[test]
fn cp01_016_can_act_for_five_damage_after_meal() {
    let mut game = from_position(&position("CP01-016", "field"));
    feed(&mut game, "CP01-016");
    activate(&mut game, "CP01-016");
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["objects"]["enemy"]["damage"], 5_u8);
    assert_eq!(view["objects"]["source"]["acted"], true);
}
#[test]
fn cp01_023_meal_spends_pp_without_replacing_printed_rush() {
    let mut game = from_position(&position("CP01-023", "field"));
    feed(&mut game, "CP01-023");
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["objects"]["source"]["keywords"], json!(["突進"]));
    assert_eq!(view["P1"]["pp"]["current"], 9_u8);
    assert_eq!(zone(&view, "carrot0"), "race");
}
#[test]
fn cp01_026_pays_three_pp_and_acts_for_three_damage() {
    let mut game = from_position(&position("CP01-026", "field"));
    feed(&mut game, "CP01-026");
    activate(&mut game, "CP01-026");
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["objects"]["enemy"]["damage"], 3_u8);
    assert_eq!(view["objects"]["source"]["acted"], true);
    assert_eq!(view["P1"]["pp"]["current"], 6_u8);
}
#[test]
fn cp01_033_race_draws_then_discards_the_drawn_card() {
    let mut game = from_position(&position("CP01-033", "field"));
    feed(&mut game, "CP01-033");
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(zone(&view, "draw0"), "cemetery");
    assert_eq!(view["P1"]["hand"], json!([]));
}
#[test]
fn cp01_034_damage_changes_only_when_an_allied_follower_is_racing() {
    for racing in [false, true] {
        let mut initial = position("CP01-023", "field");
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"spell","card":"CP01-034"}]);
        let mut game = from_position(&initial);
        if racing {
            feed(&mut game, "CP01-023");
        }
        play(&mut game, "spell");
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
            if racing { 3_u8 } else { 2_u8 }
        );
    }
}
#[test]
fn cp01_036_discards_as_a_fanfare_cost_before_four_damage() {
    let mut initial = position("CP01-036", "hand");
    initial["players"]["P1"]["zones"]["hand"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"fodder","card":"BP01-171"}));
    let mut game = from_position(&initial);
    play(&mut game, "source");
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(zone(&view, "fodder"), "cemetery");
    assert_eq!(view["objects"]["enemy"]["damage"], 4_u8);
}
#[test]
fn cp01_037_hits_enemy_leader_and_followers_then_discards_all_hand() {
    for has_fodder in [false, true] {
        let mut initial = position("CP01-037", "hand");
        if has_fodder {
            initial["players"]["P1"]["zones"]["hand"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":"fodder","card":"BP01-171"}));
        }
        let mut game = from_position(&initial);
        play(&mut game, "source");
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["objects"]["enemy"]["damage"], 5_u8);
        assert_eq!(view["P2"]["leader"]["life"], 15_u8);
        assert_eq!(view["P1"]["hand"], json!([]));
        if has_fodder {
            assert_eq!(zone(&view, "fodder"), "cemetery");
        }
    }
}

#[test]
fn cp01_038_lastword_returns_a_spell_from_cemetery() {
    let mut initial = position("CP01-038", "field");
    with_offering(&mut initial);
    initial["players"]["P1"]["zones"]["cemetery"] = json!([{"id":"spell","card":"BP01-140"}]);
    let mut game = from_position(&initial);
    offering(&mut game);
    assert_eq!(
        zone(&game.projection(View::Referee).unwrap(), "spell"),
        "hand"
    );
}
#[test]
fn cp01_039_race_buffs_draws_two_and_discards_one() {
    let mut game = from_position(&position("CP01-039", "field"));
    let before = game.projection(View::Referee).unwrap();
    feed(&mut game, "CP01-039");
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["hand"].as_array().unwrap().len(), 1_usize);
    assert_eq!(zone(&view, "draw0"), "cemetery");
    assert_eq!(zone(&view, "draw1"), "hand");
    assert_eq!(
        view["objects"]["source"]["power"].as_i64().unwrap(),
        before["objects"]["source"]["power"]
            .as_i64()
            .unwrap()
            .saturating_add(1)
    );
}

#[test]
fn optional_fanfare_costs_can_be_declined_without_resolving_their_effects() {
    for card in ["CP01-009", "CP01-010", "CP01-036"] {
        let mut initial = position(card, "hand");
        if card == "CP01-036" {
            initial["players"]["P1"]["zones"]["hand"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":"fodder","card":"BP01-171"}));
        } else {
            with_ally(&mut initial);
        }
        let mut game = from_position(&initial);
        let before = game.projection(View::Referee).unwrap();
        let entered = game
            .play_internal_card("source", PlayerId { seat: 1 })
            .unwrap();
        finish_with_cost(&mut game, entered, false);
        settle_with_cost(&mut game, false);
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(zone(&view, "source"), "field");
        assert_eq!(zone(&view, "enemy"), "field");
        assert_eq!(view["objects"]["enemy"]["damage"], 0_u8);
        assert_eq!(view["P1"]["leader"]["life"], 10_u8);
        assert_eq!(
            view["objects"]["source"]["power"],
            before["objects"]["source"]["power"]
        );
        if card == "CP01-036" {
            assert_eq!(zone(&view, "fodder"), "hand");
        } else {
            assert_eq!(zone(&view, "ally"), "field");
        }
    }
}

#[test]
fn a_return_cost_cannot_use_the_source_or_an_opposing_card_as_material() {
    for card in ["CP01-009", "CP01-010"] {
        let mut game = from_position(&position(card, "hand"));
        let before = game.projection(View::Referee).unwrap();
        play(&mut game, "source");
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(zone(&view, "source"), "field");
        assert_eq!(zone(&view, "enemy"), "field");
        assert_eq!(view["P1"]["leader"]["life"], 10_u8);
        assert_eq!(
            view["objects"]["source"]["power"],
            before["objects"]["source"]["power"]
        );
    }
}
