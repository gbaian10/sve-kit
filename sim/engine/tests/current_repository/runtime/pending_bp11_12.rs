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
            InputDomain::TokenCapacity { choices } => {
                let chosen = choices
                    .iter()
                    .position(|names| {
                        matches!(names.as_slice(), [name] if name == "魔導装甲車" || name == "リペアモード")
                    })
                    .unwrap();
                Answer::TokenCapacity(u64::try_from(chosen).unwrap())
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

fn named(state: &Value, zone: &Value, name: &str) -> Vec<String> {
    zone.as_array()
        .unwrap()
        .iter()
        .map(|id| id.as_str().unwrap().to_owned())
        .filter(|id| state["objects"][id]["name"] == name)
        .collect()
}

const HORSE: &str = "BP11-P31";

#[test]
fn bp11_006_chooses_the_armored_car_for_the_last_ex_slot_then_counts_vehicles() {
    let mut initial = in_hand("BP11-006");
    initial["players"]["P1"]["zones"]["ex"] = json!([
        {"id":"horse1","card":HORSE},{"id":"horse2","card":HORSE},
        {"id":"horse3","card":HORSE},{"id":"other","card":"BP01-171"}
    ]);
    let mut game = game(&initial);
    play(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["ex"].as_array().unwrap().len(), 5);
    assert_eq!(named(&state, &state["P1"]["ex"], "魔導装甲車").len(), 1);
    assert_eq!(named(&state, &state["P1"]["ex"], "機動二輪車").len(), 0);
    assert_eq!(state["P2"]["leader"]["life"], 16_u8);
    assert_eq!(state["objects"]["enemy"]["damage"], 4_u8);
}

#[test]
fn bp11_009_evolution_places_a_horse_whose_arrival_deals_three() {
    let mut initial = on_field("BP11-008");
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP11-009"}]);
    let mut game = game(&initial);
    evolve(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(named(&state, &state["P1"]["ex"], "従順な駿馬").len(), 1);
    assert_eq!(state["objects"]["enemy"]["damage"], 3_u8);
}

#[test]
fn bp11_014_lastword_deals_three_with_three_vehicles_on_field_and_ex() {
    let mut initial = on_field("BP11-014");
    initial["players"]["P1"]["zones"]["ex"] = json!([
        {"id":"horse1","card":HORSE},{"id":"horse2","card":HORSE}
    ]);
    push(
        &mut initial,
        "field",
        vec![json!({"id":"horse3","card":HORSE})],
    );
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"reaper","card":"BP22-084"}]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-037","state":{"hp":20_u8,"max_hp":20_u8}}]);
    let mut game = game(&initial);
    // The reaper's fanfare puts the cowboy into the cemetery as its cost.
    play_with(&mut game, "reaper", &[1]);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["cemetery"], json!(["source"]));
    assert_eq!(
        state["objects"]["enemy"]["damage"], 8_u8,
        "five from the reaper and three from the last word"
    );
}

#[test]
fn bp11_048_fanfare_horse_triggers_a_draw_and_discard_once() {
    let mut game = game(&in_hand("BP11-048"));
    play(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(named(&state, &state["P1"]["field"], "従順な駿馬").len(), 1);
    assert_eq!(state["P1"]["hand"].as_array().unwrap().len(), 0);
    assert_eq!(state["P1"]["cemetery"].as_array().unwrap().len(), 1);
    assert_eq!(state["P1"]["deck"], json!(["second", "third"]));
}

#[test]
fn bp11_048_does_not_draw_when_a_vehicle_enters_during_the_opponents_turn() {
    let mut initial = on_field("BP11-048");
    initial["turn"]["active"] = json!("P2");
    push(
        &mut initial,
        "field",
        vec![json!({"id":"producer","card":"producer"})],
    );
    let mut tree = expanded::tree().clone();
    let mut source = tree["cards"]["BP01-041"].clone();
    let mut ability = source["abilities"][0].clone();
    ability.as_object_mut().unwrap().remove("activate");
    ability["auto"] = json!({
        "on":{"phase":{"whose":"opp","which":"end"}},
        "do":[{"create":{"token":"従順な駿馬","n":1_u8,"to":"field"}}]
    });
    source["abilities"] = json!([ability]);
    tree["cards"] = json!({"producer":source});
    let producer = lowerer().lower(&tree).unwrap().remove("producer").unwrap();
    let mut game = Game::from_selected_effects(
        producer_selection().clone(),
        &initial,
        &Value::Null,
        &Value::Null,
        "opponent-vehicle-entry",
    )
    .unwrap();
    game.load_internal_card("producer", &producer).unwrap();
    assert_eq!(
        game.internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap()
            .outcome,
        "resolved"
    );
    triggers_with(&mut game, &[1]);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(named(&state, &state["P1"]["field"], "従順な駿馬").len(), 1);
    assert_eq!(state["P1"]["deck"], json!(["first", "second", "third"]));
    assert_eq!(state["P1"]["hand"], json!([]));
    assert_eq!(state["P1"]["cemetery"], json!([]));
}

#[test]
fn bp11_072_evolves_for_one_only_while_crimson() {
    let evolutions: Vec<u64> = selection().current["BP11-072"]
        .program
        .card()
        .abilities
        .iter()
        .filter(|a| {
            matches!(
                a.node.source.origin,
                AbilityOrigin::Keyword {
                    keyword: KeywordId::Evolution,
                    ..
                }
            )
        })
        .map(|a| a.node.id.0)
        .collect();
    let [_, crimson] = evolutions.as_slice() else {
        panic!("two printed evolutions")
    };
    let cheap = json!({"do":"evolve","source":"source","evolve_card":"form","definition":crimson,"pay":{"pp":1_u8}});
    let mut initial = on_field("BP11-072");
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP11-073"}]);
    let mut calm = game(&initial);
    assert_eq!(
        calm.internal_rule_action(&cheap).unwrap().outcome,
        "cannot-evolve",
        "the one-cost evolution needs crimson"
    );
    push(
        &mut initial,
        "hand",
        vec![json!({"id":"payer","card":"BP01-118"})],
    );
    let mut game = game(&initial);
    play_with(&mut game, "payer", &[1]);
    assert_eq!(
        game.internal_rule_action(&cheap).unwrap().outcome,
        "resolved"
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["evolve_deck"], json!([]));
    assert_eq!(
        state["P1"]["pp"]["current"], 8_u8,
        "one for the payer, one to evolve"
    );
}

#[test]
fn bp11_095_pays_three_to_banish_when_an_own_vehicle_leaves() {
    let mut initial = on_field("BP11-095");
    push(
        &mut initial,
        "field",
        vec![json!({"id":"horse","card":HORSE})],
    );
    let mut game = game(&initial);
    let horse = printed_activation(HORSE, 0);
    let step = game
        .activate_internal_card("horse", horse, actor())
        .unwrap();
    finish_with(&mut game, step, &[1]);
    triggers_with(&mut game, &[1]);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["banish"], json!(["enemy"]));
    assert_eq!(state["P1"]["pp"]["current"], 7_u8);
}

