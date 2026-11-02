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

// Some printed bases are not admitted yet, so a same-name companion supplies only evolution.
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
        "ready-c-bp18",
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

fn end_turn(game: &mut Game) {
    assert_eq!(
        game.internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap()
            .outcome,
        "resolved",
        "the controller ends its turn"
    );
    settle_combat(game);
}

#[test]
fn bp20_111_fanfare_discards_the_whole_hand() {
    let mut initial = position("BP20-111");
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP20-111"},{"id":"a","card":"BP01-171"},{"id":"b","card":"BP01-171"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!([]));
    assert_eq!(state["P1"]["cemetery"], json!(["a", "b"]));
}

#[test]
fn bp20_112_evolution_adds_the_crest_which_draws_at_end_with_a_small_hand() {
    let mut initial = evolved_position("BP20-111", "BP20-112");
    initial["players"]["P2"]["zones"]["deck"] = json!([{"id":"oppdraw","card":"BP01-171"}]);
    let mut game = game(&initial);
    evolve(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        names(&state, &state["P1"]["ex"]),
        ["クレスト 絶大の顕現・マゼルベイン"]
    );
    end_turn(&mut game);
    let after = game.projection(View::Referee).unwrap();
    // Empty hand: the follower destroys at end and the crest draws one.
    assert_eq!(after["P2"]["cemetery"], json!(["enemy"]));
    assert_eq!(after["P1"]["hand"], json!(["first"]));
}

#[test]
fn bp20_p70_crest_draws_at_end_with_at_most_one_card() {
    let mut initial = position("BP20-P70");
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"keep","card":"BP01-171"}]);
    initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"crest","card":"BP20-P70"}]);
    initial["players"]["P2"]["zones"]["deck"] = json!([{"id":"oppdraw","card":"BP01-171"}]);
    let mut game = game(&initial);
    end_turn(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["keep", "first"]));
}

#[test]
fn bp20_006_evolution_searches_another_spirit_card() {
    let mut initial = evolved_position("BP20-005", "BP20-006");
    initial["players"]["P1"]["zones"]["deck"] =
        json!([{"id":"plain","card":"BP01-171"},{"id":"spirit","card":"BP02-012"}]);
    let mut game = game(&initial);
    evolve(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["spirit"]));
}

#[test]
fn bp20_055_activation_sends_another_idol_to_the_cemetery() {
    let mut initial = on_field("BP20-055");
    initial["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"idol","card":"BP05-048"}));
    let mut game = game(&initial);
    let step = game
        .activate_internal_card("source", printed_activation("BP20-055"), actor())
        .unwrap();
    let CardPlay::Awaiting(question) = &step else {
        panic!("the resolving instruction must choose another idol")
    };
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("a public object choice is expected")
    };
    assert_eq!(domain.cardinality.minimum.0, 1);
    assert_eq!(domain.members.len(), 1);
    let paid = game.projection(View::Referee).unwrap();
    assert_eq!(paid["P1"]["cemetery"], json!(["source"]));
    finish(&mut game, step);
    triggers(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["field"], json!([]));
    assert_eq!(state["P1"]["cemetery"], json!(["source", "idol"]));
}

#[test]
fn bp20_055_activation_without_another_idol_only_sends_itself() {
    let mut game = game(&on_field("BP20-055"));
    assert!(activate(&mut game, "BP20-055"));
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["field"], json!([]));
    assert_eq!(state["P1"]["cemetery"], json!(["source"]));
}

#[test]
fn bp20_067_reveals_an_absolute_dragon_card() {
    let mut initial = position("BP20-067");
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"plain","card":"BP01-171"},
        {"id":"dragon","card":"BP05-058"},
        {"id":"rest","card":"BP01-171"}
    ]);
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["dragon"]));
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

#[test]
fn bp20_045_second_mode_brings_the_white_chapter() {
    let mut game = game(&position("BP20-045"));
    play_with(&mut game, "source", 2);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(names(&state, &state["P1"]["field"]), ["新約・白の章"]);
}

#[test]
fn bp20_104_pays_two_amulets_for_a_falcon_and_a_tiger() {
    let mut initial = on_field("BP20-104");
    initial["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"wish","card":"BP01-133"}));
    let mut game = game(&initial);
    assert!(activate(&mut game, "BP20-104"));
    let state = game.projection(View::Referee).unwrap();
    let mut field = names(&state, &state["P1"]["field"]);
    field.sort();
    assert_eq!(field, ["ホーリータイガー", "ホーリーファルコン"]);
    assert_eq!(state["P1"]["pp"]["current"], 9_u8);
}

#[test]
fn bp20_p62_white_chapter_last_word_brings_the_black_chapter() {
    let mut initial = on_field("BP20-P62");
    initial["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .extend([
            json!({"id":"idol1","card":"BP05-048"}),
            json!({"id":"idol2","card":"BP05-048"}),
        ]);
    let mut game = game(&initial);
    assert!(activate(&mut game, "BP20-P62"));
    let state = game.projection(View::Referee).unwrap();
    assert!(names(&state, &state["P1"]["field"]).contains(&"新約・黒の章".to_owned()));
    assert_eq!(state["P1"]["leader"]["life"], 21_u8);
}

#[test]
fn bp20_p63_black_chapter_last_word_brings_white_and_burns() {
    let mut initial = on_field("BP20-P63");
    initial["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .extend([
            json!({"id":"idol1","card":"BP05-048"}),
            json!({"id":"idol2","card":"BP05-048"}),
        ]);
    let mut game = game(&initial);
    assert!(activate(&mut game, "BP20-P63"));
    let state = game.projection(View::Referee).unwrap();
    assert!(names(&state, &state["P1"]["field"]).contains(&"新約・白の章".to_owned()));
    assert_eq!(state["P2"]["leader"]["life"], 19_u8);
}
