#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate printed card scenarios."
)]
use super::*;
use alloc::collections::BTreeMap;
use sve_engine::game::internal::SearchAnswer;
fn fixture() -> &'static SelectedEffects {
    static SELECTED: OnceLock<SelectedEffects> = OnceLock::new();
    SELECTED.get_or_init(|| {
  let (mut snapshot,keywords,mut input)=producer_inputs();
  // Minimal companions isolate printed names, traits and original costs.
  snapshot.push_str(&json!({"number":"ready-deremas","faces":[{"name":"ready-deremas","card_class":"ニュートラル","card_type":"フォロワー","cost":"1","power":"2","hp":"20","traits":["デレマス"],"text":"","sections":[]}]}).to_string());snapshot.push('\n');
  snapshot.push_str(&json!({"number":"ready-cute","faces":[{"name":"ready-cute","card_class":"ニュートラル","card_type":"フォロワー","cost":"1","power":"2","hp":"20","traits":["デレマス","キュート"],"text":"","sections":[]}]}).to_string());snapshot.push('\n');
  snapshot.push_str(&json!({"number":"ready-cool","faces":[{"name":"ready-cool","card_class":"ニュートラル","card_type":"フォロワー","cost":"1","power":"2","hp":"20","traits":["デレマス","クール"],"text":"","sections":[]}]}).to_string());snapshot.push('\n');
  snapshot.push_str(&json!({"number":"ready-passion","faces":[{"name":"ready-passion","card_class":"ニュートラル","card_type":"フォロワー","cost":"1","power":"2","hp":"20","traits":["デレマス","パッション"],"text":"","sections":[]}]}).to_string());snapshot.push('\n');
  snapshot.push_str(&json!({"number":"ready-triple","faces":[{"name":"ready-triple","card_class":"ニュートラル","card_type":"フォロワー","cost":"1","power":"2","hp":"20","traits":["デレマス","キュート","クール","パッション"],"text":"","sections":[]}]}).to_string());snapshot.push('\n');
  snapshot.push_str(&json!({"number":"ready-amulet","faces":[{"name":"ready-amulet","card_class":"ニュートラル","card_type":"アミュレット","cost":"1","power":"-","hp":"-","traits":["デレマス","クール"],"text":"","sections":[]}]}).to_string());snapshot.push('\n');
  snapshot.push_str(&json!({"number":"ready-cute-amulet","faces":[{"name":"ready-cute-amulet","card_class":"ニュートラル","card_type":"アミュレット","cost":"1","power":"-","hp":"-","traits":["デレマス","キュート"],"text":"","sections":[]}]}).to_string());snapshot.push('\n');
  snapshot.push_str(&json!({"number":"ready-wrong","faces":[{"name":"ready-wrong","card_class":"ニュートラル","card_type":"フォロワー","cost":"1","power":"2","hp":"20","traits":[],"text":"","sections":[]}]}).to_string());snapshot.push('\n');
  snapshot.push_str(&json!({"number":"ready-anastasia","faces":[{"name":"〔最小〕アナスタシア","card_class":"ニュートラル","card_type":"フォロワー","cost":"3","power":"2","hp":"20","traits":["デレマス"],"text":"","sections":[]}]}).to_string());snapshot.push('\n');
  snapshot.push_str(&json!({"number":"ready-miku","faces":[{"name":"〔最小〕前川みく","card_class":"ニュートラル","card_type":"フォロワー","cost":"1","power":"2","hp":"20","traits":["デレマス"],"text":"","sections":[]}]}).to_string());snapshot.push('\n');
  snapshot.push_str(&json!({"number":"ready-nagi","faces":[{"name":"〔最小〕久川凪","card_class":"ニュートラル","card_type":"フォロワー","cost":"1","power":"2","hp":"20","traits":["デレマス"],"text":"","sections":[]}]}).to_string());snapshot.push('\n');
  snapshot.push_str(&json!({"number":"ready-shuko","faces":[{"name":"〔最小〕塩見周子","card_class":"ニュートラル","card_type":"フォロワー","cost":"1","power":"2","hp":"20","traits":["デレマス"],"text":"","sections":[]}]}).to_string());snapshot.push('\n');
  snapshot.push_str(&json!({"number":"ready-shiki","faces":[{"name":"〔最小〕一ノ瀬志希","card_class":"ニュートラル","card_type":"フォロワー","cost":"1","power":"2","hp":"20","traits":["デレマス"],"text":"","sections":[]}]}).to_string());snapshot.push('\n');
  snapshot.push_str(&json!({"number":"ready-tokimeki","faces":[{"name":"トキメキ☆ホーダイ","card_class":"ニュートラル","card_type":"スペル","cost":"1","power":"-","hp":"-","traits":["デレマス"],"text":"","sections":[]}]}).to_string());snapshot.push('\n');
  snapshot.push_str(&json!({"number":"ready-spell","faces":[{"name":"ready-spell","card_class":"ニュートラル","card_type":"スペル","cost":"3","power":"-","hp":"-","traits":["デレマス"],"text":"","sections":[]}]}).to_string());snapshot.push('\n');
  snapshot.push_str(&json!({"number":"ready-passion2","faces":[{"name":"ready-passion2","card_class":"ニュートラル","card_type":"フォロワー","cost":"2","power":"2","hp":"20","traits":["デレマス","パッション"],"text":"","sections":[]}]}).to_string());snapshot.push('\n');
  snapshot.push_str(&json!({"number":"ready-base-ECP02-020","faces":[{"name":"〔想いプレゼント〕佐久間まゆ","card_class":"ニュートラル","card_type":"フォロワー","cost":"0","power":"2","hp":"20","traits":[],"text":"","sections":[]}]}).to_string());snapshot.push('\n');
  snapshot.push_str(&json!({"number":"ready-base-ECP02-P08","faces":[{"name":"〔想いプレゼント〕佐久間まゆ","card_class":"ニュートラル","card_type":"フォロワー","cost":"0","power":"2","hp":"20","traits":[],"text":"","sections":[]}]}).to_string());snapshot.push('\n');
  snapshot.push_str(&json!({"number":"ready-base-ECP02-031","faces":[{"name":"〔花舞うまほろば〕小早川紗枝","card_class":"ニュートラル","card_type":"フォロワー","cost":"0","power":"2","hp":"20","traits":[],"text":"","sections":[]}]}).to_string());snapshot.push('\n');
  snapshot.push_str(&json!({"number":"ready-base-ECP02-P15","faces":[{"name":"〔花舞うまほろば〕小早川紗枝","card_class":"ニュートラル","card_type":"フォロワー","cost":"0","power":"2","hp":"20","traits":[],"text":"","sections":[]}]}).to_string());snapshot.push('\n');
  snapshot.push_str(&json!({"number":"ready-base-ECP02-033","faces":[{"name":"〔瞳のトゥインクル〕関裕美","card_class":"ニュートラル","card_type":"フォロワー","cost":"0","power":"2","hp":"20","traits":[],"text":"","sections":[]}]}).to_string());snapshot.push('\n');
  snapshot.push_str(&json!({"number":"ready-base-ECP02-P17","faces":[{"name":"〔瞳のトゥインクル〕関裕美","card_class":"ニュートラル","card_type":"フォロワー","cost":"0","power":"2","hp":"20","traits":[],"text":"","sections":[]}]}).to_string());snapshot.push('\n');
  for row in snapshot.lines().map(|line|serde_json::from_str::<Value>(line).unwrap()).filter(|row|row["number"].as_str().unwrap().starts_with("ready-")) {
   let number=row["number"].as_str().unwrap();
   input["documents"].as_array_mut().unwrap().push(json!([format!("{number}.yaml"),json!({"format":1_u8,"kind":"effect_set","cards":{number:{"status":"complete","abilities":[]}}}).to_string()]));
  }
  repository::selection_with_rules_from(&snapshot,&keywords,&input)
 })
}
fn position(card: &str, zone: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    initial["players"]["P1"]["sep"] = json!(0_u8);
    initial["players"]["P1"]["zones"][zone] = json!([{"id":"source","card":card}]);
    initial["players"]["P1"]["zones"]["ex"] =
        json!([{"id":"item0","card":"CP02-T04"},{"id":"item1","card":"CP02-T04"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!(
        (0_u8..5)
            .map(|i| json!({"id":format!("draw{i}"),"card":"BP01-171"}))
            .collect::<Vec<_>>()
    );
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"ready-wrong"}]);
    initial
}
fn game(initial: &Value) -> Game {
    let mut game = Game::from_selected_effects(
        fixture().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "ready-ecp02-1",
    )
    .unwrap();
    load_companions(&mut game);
    game
}
fn finish(game: &mut Game, mut step: CardPlay, mode: u64) {
    while let CardPlay::Awaiting(question) = step {
        let answer = match &question.domain {
            InputDomain::Modes { .. } => Answer::Modes(if mode == 0 {
                vec![ModeId(1), ModeId(2)]
            } else {
                vec![ModeId(mode)]
            }),
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            InputDomain::OptionalCost => Answer::OptionalCost(mode == 1),
            InputDomain::Targets(domain) => Answer::Targets(
                domain
                    .members
                    .iter()
                    .take(if domain.cardinality.maximum == Some(Nat(2)) {
                        2
                    } else {
                        1
                    })
                    .map(|m| m.handle)
                    .collect(),
            ),
            InputDomain::CostMaterials(domain) => Answer::CostMaterials(
                domain
                    .members
                    .iter()
                    .take(usize::try_from(domain.cardinality.minimum.0).unwrap())
                    .map(|m| m.handle)
                    .collect(),
            ),
            InputDomain::Search(domain) => {
                assert_eq!(
                    domain.cards.len(),
                    1,
                    "only the single printed eligible deck card may be offered"
                );
                Answer::Search(SearchAnswer::Ungrouped(
                    domain.cards.iter().map(|m| m.handle).collect(),
                ))
            }
            InputDomain::Order(cards) => {
                Answer::Order(cards.iter().rev().map(|m| m.handle).collect())
            }
            InputDomain::Abilities(_)
            | InputDomain::ResourceEntryPayment { .. }
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
        step = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
}
fn settle(game: &mut Game, mode: u64) {
    loop {
        if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
        let Some(question) = game.internal_pending_choice().unwrap() else {
            break;
        };
        let InputDomain::Pending(options) = &question.domain else {
            panic!("pending ability");
        };
        let step = game
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap();
        finish(game, step, mode);
    }
}
fn play(game: &mut Game, mode: u64) {
    let step = game
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap();
    finish(game, step, mode);
    settle(game, mode);
}

fn load_companions(game: &mut Game) {
    let empty = WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![],
    })
    .unwrap();
    for number in [
        "ready-deremas",
        "ready-cute",
        "ready-cool",
        "ready-passion",
        "ready-triple",
        "ready-amulet",
        "ready-cute-amulet",
        "ready-wrong",
        "ready-anastasia",
        "ready-miku",
        "ready-nagi",
        "ready-shuko",
        "ready-shiki",
        "ready-passion2",
    ] {
        game.load_internal_card(number, &empty).unwrap();
    }
    let mut tree = expanded::spell_tree().clone();
    let mut spell = tree["cards"]["SD03-005"].clone();
    spell["abilities"][0]["spell"] = json!({"do":[]});
    tree["cards"] = json!({"ready-tokimeki":spell.clone(),"ready-spell":spell});
    let costs = BTreeMap::from([
        ("ready-tokimeki".to_owned(), Nat(1)),
        ("ready-spell".to_owned(), Nat(3)),
    ]);
    for (number, program) in lowerer().lower_with_printed_costs(&tree, &costs).unwrap() {
        game.load_internal_card(&number, &program).unwrap();
    }
}
fn native_activate(game: &mut Game, card: &str, index: usize) {
    let ability = fixture().current[card]
        .program
        .card()
        .abilities
        .iter()
        .filter(|a| {
            a.node.kind == AbilityKind::Activated
                && matches!(a.node.source.origin, AbilityOrigin::Printed { .. })
        })
        .nth(index)
        .unwrap()
        .node
        .id;
    let step = game
        .activate_internal_card("source", ability, PlayerId { seat: 1 })
        .unwrap();
    finish(game, step, 1);
    settle(game, 1);
}

