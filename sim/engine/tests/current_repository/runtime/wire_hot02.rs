#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate the printed scenarios."
)]
use super::*;
use alloc::collections::BTreeMap;
use sve_engine::game::internal::{DamageShare, SearchAnswer};

const PLAYER: PlayerId = PlayerId { seat: 1 };
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
fn play(game: &mut Game, id: &str) {
    let step = game.play_internal_card(id, PLAYER).unwrap();
    finish(game, step);
    settle(game);
}
fn ability(card: &str) -> AbilityId {
    selected().current[card]
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
        .activate_internal_card("source", ability(card), PLAYER)
        .unwrap();
    finish(game, step);
    settle(game);
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
fn bp11_036_evolution_applies_all_four_distinct_cost_thresholds() {
    for card in ["BP11-036", "BP11-SL08"] {
        let mut initial = evolution("wire-vincent-base", card);
        initial["players"]["P1"]["zones"]["cemetery"] = json!(
            (0_u8..12)
                .map(|i| json!({"id":format!("cost{i}"),"card":format!("wire-cost-{i}")}))
                .collect::<Vec<_>>()
        );
        let mut game = game(&initial);
        evolve(&mut game, 1);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["leader"]["life"], 13_u8);
        assert_eq!(state["P1"]["hand"], json!(["draw0"]));
        assert_eq!(state["objects"]["enemy"]["hp"], 11_u8);
        assert_eq!(state["P2"]["leader"]["life"], 8_u8);
    }
}
#[test]
fn bp15_065_distributes_maximum_pp_damage_between_two_enemies() {
    for card in ["BP15-065", "PR-412"] {
        let mut initial = position(card);
        initial["players"]["P2"]["zones"]["field"] =
            json!([{"id":"enemy","card":"wire-plain"},{"id":"enemy2","card":"wire-plain"}]);
        let mut game = game(&initial);
        play(&mut game, "source");
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["enemy"]["hp"], 19_u8);
        assert_eq!(state["objects"]["enemy2"]["hp"], 11_u8);
    }
}
#[test]
fn bp10_088_first_mode_creates_a_ghost_in_ex() {
    for card in ["BP10-088", "PR-244"] {
        let mut game = game(&position(card));
        play(&mut game, "source");
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["ex"].as_array().unwrap().len(), 1);
        assert_eq!(state["P1"]["hand"], json!([]));
    }
}
#[test]
fn bp15_063_evolution_discounts_both_ocean_types_and_observes_the_follower() {
    let card = "BP15-063";
    let mut initial = evolution("BP15-062", card);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"friend","card":"wire-ocean"},{"id":"spell","card":"wire-ocean-spell"},{"id":"other1","card":"wire-plain"},{"id":"other2","card":"wire-plain"},{"id":"other3","card":"wire-plain"},{"id":"untouched","card":"wire-plain"}]);
    let mut game = game(&initial);
    evolve(&mut game, 1);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["ex"],
        json!(["friend", "spell"])
    );
    play(&mut game, "friend");
    play(&mut game, "spell");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["pp"]["current"], 8_u8);
    assert_eq!(state["P2"]["leader"]["life"], 19_u8);
    assert_eq!(
        state["P1"]["deck"],
        json!(["untouched", "other1", "other2", "other3"])
    );
}

