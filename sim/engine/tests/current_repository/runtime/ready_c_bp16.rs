#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate these game scenarios."
)]
use super::*;
use crate::{expanded, rule_fixtures};
use std::{fs::read_to_string, path::PathBuf};
use sve_engine::catalog::EngineIdentityInput;
use sve_engine::game::internal::{DamageShare, SearchAnswer};

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
            InputDomain::Distribution(domain) => {
                let mut shares: Vec<DamageShare> = domain
                    .members
                    .iter()
                    .map(|member| DamageShare {
                        handle: member.handle,
                        amount: Nat(0),
                    })
                    .collect();
                // The first target takes all but one point, the last target the remainder.
                let last = shares.len().checked_sub(1).unwrap();
                shares[last].amount = Nat(1);
                shares[0].amount = Nat(domain.total.0.checked_sub(u64::try_from(last).unwrap()).unwrap());
                for share in shares.iter_mut().take(last).skip(1) {
                    share.amount = Nat(1);
                }
                Answer::Distribution(shares)
            }
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

fn finish(game: &mut Game, play: CardPlay) {
    finish_with(game, play, 1);
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

fn activate(game: &mut Game, card: &str) -> bool {
    let Ok(step) = game.activate_internal_card("source", printed_activation(card), actor()) else {
        return false;
    };
    finish(game, step);
    triggers(game);
    true
}



// The printed base remains a prototype, so a same-name companion supplies only evolution.
fn arakavi_base() -> &'static (SelectedEffects, WellFormedCardProgram) {
    static PREPARED: OnceLock<(SelectedEffects, WellFormedCardProgram)> = OnceLock::new();
    PREPARED.get_or_init(|| {
        let (snapshot, keywords, mut input) = inputs().clone();
        let form: Value = snapshot
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).unwrap())
            .find(|record| record["number"] == "BP16-077")
            .unwrap();
        let companion_card = json!({"number":"synthetic-evolution","faces":[{
            "name":form["faces"][0]["name"],"card_class":"ナイトメア","card_type":"フォロワー",
            "cost":"5","power":"4","hp":"4","traits":[],"text":null,"sections":[]
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
        (selected, companion)
    })
}

#[test]
fn bp16_077_low_life_evolution_splits_ten_damage() {
    let mut initial = evolved_position("synthetic-evolution", "BP16-077");
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-037"},{"id":"small","card":"BP01-171"}]);
    let (selected, base) = arakavi_base();
    let mut game = Game::from_selected_effects(
        selected.clone(),
        &initial,
        &Value::Null,
        &Value::Null,
        "ready-c-bp16-077",
    )
    .unwrap();
    game.load_internal_card("synthetic-evolution", base).unwrap();
    evolve(&mut game);
    let state = game.projection(View::Referee).unwrap();
    // Nine of the ten points destroy the 5/5; the other point lands on the 2/2.
    assert_eq!(state["P2"]["cemetery"], json!(["enemy"]));
    assert_eq!(state["objects"]["small"]["damage"], 1_u8);
}

#[test]
fn bp16_041_evolution_deals_twice_the_revelry_follower_count() {
    let mut initial = evolved_position("BP16-040", "BP16-041");
    initial["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"revel","card":"BP14-012"}));
    let mut game = game(&initial);
    evolve(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["damage"], 4_u8);
}

#[test]
fn bp16_080_evolution_revives_a_cheap_dead_follower_and_heals() {
    let mut initial = evolved_position("BP16-079", "BP16-080");
    initial["players"]["P1"]["zones"]["cemetery"] =
        json!([{"id":"skeleton","card":"BP01-117"},{"id":"large","card":"BP01-037"}]);
    let mut game = game(&initial);
    evolve(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["field"], json!(["source", "skeleton"]));
    assert_eq!(state["P1"]["cemetery"], json!(["large"]));
    assert_eq!(state["P1"]["leader"]["life"], 21_u8);
}

#[test]
fn bp16_104_searches_a_leading_amulet_and_needs_a_prayer_counter() {
    let mut initial = position("BP16-104");
    initial["players"]["P1"]["zones"]["deck"] =
        json!([{"id":"wrong","card":"BP01-133"},{"id":"lead","card":"BP10-098"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["lead"]));
    assert!(!activate(&mut game, "BP16-104"), "no prayer counter");
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["field"],
        json!(["source"])
    );
}

#[test]
fn bp16_010_evolution_puts_a_cheap_crystalia_in_ex_two_cheaper() {
    let mut initial = evolved_position("BP16-009", "BP16-010");
    initial["players"]["P1"]["pp"] = json!({"current":1_u8,"max":10_u8});
    initial["players"]["P1"]["zones"]["deck"] =
        json!([{"id":"wrong","card":"BP16-009"},{"id":"crystal","card":"BP16-007"}]);
    let mut game = game(&initial);
    evolve(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["ex"], json!(["crystal"]));
    assert_eq!(state["P1"]["pp"]["current"], 0_u8);
    play(&mut game, "crystal");
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["P1"]["field"], json!(["source", "crystal"]));
    assert_eq!(after["objects"]["enemy"]["power"], 1_u8);
}

#[test]
fn bp16_004_field_and_ex_revelry_cards_summon_another_cheap_one() {
    let mut initial = position("BP16-004");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"revel","card":"BP14-012"}]);
    initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"exrevel","card":"BP14-012"}]);
    initial["players"]["P1"]["zones"]["deck"] =
        json!([{"id":"same","card":"BP16-004"},{"id":"hostess","card":"BP14-001"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["field"], json!(["revel", "source", "hostess"]));
    assert_eq!(state["P1"]["deck"], json!(["same"]));
}

#[test]
fn bp16_008_activation_moves_a_doll_to_ex_three_cheaper() {
    let mut initial = position("BP16-008");
    initial["players"]["P1"]["pp"] = json!({"current":4_u8,"max":10_u8});
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"first","card":"BP01-171"},
        {"id":"doll","card":"BP05-016"},
        {"id":"third","card":"BP01-171"},
        {"id":"fourth","card":"BP01-171"},
        {"id":"fifth","card":"BP01-171"},
        {"id":"sixth","card":"BP01-171"}
    ]);
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["field"].as_array().unwrap().len(), 3);
    assert!(activate(&mut game, "BP16-008"));
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["P1"]["ex"], json!(["doll"]));
    assert_eq!(after["P1"]["deck"].as_array().unwrap()[0], "sixth");
    assert_eq!(after["P1"]["pp"]["current"], 0_u8);
    play(&mut game, "doll");
    let later = game.projection(View::Referee).unwrap();
    assert!(later["P1"]["field"].as_array().unwrap().contains(&json!("doll")));
}