fn keyword(state: &Value, id: &str, name: &str) -> bool {
    state["objects"][id]["keywords"]
        .as_array()
        .unwrap()
        .contains(&json!(name))
}
fn play_id(game: &mut Game, id: &str) {
    let step = game.play_internal_card(id, PlayerId { seat: 1 }).unwrap();
    finish(game, step, 1);
    settle(game, 1);
}
fn grave(initial: &mut Value, card: &str, count: u8) {
    initial["players"]["P1"]["zones"]["cemetery"] = json!(
        (0..count)
            .map(|i| json!({"id":format!("grave{i}"),"card":card}))
            .collect::<Vec<_>>()
    );
}
fn evolved(form: &str, mut initial: Value) -> Game {
    let base = format!("ready-base-{form}");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":base}]);
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
    let mut game = game(&initial);
    let mut tree = expanded::tree().clone();
    tree["cards"] = json!({base.clone():tree["cards"]["synthetic-evolution"]});
    let program = lowerer().lower(&tree).unwrap().remove(&base).unwrap();
    game.load_internal_card(&base, &program).unwrap();
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
        )
        .unwrap()
        .outcome,
        "resolved",
        "the ordinary fixture evolution must resolve"
    );
    settle(&mut game, 1);
    game
}
fn end_turn(game: &mut Game) {
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    settle(game, 1);
    while let Some(window) = game.internal_quick_choice().unwrap() {
        game.answer_internal_card(window.actor, window.token, Answer::Quick(None))
            .unwrap();
        settle(game, 1);
    }
}
#[test]
fn ecp02_004_searches_name_to_ex_discount_and_lesson_acts_an_enemy() {
    for card in ["ECP02-004", "ECP02-SL04"] {
        for already_acted in [false, true] {
            let mut initial = position(card, "hand");
            initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"found","card":"ready-anastasia"},{"id":"wrong","card":"ready-wrong"}]);
            initial["players"]["P2"]["zones"]["field"][0]["state"] = json!({"acted":already_acted});
            initial["players"]["P2"]["zones"]["deck"] = json!(
                (0_u8..4)
                    .map(|i| json!({"id":format!("opponent{i}"),"card":"ready-wrong"}))
                    .collect::<Vec<_>>()
            );
            let mut game = game(&initial);
            play(&mut game, 1);
            play_id(&mut game, "found");
            native_activate(&mut game, card, 0);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["P1"]["pp"]["current"], 7_u8);
            assert!(
                state["P1"]["field"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("found"))
            );
            assert_eq!(state["objects"]["enemy"]["acted"], true);
            assert_eq!(state["objects"]["source"]["acted"], true);
            assert_eq!(state["P1"]["ex"].as_array().unwrap().len(), 1);

            end_turn(&mut game);
            let first_start = game.projection(View::Referee).unwrap();
            assert_eq!(first_start["turn"]["active"], "P2");
            assert_eq!(
                first_start["objects"]["enemy"]["acted"], true,
                "the next opposing rule start cannot stand its target"
            );
            end_turn(&mut game);
            end_turn(&mut game);
            let later_start = game.projection(View::Referee).unwrap();
            assert_eq!(later_start["turn"]["active"], "P2");
            assert_eq!(
                later_start["objects"]["enemy"]["acted"], false,
                "the following opposing start stands it after expiry"
            );
        }
    }
}

