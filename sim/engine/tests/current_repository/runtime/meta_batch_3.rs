#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate these game scenarios."
)]
use super::*;
use sve_engine::game::internal::SearchAnswer;

const fn actor() -> PlayerId {
    PlayerId { seat: 1 }
}

fn in_hand(card: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"first","card":"BP01-171"},{"id":"second","card":"BP01-171"},
        {"id":"third","card":"BP01-171"},{"id":"fourth","card":"BP01-171"}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    initial
}

fn on_field(card: &str) -> Value {
    let mut initial = in_hand(card);
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":card}]);
    initial
}

fn push(initial: &mut Value, player: &str, zone: &str, object: Value) {
    let slot = &mut initial["players"][player]["zones"][zone];
    if slot.is_null() {
        *slot = json!([]);
    }
    slot.as_array_mut().unwrap().push(object);
}

fn fill(initial: &mut Value, zone: &str, card: &str, count: usize) {
    let start = initial["players"]["P1"]["zones"][zone]
        .as_array()
        .map_or(0, Vec::len);
    for i in (start..).take(count) {
        push(initial, "P1", zone, json!({"id":format!("{zone}{i}"),"card":card}));
    }
}

fn take(cardinality: &Cardinality, available: usize) -> usize {
    cardinality
        .maximum
        .map_or(available, |most| usize::try_from(most.0).unwrap())
        .min(available)
}

fn finish_with(game: &mut Game, mut play: CardPlay, modes: &[u64]) {
    while let CardPlay::Awaiting(question) = play {
        let answer = match &question.domain {
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            InputDomain::OptionalCost => Answer::OptionalCost(true),
            InputDomain::Modes { .. } => Answer::Modes(modes.iter().copied().map(ModeId).collect()),
            InputDomain::Integer(bounds) => Answer::Integer(bounds.maximum.unwrap_or(bounds.minimum)),
            InputDomain::Search(domain) => Answer::Search(SearchAnswer::Ungrouped(
                domain
                    .cards
                    .iter()
                    .take(take(&domain.cardinality, domain.cards.len()))
                    .map(|card| card.handle)
                    .collect(),
            )),
            InputDomain::Order(cards) => {
                Answer::Order(cards.iter().map(|card| card.handle).collect())
            }
            InputDomain::CostMaterials(domain) => Answer::CostMaterials(
                domain
                    .members
                    .iter()
                    .take(take(&domain.cardinality, domain.members.len()))
                    .map(|member| member.handle)
                    .collect(),
            ),
            InputDomain::Targets(domain) => Answer::Targets(
                domain
                    .members
                    .iter()
                    .take(take(&domain.cardinality, domain.members.len()))
                    .map(|member| member.handle)
                    .collect(),
            ),
            other => panic!("unexpected decision: {other:?}"),
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap_or_else(|e| panic!("{:?}: {e:?}", question.domain));
    }
}

fn triggers_with(game: &mut Game, modes: &[u64]) {
    loop {
        if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
        let Some(question) = game.internal_pending_choice().unwrap() else {
            break;
        };
        let InputDomain::Pending(options) = &question.domain else {
            panic!("pending ability expected")
        };
        let step = game
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap();
        finish_with(game, step, modes);
    }
}

fn triggers(game: &mut Game) {
    triggers_with(game, &[1]);
}

fn play_with(game: &mut Game, id: &str, modes: &[u64]) {
    let step = game.play_internal_card(id, actor()).unwrap();
    finish_with(game, step, modes);
    triggers_with(game, modes);
}

fn play(game: &mut Game, id: &str) {
    play_with(game, id, &[1]);
}

fn evolve(game: &mut Game, pp: u8) {
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":pp}})
        )
        .unwrap()
        .outcome,
        "resolved",
        "the base pays its printed evolution cost"
    );
    triggers(game);
}

fn activation(card: &str, index: usize) -> AbilityId {
    selection().current[card]
        .program
        .card()
        .abilities
        .iter()
        .filter(|a| {
            a.node.kind == AbilityKind::Activated
                && matches!(a.node.source.origin, AbilityOrigin::Printed { .. })
        })
        .nth(index)
        .unwrap()
        .node
        .id
}

fn activate(game: &mut Game, card: &str, index: usize) -> Result<(), EngineFailure> {
    let step = game.activate_internal_card("source", activation(card, index), actor())?;
    finish_with(game, step, &[1]);
    triggers(game);
    Ok(())
}

fn attack(game: &mut Game, attacker: &str, target: &str) {
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":attacker,"target":target}))
            .unwrap()
            .outcome,
        "resolved",
        "the attack is declared"
    );
    loop {
        triggers(game);
        let Some(question) = game.internal_quick_choice().unwrap() else {
            break;
        };
        let step = game
            .answer_internal_card(question.actor, question.token, Answer::Quick(None))
            .unwrap();
        finish_with(game, step, &[1]);
    }
}

fn named(state: &Value, zone: &Value, name: &str) -> Vec<String> {
    zone.as_array()
        .unwrap()
        .iter()
        .map(|id| id.as_str().unwrap().to_owned())
        .filter(|id| state["objects"][id]["name"] == name)
        .collect()
}

fn has_keyword(state: &Value, id: &str, keyword: &str) -> bool {
    state["objects"][id]["keywords"]
        .as_array()
        .unwrap()
        .contains(&json!(keyword))
}

fn end_phase(game: &mut Game) {
    assert_eq!(
        game.internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap()
            .outcome,
        "resolved",
        "the active player enters its end phase"
    );
    triggers(game);
}

fn rules_game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "meta-batch-3",
    )
    .unwrap()
}

