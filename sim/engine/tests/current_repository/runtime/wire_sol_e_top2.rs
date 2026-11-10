#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate these printed card scenarios."
)]
use super::*;
use sve_engine::game::internal::SearchAnswer;

const fn actor() -> PlayerId {
    PlayerId { seat: 1 }
}

fn game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "wire-sol-e-top2",
    )
    .unwrap()
}

fn position(card: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"draw","card":"BP01-171"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    initial["players"]["P2"]["zones"]["deck"] = json!([{"id":"opp-draw","card":"BP01-171"}]);
    initial
}

fn finish(game: &mut Game, mut step: CardPlay, mode: ModeId) {
    while let CardPlay::Awaiting(q) = step {
        let answer = match &q.domain {
            InputDomain::DiceReroll { .. } => panic!("unexpected fixture dice reroll"),
            InputDomain::Targets(d) => Answer::Targets(
                d.members
                    .iter()
                    .take(
                        usize::try_from(d.cardinality.maximum.unwrap_or(d.cardinality.minimum).0)
                            .unwrap(),
                    )
                    .map(|m| m.handle)
                    .collect(),
            ),
            InputDomain::CostMaterials(d) => Answer::CostMaterials(
                d.members
                    .iter()
                    .take(
                        usize::try_from(d.cardinality.maximum.unwrap_or(d.cardinality.minimum).0)
                            .unwrap(),
                    )
                    .map(|m| m.handle)
                    .collect(),
            ),
            InputDomain::Search(d) => Answer::Search(SearchAnswer::Ungrouped(
                d.cards
                    .iter()
                    .take(
                        usize::try_from(d.cardinality.maximum.unwrap_or(d.cardinality.minimum).0)
                            .unwrap(),
                    )
                    .map(|m| m.handle)
                    .collect(),
            )),
            InputDomain::Order(d) => Answer::Order(d.iter().map(|m| m.handle).collect()),
            InputDomain::Modes { .. } => Answer::Modes(vec![mode]),
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            InputDomain::OptionalCost => Answer::OptionalCost(true),
            InputDomain::TokenCapacity { .. } => Answer::TokenCapacity(0),
            InputDomain::Abilities(_)
            | InputDomain::ResourceEntryPayment { .. }
            | InputDomain::Quick { .. }
            | InputDomain::BeforeDraw
            | InputDomain::Pending(_)
            | InputDomain::TriggerQuota(_)
            | InputDomain::Integer(_)
            | InputDomain::DamageReplacements(_)
            | InputDomain::DriveTrigger { .. }
            | InputDomain::SearchPartition(_)
            | InputDomain::Distribution(_)
            | InputDomain::SearchReplacement => panic!("Unexpected card decision"),
        };
        step = game.answer_internal_card(q.actor, q.token, answer).unwrap();
    }
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}

fn play(game: &mut Game, id: &str) {
    let step = game.play_internal_card(id, actor()).unwrap();
    finish(game, step, ModeId(1));
}

fn trigger(game: &mut Game, card: &str) -> CardPlay {
    let q = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &q.domain else {
        panic!("Expected pending ability")
    };
    let chosen = options.iter().find(|o| o.card == card).unwrap();
    game.answer_internal_card(q.actor, q.token, Answer::Pending(chosen.handle))
        .unwrap()
}

fn resolve_trigger(game: &mut Game, card: &str) {
    let step = trigger(game, card);
    finish(game, step, ModeId(1));
}

fn resolve_all(game: &mut Game, card: &str) {
    for _ in 0_u8..8_u8 {
        if game.internal_pending_choice().unwrap().is_none() {
            return;
        }
        resolve_trigger(game, card);
    }
    panic!("Printed entry triggers must finish")
}

#[test]
fn ecp01_018_enters_teio_then_another_uma_from_two_separate_seven_card_looks() {
    let mut initial = position("ECP01-018");
    let mut deck = vec![json!({"id":"teio","card":"CP01-014"})];
    deck.extend((0_u8..6_u8).map(|i| json!({"id":format!("first-{i}"),"card":"BP01-171"})));
    deck.push(json!({"id":"uma","card":"ECP01-007"}));
    deck.extend((0_u8..6_u8).map(|i| json!({"id":format!("second-{i}"),"card":"BP01-171"})));
    initial["players"]["P1"]["zones"]["deck"] = json!(deck);
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["field"], json!(["teio", "uma"]));
    assert_eq!(state["P1"]["deck"].as_array().unwrap().len(), 12);
    assert_eq!(state["P1"]["pp"]["current"], 5_u8);
}

