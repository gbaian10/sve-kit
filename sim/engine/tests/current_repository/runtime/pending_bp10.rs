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
        {"id":"third","card":"BP01-171"}
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

fn in_cemetery(card: &str) -> Value {
    let mut initial = in_hand(card);
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["cemetery"] = json!([{"id":"source","card":card}]);
    initial
}

fn cards(prefix: &str, card: &str, count: usize) -> Vec<Value> {
    (0..count)
        .map(|i| json!({"id":format!("{prefix}{i}"),"card":card}))
        .collect()
}

fn push(initial: &mut Value, zone: &str, extra: Vec<Value>) {
    let slot = &mut initial["players"]["P1"]["zones"][zone];
    if slot.is_null() {
        *slot = json!([]);
    }
    slot.as_array_mut().unwrap().extend(extra);
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
            InputDomain::Targets(domain) => Answer::Targets(
                domain
                    .members
                    .iter()
                    .take(take(&domain.cardinality, domain.members.len()).max(1))
                    .map(|member| member.handle)
                    .collect(),
            ),
            InputDomain::CostMaterials(domain) => Answer::CostMaterials(
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
            .unwrap();
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

fn play_with(game: &mut Game, id: &str, modes: &[u64]) {
    let step = game.play_internal_card(id, actor()).unwrap();
    finish_with(game, step, modes);
    triggers_with(game, modes);
}

fn play(game: &mut Game) {
    play_with(game, "source", &[1]);
}

fn evolve(game: &mut Game) {
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
        )
        .unwrap()
        .outcome,
        "resolved",
        "the base pays its printed evolution cost"
    );
    triggers_with(game, &[1]);
}

/// The `index`-th printed activated ability of the card (0-based).
fn printed_activation(card: &str, index: usize) -> AbilityId {
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

fn activate(game: &mut Game, card: &str, index: usize) {
    let step = game
        .activate_internal_card("source", printed_activation(card, index), actor())
        .unwrap();
    finish_with(game, step, &[1]);
    triggers_with(game, &[1]);
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
        triggers_with(game, &[1]);
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

#[test]
fn bp10_011_puts_a_seen_card_in_ex_then_deals_damage_per_distinct_ex_cost() {
    let mut initial = in_hand("BP10-011");
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"sage","card":"BP01-037"},{"id":"goblin","card":"BP01-171"},
        {"id":"other","card":"BP01-171"}
    ]);
    initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"held","card":"BP01-171"}]);
    let mut game = game(&initial);
    play(&mut game);
    let entered = game.projection(View::Referee).unwrap();
    assert_eq!(entered["P1"]["ex"], json!(["held", "sage"]));
    assert_eq!(entered["P1"]["deck"], json!(["goblin", "other"]));
    activate(&mut game, "BP10-011", 0);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        state["objects"]["enemy"]["damage"], 2_u8,
        "costs one and five are two kinds"
    );
}

#[test]
fn bp10_026_banishes_itself_from_the_cemetery_beside_five_other_heroes() {
    let mut initial = in_cemetery("BP10-026");
    push(&mut initial, "cemetery", cards("hero", "BP13-034", 5));
    let mut game = game(&initial);
    activate(&mut game, "BP10-026", 0);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["leader"]["life"], 17_u8);
    assert_eq!(state["P1"]["banish"], json!(["source"]));
    assert_eq!(state["P1"]["pp"]["current"], 9_u8);
}

#[test]
fn bp10_031_mills_two_and_evolves_when_the_opponent_reaches_ten() {
    let mut initial = in_hand("BP10-031");
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP10-032"}]);
    initial["players"]["P2"]["zones"]["deck"] = json!(cards("theirs", "BP01-171", 2));
    initial["players"]["P2"]["zones"]["cemetery"] = json!(cards("grave", "BP01-171", 8));
    let mut game = game(&initial);
    play(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["cemetery"].as_array().unwrap().len(), 10);
    assert_eq!(state["P1"]["evolve_deck"], json!([]));
}

#[test]
fn bp10_036_second_mode_searches_a_royal_follower_beside_a_princess() {
    let mut initial = in_hand("BP10-036");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"princess","card":"BP04-032"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"goblin","card":"BP01-171"},{"id":"royal","card":"BP01-037"}
    ]);
    let mut game = game(&initial);
    play_with(&mut game, "source", &[2]);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["royal"]));
    assert_eq!(state["P1"]["deck"], json!(["goblin"]));
}

#[test]
fn bp10_064_three_armed_cards_bring_a_weapon_and_three_damage() {
    let mut initial = in_hand("BP10-064");
    push(&mut initial, "cemetery", cards("armed", "BP03-064", 3));
    let mut game = game(&initial);
    play(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        named(&state, &state["P1"]["field"], "ドラゴウェポン").len(),
        1
    );
    assert_eq!(state["P2"]["leader"]["life"], 17_u8);
}