#[test]
fn bp05_009_places_two_puppets_that_gain_power_and_storm_on_entry() {
    let mut game = game(&in_hand("BP05-009"));
    play(&mut game, "source");
    let placed = game.projection(View::Referee).unwrap();
    let puppets = named(&placed, &placed["P1"]["ex"], "操り人形");
    assert_eq!(puppets.len(), 2);
    play(&mut game, &puppets[0]);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"][puppets[0].as_str()]["power"], 2_u8);
    assert!(has_keyword(&state, &puppets[0], "疾走"));
}

#[test]
fn bp05_010_discounts_puppets_and_buffs_them_on_entry() {
    let mut initial = on_field("BP05-010");
    initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"puppet","card":"BP05-T03"}]);
    let mut game = game(&initial);
    play(&mut game, "puppet");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["pp"]["current"], 10_u8, "the 1-cost puppet costs 0");
    assert_eq!(state["objects"]["puppet"]["power"], 2_u8);
    assert!(has_keyword(&state, "puppet", "疾走"));
}

#[test]
fn bp02_t03_buffs_and_rushes_entering_royal_followers() {
    let mut initial = on_field("BP02-T03");
    push(&mut initial, "P1", "hand", json!({"id":"royal","card":"BP01-037"}));
    let mut game = game(&initial);
    play(&mut game, "royal");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["royal"]["power"], 8_u8);
    assert_eq!(state["objects"]["royal"]["hp"], 8_u8);
    assert!(has_keyword(&state, "royal", "突進"));
}

#[test]
fn bp02_009_evolution_puts_an_enemy_follower_on_the_bottom_of_its_deck() {
    let mut initial = on_field("BP02-008");
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP02-009"}]);
    initial["players"]["P2"]["zones"]["deck"] = json!([{"id":"top","card":"BP01-171"}]);
    let mut game = game(&initial);
    evolve(&mut game, 2);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["field"], json!([]));
    assert_eq!(state["P2"]["deck"], json!(["top", "enemy"]));
}

#[test]
fn bp03_108_evolution_places_up_to_two_fairy_tale_cards_in_ex() {
    let mut initial = on_field("BP03-107");
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP03-108"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"t1","card":"BP03-107"},{"id":"plain","card":"BP01-171"},
        {"id":"t2","card":"BP03-107"},{"id":"t3","card":"BP03-107"}
    ]);
    let mut game = game(&initial);
    evolve(&mut game, 1);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["ex"].as_array().unwrap().len(), 2);
    assert!(!state["P1"]["ex"].as_array().unwrap().contains(&json!("plain")));
}

#[test]
fn bp03_074_buffs_ghosts_entering_its_field() {
    let mut initial = on_field("BP03-074");
    initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"ghost","card":"BP01-T14"}]);
    let mut game = game(&initial);
    play(&mut game, "ghost");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["ghost"]["power"], 2_u8);
}

#[test]
fn bp03_075_summons_a_giant_ghost_when_its_ghost_vanishes() {
    let mut initial = on_field("BP03-074");
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP03-075"}]);
    push(&mut initial, "P1", "field", json!({"id":"ghost","card":"BP01-T14"}));
    let mut game = game(&initial);
    evolve(&mut game, 1);
    end_phase(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert!(!state["P1"]["field"].as_array().unwrap().contains(&json!("ghost")));
    assert_eq!(named(&state, &state["P1"]["field"], "ジャイアントゴースト").len(), 1);
}

#[test]
fn bp05_090_evolution_banishes_a_standing_enemy() {
    let mut initial = on_field("BP05-089");
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP05-090"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"acted","card":"BP01-037","state":{"acted":true}},{"id":"enemy","card":"BP01-037"}
    ]);
    let mut game = game(&initial);
    evolve(&mut game, 2);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["field"], json!(["acted"]));
    assert_eq!(state["P2"]["banish"], json!(["enemy"]));
}

#[test]
fn bp06_051_fanfare_makes_an_earth_sigil() {
    let mut game = game(&in_hand("BP06-051"));
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(named(&state, &state["P1"]["field"], "大地の魔片").len(), 1);
}

#[test]
fn cp03_077_draws_only_after_an_enemy_follower_died_this_turn() {
    let mut initial = on_field("CP03-077");
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-171","state":{"acted":true}}]);
    push(&mut initial, "P1", "field", json!({"id":"striker","card":"BP01-037"}));
    let mut game = rules_game(&initial);
    assert!(activate(&mut game, "CP03-077", 0).is_err(), "no enemy follower died yet");
    attack(&mut game, "striker", "enemy");
    activate(&mut game, "CP03-077", 0).unwrap();
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["first"]));
}

#[test]
fn ecp02_018_evolution_deals_two_and_places_an_item_with_five_passion_cards() {
    let mut initial = on_field("ECP02-017");
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"ECP02-018"}]);
    fill(&mut initial, "cemetery", "ECP02-017", 5);
    let mut game = rules_game(&initial);
    evolve(&mut game, 1);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["damage"], 2_u8);
    assert_eq!(named(&state, &state["P1"]["ex"], "魔法のアイテム").len(), 1);
}

#[test]
fn cp02_055_fanfare_fields_a_three_cost_and_a_one_cost_follower() {
    let mut initial = in_hand("CP02-055");
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"three","card":"BP06-044"},{"id":"one","card":"BP01-171"},{"id":"five","card":"BP01-037"}
    ]);
    let mut game = rules_game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    let field = state["P1"]["field"].as_array().unwrap();
    assert!(field.contains(&json!("three")) && field.contains(&json!("one")));
    assert!(!field.contains(&json!("five")));
}