#[test]
fn ecp02_010_storm_lastword_optionally_pays_lesson_and_finds_miku() {
    for card in ["ECP02-010", "ECP02-P05"] {
        let mut initial = position(card, "hand");
        initial["players"]["P1"]["zones"]["deck"] =
            json!([{"id":"found","card":"ready-miku"},{"id":"wrong","card":"ready-wrong"}]);
        initial["players"]["P2"]["zones"]["field"][0]["state"] = json!({"acted":true});
        let mut game = game(&initial);
        play(&mut game, 1);
        assert!(keyword(
            &game.projection(View::Referee).unwrap(),
            "source",
            "疾走"
        ));
        assert_eq!(
            game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"enemy"}))
                .unwrap()
                .outcome,
            "resolved"
        );
        while let Some(window) = game.internal_quick_choice().unwrap() {
            game.answer_internal_card(window.actor, window.token, Answer::Quick(None))
                .unwrap();
            settle(&mut game, 1);
        }
        settle(&mut game, 1);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["hand"], json!(["found"]));
        assert_eq!(state["P1"]["ex"].as_array().unwrap().len(), 1);
    }
}

#[test]
fn ecp02_020_evolution_uses_the_formal_targets_controllers_hand_count() {
    for card in ["ECP02-020", "ECP02-P08"] {
        let mut initial = position(card, "field");
        initial["players"]["P2"]["zones"]["hand"] = json!([{"id":"hand0","card":"ready-wrong"},{"id":"hand1","card":"ready-wrong"},{"id":"hand2","card":"ready-wrong"}]);
        let game = evolved(card, initial);
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
            3_u8
        );
    }
}

