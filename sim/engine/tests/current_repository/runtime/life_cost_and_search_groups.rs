use super::*;
use sve_engine::game::internal::SearchAnswer;

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
            InputDomain::DeclareName(_) => panic!("unexpected card name declaration"),
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
