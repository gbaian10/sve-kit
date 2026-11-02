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
  snapshot.push_str(&json!({"number":"ready-passion","faces":[{"name":"ready-passion","card_class":"ニュートラル","card_type":"フォロワー","cost":"1","power":"2","hp":"20","traits":["デレマス","パッション"],"text":"","sections":[]}]}).to_string());snapshot.push('\n');
  snapshot.push_str(&json!({"number":"ready-amulet","faces":[{"name":"ready-amulet","card_class":"ニュートラル","card_type":"アミュレット","cost":"1","power":"-","hp":"-","traits":["デレマス","クール"],"text":"","sections":[]}]}).to_string());snapshot.push('\n');
  snapshot.push_str(&json!({"number":"ready-wrong","faces":[{"name":"ready-wrong","card_class":"ニュートラル","card_type":"フォロワー","cost":"1","power":"2","hp":"20","traits":[],"text":"","sections":[]}]}).to_string());snapshot.push('\n');
  snapshot.push_str(&json!({"number":"ready-anastasia","faces":[{"name":"〔最小〕アナスタシア","card_class":"ニュートラル","card_type":"フォロワー","cost":"3","power":"2","hp":"20","traits":["デレマス"],"text":"","sections":[]}]}).to_string());snapshot.push('\n');
  snapshot.push_str(&json!({"number":"ready-spell","faces":[{"name":"ready-spell","card_class":"ニュートラル","card_type":"スペル","cost":"3","power":"-","hp":"-","traits":["デレマス"],"text":"","sections":[]}]}).to_string());snapshot.push('\n');
  snapshot.push_str(&json!({"number":"ready-low","faces":[{"name":"ready-low","card_class":"ニュートラル","card_type":"フォロワー","cost":"2","power":"2","hp":"20","traits":["デレマス"],"text":"","sections":[]}]}).to_string());snapshot.push('\n');
  snapshot.push_str(&json!({"number":"ready-sato","faces":[{"name":"〔最小〕佐藤心","card_class":"ニュートラル","card_type":"フォロワー","cost":"3","power":"2","hp":"20","traits":["デレマス"],"text":"","sections":[]}]}).to_string());snapshot.push('\n');
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
        "ready-ecp02-3",
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
                assert!(
                    domain.cards.iter().all(|c| [
                        "ready-deremas",
                        "ready-anastasia",
                        "ready-spell",
                        "ready-sato"
                    ]
                    .contains(&c.card.as_str())),
                    "search must exclude wrong names, traits, kinds and original costs"
                );
                Answer::Search(SearchAnswer::Ungrouped(
                    domain
                        .cards
                        .first()
                        .map(|c| vec![c.handle])
                        .unwrap_or_default(),
                ))
            }
            InputDomain::Order(cards) => {
                Answer::Order(cards.iter().rev().map(|m| m.handle).collect())
            }
            InputDomain::ResourceEntryPayment { .. }
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
        "ready-amulet",
        "ready-anastasia",
        "ready-deremas",
        "ready-low",
        "ready-passion",
        "ready-sato",
        "ready-wrong",
    ] {
        game.load_internal_card(number, &empty).unwrap();
    }
    let mut tree = expanded::spell_tree().clone();
    let mut spell = tree["cards"]["SD03-005"].clone();
    spell["abilities"][0]["spell"] = json!({"do":[]});
    tree["cards"] = json!({"ready-spell":spell});
    let costs = BTreeMap::from([("ready-spell".to_owned(), Nat(3))]);
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
fn ecp02_sp01_searches_name_to_ex_discount_and_lesson_acts_an_enemy() {
    for card in ["ECP02-SP01", "ECP02-U01"] {
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
fn ecp02_sp05_both_spell_retrievals_are_discounted_and_activation_is_once_per_turn() {
    for card in ["ECP02-SP05a", "ECP02-SP05b", "ECP02-U05a", "ECP02-U05b"] {
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
fn ecp02_sp12_fanfare_and_lesson_activation_search_only_original_one_cost_deremas_followers() {
    for card in ["ECP02-SP12", "ECP02-U12"] {
        let mut initial = position(card, "hand");
        initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"found0","card":"ready-deremas"},{"id":"found1","card":"ready-deremas"},{"id":"expensive","card":"ready-low"},{"id":"amulet","card":"ready-amulet"},{"id":"wrong","card":"ready-wrong"}]);
        let mut game = game(&initial);
        play(&mut game, 1);
        native_activate(&mut game, card, 0);
        let state = game.projection(View::Referee).unwrap();
        let field = state["P1"]["field"].as_array().unwrap();
        assert_eq!(field.len(), 3);
        assert!(field.contains(&json!("found0")) && field.contains(&json!("found1")));
        assert_eq!(state["P1"]["pp"]["current"], 6_u8);
        assert_eq!(state["objects"]["source"]["acted"], true);
        assert_eq!(state["P1"]["ex"].as_array().unwrap().len(), 1);
    }
}

#[test]
fn ecp02_071_formal_target_precedes_conditional_damage_heal_and_qualified_activation() {
    for card in ["ECP02-071", "ECP02-P39"] {
        for ally in [false, true] {
            let mut initial = position(card, "hand");
            grave(&mut initial, "ready-passion", 10);
            if ally {
                initial["players"]["P1"]["zones"]["field"] =
                    json!([{"id":"ally","card":"ready-deremas"}]);
            }
            initial["players"]["P1"]["zones"]["deck"] =
                json!([{"id":"found","card":"ready-sato"},{"id":"wrong","card":"ready-wrong"}]);
            let mut game = game(&initial);
            play(&mut game, 1);
            let entered = game.projection(View::Referee).unwrap();
            assert_eq!(
                entered["objects"]["enemy"]["damage"],
                if ally { 4_u8 } else { 0_u8 }
            );
            assert_eq!(
                entered["P1"]["leader"]["life"],
                if ally { 12_u8 } else { 10_u8 }
            );
            native_activate(&mut game, card, 0);
            let state = game.projection(View::Referee).unwrap();
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
            assert_eq!(state["P1"]["pp"]["current"], 6_u8);
        }
        let mut absent = position(card, "hand");
        absent["players"]["P1"]["zones"]["field"] = json!([{"id":"ally","card":"ready-deremas"}]);
        absent["players"]["P2"]["zones"]["field"] = json!([]);
        let mut unplayed = game(&absent);
        play(&mut unplayed, 1);
        assert_eq!(
            unplayed.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
            10_u8,
            "no formal target means no fanfare heal"
        );
    }
}