#[test]
fn ecp02_021_only_nonhand_entry_evolves_without_using_the_normal_evolution_quota() {
    for card in ["ECP02-021", "ECP02-P09"] {
        for zone in ["hand", "ex"] {
            let mut initial = position(card, zone);
            initial["players"]["P1"]["zones"]["deck"] =
                json!([{"id":"found","card":"ready-spell"},{"id":"wrong","card":"ready-wrong"}]);
            if zone == "ex" {
                initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"source","card":card}]);
            }
            initial["players"]["P1"]["zones"]["evolve_deck"] =
                json!([{"id":"form","card":"ECP02-022"}]);
            let mut game = game(&initial);
            play(&mut game, 1);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["objects"]["source"]["card"], card);
            assert_eq!(state["objects"]["source"]["evolved"], zone == "ex");
            assert_eq!(
                state["P1"]["evolution"].as_array().unwrap().len(),
                usize::from(zone == "ex")
            );
        }
    }
}

#[test]
fn ecp02_023_threshold_search_discount_and_ten_grave_sacrifice_recruits_nagi() {
    for card in ["ECP02-023", "ECP02-P11"] {
        let mut initial = position(card, "hand");
        grave(&mut initial, "ready-deremas", 10);
        initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"tokimeki","card":"ready-tokimeki"},{"id":"nagi","card":"ready-nagi"},{"id":"wrong","card":"ready-wrong"}]);
        let mut game = game(&initial);
        play(&mut game, 1);
        play_id(&mut game, "tokimeki");
        native_activate(&mut game, card, 0);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["pp"]["current"], 7_u8);
        assert!(
            state["P1"]["field"]
                .as_array()
                .unwrap()
                .contains(&json!("nagi"))
        );
        assert!(
            state["P1"]["cemetery"]
                .as_array()
                .unwrap()
                .contains(&json!("source"))
        );
    }
}

