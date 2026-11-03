#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate these game scenarios."
)]
use super::event_combat_life::life;
use super::*;
use alloc::collections::BTreeSet;
use sve_engine::game::internal::{SearchAnswer, SearchDomain};

const fn actor() -> PlayerId {
    PlayerId { seat: 1 }
}

fn game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "unlock-batch-01",
    )
    .unwrap()
}

fn titled(mut initial: Value) -> Value {
    initial["players"]["P1"]["construction"] = json!("title");
    initial["players"]["P1"]["title_code"] = json!("princess_connect_re_dive");
    initial
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

fn evolving(base: &str, form: &str) -> Value {
    let mut initial = on_field(base);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
    initial
}

fn take(cardinality: &Cardinality, available: usize) -> usize {
    cardinality
        .maximum
        .map_or(available, |most| usize::try_from(most.0).unwrap())
        .min(available)
}

/// Takes the first legal cards, one per printed name, group by group.
fn search(domain: &SearchDomain) -> SearchAnswer {
    let mut names = BTreeSet::new();
    let mut pick = |handles: &[u64], cardinality: &Cardinality| -> Vec<u64> {
        let limit = take(cardinality, handles.len());
        handles
            .iter()
            .copied()
            .filter(|handle| {
                let name = &domain.cards[usize::try_from(*handle).unwrap()].name;
                names.insert(name.clone())
            })
            .take(limit)
            .collect()
    };
    match &domain.groups {
        None => SearchAnswer::Ungrouped(pick(
            &domain.cards.iter().map(|card| card.handle).collect::<Vec<_>>(),
            &domain.cardinality,
        )),
        Some(groups) => SearchAnswer::Grouped(
            groups
                .iter()
                .map(|group| pick(&group.handles, &group.cardinality))
                .collect(),
        ),
    }
}

fn finish_with(game: &mut Game, mut play: CardPlay, modes: &[u64]) -> CardPlay {
    while let CardPlay::Awaiting(question) = play {
        let answer = match &question.domain {
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            InputDomain::OptionalCost => Answer::OptionalCost(true),
            InputDomain::TokenCapacity { choices } => {
                Answer::TokenCapacity(u64::try_from(choices.len().checked_sub(1).unwrap()).unwrap())
            }
            InputDomain::Modes { .. } => Answer::Modes(modes.iter().copied().map(ModeId).collect()),
            InputDomain::Search(domain) => Answer::Search(search(domain)),
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
    play
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

fn definition(card: &str, kind: AbilityKind, nth: usize) -> AbilityId {
    selection_with_rules().current[card]
        .program
        .card()
        .abilities
        .iter()
        .filter(|ability| ability.node.kind == kind)
        .nth(nth)
        .unwrap()
        .node
        .id
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

fn activate(game: &mut Game, card: &str) {
    let step = game
        .activate_internal_card("source", definition(card, AbilityKind::Activated, 0), actor())
        .unwrap();
    finish_with(game, step, &[1]);
    triggers_with(game, &[1]);
}

fn named(state: &Value, zone: &Value, name: &str) -> usize {
    zone.as_array()
        .unwrap()
        .iter()
        .filter(|id| state["objects"][id.as_str().unwrap()]["name"] == name)
        .count()
}

#[test]
fn bp15_057_ability_damage_on_own_turn_puts_a_zetsuketsu_card_in_ex_three_cheaper() {
    let mut initial = on_field("BP15-057");
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"storm","card":"BP01-099"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"zetsu","card":"BP05-004"},{"id":"goblin","card":"BP01-171"}
    ]);
    let mut game = game(&initial);
    play_with(&mut game, "storm", &[1]);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["source"]["hp"], 2_u8, "the awakened spell deals 3");
    assert_eq!(state["P1"]["ex"], json!(["zetsu"]));
    assert!(state["P1"]["cemetery"].as_array().unwrap().contains(&json!("goblin")));
    assert_eq!(state["P1"]["pp"]["current"], 7_u8);
    play_with(&mut game, "zetsu", &[1]);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        7_u8,
        "the three-cost zetsuketsu follower costs zero"
    );
}

