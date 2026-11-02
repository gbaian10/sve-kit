#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate the printed card scenarios."
)]
//! Posture, evolution, entry-turn, power and exclusion filters on printed cards.
use super::*;
use std::{fs::read_to_string, path::PathBuf};
use sve_engine::catalog::EngineIdentityInput;
use sve_engine::game::internal::SearchAnswer;

pub(super) struct Fixture {
    selection: SelectedEffects,
    programs: Vec<(String, WellFormedCardProgram)>,
}

// Evolution forms whose printed bases are not admitted get a same-name synthetic base.
const BASES: [&str; 2] = ["CP04-092", "PCS01-040"];

pub(super) fn fixture() -> &'static Fixture {
    static FIXTURE: OnceLock<Fixture> = OnceLock::new();
    FIXTURE.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        let rows: Vec<Value> = snapshot
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        let template = expanded::tree()["cards"]["synthetic-evolution"].clone();
        let mut tree = expanded::tree().clone();
        let mut programs = Vec::new();
        for number in BASES {
            let form = rows.iter().find(|row| row["number"] == number).unwrap();
            let name = format!("flt-base-{number}");
            add_card(
                &mut snapshot,
                &mut input,
                &name,
                &json!({"name":form["faces"][0]["name"],"card_type":"フォロワー","card_class":"ニュートラル","cost":"1","power":"1","hp":"1","traits":[],"text":null,"sections":[]}),
            );
            tree["cards"] = json!({name.clone():template.clone()});
            programs.push((
                name.clone(),
                lowerer().lower(&tree).unwrap().remove(&name).unwrap(),
            ));
        }
        for (name, kind, power, hp, traits) in [
            ("flt-enemy", "フォロワー", "1", "9", json!([])),
            ("flt-other", "フォロワー", "1", "9", json!([])),
            ("flt-amulet", "アミュレット", "-", "-", json!([])),
            ("flt-dragon4", "フォロワー", "4", "4", json!(["武闘竜人"])),
            ("flt-dragon3", "フォロワー", "3", "4", json!(["武闘竜人"])),
            ("flt-uma", "フォロワー", "1", "1", json!(["ウマ娘"])),
        ] {
            add_card(
                &mut snapshot,
                &mut input,
                name,
                &json!({"name":name,"card_type":kind,"card_class":"ニュートラル","cost":"1","power":power,"hp":hp,"traits":traits,"text":null,"sections":[]}),
            );
            programs.push((
                name.into(),
                WellFormedCardProgram::check(CardProgram {
                    abilities: vec![],
                    card_play_costs: vec![],
                })
                .unwrap(),
            ));
        }
        let mut rules: Value = serde_saphyr::from_str(
            &read_to_string(
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("../../authored/rules/engine/index.yaml"),
            )
            .unwrap(),
        )
        .unwrap();
        rules["input"]["snapshot_sha256"] = json!(rule_fixtures::hash(snapshot.as_bytes()));
        let selection = repository::select_with_rules(
            &snapshot,
            &keywords,
            &input,
            &rules.to_string(),
            &EngineIdentityInput::LegacyJp,
        )
        .unwrap();
        Fixture {
            selection,
            programs,
        }
    })
}

fn add_card(snapshot: &mut String, input: &mut Value, name: &str, face: &Value) {
    snapshot.push_str(&json!({"number":name,"faces":[face]}).to_string());
    snapshot.push('\n');
    input["documents"].as_array_mut().unwrap().push(json!([
        format!("{name}.yaml"),
        json!({"format":1_u8,"kind":"effect_set","cards":{name:{"status":"complete","abilities":[]}}}).to_string()
    ]));
}

pub(super) fn position(card: &str, zone: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"][zone] = json!([{"id":"source","card":card}]);
    for seat in ["P1", "P2"] {
        initial["players"][seat]["zones"]["deck"] = json!(
            (0_u8..3)
                .map(|n| json!({"id":format!("{seat}-draw{n}"),"card":"BP01-171"}))
                .collect::<Vec<_>>()
        );
    }
    initial
}