#[test]
fn ecp02_024_ward_discards_two_cute_cards_heals_draws_then_lesson_damages_two_targets() {
    for card in ["ECP02-024", "ECP02-P12"] {
        for old in [2_u8, 3] {
            let mut initial = position(card, "hand");
            grave(&mut initial, "ready-cute-amulet", old);
            initial["players"]["P1"]["zones"]["hand"]
                .as_array_mut()
                .unwrap()
                .extend([
                    json!({"id":"material0","card":"ready-cute"}),
                    json!({"id":"material1","card":"ready-cute-amulet"}),
                ]);
            initial["players"]["P2"]["zones"]["field"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":"enemy2","card":"ready-wrong"}));
            let mut game = game(&initial);
            play(&mut game, 1);
            native_activate(&mut game, card, 0);
            let state = game.projection(View::Referee).unwrap();
            assert!(keyword(&state, "source", "守護"));
            assert_eq!(state["P1"]["leader"]["life"], 12_u8);
            assert_eq!(state["P1"]["hand"], json!(["draw0", "draw1"]));
            for id in ["enemy", "enemy2"] {
                assert_eq!(
                    state["objects"][id]["damage"],
                    if old == 3 { 3_u8 } else { 2_u8 },
                    "both formal targets receive threshold damage"
                );
            }
        }
    }
}

#[test]
fn ecp02_026_optional_discard_draw_then_qualified_sacrifice_recruits_shuko() {
    for card in ["ECP02-026", "ECP02-SL13"] {
        let mut initial = position(card, "hand");
        grave(&mut initial, "ready-deremas", 9);
        initial["players"]["P1"]["zones"]["hand"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"material","card":"ready-deremas"}));
        initial["players"]["P1"]["zones"]["deck"] =
            json!([{"id":"drawn","card":"ready-wrong"},{"id":"found","card":"ready-shuko"}]);
        let mut game = game(&initial);
        play(&mut game, 1);
        native_activate(&mut game, card, 0);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["hand"], json!(["drawn"]));
        assert!(
            state["P1"]["field"]
                .as_array()
                .unwrap()
                .contains(&json!("found"))
        );
        assert!(
            state["P1"]["cemetery"]
                .as_array()
                .unwrap()
                .contains(&json!("source"))
        );
        assert_eq!(state["P1"]["pp"]["current"], 7_u8);
    }
}