#[test]
fn bp11_098_evolution_draws_two_with_four_ward_followers() {
    let mut initial = on_field("BP11-097");
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP11-098"}]);
    push(&mut initial, "field", cards("warden", "BP11-097", 3));
    let mut game = game(&initial);
    evolve(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["hand"],
        json!(["first", "second"])
    );
}

const TREE: &str = "BP07-T03";

#[test]
fn bp12_009_evolution_fields_a_cheap_hunter_from_the_top_four() {
    let mut initial = on_field("BP12-008");
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP12-009"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"goblin","card":"BP01-171"},{"id":"hunter","card":"BP04-010"},
        {"id":"sage","card":"BP01-037"},{"id":"other","card":"BP01-171"}
    ]);
    let mut game = game(&initial);
    evolve(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["field"], json!(["source", "hunter"]));
    assert_eq!(state["P1"]["deck"], json!(["goblin", "sage", "other"]));
}

#[test]
fn bp12_019_evolution_recovers_five_pp_with_five_nature_cards() {
    let mut initial = on_field("BP12-018");
    initial["players"]["P1"]["pp"]["current"] = json!(1_u8);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP12-019"}]);
    push(&mut initial, "field", cards("tree", TREE, 3));
    initial["players"]["P1"]["zones"]["ex"] = json!(cards("held", TREE, 1));
    let mut game = game(&initial);
    evolve(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        5_u8,
        "itself, three field trees and one EX tree are five nature cards"
    );
}

#[test]
fn bp12_038_costs_less_per_distinct_cemetery_cost() {
    let mut initial = in_hand("BP12-038");
    initial["players"]["P1"]["zones"]["cemetery"] = json!([
        {"id":"one","card":"BP01-171"},{"id":"five","card":"BP01-037"},
        {"id":"two","card":"BP07-044"},{"id":"three","card":"BP02-042"},{"id":"dup","card":"BP01-171"}
    ]);
    let mut game = game(&initial);
    play(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["field"], json!(["source"]));
    assert_eq!(
        state["P1"]["pp"]["current"], 0_u8,
        "fourteen minus four kinds"
    );
}

#[test]
fn bp12_040_grows_four_and_four_with_five_mages_and_spellchain() {
    let mut initial = in_hand("BP12-040");
    push(&mut initial, "cemetery", cards("mage", "BP02-042", 5));
    push(&mut initial, "cemetery", cards("spell", "BP07-017", 5));
    let mut game = game(&initial);
    play(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["source"]["power"], 5_u8);
    assert_eq!(state["objects"]["source"]["hp"], 5_u8);
}

#[test]
fn bp12_041_fields_a_machine_and_exes_a_one_cost_witch_spell() {
    let mut initial = in_hand("BP12-041");
    push(&mut initial, "cemetery", cards("machine", "BP07-044", 5));
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"goblin","card":"BP01-171"},{"id":"machine","card":"BP17-077"},
        {"id":"spell","card":"BP03-053"}
    ]);
    let mut game = game(&initial);
    play(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["field"], json!(["machine"]));
    assert_eq!(state["P1"]["ex"], json!(["spell"]));
}

