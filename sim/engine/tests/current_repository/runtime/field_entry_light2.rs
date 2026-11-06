#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate printed hand-entry scenarios."
)]
use super::*;
use alloc::collections::BTreeMap;
use sve_engine::game::internal::SearchAnswer;

type Case = (
    &'static str,
    &'static str,
    &'static str,
    u64,
    &'static str,
    &'static str,
);
const CASES: &[Case] = &[
    ("BP07-054", "play", "フォロワー", 5_u64, "ドラゴン", "海洋"),
    (
        "BP14-003",
        "activate",
        "フォロワー",
        3_u64,
        "ニュートラル",
        "狩人",
    ),
    (
        "BP19-096",
        "last",
        "アミュレット",
        1_u64,
        "ニュートラル",
        "",
    ),
    (
        "BP19-097",
        "evolve_last",
        "アミュレット",
        1_u64,
        "ニュートラル",
        "",
    ),
    ("BP03-005", "bounce", "フォロワー", 2_u64, "エルフ", ""),
    ("CP04-025", "play", "フォロワー", 2_u64, "ニュートラル", ""),
    (
        "BP03-006",
        "evolve_bounce",
        "フォロワー",
        2_u64,
        "エルフ",
        "",
    ),
    (
        "BP16-102",
        "evolve",
        "アミュレット",
        2_u64,
        "ニュートラル",
        "",
    ),
    ("BP15-030", "play", "フォロワー", 2_u64, "ロイヤル", "兵士"),
    ("BP13-011", "activate", "フォロワー", 4_u64, "エルフ", "獣"),
    (
        "BP09-094",
        "evolve",
        "アミュレット",
        2_u64,
        "ビショップ",
        "",
    ),
    ("BP11-015", "put", "フォロワー", 5_u64, "ニュートラル", ""),
    ("BP18-055", "play", "フォロワー", 6_u64, "ドラゴン", ""),
    ("BP21-105", "evolve", "フォロワー", 5_u64, "ビショップ", ""),
    ("BP22-102", "play", "アミュレット", 2_u64, "ビショップ", ""),
];
fn face(name: &str, kind: &str, cost: u64, class: &str, traits: &[&str]) -> Value {
    let follower = kind == "フォロワー";
    json!({"name":name,"card_type":kind,"card_class":class,"cost":cost.to_string(),
        "power":if follower {"2"} else {"-"},"hp":if follower {"10"} else {"-"},
        "traits":traits,"text":Value::Null,"sections":[]})
}
fn prepared() -> &'static (SelectedEffects, BTreeMap<String, WellFormedCardProgram>) {
    static PREPARED: OnceLock<(SelectedEffects, BTreeMap<String, WellFormedCardProgram>)> =
        OnceLock::new();
    PREPARED.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        let originals = snapshot.lines().map(|line| serde_json::from_str::<Value>(line).unwrap()).map(|row| (row["number"].as_str().unwrap().to_owned(), row["faces"][0].clone())).collect::<BTreeMap<_,_>>();
        let mut controls = vec![
            ("entry-plain".to_owned(), face("plain", "スペル", 1, "ニュートラル", &[])),
            ("entry-enemy".to_owned(), face("enemy", "フォロワー", 5, "ニュートラル", &[])),
            ("entry-look-amulet".to_owned(), face("look amulet", "アミュレット", 5, "ビショップ", &[])),
            ("entry-unu".to_owned(), face("翼天の執行者・ウヌエル", "フォロワー", 7, "ビショップ", &[])),
        ];
        let mut tree = expanded::tree().clone();
        let template = tree["cards"]["synthetic-evolution"].clone();
        tree["cards"] = json!({});
        for (number, mode, kind, cost, class, trait_id) in CASES {
            let traits = if trait_id.is_empty() { vec![] } else { vec![*trait_id] };
            let name = "entry match";
            controls.push((format!("{number}-match"), face(name, kind, *cost, class, &traits)));
            controls.push((format!("{number}-high"), face("over cost", kind, cost.checked_add(1).unwrap(), class, &traits)));
            let wrong_kind = if *kind == "アミュレット" { "フォロワー" } else { "アミュレット" };
            controls.push((format!("{number}-wrong"), face(name, wrong_kind, *cost, class, &traits)));
            controls.push((format!("{number}-filter"), face("wrong filter", kind, *cost, class, &[])));
            controls.push((format!("{number}-class"), face(name, kind, *cost, "ウィッチ", &traits)));
            if mode.starts_with("evolve") {
                let base = format!("{number}-base");
                let base_name = originals[*number]["name"].as_str().unwrap();
                controls.push((base.clone(), face(base_name, "フォロワー", 1, class, &[])));
                tree["cards"][&base] = template.clone();
            }
        }
        for producer in ["entry-mover", "entry-bouncer", "entry-doom"] {
            controls.push((producer.to_owned(), face(producer, "アミュレット", 1, "ニュートラル", &[])));
            let steps = match producer {
                "entry-mover" => json!([
                    {"choose":{"bind":"picked","from":{"who":"self","zone":"ex","kind":"follower"},"n":1_u8}},
                    {"move":{"ref":"picked","to":"field","result":"entered"}}
                ]),
                "entry-bouncer" => json!([{"move":{"ref":{"who":"self","zone":"field","kind":"follower"},"to":"hand"}}]),
                _ => json!([{"destroy":{"ref":{"who":"self","zone":"field","kind":"follower"}}}]),
            };
            let mut ability = template["abilities"][0].clone();
            ability.as_object_mut().unwrap().remove("evolve");
            ability["activate"] = json!({"cost":{"pp":0_u8},"do":steps});
            tree["cards"][producer] = json!({"meta":template["meta"],"abilities":[ability]});
        }
        for (number, card_face) in controls {
            snapshot.push_str(&json!({"number":number,"faces":[card_face]}).to_string());
            snapshot.push('\n');
            input["documents"].as_array_mut().unwrap().push(json!([format!("{number}.yaml"), json!({"format":1_u8,"kind":"effect_set","cards":{number:{"status":"complete","abilities":[]}}}).to_string()]));
        }
        (repository::selection_with_rules_from(&snapshot, &keywords, &input), lowerer().lower(&tree).unwrap())
    })
}
fn build(initial: &Value) -> Game {
    let (selected, programs) = prepared();
    let mut game = Game::from_selected_effects(
        selected.clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "hand-entry-light2",
    )
    .unwrap();
    let empty = WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![],
    })
    .unwrap();
    for number in [
        "entry-plain",
        "entry-enemy",
        "entry-look-amulet",
        "entry-unu",
    ] {
        game.load_internal_card(number, &empty).unwrap();
    }
    for producer in ["entry-mover", "entry-bouncer", "entry-doom"] {
        game.load_internal_card(producer, &programs[producer])
            .unwrap();
    }
    for (number, mode, ..) in CASES {
        for suffix in ["match", "high", "wrong", "filter", "class"] {
            game.load_internal_card(
                &format!("{number}-{suffix}"),
                programs
                    .get(&format!("{number}-{suffix}"))
                    .unwrap_or(&empty),
            )
            .unwrap();
        }
        if mode.starts_with("evolve") {
            let base = format!("{number}-base");
            game.load_internal_card(&base, &programs[&base]).unwrap();
        }
    }
    game
}
fn finish(game: &mut Game, mut play: CardPlay, trace: &mut Vec<Vec<String>>) {
    while let CardPlay::Awaiting(question) = play {
        let answer = match &question.domain {
            InputDomain::Targets(domain) => {
                trace.push(
                    domain
                        .members
                        .iter()
                        .filter_map(|member| member.card.clone())
                        .collect(),
                );
                Answer::Targets(
                    domain
                        .members
                        .iter()
                        .take(usize::try_from(domain.cardinality.maximum.unwrap().0).unwrap())
                        .map(|member| member.handle)
                        .collect(),
                )
            }
            InputDomain::CostMaterials(domain) => Answer::CostMaterials(
                domain
                    .members
                    .iter()
                    .take(usize::try_from(domain.cardinality.minimum.0).unwrap())
                    .map(|m| m.handle)
                    .collect(),
            ),
            InputDomain::Order(cards) => {
                Answer::Order(cards.iter().rev().map(|c| c.handle).collect())
            }
            InputDomain::Search(domain) => Answer::Search(SearchAnswer::Ungrouped(
                domain
                    .cards
                    .iter()
                    .take(usize::try_from(domain.cardinality.maximum.unwrap().0).unwrap())
                    .map(|m| m.handle)
                    .collect(),
            )),
            InputDomain::OptionalCost => Answer::OptionalCost(true),
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            InputDomain::Integer(bounds) => Answer::Integer(bounds.maximum.unwrap()),
            _ => panic!("unexpected hand entry choice: {:?}", question.domain),
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    assert!(
        !matches!(play, CardPlay::Cancelled),
        "the printed instruction has legal choices and payment"
    );
}
fn settle(game: &mut Game, trace: &mut Vec<Vec<String>>) {
    loop {
        if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
        let Some(question) = game.internal_pending_choice().unwrap() else {
            break;
        };
        let InputDomain::Pending(options) = &question.domain else {
            panic!("pending hand entry ability")
        };
        let play = game
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(
                    options
                        .iter()
                        .min_by_key(|option| option.line)
                        .unwrap()
                        .handle,
                ),
            )
            .unwrap();
        finish(game, play, trace);
    }
}
fn matching_card(number: &str) -> String {
    if matches!(number, "BP11-015" | "BP21-105") {
        "BP01-176".to_owned()
    } else {
        format!("{number}-match")
    }
}
fn higher_card(number: &str) -> String {
    if matches!(number, "BP11-015" | "BP21-105") {
        "BP13-017".to_owned()
    } else {
        format!("{number}-high")
    }
}
fn position(number: &str, alias: &str, mode: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"matching","card":matching_card(number)}, {"id":"high","card":higher_card(number)}, {"id":"wrong","card":format!("{number}-wrong")}
    ]);
    initial["players"]["P1"]["zones"]["deck"] = json!(
        (0_u8..8_u8)
            .map(|i| json!({"id":format!("d{i}"),"card":"entry-plain"}))
            .collect::<Vec<_>>()
    );
    if number == "BP19-097" {
        initial["players"]["P1"]["zones"]["deck"]
            .as_array_mut()
            .unwrap()
            .splice(
                0..2,
                [
                    json!({"id":"look-amulet","card":"entry-look-amulet"}),
                    json!({"id":"look-name","card":"entry-unu"}),
                ],
            );
    }
    if number == "BP11-015" {
        initial["players"]["P1"]["zones"]["deck"][0] =
            json!({"id":"found-ward","card":higher_card(number)});
    }
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"entry-enemy"}]);
    if mode == "play" {
        initial["players"]["P1"]["zones"]["hand"]
            .as_array_mut()
            .unwrap()
            .insert(0, json!({"id":"source","card":alias}));
    } else {
        let source = if matches!(mode, "activate" | "last" | "bounce") {
            alias.to_owned()
        } else {
            format!("{number}-base")
        };
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":source}]);
    }
    if mode.starts_with("evolve") {
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":alias}]);
    }
    if number == "CP04-025" {
        initial["players"]["P1"]["construction"] = json!("title");
        initial["players"]["P1"]["title_code"] = json!("princess_connect_re_dive");
    }
    if number == "BP15-030" {
        initial["players"]["P1"]["zones"]["hand"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"matching2","card":format!("{number}-match")}));
    }
    if number == "BP14-003" {
        initial["players"]["P1"]["zones"]["cemetery"] = json!(
            (0_u8..3_u8)
                .map(|i| json!({"id":format!("grave{i}"),"card":format!("{number}-match")}))
                .collect::<Vec<_>>()
        );
    }
    if matches!(mode, "last" | "evolve_last" | "bounce" | "evolve_bounce") {
        let producer = if mode.contains("last") {
            "entry-doom"
        } else {
            "entry-bouncer"
        };
        initial["players"]["P1"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"producer","card":producer}));
    }
    if mode == "put" {
        initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"source","card":alias}]);
        initial["players"]["P1"]["zones"]["field"] =
            json!([{"id":"producer","card":"entry-mover"}]);
    }
    let trait_id = CASES.iter().find(|(card, ..)| *card == number).unwrap().5;
    if !trait_id.is_empty() || matches!(number, "BP11-015" | "BP21-105") {
        initial["players"]["P1"]["zones"]["hand"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"filter","card":format!("{number}-filter")}));
    }
    if matches!(number, "BP03-005" | "BP03-006" | "BP18-055") {
        initial["players"]["P1"]["zones"]["hand"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"class","card":format!("{number}-class")}));
    }
    initial
}
fn run(number: &str) {
    let (_, mode, ..) = *CASES.iter().find(|(card, ..)| *card == number).unwrap();
    let (selected, _) = prepared();
    let card_id = &selected.current[number].card_id;
    for (alias, _) in selected
        .current
        .iter()
        .filter(|(_, card)| &card.card_id == card_id)
    {
        let initial = position(number, alias, mode);
        let mut trace = Vec::new();
        let mut game = build(&initial);
        match mode {
            "play" => {
                let play = game
                    .play_internal_card("source", PlayerId { seat: 1 })
                    .unwrap();
                finish(&mut game, play, &mut trace);
            }
            "activate" => {
                let id = selected.current[number]
                    .program
                    .card()
                    .abilities
                    .iter()
                    .find(|a| a.node.kind == AbilityKind::Activated)
                    .unwrap()
                    .node
                    .id;
                let play = game
                    .activate_internal_card("source", id, PlayerId { seat: 1 })
                    .unwrap();
                finish(&mut game, play, &mut trace);
            }
            "evolve" | "evolve_last" | "evolve_bounce" => {
                let pay = json!({"pp":1_u8});
                assert_eq!(
                    game.internal_rule_action(
                        &json!({"do":"evolve","source":"source","evolve_card":"form","pay":pay}),
                    )
                    .unwrap()
                    .outcome,
                    "resolved",
                    "the declared evolution resolves"
                );
            }
            "last" | "bounce" => {}
            "put" => {
                let id = prepared().1["entry-mover"].card().abilities[0].node.id;
                let play = game
                    .activate_internal_card("producer", id, PlayerId { seat: 1 })
                    .unwrap();
                finish(&mut game, play, &mut trace);
            }
            _ => panic!("unknown entry mode"),
        }
        settle(&mut game, &mut trace);
        if matches!(mode, "last" | "evolve_last" | "bounce" | "evolve_bounce") {
            let producer = if mode.contains("last") {
                "entry-doom"
            } else {
                "entry-bouncer"
            };
            let id = prepared().1[producer].card().abilities[0].node.id;
            let play = game
                .activate_internal_card("producer", id, PlayerId { seat: 1 })
                .unwrap();
            finish(&mut game, play, &mut trace);
            settle(&mut game, &mut trace);
        }
        verify(&mut game, number, alias, &mut trace);
    }
}

