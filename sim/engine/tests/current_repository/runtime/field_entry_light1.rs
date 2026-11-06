#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate printed hand-entry scenarios."
)]
use super::*;
use alloc::collections::BTreeMap;

type Case = (
    &'static str,
    &'static str,
    &'static str,
    u64,
    &'static str,
    &'static str,
);
const CASES: &[Case] = &[
    (
        "ECP02-014",
        "super",
        "フォロワー",
        6,
        "ニュートラル",
        "パッション",
    ),
    (
        "CP02-002",
        "evolve",
        "フォロワー",
        3,
        "ニュートラル",
        "パッション",
    ),
    ("BP08-106", "play", "フォロワー", 2, "ニュートラル", ""),
    (
        "ECP02-011",
        "play",
        "フォロワー",
        6,
        "ニュートラル",
        "パッション",
    ),
    ("BP07-102", "activate", "フォロワー", 2, "ニュートラル", ""),
    (
        "BP10-058",
        "activate",
        "フォロワー",
        3,
        "ニュートラル",
        "武装",
    ),
    ("BP13-105", "play", "フォロワー", 6, "ニュートラル", ""),
    (
        "CP03-104",
        "play",
        "フォロワー",
        4,
        "ニュートラル",
        "オラクルシンクタンク",
    ),
    (
        "BP17-101",
        "evolve",
        "フォロワー",
        2,
        "ニュートラル",
        "機械",
    ),
    (
        "CP03-004",
        "play",
        "フォロワー",
        3,
        "ニュートラル",
        "アクアフォース",
    ),
    (
        "CP03-006",
        "evolve",
        "フォロワー",
        1,
        "ニュートラル",
        "アクアフォース",
    ),
    ("BP03-112", "play", "フォロワー", 10, "ニュートラル", ""),
    ("BP06-055", "evolve", "フォロワー", 5, "ドラゴン", ""),
    (
        "CSD02c-002",
        "activate",
        "フォロワー",
        3,
        "ニュートラル",
        "パッション",
    ),
    ("BP03-062", "activate", "フォロワー", 3, "ニュートラル", ""),
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
            ("entry-weapon".to_owned(), face("ドラゴウェポン", "アミュレット", 1, "ドラゴン", &[])),
            ("entry-passion".to_owned(), face("passion material", "フォロワー", 1, "ニュートラル", &["パッション"])),
            ("entry-aqua".to_owned(), face("aqua material", "フォロワー", 1, "ニュートラル", &["アクアフォース"])),
        ];
        let mut tree = expanded::tree().clone();
        let template = tree["cards"]["synthetic-evolution"].clone();
        tree["cards"] = json!({});
        for (number, mode, kind, cost, class, trait_id) in CASES {
            let traits = if trait_id.is_empty() { vec![] } else { vec![*trait_id] };
            let name = if *number == "BP07-102" { "ムニャール control" } else { "entry match" };
            controls.push((format!("{number}-match"), face(name, kind, *cost, class, &traits)));
            controls.push((format!("{number}-high"), face("over cost", kind, cost.checked_add(1).unwrap(), class, &traits)));
            let wrong_kind = if *kind == "アミュレット" { "フォロワー" } else { "アミュレット" };
            controls.push((format!("{number}-wrong"), face(name, wrong_kind, *cost, class, &traits)));
            controls.push((format!("{number}-filter"), face("wrong filter", kind, *cost, class, &[])));
            controls.push((format!("{number}-class"), face(name, kind, *cost, "ウィッチ", &traits)));
            if matches!(*mode, "evolve" | "super") {
                let base = format!("{number}-base");
                let base_name = originals[*number]["name"].as_str().unwrap();
                controls.push((base.clone(), face(base_name, "フォロワー", 1, class, &[])));
                tree["cards"][&base] = template.clone();
            }
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
        "hand-entry-light1",
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
        "entry-weapon",
        "entry-passion",
        "entry-aqua",
    ] {
        game.load_internal_card(number, &empty).unwrap();
    }
    for (number, mode, ..) in CASES {
        for suffix in ["match", "high", "wrong", "filter", "class"] {
            game.load_internal_card(&format!("{number}-{suffix}"), &empty)
                .unwrap();
        }
        if matches!(*mode, "evolve" | "super") {
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
                Answer::Pending(options[0].handle),
            )
            .unwrap();
        finish(game, play, trace);
    }
}
fn position(number: &str, alias: &str, mode: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"matching","card":format!("{number}-match")}, {"id":"high","card":format!("{number}-high")}, {"id":"wrong","card":format!("{number}-wrong")}
    ]);
    initial["players"]["P1"]["zones"]["deck"] = json!(
        (0_u8..8_u8)
            .map(|i| json!({"id":format!("d{i}"),"card":"entry-plain"}))
            .collect::<Vec<_>>()
    );
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"entry-enemy"}]);
    if mode == "play" {
        initial["players"]["P1"]["zones"]["hand"]
            .as_array_mut()
            .unwrap()
            .insert(0, json!({"id":"source","card":alias}));
    } else {
        let source = if mode == "activate" {
            alias.to_owned()
        } else {
            format!("{number}-base")
        };
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":source}]);
    }
    if matches!(mode, "evolve" | "super") {
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":alias}]);
    }
    if mode == "super" {
        initial["players"]["P1"]["sep"] = json!(1_u8);
        initial["turn"]["elapsed_turns"] = json!({"P1":7_u8,"P2":6_u8});
    }
    if number == "BP10-058" {
        initial["players"]["P1"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"weapon","card":"entry-weapon"}));
    }
    if number == "CSD02c-002" {
        initial["players"]["P1"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .extend(
                (0_u8..3_u8).map(|i| json!({"id":format!("material{i}"),"card":"entry-passion"})),
            );
    }
    if number == "CP03-004" {
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"material","card":"entry-aqua"}]);
    }
    let trait_id = CASES.iter().find(|(card, ..)| *card == number).unwrap().5;
    if !trait_id.is_empty() || number == "BP07-102" {
        initial["players"]["P1"]["zones"]["hand"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"filter","card":format!("{number}-filter")}));
    }
    if matches!(number, "BP08-106" | "BP03-112" | "BP06-055") {
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
            "evolve" | "super" => {
                let pay = if mode == "super" {
                    json!({"pp":1_u8,"sep":1_u8})
                } else {
                    json!({"pp":1_u8})
                };
                assert_eq!(
                    game.internal_rule_action(
                        &json!({"do":"evolve","source":"source","evolve_card":"form","pay":pay}),
                    )
                    .unwrap()
                    .outcome,
                    "resolved",
                    "the declared ordinary or super evolution resolves"
                );
            }
            _ => panic!("unknown entry mode"),
        }
        settle(&mut game, &mut trace);
        verify(&mut game, number, &mut trace);
    }
}