#[test]
fn bp15_104_reveals_a_ward_then_sacrifices_itself_to_raise_a_ward_hp() {
    for card in ["BP15-104", "PR-393"] {
        let mut initial = position(card);
        initial["players"]["P1"]["zones"]["field"] = json!(
            (0_u8..3)
                .map(|i| json!({"id":format!("ward{i}"),"card":"BP01-146"}))
                .collect::<Vec<_>>()
        );
        initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"found","card":"BP01-146"},{"id":"other1","card":"wire-plain"},{"id":"other2","card":"wire-plain"},{"id":"untouched","card":"wire-plain"}]);
        let mut game = game(&initial);
        play(&mut game, "source");
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["hand"],
            json!(["found"])
        );
        activate(&mut game, card);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["ward0"]["hp"], 4_u8);
        assert_eq!(state["P1"]["cemetery"], json!(["source"]));
    }
}
#[test]
fn bp10_105_pays_one_to_recover_and_strengthen_a_faith_follower() {
    for card in ["BP10-105", "PR-245"] {
        let mut initial = position(card);
        initial["players"]["P1"]["zones"]["cemetery"] = json!([{"id":"faith","card":"wire-faith"}]);
        let mut game = game(&initial);
        play(&mut game, "source");
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["ex"], json!(["faith"]));
        assert_eq!(state["objects"]["faith"]["power"], 4_u8);
        assert!(
            state["objects"]["source"]["keywords"]
                .as_array()
                .unwrap()
                .contains(&json!("守護"))
        );
    }
}
#[test]
fn bp19_031_recovers_a_soldier_with_both_stats_increased() {
    let mut initial = position("BP19-031");
    initial["players"]["P1"]["zones"]["cemetery"] = json!([{"id":"soldier","card":"wire-soldier"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["ex"], json!(["soldier"]));
    assert_eq!(state["objects"]["soldier"]["power"], 4_u8);
    assert_eq!(state["objects"]["soldier"]["hp"], 21_u8);
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

#[test]
fn cp04_111_ub_distributes_eight_damage() {
    for card in ["CP04-111", "CP04-SL27"] {
        let mut initial = position(card);
        initial["players"]["P2"]["zones"]["field"] =
            json!([{"id":"enemy","card":"wire-plain"},{"id":"enemy2","card":"wire-plain"}]);
        let mut game = game(&initial);
        play(&mut game, "source");
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["enemy"]["hp"], 19_u8);
        assert_eq!(state["objects"]["enemy2"]["hp"], 13_u8);
    }
}
#[test]
fn bp09_043_searches_two_onion_followers_into_the_field() {
    for card in ["BP09-043", "BP09-P14"] {
        let mut initial = position(card);
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":card}]);
        initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"onion1","card":"wire-onion"},{"id":"onion2","card":"wire-onion"},{"id":"plain","card":"wire-plain"}]);
        let mut game = game(&initial);
        activate(&mut game, card);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["field"], json!(["source", "onion1", "onion2"]));
        assert_eq!(state["P1"]["pp"]["current"], 9_u8);
    }
}
#[test]
fn bp22_059_searches_and_discounts_a_whale_then_buffs_its_entry() {
    for card in ["BP22-059", "BP22-SL17"] {
        let mut initial = position(card);
        initial["players"]["P1"]["zones"]["deck"] =
            json!([{"id":"whale","card":"wire-whale"},{"id":"plain","card":"wire-plain"}]);
        let mut game = game(&initial);
        play(&mut game, "source");
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["ex"],
            json!(["whale"])
        );
        play(&mut game, "whale");
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["whale"]["power"], 4_u8);
        assert_eq!(state["objects"]["whale"]["hp"], 21_u8);
        assert_eq!(state["P1"]["pp"]["current"], 2_u8);
    }
}
#[test]
fn bp21_100_academy_fanfare_hp_gain_damages_every_enemy() {
    for card in ["BP21-100", "BP21-P53"] {
        let mut initial = position(card);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"friend","card":"wire-academy"}]);
        initial["players"]["P2"]["zones"]["field"] =
            json!([{"id":"enemy","card":"wire-plain"},{"id":"enemy2","card":"wire-plain"}]);
        let mut game = game(&initial);
        play(&mut game, "source");
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["enemy"]["hp"], 19_u8);
        assert_eq!(state["objects"]["enemy2"]["hp"], 19_u8);
    }
}
#[test]
fn bp21_103_acts_to_heal_its_leader_with_another_academy_follower() {
    for card in ["BP21-103", "BP21-P56"] {
        let mut initial = position(card);
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] =
            json!([{"id":"source","card":card},{"id":"friend","card":"wire-academy"}]);
        let mut game = game(&initial);
        activate(&mut game, card);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
            11_u8
        );
    }
}
#[test]
fn bp11_012_fanfare_bottoms_the_top_card_and_creates_two_fairies_in_ex() {
    let mut game = game(&position("BP11-012"));
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["ex"].as_array().unwrap().len(), 2);
    assert_eq!(
        state["P1"]["deck"],
        json!(["draw1", "draw2", "draw3", "draw4", "draw5", "draw0"])
    );
}
#[test]
fn bp11_013_evolution_creates_two_fairies_in_the_field() {
    let mut game = game(&evolution("BP11-012", "BP11-013"));
    evolve(&mut game, 1);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["field"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
}

#[test]
fn bp21_056_end_phase_creates_the_admitted_young_dragon() {
    for card in ["BP21-056", "BP21-SL14"] {
        let mut initial = position(card);
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":card},{"id":"friend1","card":"wire-academy"},{"id":"friend2","card":"wire-academy"}]);
        let mut game = game(&initial);
        assert_eq!(
            game.internal_rule_action(&json!({"do":"end-phase"}))
                .unwrap()
                .outcome,
            "resolved",
            "the end phase creates the printed occurrence"
        );
        settle(&mut game);
        let state = game.projection(View::Referee).unwrap();
        let field = state["P1"]["field"].as_array().unwrap();
        assert_eq!(field.len(), 4);
        let id = field
            .iter()
            .filter_map(Value::as_str)
            .find(|id| !["source", "friend1", "friend2"].contains(id))
            .unwrap();
        assert!(
            state["objects"][id]["keywords"]
                .as_array()
                .unwrap()
                .contains(&json!("守護"))
        );
    }
}
#[test]
fn bp21_056_super_evolution_creates_the_admitted_giant_dragon() {
    for card in ["BP21-056", "BP21-SL14"] {
        let mut initial = evolution("wire-lilia-base", card);
        initial["players"]["P1"]["sep"] = json!(1_u8);
        initial["turn"]["elapsed_turns"]["P1"] = json!(7_u8);
        initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"friend","card":"wire-academy"}]);
        let mut game = game(&initial);
        assert_eq!(game.internal_rule_action(&json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8,"ep":0_u8,"sep":1_u8}})).unwrap().outcome,"resolved","the printed base evolves with SEP");
        settle(&mut game);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["sep"], 0_u8);
        assert_eq!(state["P1"]["hand"], json!(["friend"]));
        let field = state["P1"]["field"].as_array().unwrap();
        assert_eq!(field.len(), 2);
        let id = field
            .iter()
            .filter_map(Value::as_str)
            .find(|id| *id != "source")
            .unwrap();
        let keywords = state["objects"][id]["keywords"].as_array().unwrap();
        assert!(keywords.contains(&json!("突進")));
        assert!(keywords.contains(&json!("指定攻撃")));
    }
}
