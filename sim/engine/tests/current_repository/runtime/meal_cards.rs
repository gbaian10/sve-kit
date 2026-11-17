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
        "meal-cards",
    )
    .unwrap()
}
fn finish(game: &mut Game, play: CardPlay) {
    finish_with_cost(game, play, true);
}
fn finish_with_cost(game: &mut Game, mut play: CardPlay, pay_cost: bool) {
    while let CardPlay::Awaiting(question) = play {
        let answer = match &question.domain {
            InputDomain::DiceReroll { .. } => panic!("unexpected fixture dice reroll"),
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
            | InputDomain::Abilities(_)
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
fn cp01_033_race_draws_then_discards_the_drawn_card() {
    let mut game = from_position(&position("CP01-033", "field"));
    feed(&mut game, "CP01-033");
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(zone(&view, "draw0"), "cemetery");
    assert_eq!(view["P1"]["hand"], json!([]));
}