#[test]
fn bp15_091_destroys_the_enemy_then_randomly_discards_one_of_its_controllers_cards() {
    let mut initial = position("BP15-091");
    initial["players"]["P2"]["zones"]["hand"] =
        json!([{"id":"first","card":"BP01-171"},{"id":"second","card":"BP01-171"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    resolve_trigger(&mut game, "BP15-091");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["field"], json!([]));
    assert_eq!(state["P2"]["hand"].as_array().unwrap().len(), 1);
    assert_eq!(state["P2"]["cemetery"].as_array().unwrap().len(), 2);
    assert!(
        state["P2"]["cemetery"]
            .as_array()
            .unwrap()
            .contains(&json!("enemy"))
    );
}

#[test]
fn sd05_001_creates_four_bats_and_gives_each_of_their_entries_power_and_ward() {
    let mut game = game(&position("SD05-001"));
    play(&mut game, "source");
    resolve_all(&mut game, "SD05-001");
    let ability_id = selection_with_rules().current["SD05-001"]
        .program
        .card()
        .abilities
        .iter()
        .find(|a| a.node.kind == AbilityKind::Activated)
        .unwrap()
        .node
        .id;
    let step = game
        .activate_internal_card("source", ability_id, actor())
        .unwrap();
    finish(&mut game, step, ModeId(1));
    resolve_all(&mut game, "SD05-001");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["field"].as_array().unwrap().len(), 5);
    assert_eq!(state["P1"]["leader"]["life"], 19_u8);
    assert_eq!(state["objects"]["source"]["acted"], true);
    for bat in state["P1"]["field"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|id| id.as_str() != Some("source"))
    {
        let id = bat.as_str().unwrap();
        assert_eq!(state["objects"][id]["power"], 2_u8);
        assert_eq!(state["objects"][id]["hp"], 1_u8);
        assert!(
            state["objects"][id]["keywords"]
                .as_array()
                .unwrap()
                .contains(&json!("守護"))
        );
    }
}

#[test]
fn ecp02_sp07a_mills_cute_cool_and_passion_to_heal_gain_assail_and_gain_power() {
    let mut initial = position("ECP02-SP07a");
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"cute","card":"CP02-014"},{"id":"cool","card":"CP02-100"},{"id":"passion","card":"CP02-101"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    resolve_trigger(&mut game, "ECP02-SP07a");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["cemetery"], json!(["cute", "cool", "passion"]));
    assert_eq!(state["P1"]["leader"]["life"], 22_u8);
    assert_eq!(state["objects"]["source"]["power"], 5_u8);
    let keywords = state["objects"]["source"]["keywords"].as_array().unwrap();
    assert!(keywords.contains(&json!("突進")));
    assert!(keywords.contains(&json!("指定攻撃")));
}

#[test]
fn bp16_017_excludes_all_same_name_printings_from_its_beast_cost_before_drawing() {
    let mut initial = position("BP16-017");
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"beast","card":"BP01-013"},
        {"id":"same-name","card":"PR-424"}
    ]);
    let mut game = game(&initial);
    play(&mut game, "source");
    let mut step = trigger(&mut game, "BP16-017");
    if let CardPlay::Awaiting(q) = &step
        && matches!(&q.domain, InputDomain::OptionalCost)
    {
        step = game
            .answer_internal_card(q.actor, q.token, Answer::OptionalCost(true))
            .unwrap();
    }
    let CardPlay::Awaiting(q) = &step else {
        panic!("Expected the public beast material choice")
    };
    let InputDomain::CostMaterials(domain) = &q.domain else {
        panic!("Expected cost materials")
    };
    assert_eq!(domain.members.len(), 1);
    finish(&mut game, step, ModeId(1));
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["field"], json!(["same-name", "source"]));
    assert_eq!(state["P1"]["hand"], json!(["beast", "draw"]));
    assert_eq!(state["P1"]["pp"]["current"], 9_u8);
}
