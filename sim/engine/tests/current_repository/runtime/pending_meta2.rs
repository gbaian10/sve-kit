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

const GHOST: &str = "ゴースト";

#[test]
fn bp13_088_pays_two_life_to_destroy_a_cheap_follower() {
    let mut initial = in_hand("BP13-088");
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-171"}]);
    let mut game = game(&initial);
    play(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["leader"]["life"], 18_u8);
    assert_eq!(state["P2"]["cemetery"], json!(["enemy"]));
}

#[test]
fn bp13_089_evolution_hits_everything_for_three_and_heals_three() {
    let mut initial = on_field("BP13-088");
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP13-089"}]);
    let mut game = game(&initial);
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":5_u8}})
        )
        .unwrap()
        .outcome,
        "resolved"
    );
    triggers_with(&mut game, &[1]);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["leader"]["life"], 17_u8);
    assert_eq!(state["objects"]["enemy"]["damage"], 3_u8);
    assert_eq!(state["P1"]["leader"]["life"], 23_u8);
}

#[test]
fn bp13_014_evolution_places_two_fairies_and_heals_with_three_in_ex() {
    let mut initial = on_field("BP13-013");
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP13-014"}]);
    initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"fairy","card":"BP01-T03"}]);
    let mut game = game(&initial);
    evolve(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["ex"].as_array().unwrap().len(), 3);
    assert_eq!(state["P1"]["leader"]["life"], 22_u8);
}

#[test]
fn bp14_117_evolution_draws_one() {
    let mut initial = on_field("BP14-116");
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP14-117"}]);
    let mut game = game(&initial);
    evolve(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["hand"],
        json!(["first"])
    );
}

#[test]
fn bp13_012_places_a_fairy_then_sacrifices_itself_to_draw_two() {
    let mut initial = in_hand("BP13-012");
    initial["players"]["P1"]["zones"]["ex"] = json!(cards("fairy", "BP01-T03", 2));
    let mut game = game(&initial);
    play(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["ex"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    activate(&mut game, "BP13-012", 0);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["cemetery"], json!(["source"]));
    assert_eq!(state["P1"]["hand"], json!(["first", "second"]));
}

#[test]
fn bp15_034_places_cup_and_boots_mills_the_opponent_and_grows_on_treasure() {
    let mut initial = in_hand("BP15-034");
    initial["players"]["P2"]["zones"]["deck"] = json!([{"id":"theirs","card":"BP01-171"}]);
    let mut game = game(&initial);
    play(&mut game);
    let entered = game.projection(View::Referee).unwrap();
    assert_eq!(entered["P2"]["cemetery"], json!(["theirs"]));
    assert_eq!(named(&entered, &entered["P1"]["ex"], "黄金の杯").len(), 1);
    let boots = named(&entered, &entered["P1"]["ex"], "黄金の靴");
    assert_eq!(boots.len(), 1);
    play_with(&mut game, &boots[0], &[1]);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["source"]["power"], 4_u8);
    assert_eq!(state["objects"]["source"]["hp"], 4_u8);
}

#[test]
fn bp15_090_pays_life_for_intimidate_and_once_per_turn_for_power() {
    let mut game = game(&in_hand("BP15-090"));
    play(&mut game);
    let entered = game.projection(View::Referee).unwrap();
    assert!(has_keyword(&entered, "source", "威圧"));
    assert_eq!(entered["P1"]["leader"]["life"], 19_u8);
    activate(&mut game, "BP15-090", 0);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["source"]["power"], 2_u8);
    assert_eq!(state["P1"]["leader"]["life"], 18_u8);
    let before = game.projection(View::Referee).unwrap();
    game.activate_internal_card("source", printed_activation("BP15-090", 0), actor())
        .unwrap_err();
    assert_eq!(game.projection(View::Referee).unwrap(), before);
}

#[test]
fn bp10_082_summons_a_ghost() {
    let mut game = game(&in_hand("BP10-082"));
    play(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(named(&state, &state["P1"]["field"], GHOST).len(), 1);
}

#[test]
fn bp10_083_evolution_places_a_ghost_in_ex_and_mills_one() {
    let mut initial = on_field("BP10-082");
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP10-083"}]);
    let mut game = game(&initial);
    evolve(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(named(&state, &state["P1"]["ex"], GHOST).len(), 1);
    assert_eq!(state["P1"]["cemetery"], json!(["first"]));
}

#[test]
fn bp13_021_banishes_ten_and_hits_everything_for_five() {
    let mut initial = in_hand("BP13-021");
    initial["players"]["P1"]["zones"]["deck"] = json!(cards("top", "BP01-171", 11));
    let mut game = game(&initial);
    play(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["banish"].as_array().unwrap().len(), 10);
    assert_eq!(state["P2"]["leader"]["life"], 15_u8);
    assert_eq!(state["P2"]["cemetery"], json!(["enemy"]));
}
