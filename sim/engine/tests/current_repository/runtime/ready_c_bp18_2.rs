#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate these game scenarios."
)]
use super::*;
use crate::{expanded, rule_fixtures};
use alloc::collections::BTreeMap;
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

fn evolved_position(base: &str, form: &str) -> Value {
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

fn finish_with(game: &mut Game, mut play: CardPlay, mode: u64) {
    while let CardPlay::Awaiting(question) = play {
        let answer = match &question.domain {
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            InputDomain::OptionalCost => Answer::OptionalCost(true),
            InputDomain::Modes { .. } => Answer::Modes(vec![ModeId(mode)]),
            InputDomain::Search(domain) => Answer::Search(domain.groups.as_ref().map_or_else(
                || {
                    SearchAnswer::Ungrouped(
                        domain
                            .cards
                            .iter()
                            .take(take(&domain.cardinality, domain.cards.len()))
                            .map(|card| card.handle)
                            .collect(),
                    )
                },
                |groups| {
                    // Each printed group greedily takes cards no earlier group used.
                    let mut used = Vec::new();
                    SearchAnswer::Grouped(
                        groups
                            .iter()
                            .map(|group| {
                                let picked: Vec<u64> = group
                                    .handles
                                    .iter()
                                    .copied()
                                    .filter(|handle| !used.contains(handle))
                                    .take(take(&group.cardinality, group.handles.len()))
                                    .collect();
                                used.extend(picked.iter().copied());
                                picked
                            })
                            .collect(),
                    )
                },
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
            InputDomain::Integer(bounds) => {
                Answer::Integer(bounds.maximum.unwrap_or(bounds.minimum))
            }
            other => panic!("unexpected decision: {other:?}"),
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
}


fn triggers_with(game: &mut Game, mode: u64) {
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
        finish_with(game, step, mode);
    }
}

fn triggers(game: &mut Game) {
    triggers_with(game, 1);
}

fn play_with(game: &mut Game, id: &str, mode: u64) {
    let step = game.play_internal_card(id, actor()).unwrap();
    finish_with(game, step, mode);
    triggers_with(game, mode);
}

fn play(game: &mut Game, id: &str) {
    play_with(game, id, 1);
}

fn evolve(game: &mut Game) {
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
        )
        .unwrap()
        .outcome,
        "resolved",
        "the real base uses its printed evolution cost"
    );
    triggers(game);
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

struct SyntheticEvolution {
    selection: SelectedEffects,
    programs: BTreeMap<String, WellFormedCardProgram>,
}

// Some printed bases are not admitted yet, so same-name companions supply only evolution.
fn evolution_fixtures() -> &'static SyntheticEvolution {
    static PREPARED: OnceLock<SyntheticEvolution> = OnceLock::new();
    PREPARED.get_or_init(|| {
        let (snapshot, keywords, mut input) = inputs().clone();
        let rows: Vec<Value> = snapshot
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        let mut snapshot = format!("{}\n", snapshot.trim_end());
        let mut tree = expanded::tree().clone();
        let template = tree["cards"]["synthetic-evolution"].clone();
        tree["cards"] = json!({});
        for form in ["BP18-015", "BP18-087", "BP18-091"] {
            let printed = rows.iter().find(|record| record["number"] == form).unwrap();
            let number = format!("synthetic-evolution-{form}");
            let companion = json!({"number":number,"faces":[{
                "name":printed["faces"][0]["name"],"card_class":"ニュートラル","card_type":"フォロワー",
                "cost":"2","power":"2","hp":"2","traits":[],"text":null,"sections":[]
            }]});
            snapshot.push_str(&companion.to_string());
            snapshot.push('\n');
            input["documents"].as_array_mut().unwrap().push(json!([format!("{number}.yaml"),json!({
                "format":1_u8,"kind":"effect_set","cards":{number.clone():{"status":"complete","abilities":[]}}
            }).to_string()]));
            tree["cards"][&number] = template.clone();
        }
        let programs = lowerer().lower(&tree).unwrap();
        let mut rules: Value = serde_saphyr::from_str(
            &read_to_string(
                PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../authored/rules/engine/index.yaml"),
            )
            .unwrap(),
        )
        .unwrap();
        rules["input"] = rule_fixtures::rules(&snapshot, json!([]), json!([]))["input"].clone();
        let selection = repository::select_with_rules(
            &snapshot,
            &keywords,
            &input,
            &rules.to_string(),
            &EngineIdentityInput::LegacyJp,
        )
        .unwrap();
        SyntheticEvolution { selection, programs }
    })
}

fn synthetic_base(initial: &Value) -> Game {
    let prepared = evolution_fixtures();
    let mut game = Game::from_selected_effects(
        prepared.selection.clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "ready-c-bp18",
    )
    .unwrap();
    for (number, program) in &prepared.programs {
        game.load_internal_card(number, program).unwrap();
    }
    game
}

#[test]
fn bp18_101_paid_fanfare_returns_another_tokyo_follower() {
    let mut initial = position("BP18-101");
    initial["players"]["P1"]["zones"]["cemetery"] =
        json!([{"id":"same","card":"BP18-101"},{"id":"clerk","card":"BP18-126"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["field"], json!(["source", "clerk"]));
    assert_eq!(state["P1"]["cemetery"], json!(["same"]));
    assert_eq!(state["P1"]["pp"]["current"], 5_u8);
}

#[test]
fn bp18_102_evolution_heals_two() {
    let mut game = game(&evolved_position("BP18-101", "BP18-102"));
    evolve(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["leader"]["life"], 22_u8);
}

#[test]
fn bp18_045_ten_banished_cards_give_assail_and_stats() {
    let mut initial = position("BP18-045");
    initial["players"]["P1"]["zones"]["banish"] = json!(
        (0..8_u8)
            .map(|i| json!({"id":format!("gone{i}"),"card":"BP01-171"}))
            .collect::<Vec<_>>()
    );
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["banish"].as_array().unwrap().len(), 10);
    assert_eq!(state["objects"]["source"]["power"], 3_u8);
    assert_eq!(state["objects"]["source"]["hp"], 5_u8);
    assert!(
        state["objects"]["source"]["keywords"]
            .as_array()
            .unwrap()
            .contains(&json!("指定攻撃"))
    );
}

#[test]
fn bp18_114_gains_one_hp_beside_another_ward_follower() {
    let mut initial = position("BP18-114");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"lancer","card":"BP01-046"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["source"]["hp"], 3_u8);
}

#[test]
fn bp18_p53_destroys_an_enemy_and_gives_a_tokyo_follower_storm() {
    let mut initial = position("BP18-P53");
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"source","card":"BP18-P53"}]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"clerk","card":"BP18-126"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["cemetery"], json!(["enemy"]));
    assert!(
        state["objects"]["clerk"]["keywords"]
            .as_array()
            .unwrap()
            .contains(&json!("疾走"))
    );
}

