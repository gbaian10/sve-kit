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
    activate_on(game, "source", card);
}

fn activate_on(game: &mut Game, object: &str, card: &str) {
    let step = game
        .activate_internal_card(object, printed_activation(card), actor())
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

fn attack(game: &mut Game, attacker: &str, target: &str) {
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":attacker,"target":target}))
            .unwrap()
            .outcome,
        "resolved",
        "the attack is declared"
    );
    settle_combat(game);
}

// BP22-027 is not admitted yet, so a same-name companion supplies only evolution.
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
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../authored/rules/engine/index.yaml"),
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
        "ready-c-bp22",
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
fn bp22_001_activation_deals_damage_equal_to_cards_played() {
    let mut initial = on_field("BP22-001");
    initial["players"]["P1"]["zones"]["hand"] =
        json!([{"id":"goblin1","card":"BP01-171"},{"id":"goblin2","card":"BP01-171"}]);
    let mut game = game(&initial);
    play(&mut game, "goblin1");
    play(&mut game, "goblin2");
    activate(&mut game, "BP22-001");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["damage"], 2_u8);
}

#[test]
fn bp22_005_costs_one_less_beside_brilliant_fairy() {
    let mut initial = position("BP22-005");
    initial["players"]["P1"]["pp"] = json!({"current":0_u8,"max":10_u8});
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"fairy","card":"BP22-001"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["field"], json!(["fairy", "source"]));
}

#[test]
fn bp22_007_costs_less_per_ex_fairy_and_destroys() {
    let mut initial = position("BP22-007");
    initial["players"]["P1"]["pp"] = json!({"current":5_u8,"max":10_u8});
    initial["players"]["P1"]["zones"]["ex"] =
        json!([{"id":"fairy1","card":"BP22-T05"},{"id":"fairy2","card":"BP22-T05"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["pp"]["current"], 0_u8);
    assert_eq!(state["P2"]["cemetery"], json!(["enemy"]));
    // Draw two, then discard one of them.
    assert_eq!(state["P1"]["hand"].as_array().unwrap().len(), 1);
    assert_eq!(state["P1"]["cemetery"].as_array().unwrap().len(), 1);
}

#[test]
fn bp22_010_evolution_reveals_a_fairy_or_elf_card() {
    let mut initial = on_field("BP22-009");
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP22-010"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"first","card":"BP01-171"},
        {"id":"elf","card":"BP01-002"},
        {"id":"third","card":"BP01-171"},
        {"id":"fourth","card":"BP01-171"}
    ]);
    let mut game = game(&initial);
    evolve(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["elf"]));
}

#[test]
fn bp22_012_acts_to_give_another_crystalia_storm() {
    let mut initial = on_field("BP22-012");
    initial["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"lily","card":"BP02-008"}));
    initial["players"]["P1"]["zones"]["cemetery"] = graves("BP02-008", 3);
    let mut game = game(&initial);
    activate(&mut game, "BP22-012");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["source"]["acted"], true);
    assert!(
        state["objects"]["lily"]["keywords"]
            .as_array()
            .unwrap()
            .contains(&json!("疾走"))
    );
}

#[test]
fn bp22_016_banishes_two_ex_fairies_to_return_a_follower() {
    let mut game = game(&position("BP22-016"));
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        names(&state, &state["P1"]["ex"]),
        ["フェアリー", "フェアリー"]
    );
    activate(&mut game, "BP22-016");
    let after = game.projection(View::Referee).unwrap();
    // Banished tokens leave the game.
    assert_eq!(after["P1"]["ex"], json!([]));
    assert_eq!(after["P2"]["hand"], json!(["enemy"]));
}

#[test]
fn bp22_018_hits_a_follower_and_the_leader_beside_a_plant() {
    let mut initial = position("BP22-018");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"plant","card":"BP03-009"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["damage"], 3_u8);
    assert_eq!(state["P2"]["leader"]["life"], 19_u8);
    assert_eq!(state["P1"]["hand"], json!(["first"]));
}

#[test]
fn bp22_020_fourth_coin_pings_and_recovers_three_pp() {
    let mut initial = position("BP22-020");
    initial["players"]["P1"]["zones"]["ex"] = json!([
        {"id":"coin1","card":"BP22-T07"},
        {"id":"coin2","card":"BP22-T07"},
        {"id":"coin3","card":"BP22-T07"}
    ]);
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["ex"].as_array().unwrap().len(), 4);
    assert_eq!(state["objects"]["enemy"]["damage"], 1_u8);
    assert_eq!(state["P1"]["pp"]["current"], 9_u8);
}

#[test]
fn bp22_021_adds_a_coin_and_heals_two() {
    let mut game = game(&position("BP22-021"));
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(names(&state, &state["P1"]["ex"]), ["輝く金貨"]);
    assert_eq!(state["P1"]["leader"]["life"], 22_u8);
}

#[test]
fn bp22_028_evolution_brings_a_viking() {
    let mut initial = on_field("synthetic-evolution");
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP22-028"}]);
    let mut game = synthetic_base("BP22-028", &initial);
    evolve(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert!(names(&state, &state["P1"]["field"]).contains(&"ヴァイキング".to_owned()));
}

#[test]
fn bp22_033_adds_a_coin_on_entry_and_on_leaving() {
    let mut entry = game(&position("BP22-033"));
    play(&mut entry, "source");
    let entered = entry.projection(View::Referee).unwrap();
    assert_eq!(names(&entered, &entered["P1"]["ex"]), ["輝く金貨"]);
    // A sweep destroys it and its last word adds the coin.
    let mut initial = on_field("BP22-033");
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"storm","card":"BP01-089"}]);
    let mut leaving = game(&initial);
    play(&mut leaving, "storm");
    let state = leaving.projection(View::Referee).unwrap();
    assert!(
        state["P1"]["cemetery"]
            .as_array()
            .unwrap()
            .contains(&json!("source"))
    );
    assert_eq!(names(&state, &state["P1"]["ex"]), ["輝く金貨"]);
}

#[test]
fn bp22_034_storm_attacks_after_adding_a_coin() {
    let mut game = game(&position("BP22-034"));
    play(&mut game, "source");
    attack(&mut game, "source", "P2.leader");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(names(&state, &state["P1"]["ex"]), ["輝く金貨"]);
    assert_eq!(state["P2"]["leader"]["life"], 17_u8);
}

#[test]
fn bp22_045_ten_banished_cards_bring_two_artifacts() {
    let mut initial = on_field("BP22-045");
    initial["players"]["P1"]["zones"]["banish"] = graves("BP01-171", 10);
    let mut game = game(&initial);
    activate(&mut game, "BP22-045");
    let state = game.projection(View::Referee).unwrap();
    let mut field = names(&state, &state["P1"]["field"]);
    field.sort();
    assert_eq!(
        field,
        [
            "エンシェントアーティファクト",
            "レディアントアーティファクト",
            "界門のホムンクルス・ラズリ"
        ]
    );
    assert_eq!(state["P1"]["pp"]["current"], 8_u8);
}

#[test]
fn bp22_053_acts_followers_and_brings_a_golem_on_attack() {
    let mut initial = position("BP22-053");
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-037"},{"id":"goblin","card":"BP01-171"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["acted"], true);
    assert_eq!(state["objects"]["goblin"]["acted"], true);
    attack(&mut game, "source", "goblin");
    let after = game.projection(View::Referee).unwrap();
    assert!(names(&after, &after["P1"]["field"]).contains(&"攻撃型ゴーレム".to_owned()));
}