fn hand_candidates(trace: &[Vec<String>], number: &str) {
    let prefix = format!("{number}-");
    let mut candidates = trace
        .iter()
        .rev()
        .find(|cards| cards.iter().any(|card| card.starts_with(&prefix)))
        .unwrap()
        .clone();
    let mut expected = vec![format!("{number}-match")];
    if matches!(number, "BP08-106" | "CSD02c-002" | "BP03-062") {
        expected.push(format!("{number}-high"));
    }
    if number == "CP03-004" {
        expected.push("entry-aqua".to_owned());
    }
    candidates.sort();
    expected.sort();
    assert_eq!(
        candidates, expected,
        "{number} exposes exactly its printed hand candidates"
    );
}
fn verify(game: &mut Game, number: &str, trace: &mut Vec<Vec<String>>) {
    hand_candidates(trace, number);
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
    if number == "BP06-055" {
        assert_eq!(
            state["objects"]["matching"]["keywords"],
            json!(["守護"]),
            "the source gives the entered Dragon Ward"
        );
    }
    if number == "ECP02-011" {
        assert_eq!(
            state["P1"]["ex"].as_array().unwrap().len(),
            1_usize,
            "the spell creates exactly one Magic Item in EX"
        );
    }
    if number == "BP10-058" {
        assert!(
            state["P1"]["cemetery"]
                .as_array()
                .unwrap()
                .contains(&json!("weapon")),
            "the Dragoweapon cost goes to the cemetery"
        );
    }
    if number == "CP03-004" {
        assert!(
            state["P1"]["hand"]
                .as_array()
                .unwrap()
                .contains(&json!("material")),
            "the paid Aqua Force material returns to hand"
        );
    }
    if matches!(
        number,
        "BP08-106" | "BP13-105" | "BP03-112" | "CSD02c-002" | "BP03-062"
    ) {
        game.internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap();
        settle(game, trace);
        let after = game.projection(View::Referee).unwrap();
        let destination = if number == "BP03-062" {
            "cemetery"
        } else if number == "BP03-112" {
            "deck"
        } else {
            "hand"
        };
        assert!(
            after["P1"][destination]
                .as_array()
                .unwrap()
                .contains(&json!("matching")),
            "the granted End ability sends its holder to its specified destination"
        );
        assert!(
            !after["P1"]["field"]
                .as_array()
                .unwrap()
                .contains(&json!("matching")),
            "the granted End ability removes its holder from the field"
        );
    }
}

#[test]
fn ecp02_014_executes_its_printed_hand_entry_and_followup() {
    run("ECP02-014");
}

#[test]
fn cp02_002_executes_its_printed_hand_entry_and_followup() {
    run("CP02-002");
}

#[test]
fn bp08_106_executes_its_printed_hand_entry_and_followup() {
    run("BP08-106");
}

#[test]
fn ecp02_011_executes_its_printed_hand_entry_and_followup() {
    run("ECP02-011");
}

#[test]
fn bp07_102_executes_its_printed_hand_entry_and_followup() {
    run("BP07-102");
}

#[test]
fn bp10_058_executes_its_printed_hand_entry_and_followup() {
    run("BP10-058");
}

#[test]
fn bp13_105_executes_its_printed_hand_entry_and_followup() {
    run("BP13-105");
}

#[test]
fn cp03_104_executes_its_printed_hand_entry_and_followup() {
    run("CP03-104");
}

#[test]
fn bp17_101_executes_its_printed_hand_entry_and_followup() {
    run("BP17-101");
}

#[test]
fn cp03_004_executes_its_printed_hand_entry_and_followup() {
    run("CP03-004");
}

#[test]
fn cp03_006_executes_its_printed_hand_entry_and_followup() {
    run("CP03-006");
}

#[test]
fn bp03_112_executes_its_printed_hand_entry_and_followup() {
    run("BP03-112");
}

#[test]
fn bp06_055_executes_its_printed_hand_entry_and_followup() {
    run("BP06-055");
}

#[test]
fn csd02c_002_executes_its_printed_hand_entry_and_followup() {
    run("CSD02c-002");
}

#[test]
fn bp03_062_executes_its_printed_hand_entry_and_followup() {
    run("BP03-062");
}