#[test]
fn bp18_059_evolution_puts_a_martial_dragonewt_in_ex_three_cheaper() {
    let mut initial = evolved_position("BP18-058", "BP18-059");
    initial["players"]["P1"]["pp"] = json!({"current":1_u8,"max":10_u8});
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"plain","card":"BP01-171"},
        {"id":"lancer","card":"BP09-065"},
        {"id":"other","card":"BP01-171"},
        {"id":"rest","card":"BP01-171"}
    ]);
    let mut game = game(&initial);
    evolve(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["ex"], json!(["lancer"]));
    assert_eq!(state["P1"]["deck"].as_array().unwrap()[0], "rest");
    play(&mut game, "lancer");
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["P1"]["field"], json!(["source", "lancer"]));
}

#[test]
fn bp18_091_evolution_destroys_an_enemy_follower() {
    let mut initial = evolved_position("synthetic-evolution-BP18-091", "BP18-091");
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    let mut game = synthetic_base(&initial);
    evolve(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["cemetery"], json!(["enemy"]));
}

#[test]
fn bp18_094_fanfare_returns_a_one_cost_follower() {
    let mut initial = position("BP18-094");
    initial["players"]["P1"]["zones"]["cemetery"] =
        json!([{"id":"large","card":"BP01-037"},{"id":"skeleton","card":"BP01-117"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["field"], json!(["source", "skeleton"]));
    assert_eq!(state["P1"]["cemetery"], json!(["large"]));
}

#[test]
fn bp18_085_returns_a_two_cost_tokyo_follower() {
    let mut initial = position("BP18-085");
    initial["players"]["P1"]["zones"]["cemetery"] =
        json!([{"id":"clerk","card":"BP18-126"},{"id":"punk","card":"BP18-124"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["field"], json!(["punk"]));
    assert!(state["P1"]["cemetery"].as_array().unwrap().contains(&json!("clerk")));
}

#[test]
fn bp18_p54_returns_differently_named_cheap_tokyo_cards() {
    let mut initial = position("BP18-P54");
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"source","card":"BP18-P54"}]);
    initial["players"]["P1"]["zones"]["cemetery"] =
        json!([{"id":"clerk","card":"BP18-126"},{"id":"claw","card":"BP18-071"},{"id":"large","card":"BP01-037"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["clerk", "claw"]));
}

#[test]
fn bp18_087_evolution_returns_a_demon_follower_to_hand() {
    let mut initial = evolved_position("synthetic-evolution-BP18-087", "BP18-087");
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["cemetery"] =
        json!([{"id":"plain","card":"BP01-171"},{"id":"demon","card":"BP01-109"}]);
    let mut game = synthetic_base(&initial);
    evolve(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["demon"]));
}

#[test]
fn bp18_049_fanfare_draws_then_mills_one() {
    let mut game = game(&position("BP18-049"));
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["first"]));
    assert_eq!(state["P1"]["cemetery"], json!(["second"]));
}