#[test]
fn bp16_037_discards_two_academy_cards_to_damage_and_draw() {
    let mut initial = position("BP16-037");
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"source","card":"BP16-037"},
        {"id":"plain","card":"BP01-171"},
        {"id":"lesson1","card":"BP16-045"},
        {"id":"lesson2","card":"BP16-045"}
    ]);
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["cemetery"], json!(["enemy"]));
    assert_eq!(state["P1"]["cemetery"], json!(["lesson1", "lesson2"]));
    assert_eq!(state["P1"]["hand"], json!(["plain", "first", "second"]));
}

#[test]
fn bp16_075_necrocharge_hounds_get_storm_and_one_is_paid_for_damage() {
    let mut initial = position("BP16-075");
    initial["players"]["P1"]["zones"]["cemetery"] = json!(
        (0..10_u8)
            .map(|i| json!({"id":format!("grave{i}"),"card":"BP01-171"}))
            .collect::<Vec<_>>()
    );
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    let field = state["P1"]["field"].as_array().unwrap();
    assert_eq!(field.len(), 3);
    for hound in field.iter().filter(|id| *id != "source") {
        assert!(
            state["objects"][hound.as_str().unwrap()]["keywords"]
                .as_array()
                .unwrap()
                .contains(&json!("疾走"))
        );
    }
    assert!(activate(&mut game, "BP16-075"));
    let after = game.projection(View::Referee).unwrap();
    // The first cost material is Mimi, whose last word adds its own two damage.
    assert_eq!(after["objects"]["enemy"]["damage"], 4_u8);
    assert_eq!(after["P1"]["field"].as_array().unwrap().len(), 2);
}
