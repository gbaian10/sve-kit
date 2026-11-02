#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate these game scenarios."
)]
use super::*;
use crate::{expanded, rule_fixtures};
use std::{fs::read_to_string, path::PathBuf};
use sve_engine::catalog::EngineIdentityInput;
use sve_engine::game::internal::SearchAnswer;

const fn actor() -> PlayerId {
    PlayerId { seat: 1 }
}

fn position(card: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"first","card":"BP01-171"},
        {"id":"second","card":"BP01-171"},
        {"id":"third","card":"BP01-171"},
        {"id":"fourth","card":"BP01-171"}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    initial
}

fn on_field(card: &str) -> Value {
    let mut initial = position(card);
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":card}]);
    initial
}

fn take(cardinality: &Cardinality, available: usize) -> usize {
    cardinality
        .maximum
        .map_or(available, |most| usize::try_from(most.0).unwrap())
        .min(available)
}

fn finish(game: &mut Game, mut play: CardPlay) {
    while let CardPlay::Awaiting(question) = play {
        let answer = match &question.domain {
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            InputDomain::OptionalCost => Answer::OptionalCost(true),
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
            .unwrap();
    }
}

fn triggers(game: &mut Game) {
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
        finish(game, step);
    }
}

fn play(game: &mut Game, id: &str) {
    let step = game.play_internal_card(id, actor()).unwrap();
    finish(game, step);
    triggers(game);
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
    triggers(game);
}

fn printed_activation(card: &str) -> AbilityId {
    selection().current[card]
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
    let step = game
        .activate_internal_card("source", printed_activation(card), actor())
        .unwrap();
    finish(game, step);
    triggers(game);
}

fn settle_combat(game: &mut Game) {
    loop {
        triggers(game);
        let Some(question) = game.internal_quick_choice().unwrap() else {
            break;
        };
        let step = game
            .answer_internal_card(question.actor, question.token, Answer::Quick(None))
            .unwrap();
        finish(game, step);
    }
}

fn attack_leader(game: &mut Game, attacker: &str) {
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"attack","attacker":attacker,"target":"P2.leader"})
        )
        .unwrap()
        .outcome,
        "resolved",
        "the attack is declared"
    );
    settle_combat(game);
}

// BP21-023 is not admitted yet, so a same-name companion supplies only evolution.
fn synthetic_base(form: &str, initial: &Value) -> Game {
    let (snapshot, keywords, mut input) = inputs().clone();
    let printed: Value = snapshot
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .find(|record| record["number"] == form)
        .unwrap();
    let companion_card = json!({"number":"synthetic-evolution","faces":[{
        "name":printed["faces"][0]["name"],"card_class":"ニュートラル","card_type":"フォロワー",
        "cost":"2","power":"2","hp":"2","traits":[],"text":null,"sections":[]
    }]});
    let snapshot = format!("{}\n{companion_card}", snapshot.trim_end());
    input["documents"].as_array_mut().unwrap().push(json!(["ready-c-companion.yaml",json!({
        "format":1_u8,"kind":"effect_set","cards":{"synthetic-evolution":{"status":"complete","abilities":[]}}
    }).to_string()]));
    let mut tree = expanded::tree().clone();
    tree["cards"] = json!({"synthetic-evolution":tree["cards"]["synthetic-evolution"]});
    let companion = lowerer()
        .lower(&tree)
        .unwrap()
        .remove("synthetic-evolution")
        .unwrap();
    let mut rules: Value = serde_saphyr::from_str(
        &read_to_string(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../authored/rules/engine/index.yaml"),
        )
        .unwrap(),
    )
    .unwrap();
    rules["input"] = rule_fixtures::rules(&snapshot, json!([]), json!([]))["input"].clone();
    let selected = repository::select_with_rules(
        &snapshot,
        &keywords,
        &input,
        &rules.to_string(),
        &EngineIdentityInput::LegacyJp,
    )
    .unwrap();
    let mut game = Game::from_selected_effects(
        selected,
        initial,
        &Value::Null,
        &Value::Null,
        "ready-c-bp21",
    )
    .unwrap();
    game.load_internal_card("synthetic-evolution", &companion)
        .unwrap();
    game
}

fn names(state: &Value, zone: &Value) -> Vec<String> {
    zone.as_array()
        .unwrap()
        .iter()
        .map(|id| {
            state["objects"][id.as_str().unwrap()]["name"]
                .as_str()
                .unwrap()
                .to_owned()
        })
        .collect()
}

fn graves(card: &str, count: u8) -> Value {
    json!(
        (0..count)
            .map(|i| json!({"id":format!("grave{i}"),"card":card}))
            .collect::<Vec<_>>()
    )
}