#[test]
fn bp18_073_only_searches_while_the_damaged_generation_remains_on_field() {
    let mut initial = position("BP01-179");
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"source","card":"BP01-179"},
        {"id":"again","card":"BP01-179"}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP18-073"}]);
    initial["players"]["P2"]["zones"]["deck"] = json!([
        {"id":"first_snake","card":"BP18-073"},
        {"id":"second_snake","card":"BP18-073"}
    ]);
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["damage"], 2_u8);
    assert_eq!(state["P2"]["field"].as_array().unwrap().len(), 2);
    assert_eq!(state["P2"]["deck"].as_array().unwrap().len(), 1);
    let summoned = state["P2"]["field"]
        .as_array()
        .unwrap()
        .iter()
        .find(|id| *id != "enemy")
        .unwrap()
        .as_str()
        .unwrap();
    assert_eq!(state["objects"][summoned]["power"], 6_u8);
    assert_eq!(state["objects"][summoned]["hp"], 5_u8);
    play(&mut game, "again");
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["P2"]["cemetery"], json!(["enemy"]));
    assert_eq!(after["P2"]["field"].as_array().unwrap().len(), 1);
    assert_eq!(after["P2"]["deck"].as_array().unwrap().len(), 1);
}

#[test]
fn bp18_100_searches_eira_prayer() {
    let mut initial = position("BP18-100");
    initial["players"]["P1"]["zones"]["deck"] =
        json!([{"id":"plain","card":"BP01-171"},{"id":"prayer","card":"BP02-095"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["prayer"]));
}

#[test]
fn bp18_120_reveals_another_tokyo_card() {
    let mut initial = position("BP18-120");
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"same","card":"BP18-120"},
        {"id":"clerk","card":"BP18-126"},
        {"id":"plain","card":"BP01-171"},
        {"id":"rest","card":"BP01-171"}
    ]);
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["clerk"]));
    assert_eq!(state["P1"]["deck"].as_array().unwrap()[0], "rest");
}

#[test]
fn bp18_038_searches_the_named_luminous_followers() {
    let mut initial = position("BP18-038");
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"plain","card":"BP01-171"},
        {"id":"knight","card":"BP16-027"},
        {"id":"mage","card":"BP16-029"}
    ]);
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(names(&state, &state["P1"]["hand"]), ["統率のルミナスナイト", "卓越のルミナスメイジ"]);
    assert_eq!(state["P1"]["deck"], json!(["plain"]));
}


#[test]
fn bp18_015_evolution_returns_another_tokyo_follower_to_hand() {
    let mut initial = evolved_position("synthetic-evolution-BP18-015", "BP18-015");
    initial["players"]["P1"]["zones"]["cemetery"] = json!([
        {"id":"same","card":"BP18-015"},
        {"id":"plain","card":"BP01-171"},
        {"id":"clerk","card":"BP18-126"}
    ]);
    let mut game = synthetic_base(&initial);
    evolve(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["clerk"]));
}
