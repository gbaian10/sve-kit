#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate the printed scenarios."
)]
use super::*;
use alloc::collections::BTreeMap;
use sve_engine::game::internal::{DamageShare, SearchAnswer};

const CONTROLS: [&str; 10] = [
    "wire-onion",
    "wire-plain",
    "wire-ocean",
    "wire-ocean-spell",
    "wire-faith",
    "wire-soldier",
    "wire-academy",
    "wire-whale",
    "wire-vincent-base",
    "wire-lilia-base",
];

fn append(snapshot: &mut String, input: &mut Value, number: &str, face: &Value) {
    snapshot.push_str(&json!({"number":number,"faces":[face]}).to_string());
    snapshot.push('\n');
    input["documents"].as_array_mut().unwrap().push(json!([
        format!("{number}.yaml"),
        json!({"format":1_u8,"kind":"effect_set","cards":{number:{"status":"complete","abilities":[]}}}).to_string()
    ]));
}

fn selected() -> &'static SelectedEffects {
    static READY: OnceLock<SelectedEffects> = OnceLock::new();
    READY.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = producer_inputs();
        // Controlled companions isolate filters and resources from unrelated programs.
        for (number,name,class,kind,cost,traits) in [
            ("wire-onion","オニオン軍団","ウィッチ","フォロワー","1",vec![]),
            ("wire-plain","plain","ニュートラル","フォロワー","1",vec![]),
            ("wire-ocean","ocean follower","ドラゴン","フォロワー","3",vec!["海洋"]),
            ("wire-ocean-spell","ocean spell","ドラゴン","スペル","2",vec!["海洋"]),
            ("wire-faith","faith","ビショップ","フォロワー","3",vec!["信仰"]),
            ("wire-soldier","soldier","ロイヤル","フォロワー","2",vec!["兵士"]),
            ("wire-academy","academy","エルフ","フォロワー","2",vec!["学院"]),
            ("wire-whale","controlledホエール","ドラゴン","フォロワー","7",vec!["海洋"]),
            ("wire-vincent-base","ピースキーパー・ヴィンセント","ウィッチ","フォロワー","2",vec![]),
            ("wire-lilia-base","竜の魔女・リリウム","ドラゴン","フォロワー","2",vec!["学院"]),
        ] {
            append(&mut snapshot,&mut input,number,&json!({"name":name,"card_class":class,"card_type":kind,"cost":cost,"power":if kind=="スペル" {"-"} else {"3"},"hp":if kind=="スペル" {"-"} else {"20"},"traits":traits,"text":null,"sections":[]}));
        }
        for cost in 0_u8..12 {
            let number = format!("wire-cost-{cost}");
            append(&mut snapshot,&mut input,&number,&json!({"name":number,"card_class":"ニュートラル","card_type":"フォロワー","cost":cost.to_string(),"power":"3","hp":"20","traits":[],"text":null,"sections":[]}));
        }
        repository::selection_with_rules_from(&snapshot,&keywords,&input)
    })
}

fn programs() -> &'static BTreeMap<String, WellFormedCardProgram> {
    static PROGRAMS: OnceLock<BTreeMap<String, WellFormedCardProgram>> = OnceLock::new();
    PROGRAMS.get_or_init(|| {
        let base = expanded::tree();
        let mut out = BTreeMap::new();
        for (number, abilities) in [
            (
                "wire-vincent-base",
                base["cards"]["synthetic-evolution"]["abilities"].clone(),
            ),
            (
                "wire-lilia-base",
                base["cards"]["synthetic-evolution"]["abilities"].clone(),
            ),
            (
                "wire-ocean-spell",
                json!([{"line":1_u8,"spell":{"do":[{"draw":{"n":0_u8}}]}}]),
            ),
        ] {
            let mut tree = base.clone();
            let mut card = tree["cards"]["synthetic-evolution"].clone();
            let mut abilities = abilities;
            for entry in abilities.as_array_mut().unwrap() {
                if entry.get("ports").is_none() {
                    entry["ports"] =
                        base["cards"]["synthetic-evolution"]["abilities"][0]["ports"].clone();
                    entry["sources"] =
                        base["cards"]["synthetic-evolution"]["abilities"][0]["sources"].clone();
                }
            }
            card["abilities"] = abilities;
            tree["cards"] = json!({number:card});
            out.insert(
                number.to_owned(),
                lowerer()
                    .lower_with_printed_costs(&tree, &BTreeMap::from([(number.to_owned(), Nat(2))]))
                    .unwrap()
                    .remove(number)
                    .unwrap(),
            );
        }
        out
    })
}