#[test]
fn bp10_065_evolution_fields_an_armed_follower_and_a_weapon() {
    let mut initial = on_field("BP10-064");
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP10-065"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"goblin","card":"BP01-171"},{"id":"hammer","card":"BP03-064"}
    ]);
    let mut game = game(&initial);
    evolve(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert!(
        state["P1"]["field"]
            .as_array()
            .unwrap()
            .contains(&json!("hammer"))
    );
    assert_eq!(
        named(&state, &state["P1"]["field"], "ドラゴウェポン").len(),
        1
    );
}

#[test]
fn bp10_073_awakened_places_a_dragon_follower_in_ex_two_cheaper() {
    let mut initial = in_hand("BP10-073");
    initial["players"]["P1"]["pp"] = json!({"current":7_u8,"max":7_u8});
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"goblin","card":"BP01-171"},{"id":"dragon","card":"BP10-064"},
        {"id":"other","card":"BP01-171"}
    ]);
    let mut game = game(&initial);
    play(&mut game);
    let placed = game.projection(View::Referee).unwrap();
    assert_eq!(placed["P1"]["ex"], json!(["dragon"]));
    assert_eq!(placed["P1"]["pp"]["current"], 5_u8);
    play_with(&mut game, "dragon", &[1]);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        4_u8,
        "the three-cost dragonewt costs one"
    );
}

#[test]
fn bp10_080_crimson_fanfare_then_attack_damage_equals_power() {
    let mut initial = in_hand("BP10-080");
    push(
        &mut initial,
        "hand",
        vec![json!({"id":"payer","card":"BP01-118"})],
    );
    let mut game = game(&initial);
    // The payer pings every leader, so our life decreased this turn.
    play_with(&mut game, "payer", &[1]);
    play(&mut game);
    let entered = game.projection(View::Referee).unwrap();
    assert_eq!(entered["objects"]["source"]["power"], 3_u8);
    assert!(has_keyword(&entered, "source", "突進"));
    attack(&mut game, "source", "enemy");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["leader"]["life"], 16_u8, "one from the payer, three from the attack");
    assert_eq!(state["P2"]["cemetery"], json!(["enemy"]), "bane destroys");
}

#[test]
fn bp10_096_evolution_places_a_ward_follower_in_ex_one_cheaper() {
    let mut initial = on_field("BP10-095");
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP10-096"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"goblin","card":"BP01-171"},{"id":"warden","card":"DSD01b-013"},
        {"id":"a","card":"BP01-171"},{"id":"b","card":"BP01-171"}
    ]);
    let mut game = game(&initial);
    evolve(&mut game);
    let placed = game.projection(View::Referee).unwrap();
    assert_eq!(placed["P1"]["ex"], json!(["warden"]));
    let before = placed["P1"]["pp"]["current"].clone();
    play_with(&mut game, "warden", &[1]);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        json!(before.as_u64().unwrap().checked_sub(3).unwrap()),
        "the four-cost ward follower costs three"
    );
}

#[test]
fn bp10_099_pays_two_for_plus_one_plus_three() {
    let mut initial = in_hand("BP10-099");
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"goblin","card":"BP01-171"}]);
    let mut game = game(&initial);
    play(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["source"]["power"], 3_u8);
    assert_eq!(state["objects"]["source"]["hp"], 6_u8);
    assert_eq!(state["P1"]["pp"]["current"], 5_u8);
}

#[test]
fn bp10_042_spell_deals_damage_per_chess_card_in_the_cemetery() {
    let mut initial = in_hand("BP10-042");
    push(&mut initial, "cemetery", cards("chess", "BP03-053", 2));
    let mut game = game(&initial);
    play(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["damage"], 2_u8);
    assert_eq!(state["P1"]["cemetery"].as_array().unwrap().len(), 3);
}

#[test]
fn bp10_084_spell_costs_two_life_for_two_cards_and_the_grave_ability_needs_temperance() {
    let mut game = game(&in_hand("BP10-084"));
    play(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["leader"]["life"], 18_u8);
    assert_eq!(state["P1"]["hand"], json!(["first", "second"]));
    assert_eq!(state["P1"]["cemetery"], json!(["source"]));
    let before = game.projection(View::Referee).unwrap();
    game.activate_internal_card("source", printed_activation("BP10-084", 0), actor())
        .unwrap_err();
    assert_eq!(
        game.projection(View::Referee).unwrap(),
        before,
        "without Temperance the grave ability is refused"
    );
}
