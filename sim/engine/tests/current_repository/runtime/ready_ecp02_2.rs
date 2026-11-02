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
  snapshot.push_str(&json!({"number":"ready-cool","faces":[{"name":"ready-cool","card_class":"ニュートラル","card_type":"フォロワー","cost":"1","power":"2","hp":"20","traits":["デレマス","クール"],"text":"","sections":[]}]}).to_string());snapshot.push('\n');
  snapshot.push_str(&json!({"number":"ready-amulet","faces":[{"name":"ready-amulet","card_class":"ニュートラル","card_type":"アミュレット","cost":"1","power":"-","hp":"-","traits":["デレマス","クール"],"text":"","sections":[]}]}).to_string());snapshot.push('\n');
  snapshot.push_str(&json!({"number":"ready-wrong","faces":[{"name":"ready-wrong","card_class":"ニュートラル","card_type":"フォロワー","cost":"1","power":"2","hp":"20","traits":[],"text":"","sections":[]}]}).to_string());snapshot.push('\n');
  snapshot.push_str(&json!({"number":"ready-anastasia","faces":[{"name":"〔最小〕アナスタシア","card_class":"ニュートラル","card_type":"フォロワー","cost":"3","power":"2","hp":"20","traits":["デレマス"],"text":"","sections":[]}]}).to_string());snapshot.push('\n');
  snapshot.push_str(&json!({"number":"ready-tsukasa","faces":[{"name":"〔最小〕桐生つかさ","card_class":"ニュートラル","card_type":"フォロワー","cost":"3","power":"2","hp":"20","traits":["デレマス"],"text":"","sections":[]}]}).to_string());snapshot.push('\n');
  snapshot.push_str(&json!({"number":"ready-low","faces":[{"name":"ready-low","card_class":"ニュートラル","card_type":"フォロワー","cost":"2","power":"2","hp":"20","traits":["デレマス"],"text":"","sections":[]}]}).to_string());snapshot.push('\n');
  snapshot.push_str(&json!({"number":"ready-lastdaylight","faces":[{"name":"ラストデイライト","card_class":"ニュートラル","card_type":"スペル","cost":"2","power":"-","hp":"-","traits":["デレマス"],"text":"","sections":[]}]}).to_string());snapshot.push('\n');
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
        "ready-ecp02-2",
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
                        "ready-tsukasa",
                        "ready-lastdaylight",
                        "ready-deremas"
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
        "ready-cool",
        "ready-deremas",
        "ready-low",
        "ready-tsukasa",
        "ready-wrong",
    ] {
        game.load_internal_card(number, &empty).unwrap();
    }
    let mut tree = expanded::spell_tree().clone();
    let mut spell = tree["cards"]["SD03-005"].clone();
    spell["abilities"][0]["spell"] = json!({"do":[]});
    tree["cards"] = json!({"ready-lastdaylight":spell});
    let costs = BTreeMap::from([("ready-lastdaylight".to_owned(), Nat(2))]);
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
#[test]
fn ecp02_046_optional_extra_pp_recruits_tsukasa_only_when_paid_and_requires_a_target() {
    for card in ["ECP02-046", "ECP02-P25"] {
        for paid in [false, true] {
            let mut initial = position(card, "hand");
            initial["players"]["P1"]["zones"]["deck"] =
                json!([{"id":"found","card":"ready-tsukasa"},{"id":"wrong","card":"ready-wrong"}]);
            let mut game = game(&initial);
            play(&mut game, if paid { 1 } else { 2 });
            let state = game.projection(View::Referee).unwrap();
            assert!(
                state["P2"]["cemetery"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("enemy"))
            );
            assert_eq!(
                state["P1"]["field"],
                if paid { json!(["found"]) } else { json!([]) }
            );
            assert_eq!(state["P1"]["pp"]["current"], if paid { 4_u8 } else { 7_u8 });
        }
        let mut absent = position(card, "hand");
        absent["players"]["P2"]["zones"]["field"] = json!([]);
        for consent in [false, true] {
            let mut rejected = game(&absent);
            let before = rejected.projection(View::Referee).unwrap();
            let CardPlay::Awaiting(declaration) = rejected
                .play_internal_card("source", PlayerId { seat: 1 })
                .unwrap()
            else {
                panic!("optional payment declaration")
            };
            assert!(matches!(declaration.domain, InputDomain::OptionalCost));
            rejected
                .answer_internal_card(
                    declaration.actor,
                    declaration.token,
                    Answer::OptionalCost(consent),
                )
                .expect_err(
                    "a missing formal target prevents committing either payment declaration",
                );
            assert_eq!(
                rejected.projection(View::Referee).unwrap(),
                before,
                "failed commit restores the whole play origin"
            );
            assert_eq!(before["P1"]["pp"]["current"], 10_u8);
            assert_eq!(before["P1"]["hand"], json!(["source"]));
        }
    }
}