#[test]
fn cp04_039_ub_fanfare_finds_two_differently_named_nakayoshi_cards_two_cheaper() {
    let mut initial = titled(in_hand("CP04-039"));
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"chieru","card":"CP04-047"},{"id":"carnival","card":"CP04-053"},
        {"id":"twin","card":"CP04-047"},{"id":"goblin","card":"BP01-171"}
    ]);
    let mut game = game(&initial);
    play(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["ex"], json!(["carnival", "chieru"]), "one Chieru, not her twin");
    assert_eq!(state["P1"]["pp"]["current"], 5_u8);
    play_with(&mut game, "carnival", &[1]);
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["P1"]["pp"]["current"], 5_u8, "the two-cost spell costs zero");
    assert_eq!(after["objects"]["enemy"]["hp"], 2_u8);
}

#[test]
fn bp05_071_evolution_discards_a_random_opposing_hand_card() {
    let mut initial = evolving("BP05-070", "BP05-071");
    initial["players"]["P2"]["zones"]["hand"] = json!([
        {"id":"h1","card":"BP01-171"},{"id":"h2","card":"BP01-171"}
    ]);
    let mut game = game(&initial);
    evolve(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["hand"].as_array().unwrap().len(), 1);
    assert_eq!(state["P2"]["cemetery"].as_array().unwrap().len(), 1);
}

#[test]
fn cp04_057_ub_activation_deals_five_and_recovers_two_pp_while_awakened() {
    let mut initial = titled(on_field("CP04-057"));
    initial["players"]["P1"]["pp"] = json!({"current":3_u8,"max":7_u8});
    let mut game = game(&initial);
    activate(&mut game, "CP04-057");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["source"]["acted"], true);
    assert_eq!(state["P2"]["cemetery"], json!(["enemy"]), "five damage destroys the 5/5");
    assert_eq!(state["P1"]["pp"]["current"], 5_u8);
}

#[test]
fn bp05_085_necrocharge_ten_allows_both_modes() {
    let mut initial = in_hand("BP05-085");
    initial["players"]["P1"]["zones"]["cemetery"] = json!((0_u8..10_u8)
        .map(|n| json!({"id":format!("dead{n}"),"card":"BP01-171"}))
        .collect::<Vec<_>>());
    initial["players"]["P2"]["zones"]["hand"] = json!([
        {"id":"h1","card":"BP01-171"},{"id":"h2","card":"BP01-171"}
    ]);
    let mut game = game(&initial);
    play_with(&mut game, "source", &[1, 2]);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["hand"].as_array().unwrap().len(), 1);
    assert_eq!(state["objects"]["enemy"]["hp"], 2_u8);
}

#[test]
fn bp20_113_buffs_another_follower_and_places_kanro_when_sep_is_zero() {
    let mut initial = in_hand("BP20-113");
    initial["players"]["P2"]["zones"]["field"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"ally","card":"BP01-037"}]);
    let mut game = game(&initial);
    play(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["ally"]["power"], 8_u8);
    assert_eq!(state["objects"]["ally"]["hp"], 2_u8);
    assert_eq!(named(&state, &state["P1"]["ex"], "干絶の甘露"), 1);
}

#[test]
fn cp04_059_fanfare_deals_four_below_ten_max_pp_and_answers_another_ub_with_two_leader_damage() {
    let mut initial = titled(in_hand("CP04-059"));
    initial["players"]["P1"]["pp"] = json!({"current":9_u8,"max":9_u8});
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"homare","card":"CP04-057"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"CP04-060"}]);
    let mut game = game(&initial);
    play(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["hp"], 1_u8);
    assert_eq!(state["objects"]["source"]["evolved_with"], Value::Null, "nine max PP does not evolve");
    let step = game
        .activate_internal_card("homare", definition("CP04-057", AbilityKind::Activated, 0), actor())
        .unwrap();
    finish_with(&mut game, step, &[1]);
    triggers_with(&mut game, &[1]);
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["P2"]["leader"]["life"], 18_u8, "Kaya answers Homare's UB");
    assert_eq!(after["P2"]["cemetery"], json!(["enemy"]));
}

#[test]
fn bp16_076_pays_two_life_to_draw() {
    let mut game = game(&in_hand("BP16-076"));
    play(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["leader"]["life"], 18_u8);
    assert_eq!(state["P1"]["hand"], json!(["first"]));
}

#[test]
fn bp19_075_evolves_on_the_fourth_life_decrease_of_the_turn() {
    let mut initial = evolving("BP19-075", "BP19-076");
    initial["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"producer","card":"producer"}));
    let mut game = Game::from_selected_effects(
        producer_selection().clone(),
        &initial,
        &Value::Null,
        &Value::Null,
        "unlock-life",
    )
    .unwrap();
    let program = producer_steps(vec![life(PlayerRef::Controller {}, -1); 4]);
    game.load_internal_card("producer", &program).unwrap();
    super::activate(&mut game, "P1");
    triggers_with(&mut game, &[1]);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["source"]["evolved_with"], "form");
    assert_eq!(state["P1"]["leader"]["life"], 16_u8);
}