#[test]
fn ecp02_027_both_spell_retrievals_are_discounted_and_activation_is_once_per_turn() {
    for card in ["ECP02-027", "ECP02-SL14"] {
        let mut initial = position(card, "hand");
        grave(&mut initial, "ready-passion", 10);
        initial["players"]["P1"]["zones"]["cemetery"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"recover","card":"ready-spell"}));
        initial["players"]["P1"]["zones"]["deck"] =
            json!([{"id":"found","card":"ready-spell"},{"id":"wrong","card":"ready-wrong"}]);
        let mut game = game(&initial);
        play(&mut game, 1);
        play_id(&mut game, "found");
        native_activate(&mut game, card, 0);
        let state = game.projection(View::Referee).unwrap();
        assert!(
            state["P1"]["ex"]
                .as_array()
                .unwrap()
                .contains(&json!("recover"))
        );
        play_id(&mut game, "recover");
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
            6_u8
        );
        let ability = fixture().current[card]
            .program
            .card()
            .abilities
            .iter()
            .find(|a| a.node.kind == AbilityKind::Activated)
            .unwrap()
            .node
            .id;
        game.activate_internal_card("source", ability, PlayerId { seat: 1 })
            .expect_err("the printed activation is limited to once per turn");
    }
}

#[test]
fn ecp02_031_one_triple_trait_card_satisfies_all_three_evolution_thresholds() {
    for card in ["ECP02-031", "ECP02-P15"] {
        for all in [false, true] {
            let mut initial = position(card, "field");
            grave(
                &mut initial,
                if all { "ready-triple" } else { "ready-cute" },
                1,
            );
            let game = evolved(card, initial);
            assert_eq!(
                game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
                if all { 4_u8 } else { 2_u8 }
            );
        }
    }
}

#[test]
fn ecp02_033_evolution_recovers_a_cute_amulet_excluding_a_non_cute_follower() {
    for card in ["ECP02-033", "ECP02-P17"] {
        let mut initial = position(card, "field");
        initial["players"]["P1"]["zones"]["cemetery"] = json!([{"id":"wrong","card":"ready-wrong"},{"id":"recover","card":"ready-cute-amulet"}]);
        let game = evolved(card, initial);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["hand"], json!(["recover"]));
        assert_eq!(state["P1"]["cemetery"], json!(["wrong"]));
    }
}

#[test]
fn ecp02_035_optional_lesson_searches_any_of_four_printed_name_substrings() {
    for card in ["ECP02-035", "ECP02-P19"] {
        let mut initial = position(card, "hand");
        initial["players"]["P1"]["zones"]["deck"] =
            json!([{"id":"found","card":"ready-shiki"},{"id":"wrong","card":"ready-wrong"}]);
        let mut game = game(&initial);
        play(&mut game, 1);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["hand"], json!(["found"]));
        assert_eq!(state["P1"]["ex"].as_array().unwrap().len(), 1);
    }
}

#[test]
fn ecp02_037_counts_itself_but_excludes_a_cool_amulet_from_three_followers() {
    for card in ["ECP02-037", "ECP02-SL17"] {
        for followers in [1_u8, 2] {
            let mut initial = position(card, "hand");
            let mut field = (0..followers)
                .map(|i| json!({"id":format!("ally{i}"),"card":"ready-cool"}))
                .collect::<Vec<_>>();
            field.push(json!({"id":"amulet","card":"ready-amulet"}));
            initial["players"]["P1"]["zones"]["field"] = json!(field);
            let mut game = game(&initial);
            play(&mut game, 1);
            assert_eq!(
                game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
                if followers == 2 { 19_u8 } else { 20_u8 }
            );
        }
    }
}