pub(super) fn princess(initial: &mut Value) {
    initial["players"]["P1"]["construction"] = json!("title");
    initial["players"]["P1"]["title_code"] = json!("princess_connect_re_dive");
}

pub(super) fn build(initial: &Value) -> Game {
    let prepared = fixture();
    let mut game = Game::from_selected_effects(
        prepared.selection.clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "filter-blocks",
    )
    .unwrap();
    for (card, program) in &prepared.programs {
        game.load_internal_card(card, program).unwrap();
    }
    game
}

/// Candidate card numbers offered by the current target question.
pub(super) fn offered(play: &CardPlay) -> Vec<String> {
    let CardPlay::Awaiting(question) = play else {
        panic!("expected a target question")
    };
    let (InputDomain::Targets(domain) | InputDomain::CostMaterials(domain)) = &question.domain
    else {
        panic!("expected targets: {:?}", question.domain)
    };
    domain
        .members
        .iter()
        .map(|member| member.card.clone().unwrap_or_default())
        .collect()
}

/// Answers each question with the first candidate whose card is in `prefer`, else the first.
pub(super) fn finish(game: &mut Game, mut play: CardPlay, prefer: &[&str]) -> CardPlay {
    while let CardPlay::Awaiting(question) = play {
        let answer = match &question.domain {
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            InputDomain::OptionalCost => Answer::OptionalCost(true),
            InputDomain::Modes { .. } => Answer::Modes(vec![ModeId(1)]),
            InputDomain::Targets(domain) | InputDomain::CostMaterials(domain) => {
                let count = usize::try_from(domain.cardinality.minimum.0.max(1)).unwrap();
                let mut members: Vec<_> = domain.members.iter().collect();
                members.sort_by_key(|member| {
                    !prefer.contains(&member.card.as_deref().unwrap_or_default())
                });
                let handles = members
                    .iter()
                    .take(count)
                    .map(|member| member.handle)
                    .collect();
                if matches!(question.domain, InputDomain::Targets(_)) {
                    Answer::Targets(handles)
                } else {
                    Answer::CostMaterials(handles)
                }
            }
            InputDomain::Search(domain) => {
                Answer::Search(SearchAnswer::Ungrouped(
                    domain.cards.first().map(|m| vec![m.handle]).unwrap_or_default(),
                ))
            }
            InputDomain::Order(cards) => Answer::Order(cards.iter().map(|m| m.handle).collect()),
            InputDomain::MealPayment { .. }
            | InputDomain::Quick { .. }
            | InputDomain::BeforeDraw
            | InputDomain::Pending(_)
            | InputDomain::TriggerQuota(_)
            | InputDomain::Integer(_)
            | InputDomain::DamageReplacements(_)
            | InputDomain::DriveTrigger { .. }
            | InputDomain::SearchPartition(_)
            | InputDomain::Distribution(_)
            | InputDomain::SearchReplacement
            | InputDomain::TokenCapacity { .. } => {
                panic!("unexpected printed question: {:?}", question.domain)
            }
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    play
}

pub(super) fn checks(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}

/// Opens the single pending printed ability of `card` and returns its first question.
pub(super) fn pending(game: &mut Game, card: &str) -> CardPlay {
    checks(game);
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("pending")
    };
    let option = options.iter().find(|option| option.card == card).unwrap();
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::Pending(option.handle),
    )
    .unwrap()
}

pub(super) fn evolve(game: &mut Game) {
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
        )
        .unwrap()
        .outcome,
        "resolved",
        "the base evolves into the printed form"
    );
}

fn state(game: &Game) -> Value {
    game.projection(View::Referee).unwrap()
}