fn hand_candidates(trace: &[Vec<String>], number: &str, alias: &str) {
    let matching = matching_card(number);
    let mut candidates = trace
        .iter()
        .rev()
        .find(|cards| cards.contains(&matching))
        .unwrap()
        .clone();
    let mut expected = vec![matching_card(number)];
    if number == "BP11-015" {
        expected.extend([higher_card(number), higher_card(number)]);
    }
    if number == "BP15-030" {
        expected.push(format!("{number}-match"));
    }
    if number == "BP03-005" {
        expected.push(alias.to_owned());
    }
    if number == "BP03-006" {
        expected.push(format!("{number}-base"));
    }
    candidates.sort();
    expected.sort();
    assert_eq!(
        candidates, expected,
        "{number} exposes exactly its printed hand candidates"
    );
}
fn verify_discovery(state: &Value, number: &str, trace: &[Vec<String>]) {
    if number == "BP19-097" {
        let mut candidates = trace
            .iter()
            .find(|cards| cards.contains(&"entry-unu".to_owned()))
            .unwrap()
            .clone();
        candidates.sort();
        assert_eq!(
            candidates,
            vec!["entry-look-amulet", "entry-unu"],
            "Look offers both branches and excludes unrelated spells"
        );
        assert!(
            state["P1"]["hand"]
                .as_array()
                .unwrap()
                .contains(&json!("look-amulet")),
            "the selected Look amulet reaches the hand"
        );
    }
    if number == "BP11-015" {
        assert_eq!(
            state["P1"]["pp"]["current"], 5_u8,
            "the second Fanfare pays five PP"
        );
        assert!(
            state["P1"]["hand"]
                .as_array()
                .unwrap()
                .contains(&json!("found-ward")),
            "the first Fanfare finds a printed Ward follower"
        );
    }
}
fn verify(game: &mut Game, number: &str, alias: &str, trace: &mut Vec<Vec<String>>) {
    hand_candidates(trace, number, alias);
    let state = game.projection(View::Referee).unwrap();
    assert!(
        state["P1"]["field"]
            .as_array()
            .unwrap()
            .contains(&json!("matching")),
        "{number} puts the chosen hand card into play"
    );
    assert!(
        state["P1"]["hand"]
            .as_array()
            .unwrap()
            .contains(&json!("high")),
        "the unchosen higher-cost control remains in hand"
    );
    assert!(
        state["P1"]["hand"]
            .as_array()
            .unwrap()
            .contains(&json!("wrong")),
        "the wrong-kind control remains in hand"
    );
    verify_discovery(&state, number, trace);
    if number == "BP07-054" {
        assert_eq!(
            state["objects"]["matching"]["power"], 4_u8,
            "the entered Ocean follower gains two power"
        );
        assert_eq!(
            state["objects"]["matching"]["hp"], 12_u8,
            "the entered Ocean follower gains two health"
        );
    }
    if number == "BP14-003" {
        assert_eq!(
            state["objects"]["enemy"]["hp"], 6_u8,
            "the activation damages the chosen enemy follower before entry"
        );
    }
    if number == "BP09-094" {
        assert_eq!(
            state["P1"]["leader"]["life"], 11_u8,
            "amulet entry triggers the evolved leader heal"
        );
    }
    if number == "BP15-030" {
        assert!(
            state["P1"]["field"]
                .as_array()
                .unwrap()
                .contains(&json!("matching2")),
            "both selected Soldiers enter"
        );
    }
    if number == "CP04-025" {
        assert!(
            state["objects"]["source"]["keywords"]
                .as_array()
                .unwrap()
                .contains(&json!("突進")),
            "the printed extra section gives Lili Rush"
        );

        assert_eq!(
            state["P1"]["deck"].as_array().unwrap().len(),
            7_usize,
            "UB fanfare draws one card"
        );
    }
    if number == "BP18-055" {
        for keyword in ["突進", "指定攻撃"] {
            assert!(
                state["objects"]["matching"]["keywords"]
                    .as_array()
                    .unwrap()
                    .contains(&json!(keyword)),
                "the entered follower gains both keywords"
            );
        }
        game.internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap();
        settle(game, trace);
        let after = game.projection(View::Referee).unwrap();
        assert!(
            after["P1"]["hand"]
                .as_array()
                .unwrap()
                .contains(&json!("matching")),
            "the granted End ability returns the follower"
        );
    }
}