#[test]
fn bp14_100_evolution_deals_four() {
    let mut game = game(&evolving("BP14-099", "BP14-100"));
    evolve(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["enemy"]["hp"],
        1_u8
    );
}

#[test]
fn bp15_040_evolution_places_up_to_two_idol_cards_in_ex() {
    let mut initial = evolving("BP15-039", "BP15-040");
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"i1","card":"BP05-047"},{"id":"goblin","card":"BP01-171"},
        {"id":"i2","card":"BP05-048"},{"id":"other","card":"BP01-171"},{"id":"rest","card":"BP01-171"}
    ]);
    let mut game = game(&initial);
    evolve(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["ex"], json!(["i1", "i2"]));
    assert_eq!(state["P1"]["deck"][0], "rest");
}

#[test]
fn bp13_007_banishes_three_cemetery_cards_for_two_artifacts() {
    let mut initial = on_field("BP13-007");
    initial["players"]["P1"]["zones"]["cemetery"] = json!([
        {"id":"c1","card":"BP01-171"},{"id":"c2","card":"BP01-171"},{"id":"c3","card":"BP01-171"}
    ]);
    let mut game = game(&initial);
    activate(&mut game, "BP13-007");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["banish"].as_array().unwrap().len(), 3);
    assert_eq!(named(&state, &state["P1"]["field"], "エッジアーティファクト"), 1);
    assert_eq!(named(&state, &state["P1"]["field"], "ミスティックアーティファクト"), 1);
}

#[test]
fn bp13_007_q1726_last_field_slot_can_hold_the_mystic_artifact() {
    let mut initial = on_field("BP13-007");
    initial["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .extend((0_u8..3_u8).map(|n| json!({"id":format!("old{n}"),"card":"BP01-171"})));
    initial["players"]["P1"]["zones"]["cemetery"] = json!([
        {"id":"c1","card":"BP01-171"},{"id":"c2","card":"BP01-171"},{"id":"c3","card":"BP01-171"}
    ]);
    let mut game = game(&initial);
    activate(&mut game, "BP13-007");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(named(&state, &state["P1"]["field"], "ミスティックアーティファクト"), 1);
    assert_eq!(named(&state, &state["P1"]["field"], "エッジアーティファクト"), 0);
    assert_eq!(state["objects"]["source"]["acted"], true);
    assert_eq!(state["P1"]["banish"], json!(["c1", "c2", "c3"]));
    assert_eq!(state["P1"]["hand"], json!(["first"]));
}

#[test]
fn bp15_040_last_ex_slot_limits_the_idol_pick_and_keeps_the_remainder() {
    let mut initial = evolving("BP15-039", "BP15-040");
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"i1","card":"BP05-047"},{"id":"i2","card":"BP05-048"},
        {"id":"other","card":"BP01-171"},{"id":"rest","card":"BP01-171"},
        {"id":"unseen","card":"BP01-171"}
    ]);
    initial["players"]["P1"]["zones"]["ex"] = json!((0_u8..4_u8)
        .map(|n| json!({"id":format!("ex{n}"),"card":"BP01-171"}))
        .collect::<Vec<_>>());
    let mut game = game(&initial);
    evolve(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["ex"], json!(["ex0", "ex1", "ex2", "ex3", "i1"]));
    assert_eq!(state["P1"]["deck"], json!(["unseen", "i2", "other", "rest"]));
}

#[test]
fn bp15_040_full_ex_keeps_all_four_observed_cards_in_the_deck() {
    let mut initial = evolving("BP15-039", "BP15-040");
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"i1","card":"BP05-047"},{"id":"i2","card":"BP05-048"},
        {"id":"other","card":"BP01-171"},{"id":"rest","card":"BP01-171"},
        {"id":"unseen","card":"BP01-171"}
    ]);
    initial["players"]["P1"]["zones"]["ex"] = json!((0_u8..5_u8)
        .map(|n| json!({"id":format!("ex{n}"),"card":"BP01-171"}))
        .collect::<Vec<_>>());
    let mut game = game(&initial);
    evolve(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["ex"], json!(["ex0", "ex1", "ex2", "ex3", "ex4"]));
    assert_eq!(state["P1"]["deck"], json!(["unseen", "i1", "i2", "other", "rest"]));
}