#[test]
fn bp04_109_damages_only_acted_enemies_and_heals_with_an_amulet() {
    for card in ["BP04-109", "BP04-P28"] {
        for amulet in [false, true] {
            let mut initial = position(card, "hand");
            initial["players"]["P2"]["zones"]["field"] = json!([
                {"id":"acted","card":"flt-enemy","state":{"acted":true}},
                {"id":"standing","card":"flt-other"}
            ]);
            if amulet {
                initial["players"]["P1"]["zones"]["field"] =
                    json!([{"id":"charm","card":"flt-amulet"}]);
            }
            let mut game = build(&initial);
            let play = game
                .play_internal_card("source", PlayerId { seat: 1 })
                .unwrap();
            finish(&mut game, play, &[]);
            checks(&mut game);
            let after = state(&game);
            assert_eq!(after["objects"]["acted"]["damage"], 3_u8);
            assert_eq!(
                after["objects"]["standing"]["damage"], 0_u8,
                "a standing follower is not an acted follower"
            );
            assert_eq!(
                after["P1"]["leader"]["life"],
                if amulet { 22_u8 } else { 20_u8 }
            );
        }
    }
}

#[test]
fn bp07_022_has_intimidate_and_aura_only_while_standing() {
    for card in ["BP07-022", "BP21-PR05", "PR-182", "PR-325"] {
        for acted in [false, true] {
            let mut initial = position(card, "field");
            initial["players"]["P1"]["zones"]["field"][0]["state"] = json!({"acted":acted});
            let game = build(&initial);
            let keywords = state(&game)["objects"]["source"]["keywords"].clone();
            for keyword in ["威圧", "オーラ"] {
                assert_eq!(
                    keywords.as_array().unwrap().contains(&json!(keyword)),
                    !acted,
                    "{keyword} follows the standing posture"
                );
            }
        }
    }
}

#[test]
fn bp02_041_returns_only_an_unevolved_enemy() {
    for card in ["BP02-041", "BP02-P11"] {
        let mut initial = position(card, "hand");
        initial["players"]["P2"]["zones"]["field"] = json!([
            {"id":"evolved","card":"flt-other","state":{"evolved":true}},
            {"id":"base","card":"flt-enemy"}
        ]);
        let mut game = build(&initial);
        let play = game
            .play_internal_card("source", PlayerId { seat: 1 })
            .unwrap();
        assert_eq!(offered(&play), ["flt-enemy"], "an evolved follower is excluded");
        finish(&mut game, play, &[]);
        checks(&mut game);
        let after = state(&game);
        assert_eq!(after["P2"]["hand"], json!(["base"]));
        assert_eq!(after["P2"]["field"], json!(["evolved"]));
    }
}

#[test]
fn bp02_101_buffs_only_a_follower_that_entered_this_turn() {
    let mut initial = position("BP02-101", "hand");
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"fresh","card":"flt-enemy","state":{"entered_this_turn":true}},
        {"id":"old","card":"flt-other"}
    ]);
    let mut game = build(&initial);
    let play = game
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap();
    finish(&mut game, play, &[]);
    let fanfare = pending(&mut game, "BP02-101");
    assert_eq!(
        offered(&fanfare),
        ["flt-enemy"],
        "neither the old follower nor the source itself qualifies"
    );
    finish(&mut game, fanfare, &[]);
    checks(&mut game);
    let after = state(&game);
    assert_eq!(after["objects"]["fresh"]["power"], 3_u8);
    assert_eq!(after["objects"]["fresh"]["hp"], 11_u8);
    assert_eq!(after["objects"]["old"]["power"], 1_u8);
}

#[test]
fn turn_start_clears_entry_marks_on_both_fields() {
    let mut initial = position("flt-other", "field");
    initial["players"]["P1"]["zones"]["field"][0]["state"] = json!({"entered_this_turn":true});
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"flt-enemy","state":{"entered_this_turn":true}}]);
    let mut game = build(&initial);
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    while let Some(window) = game.internal_quick_choice().unwrap() {
        game.answer_internal_card(window.actor, window.token, Answer::Quick(None))
            .unwrap();
    }
    let after = state(&game);
    assert_eq!(after["turn"]["active"], "P2");
    for id in ["source", "enemy"] {
        assert_eq!(
            after["objects"][id]["entered_this_turn"],
            false,
            "{id} did not enter during the new turn"
        );
    }
}