#[test]
fn ecp02_048_mills_then_pays_lesson_two_to_discount_a_low_cost_grave_follower() {
    for card in ["ECP02-048", "ECP02-SL23"] {
        let mut initial = position(card, "hand");
        initial["players"]["P1"]["zones"]["cemetery"] =
            json!([{"id":"wrong","card":"ready-anastasia"},{"id":"recover","card":"ready-low"}]);
        let mut game = game(&initial);
        play(&mut game, 1);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["cemetery"],
            json!(["wrong", "recover", "draw0", "draw1"])
        );
        native_activate(&mut game, card, 0);
        play_id(&mut game, "recover");
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["field"], json!(["source", "recover"]));
        assert_eq!(state["P1"]["pp"]["current"], 7_u8);
        assert_eq!(state["P1"]["ex"], json!([]));
        assert_eq!(state["objects"]["source"]["acted"], true);
    }
}

#[test]
fn ecp02_049_its_real_mill_crosses_the_grave_threshold_then_storm_attack_damages() {
    for card in ["ECP02-049", "ECP02-SL24"] {
        let mut initial = position(card, "hand");
        grave(&mut initial, "ready-deremas", 8);
        initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"mill0","card":"ready-deremas"},{"id":"mill1","card":"ready-deremas"},{"id":"tail","card":"ready-wrong"}]);
        initial["players"]["P2"]["zones"]["field"][0]["state"] =
            json!({"power":0_u8,"hp":20_u8,"max_hp":20_u8,"acted":true});
        let mut game = game(&initial);
        play(&mut game, 1);
        let entered = game.projection(View::Referee).unwrap();
        assert!(keyword(&entered, "source", "疾走"));
        assert_eq!(entered["P1"]["cemetery"].as_array().unwrap().len(), 10);
        assert_eq!(
            game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"enemy"}))
                .unwrap()
                .outcome,
            "resolved"
        );
        settle(&mut game, 1);
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
            2_u8,
            "attack trigger happens before combat"
        );
        while let Some(window) = game.internal_quick_choice().unwrap() {
            game.answer_internal_card(window.actor, window.token, Answer::Quick(None))
                .unwrap();
            settle(&mut game, 1);
        }
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
            4_u8,
            "combat adds its own two damage"
        );
    }
}

#[test]
fn ecp02_058_excludes_its_own_name_revives_only_with_three_cool_followers_and_discounts_named_spell()
 {
    for card in ["ECP02-058", "ECP02-P31"] {
        for allies in [1_u8, 2] {
            let mut initial = position(card, "hand");
            initial["players"]["P1"]["zones"]["field"] = json!(
                (0..allies)
                    .map(|i| json!({"id":format!("ally{i}"),"card":"ready-cool"}))
                    .collect::<Vec<_>>()
            );
            initial["players"]["P1"]["zones"]["cemetery"] = json!([{"id":"same","card":card},{"id":"wrong","card":"ready-wrong"},{"id":"recover","card":"ready-cool"}]);
            initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"found","card":"ready-lastdaylight"},{"id":"wrong-deck","card":"ready-wrong"}]);
            let mut game = game(&initial);
            play(&mut game, 1);
            let entered = game.projection(View::Referee).unwrap();
            assert_eq!(
                entered["P1"]["field"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("recover")),
                allies == 2
            );
            assert!(
                entered["P1"]["cemetery"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("same"))
            );
            native_activate(&mut game, card, 0);
            play_id(&mut game, "found");
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["P1"]["pp"]["current"], 6_u8);
            assert_eq!(state["objects"]["source"]["acted"], true);
            assert!(
                state["P1"]["cemetery"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("found"))
            );
        }
    }
}

#[test]
fn ecp02_062_ward_fanfare_uses_the_threshold_and_lesson_discard_activation_destroys() {
    for card in ["ECP02-062", "ECP02-SL30"] {
        for old in [9_u8, 10] {
            let mut initial = position(card, "hand");
            grave(&mut initial, "ready-deremas", old);
            initial["players"]["P1"]["zones"]["hand"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":"material","card":"ready-wrong"}));
            if old == 10 {
                initial["players"]["P2"]["zones"]["field"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!({"id":"enemy2","card":"ready-wrong"}));
            }
            let mut game = game(&initial);
            play(&mut game, 1);
            let entered = game.projection(View::Referee).unwrap();
            assert!(keyword(&entered, "source", "守護"));
            assert_eq!(
                entered["P2"]["cemetery"].as_array().unwrap().len(),
                usize::from(old == 10)
            );
            native_activate(&mut game, card, 0);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["P2"]["field"], json!([]));
            assert_eq!(
                state["P2"]["cemetery"].as_array().unwrap().len(),
                if old == 10 { 2 } else { 1 }
            );
            assert!(
                state["P1"]["cemetery"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("material"))
            );
            assert_eq!(state["objects"]["source"]["acted"], true);
            assert_eq!(state["P1"]["ex"].as_array().unwrap().len(), 1);
        }
    }
}

#[test]
fn ecp02_064_fanfare_and_lesson_activation_search_only_original_one_cost_deremas_followers() {
    for card in ["ECP02-064", "ECP02-SL32"] {
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