#[test]
fn bp07_054_executes_its_printed_hand_entry_and_followup() {
    run("BP07-054");
}

#[test]
fn bp14_003_executes_its_printed_hand_entry_and_followup() {
    run("BP14-003");
}

#[test]
fn bp19_096_executes_its_printed_hand_entry_and_followup() {
    run("BP19-096");
}

#[test]
fn bp19_097_executes_its_printed_hand_entry_and_followup() {
    run("BP19-097");
}

#[test]
fn bp03_005_executes_its_printed_hand_entry_and_followup() {
    run("BP03-005");
}

#[test]
fn cp04_025_executes_its_printed_hand_entry_and_followup() {
    run("CP04-025");
}

#[test]
fn bp03_006_executes_its_printed_hand_entry_and_followup() {
    run("BP03-006");
}

#[test]
fn bp16_102_executes_its_printed_hand_entry_and_followup() {
    run("BP16-102");
}

#[test]
fn bp15_030_executes_its_printed_hand_entry_and_followup() {
    run("BP15-030");
}

#[test]
fn bp13_011_executes_its_printed_hand_entry_and_followup() {
    run("BP13-011");
}

#[test]
fn bp09_094_executes_its_printed_hand_entry_and_followup() {
    run("BP09-094");
}

#[test]
fn bp11_015_executes_its_printed_hand_entry_and_followup() {
    run("BP11-015");
}

#[test]
fn bp18_055_executes_its_printed_hand_entry_and_followup() {
    run("BP18-055");
}

#[test]
fn bp21_105_executes_its_printed_hand_entry_and_followup() {
    run("BP21-105");
}

#[test]
fn bp22_102_executes_its_printed_hand_entry_and_followup() {
    run("BP22-102");
}