#[test]
fn bp18_070_activates_only_with_a_four_power_martial_dragon() {
    for card in ["BP18-070", "BP18-P28"] {
        for (ally, usable) in [("flt-dragon4", true), ("flt-dragon3", false)] {
            let mut initial = position(card, "field");
            initial["players"]["P1"]["zones"]["field"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":"ally","card":ally}));
            let mut game = build(&initial);
            let ability = fixture().selection.current["BP18-070"]
                .program
                .card()
                .abilities
                .iter()
                .find(|a| a.node.kind == AbilityKind::Activated)
                .unwrap()
                .node
                .id;
            let result = game.activate_internal_card("source", ability, PlayerId { seat: 1 });
            if !usable {
                assert!(result.is_err(), "three power does not meet 攻撃力4以上");
                assert_eq!(state(&game)["P1"]["field"], json!(["source", "ally"]));
                continue;
            }
            finish(&mut game, result.unwrap(), &[]);
            checks(&mut game);
            let after = state(&game);
            assert_eq!(after["P1"]["leader"]["life"], 21_u8);
            assert_eq!(after["P1"]["cemetery"], json!(["source"]));
        }
    }
}

#[test]
fn cp01_055_lastword_cannot_return_its_own_name() {
    for card in ["CP01-055", "CP01-SL15", "CP01-SP15"] {
        let mut initial = position(card, "field");
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"removal","card":"BP01-116"}]);
        initial["players"]["P1"]["zones"]["cemetery"] = json!([
            {"id":"twin","card":"CP01-055"},
            {"id":"runner","card":"flt-uma"}
        ]);
        let mut game = build(&initial);
        let play = game
            .play_internal_card("removal", PlayerId { seat: 1 })
            .unwrap();
        finish(&mut game, play, &[card]);
        let lastword = pending(&mut game, card);
        assert_eq!(
            offered(&lastword),
            ["flt-uma"],
            "a same-name card is not これと同名でない"
        );
        finish(&mut game, lastword, &[]);
        checks(&mut game);
        let after = state(&game);
        assert!(after["P1"]["hand"].as_array().unwrap().contains(&json!("runner")));
        assert!(after["P1"]["cemetery"].as_array().unwrap().contains(&json!("twin")));
    }
}

#[test]
fn cp04_092_spares_the_chosen_target_from_the_sweep() {
    for card in ["CP04-092", "CP04-SL22", "CP04-SP11", "CP04-U11"] {
        let mut initial = position("flt-base-CP04-092", "field");
        princess(&mut initial);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":card}]);
        initial["players"]["P2"]["zones"]["field"] = json!([
            {"id":"chosen","card":"flt-enemy"},
            {"id":"bystander","card":"flt-other"}
        ]);
        let mut game = build(&initial);
        evolve(&mut game);
        let play = pending(&mut game, card);
        finish(&mut game, play, &["flt-enemy"]);
        checks(&mut game);
        let after = state(&game);
        assert_eq!(
            after["objects"]["chosen"]["damage"], 4_u8,
            "the chosen target takes 4 and no extra sweep damage"
        );
        assert_eq!(after["objects"]["bystander"]["damage"], 1_u8);
    }
}

#[test]
fn bp02_108_destroys_every_amulet_but_no_follower() {
    for card in ["BP02-108", "BP02-SL21", "BP02-U07"] {
        let mut initial = position("BP02-107", "field");
        initial["turn"]["elapsed_turns"] = json!({"P1":7_u8,"P2":7_u8});
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":card}]);
        initial["players"]["P1"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"own-charm","card":"flt-amulet"}));
        initial["players"]["P2"]["zones"]["field"] = json!([
            {"id":"enemy-charm","card":"flt-amulet"},
            {"id":"enemy","card":"flt-enemy"}
        ]);
        let mut game = build(&initial);
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":0_u8}}),
        )
        .unwrap();
        let play = pending(&mut game, card);
        finish(&mut game, play, &[]);
        checks(&mut game);
        let after = state(&game);
        assert_eq!(after["P1"]["cemetery"], json!(["own-charm"]));
        assert_eq!(after["P2"]["cemetery"], json!(["enemy-charm"]));
        assert_eq!(after["P2"]["field"], json!(["enemy"]));
    }
}