fn load_controls(game: &mut Game) {
    let empty = WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![],
    })
    .unwrap();
    for card in CONTROLS {
        if !programs().contains_key(card) {
            game.load_internal_card(card, &empty).unwrap();
        }
    }
    for cost in 0_u8..12 {
        game.load_internal_card(&format!("wire-cost-{cost}"), &empty)
            .unwrap();
    }
    for (number, program) in programs() {
        game.load_internal_card(number, program).unwrap();
    }
}

fn position(card: &str) -> Value {
    let mut initial = draw_position();
    if card.starts_with("CP04") {
        initial["players"]["P1"]["construction"] = json!("title");
        initial["players"]["P1"]["title_code"] = json!("princess_connect_re_dive");
    }
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    initial["players"]["P1"]["zones"]["deck"] = json!(
        (0_u8..6)
            .map(|i| json!({"id":format!("draw{i}"),"card":"wire-plain"}))
            .collect::<Vec<_>>()
    );
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"wire-plain"}]);
    initial
}
fn game(initial: &Value) -> Game {
    let mut game = Game::from_selected_effects(
        selected().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "wire-hot02",
    )
    .unwrap();
    load_controls(&mut game);
    game
}
fn finish(game: &mut Game, mut step: CardPlay) {
    while let CardPlay::Awaiting(q) = step {
        let answer = match &q.domain {
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            InputDomain::OptionalCost => Answer::OptionalCost(true),
            InputDomain::Modes { .. } => Answer::Modes(vec![ModeId(1)]),
            InputDomain::Targets(d) => Answer::Targets(
                d.members
                    .iter()
                    .take(
                        d.cardinality
                            .maximum
                            .map_or(d.members.len(), |n| usize::try_from(n.0).unwrap()),
                    )
                    .map(|m| m.handle)
                    .collect(),
            ),
            InputDomain::CostMaterials(d) => Answer::CostMaterials(
                d.members
                    .iter()
                    .take(usize::try_from(d.cardinality.minimum.0).unwrap())
                    .map(|m| m.handle)
                    .collect(),
            ),
            InputDomain::Search(d) => Answer::Search(SearchAnswer::Ungrouped(
                d.cards
                    .iter()
                    .take(
                        d.cardinality
                            .maximum
                            .map_or(d.cards.len(), |n| usize::try_from(n.0).unwrap()),
                    )
                    .map(|m| m.handle)
                    .collect(),
            )),
            InputDomain::Order(cards) => Answer::Order(cards.iter().map(|m| m.handle).collect()),
            InputDomain::Distribution(d) => {
                let last = d.members.len().saturating_sub(1);
                Answer::Distribution(
                    d.members
                        .iter()
                        .enumerate()
                        .map(|(i, m)| DamageShare {
                            handle: m.handle,
                            amount: Nat(if i == last {
                                d.total.0.checked_sub(u64::try_from(last).unwrap()).unwrap()
                            } else {
                                1_u64
                            }),
                        })
                        .collect(),
                )
            }
            other => panic!("unexpected decision: {other:?}"),
        };
        step = game.answer_internal_card(q.actor, q.token, answer).unwrap();
    }
}
fn settle(game: &mut Game) {
    loop {
        if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
        let Some(q) = game.internal_pending_choice().unwrap() else {
            break;
        };
        let InputDomain::Pending(options) = &q.domain else {
            panic!("pending")
        };
        let step = game
            .answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
            .unwrap();
        finish(game, step);
    }
}
fn evolution(base: &str, form: &str) -> Value {
    let mut initial = position(base);
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":base}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
    initial
}
fn evolve(game: &mut Game, pp: u8) {
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":pp}})
        )
        .unwrap()
        .outcome,
        "resolved",
        "the printed evolution resolves"
    );
    settle(game);
}

#[test]
fn cp02_019_evolution_acts_the_selected_enemy() {
    for card in ["CP02-019", "CP02-SL05", "CP02-SP03", "CP02-U03"] {
        let mut game = game(&evolution("CP02-018", card));
        evolve(&mut game, 1);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["enemy"]["acted"], true);
        assert!(
            state["objects"]["source"]["keywords"]
                .as_array()
                .unwrap()
                .contains(&json!("守護"))
        );
    }
}
#[test]
fn bp21_056_evolution_searches_a_differently_named_academy_follower() {
    for card in ["BP21-056", "BP21-SL14"] {
        let mut initial = evolution("wire-lilia-base", card);
        initial["players"]["P1"]["zones"]["deck"] =
            json!([{"id":"same","card":"wire-lilia-base"},{"id":"friend","card":"wire-academy"}]);
        let mut game = game(&initial);
        evolve(&mut game, 1);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["hand"], json!(["friend"]));
        assert_eq!(state["P1"]["deck"], json!(["same"]));
    }
}