#[test]
fn bp21_002_discards_a_doll_to_draw_two() {
    let mut initial = position("BP21-002");
    initial["players"]["P1"]["zones"]["hand"] =
        json!([{"id":"source","card":"BP21-002"},{"id":"doll","card":"BP05-014"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["cemetery"], json!(["doll"]));
    assert_eq!(state["P1"]["hand"], json!(["first", "second"]));
}

#[test]
fn bp21_005_discards_to_reveal_an_academy_or_beast_card() {
    let mut initial = position("BP21-005");
    initial["players"]["P1"]["zones"]["hand"] =
        json!([{"id":"source","card":"BP21-005"},{"id":"junk","card":"BP01-171"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"first","card":"BP01-171"},
        {"id":"beast","card":"BP01-021"},
        {"id":"third","card":"BP01-171"},
        {"id":"fourth","card":"BP01-171"}
    ]);
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["cemetery"], json!(["junk"]));
    assert_eq!(state["P1"]["hand"], json!(["beast"]));
}

#[test]
fn bp21_015_attack_banishes_three_beasts_to_deal_four() {
    let mut initial = on_field("BP21-015");
    initial["players"]["P1"]["zones"]["cemetery"] = graves("BP01-021", 3);
    let mut game = game(&initial);
    attack_leader(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["banish"].as_array().unwrap().len(), 3);
    assert_eq!(state["objects"]["enemy"]["damage"], 4_u8);
    assert_eq!(state["P2"]["leader"]["life"], 16_u8);
}

#[test]
fn bp21_019_searches_a_one_cost_follower_and_has_bane_beside_academy() {
    let mut initial = position("BP21-019");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"academy","card":"BP02-046"}]);
    initial["players"]["P1"]["zones"]["deck"] =
        json!([{"id":"big","card":"BP01-037"},{"id":"maid","card":"BP01-034"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert!(state["P1"]["field"].as_array().unwrap().contains(&json!("maid")));
    assert!(state["objects"]["source"]["keywords"]
        .as_array()
        .unwrap()
        .contains(&json!("必殺")));
}

#[test]
fn bp21_024_attack_pays_two_to_stand_beside_another_academy_follower() {
    let mut initial = on_field("synthetic-evolution");
    initial["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"academy","card":"BP02-046"}));
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP21-024"}]);
    let mut game = synthetic_base("BP21-024", &initial);
    evolve(&mut game);
    attack_leader(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["source"]["acted"], false);
    assert_eq!(state["P1"]["pp"]["current"], 7_u8);
    assert_eq!(state["P2"]["leader"]["life"], 17_u8);
}

#[test]
fn bp21_027_evolves_itself_when_searched_onto_the_field() {
    let mut initial = position("BP21-019");
    initial["players"]["P1"]["zones"]["deck"] =
        json!([{"id":"tony","card":"BP21-027"},{"id":"big","card":"BP01-037"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP21-028"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["evolve_deck"], json!([]));
    // The evolved Tony draws the remaining deck card.
    assert_eq!(state["P1"]["hand"], json!(["big"]));
    assert_eq!(
        names(&state, &state["P1"]["field"]),
        ["学び舎の黄昏・リーシャ＆ナノ", "愚直なる研磨・トニー"]
    );
}

#[test]
fn bp21_029_searched_onto_the_field_destroys_and_pings_the_leader() {
    let mut initial = position("BP21-020");
    initial["players"]["P1"]["zones"]["deck"] =
        json!([{"id":"trainee","card":"BP21-029"},{"id":"first","card":"BP01-171"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert!(state["P1"]["field"].as_array().unwrap().contains(&json!("trainee")));
    assert_eq!(state["P2"]["cemetery"], json!(["enemy"]));
    assert_eq!(state["P2"]["leader"]["life"], 19_u8);
}

#[test]
fn bp21_036_returns_two_soldiers_from_the_cemetery() {
    let mut initial = position("BP21-036");
    initial["players"]["P1"]["zones"]["cemetery"] = json!([
        {"id":"lancer1","card":"BP01-046"},
        {"id":"goblin","card":"BP01-171"},
        {"id":"lancer2","card":"BP01-046"}
    ]);
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["lancer1", "lancer2"]));
}

#[test]
fn bp21_038_discards_an_academy_card_to_draw() {
    let mut initial = position("BP21-038");
    initial["players"]["P1"]["zones"]["hand"] =
        json!([{"id":"source","card":"BP21-038"},{"id":"academy","card":"BP02-046"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["cemetery"], json!(["academy"]));
    assert_eq!(state["P1"]["hand"], json!(["first"]));
}

#[test]
fn bp21_047_activation_with_five_banished_deals_four_and_draws() {
    let mut initial = on_field("BP21-047");
    initial["players"]["P1"]["zones"]["banish"] = graves("BP01-171", 5);
    let mut game = game(&initial);
    activate(&mut game, "BP21-047");
    let state = game.projection(View::Referee).unwrap();
    assert!(state["P1"]["banish"].as_array().unwrap().contains(&json!("source")));
    assert_eq!(state["objects"]["enemy"]["damage"], 4_u8);
    assert_eq!(state["P1"]["hand"], json!(["first"]));
}

#[test]
fn bp21_053_spellchain_ten_costs_two_less() {
    let mut initial = position("BP21-053");
    initial["players"]["P1"]["pp"] = json!({"current":1_u8,"max":10_u8});
    initial["players"]["P1"]["zones"]["cemetery"] = graves("BP01-064", 10);
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["pp"]["current"], 0_u8);
    assert_eq!(state["objects"]["enemy"]["damage"], 4_u8);
}