#[test]
fn bp12_048_second_choice_places_repair_mode() {
    let mut game = game(&in_hand("BP12-048"));
    play_with(&mut game, "source", &[2]);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(named(&state, &state["P1"]["ex"], "リペアモード").len(), 1);
}

#[test]
fn bp12_049_chooses_repair_mode_for_the_last_ex_slot_then_recovers_one_pp() {
    let mut initial = in_hand("BP12-049");
    push(
        &mut initial,
        "hand",
        vec![json!({"id":"fuel","card":"BP07-044"})],
    );
    initial["players"]["P1"]["zones"]["ex"] = json!(cards("repair", "BP07-T02", 4));
    let mut game = game(&initial);
    play(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["cemetery"], json!(["fuel"]));
    assert_eq!(state["P1"]["hand"], json!(["first"]));
    assert_eq!(named(&state, &state["P1"]["ex"], "リペアモード").len(), 5);
    assert_eq!(named(&state, &state["P1"]["ex"], "プロダクトマシーン").len(), 0);
    assert_eq!(state["P1"]["pp"]["current"], 9_u8);
}

#[test]
fn bp12_057_awakened_gives_other_ocean_followers_rush_and_assail() {
    let mut initial = in_hand("BP12-057");
    initial["players"]["P1"]["pp"] = json!({"current":7_u8,"max":7_u8});
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"whale","card":"BP04-069"}]);
    let mut game = game(&initial);
    play(&mut game);
    let state = game.projection(View::Referee).unwrap();
    let keywords = state["objects"]["whale"]["keywords"].as_array().unwrap();
    assert!(keywords.contains(&json!("突進")) && keywords.contains(&json!("指定攻撃")));
    assert_eq!(state["P1"]["leader"]["life"], 22_u8);
}

#[test]
fn bp12_059_acts_to_give_a_cheap_dragon_storm_and_one_one() {
    let mut initial = on_field("BP12-059");
    push(
        &mut initial,
        "field",
        vec![json!({"id":"ally","card":"BP12-067"})],
    );
    let mut game = game(&initial);
    let mut step = game
        .activate_internal_card("source", printed_activation("BP12-059", 0), actor())
        .unwrap();
    while let CardPlay::Awaiting(question) = step {
        let InputDomain::Targets(domain) = &question.domain else {
            panic!("only the target is asked")
        };
        let ally = domain
            .members
            .iter()
            .find(|member| member.card.as_deref() == Some("BP12-067"))
            .unwrap()
            .handle;
        step = game
            .answer_internal_card(question.actor, question.token, Answer::Targets(vec![ally]))
            .unwrap();
    }
    triggers_with(&mut game, &[1]);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["ally"]["power"], 3_u8);
    assert!(
        state["objects"]["ally"]["keywords"]
            .as_array()
            .unwrap()
            .contains(&json!("疾走"))
    );
    assert_eq!(state["objects"]["source"]["acted"], true);
}

#[test]
fn bp12_061_discards_two_to_draw_two_and_acts_with_five_nature_cards() {
    let mut initial = in_hand("BP12-061");
    push(&mut initial, "hand", cards("junk", "BP01-171", 2));
    push(&mut initial, "cemetery", cards("nature", "BP07-017", 5));
    let mut game = game(&initial);
    play(&mut game);
    let entered = game.projection(View::Referee).unwrap();
    assert_eq!(entered["P1"]["hand"], json!(["first", "second"]));
    activate(&mut game, "BP12-061", 0);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["cemetery"],
        json!(["enemy"])
    );
}

#[test]
fn bp12_067_awakened_searches_a_one_cost_dragon_follower() {
    let mut initial = in_hand("BP12-067");
    initial["players"]["P1"]["pp"] = json!({"current":7_u8,"max":7_u8});
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"goblin","card":"BP01-171"},{"id":"mermaid","card":"BP12-057"}
    ]);
    let mut game = game(&initial);
    play(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["hand"],
        json!(["mermaid"])
    );
}

#[test]
fn bp12_068_destroys_a_follower_and_searches_a_dragon_follower() {
    let mut initial = in_hand("BP12-068");
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"goblin","card":"BP01-171"},{"id":"dragon","card":"BP12-067"}
    ]);
    let mut game = game(&initial);
    play(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["cemetery"], json!(["enemy"]));
    assert_eq!(state["P1"]["hand"], json!(["dragon"]));
}

#[test]
fn bp12_094_evolution_fields_a_cheap_ward_follower() {
    let mut initial = on_field("BP12-093");
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP12-094"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"goblin","card":"BP01-171"},{"id":"warden","card":"BP11-097"}
    ]);
    let mut game = game(&initial);
    evolve(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["field"],
        json!(["source", "warden"])
    );
}

#[test]
fn bp12_099_places_repair_mode_and_draws_with_three() {
    let mut initial = in_hand("BP12-099");
    initial["players"]["P1"]["zones"]["ex"] = json!(cards("repair", "BP07-T02", 2));
    let mut game = game(&initial);
    play(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(named(&state, &state["P1"]["ex"], "リペアモード").len(), 3);
    assert_eq!(state["P1"]["hand"], json!(["first"]));
}
