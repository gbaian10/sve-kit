use super::*;
use crate::{expanded, rule_fixtures};
use std::{fs::read_to_string, path::PathBuf};
use sve_engine::catalog::EngineIdentityInput;
use sve_engine::game::internal::SearchAnswer;

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

fn evolve(game: &mut Game) {
    evolve_with(game, 1, 1);
}

fn evolve_with(game: &mut Game, cost: u8, mode: u64) {
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":cost}})
        )
        .unwrap()
        .outcome,
        "resolved",
        "the real base uses its printed evolution cost"
    );
    triggers_with(game, mode);
}

// Some printed bases are not admitted yet, so a same-name companion supplies only evolution.
pub(super) fn synthetic_base(form: &str, initial: &Value) -> Game {
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
        "ready-c-bp20",
    )
    .unwrap();
    game.load_internal_card("synthetic-evolution", &companion)
        .unwrap();
    game
}

#[test]
fn bp20_095_evolution_searches_another_absolute_zealot_card() {
    let mut initial = evolved_position("synthetic-evolution", "BP20-095");
    initial["players"]["P1"]["zones"]["deck"] =
        json!([{"id":"plain","card":"BP01-171"},{"id":"zealot","card":"BP05-093"}]);
    let mut game = synthetic_base("BP20-095", &initial);
    evolve(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["zealot"]));
}
