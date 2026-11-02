#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate printed card scenarios."
)]
use super::*;
use sve_engine::game::internal::SearchAnswer;
fn fixture() -> &'static SelectedEffects {
    static SELECTED: OnceLock<SelectedEffects> = OnceLock::new();
    SELECTED.get_or_init(|| {
  let (mut snapshot,keywords,mut input)=producer_inputs();
  // Minimal companions isolate search filters, current costs, and ordinary evolution.
  for (number,name,kind,traits) in [
   ("ready-deremas","kagero companion","フォロワー",vec!["かげろう"]),
   ("ready-rin","ヴァンガード material","フォロワー",vec!["かげろう","シャドウパラディン"]),
   ("ready-mio","ブラスター・ブレード","フォロワー",vec![]),
   ("ready-amulet","plain amulet","アミュレット",vec![]),
  ] {
   snapshot.push_str(&json!({"number":number,"faces":[{"name":name,"card_class":"ニュートラル","card_type":kind,"cost":"4","power":if kind=="フォロワー" {"3"} else {"-"},"hp":if kind=="フォロワー" {"4"} else {"-"},"traits":traits,"text":null,"sections":[]}]}).to_string());snapshot.push('\n');
   input["documents"].as_array_mut().unwrap().push(json!([format!("{number}.yaml"),json!({"format":1_u8,"kind":"effect_set","cards":{number:{"status":"complete","abilities":[]}}}).to_string()]));
  }
  let form = "CP03-085";
   let name=snapshot.lines().map(|line|serde_json::from_str::<Value>(line).unwrap()).find(|row|row["number"]==form).unwrap()["faces"][0]["name"].clone();
   let number=format!("ready-base-{form}");
   snapshot.push_str(&json!({"number":number,"faces":[{"name":name,"card_class":"ニュートラル","card_type":"フォロワー","cost":"0","power":"3","hp":"4","traits":[],"text":null,"sections":[]}]}).to_string());snapshot.push('\n');
   input["documents"].as_array_mut().unwrap().push(json!([format!("{number}.yaml"),json!({"format":1_u8,"kind":"effect_set","cards":{number:{"status":"complete","abilities":[]}}}).to_string()]));
  repository::selection_with_rules_from(&snapshot,&keywords,&input)
 })
}
fn position(card: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    initial["players"]["P1"]["zones"]["deck"] = json!(
        (0_u8..5)
            .map(|i| json!({"id":format!("draw{i}"),"card":"BP01-171"}))
            .collect::<Vec<_>>()
    );
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    initial
}
fn load_companions(game: &mut Game) {
    let empty = WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![],
    })
    .unwrap();
    for number in ["ready-deremas", "ready-rin", "ready-mio", "ready-amulet"] {
        game.load_internal_card(number, &empty).unwrap();
    }
}
fn game(initial: &Value) -> Game {
    let mut game = Game::from_selected_effects(
        fixture().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "ready-cp03-2",
    )
    .unwrap();
    load_companions(&mut game);
    game
}
fn finish(game: &mut Game, mut step: CardPlay, mode: u64) {
    while let CardPlay::Awaiting(question) = step {
        let answer = match &question.domain {
            InputDomain::Modes { .. } => Answer::Modes(vec![ModeId(mode)]),
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            InputDomain::OptionalCost => Answer::OptionalCost(mode != 3),
            InputDomain::Targets(domain) => Answer::Targets(
                domain
                    .members
                    .iter()
                    .take(if mode == 0 {
                        0
                    } else {
                        usize::try_from(
                            domain
                                .cardinality
                                .maximum
                                .unwrap_or(domain.cardinality.minimum)
                                .0,
                        )
                        .unwrap()
                    })
                    .map(|m| m.handle)
                    .collect(),
            ),
            InputDomain::Search(domain) => Answer::Search(SearchAnswer::Ungrouped(
                domain
                    .cards
                    .first()
                    .map(|m| vec![m.handle])
                    .unwrap_or_default(),
            )),
            InputDomain::Order(cards) => {
                Answer::Order(cards.iter().rev().map(|m| m.handle).collect())
            }
            InputDomain::CostMaterials(domain) => Answer::CostMaterials(
                domain
                    .members
                    .iter()
                    .take(usize::try_from(domain.cardinality.minimum.0).unwrap())
                    .map(|m| m.handle)
                    .collect(),
            ),
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
            panic!("pending");
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
fn play(game: &mut Game, id: &str, mode: u64) {
    let step = game.play_internal_card(id, PlayerId { seat: 1 }).unwrap();
    finish(game, step, mode);
    settle(game, mode);
}
fn evolution_position(form: &str) -> Value {
    let mut initial = position(form);
    let base = format!("ready-base-{}", fixture().current[form].card_no);
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":base}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
    initial
}
fn prepare_evolution(initial: &Value, form: &str) -> Game {
    let base = format!("ready-base-{}", fixture().current[form].card_no);
    let mut game = game(initial);
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
    game
}
fn evolved(initial: &Value, form: &str) -> Game {
    let mut game = prepare_evolution(initial, form);
    settle(&mut game, 1);
    game
}
fn native_activate(game: &mut Game, card: &str, index: usize) {
    let ability = fixture().current[card]
        .program
        .card()
        .abilities
        .iter()
        .filter(|a| a.node.kind == AbilityKind::Activated)
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
fn has_keyword(state: &Value, object: &str, keyword: &str) -> bool {
    state["objects"][object]["keywords"]
        .as_array()
        .unwrap()
        .contains(&json!(keyword))
}

#[test]
fn cp03_056_top_banish_counts_itself_before_the_five_card_threshold() {
    for card in ["CP03-056", "CP03-P39"] {
        for count in [3_u8, 4] {
            let mut initial = position(card);
            initial["players"]["P1"]["zones"]["banish"] = json!(
                (0..count)
                    .map(|i| json!({"id":format!("old{i}"),"card":"BP01-171"}))
                    .collect::<Vec<_>>()
            );
            let mut game = game(&initial);
            play(&mut game, "source", 1);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["P1"]["banish"].as_array().unwrap().len(),
                usize::from(count) + 1
            );
            assert_eq!(
                state["P2"]["leader"]["life"],
                if count == 4 { 18_u8 } else { 20_u8 }
            );
        }
    }
}

#[test]
fn cp03_059_ward_fanfare_heals_and_banishes_the_real_top_card() {
    for card in ["CP03-059", "CP03-P42"] {
        let mut initial = position(card);
        initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
        let mut game = game(&initial);
        play(&mut game, "source", 1);
        let state = game.projection(View::Referee).unwrap();
        assert!(has_keyword(&state, "source", "守護"));
        assert_eq!(state["P1"]["leader"]["life"], 12_u8);
        assert_eq!(state["P1"]["banish"], json!(["draw0"]));
        assert_eq!(state["P1"]["deck"].as_array().unwrap().len(), 4);
    }
}

#[test]
fn cp03_068_rush_discards_kagero_before_drawing_two() {
    for card in ["CP03-068", "CP03-P46"] {
        let mut initial = position(card);
        initial["players"]["P1"]["zones"]["hand"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"material","card":"ready-deremas"}));
        let mut game = game(&initial);
        play(&mut game, "source", 1);
        let state = game.projection(View::Referee).unwrap();
        assert!(has_keyword(&state, "source", "突進"));
        assert_eq!(state["P1"]["cemetery"], json!(["material"]));
        assert_eq!(state["P1"]["hand"], json!(["draw0", "draw1"]));
    }
}

#[test]
fn cp03_069_banishes_three_grave_materials_then_destroys_a_selected_enemy() {
    for card in ["CP03-069", "CP03-P47"] {
        let mut initial = position(card);
        initial["players"]["P1"]["zones"]["cemetery"] = json!([{"id":"wrong","card":"BP01-171"},{"id":"a","card":"ready-deremas"},{"id":"b","card":"ready-deremas"},{"id":"c","card":"ready-deremas"}]);
        let mut game = game(&initial);
        play(&mut game, "source", 1);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["banish"], json!(["a", "b", "c"]));
        assert_eq!(state["P1"]["cemetery"], json!(["wrong"]));
        assert_eq!(state["P2"]["cemetery"], json!(["enemy"]));
    }
}

#[test]
fn cp03_074_can_destroy_an_amulet_or_damage_the_opposing_leader() {
    for card in ["CP03-074", "CP03-P51"] {
        for mode in [1_u64, 2] {
            let mut initial = position(card);
            initial["players"]["P2"]["zones"]["field"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":"amulet","card":"ready-amulet"}));
            let mut game = game(&initial);
            play(&mut game, "source", mode);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["P2"]["cemetery"],
                if mode == 1 {
                    json!(["amulet"])
                } else {
                    json!([])
                }
            );
            assert_eq!(
                state["P2"]["leader"]["life"],
                if mode == 1 { 20_u8 } else { 18_u8 }
            );
        }
    }
}

#[test]
fn cp03_078_requires_another_kagero_then_stands_but_cannot_attack() {
    for card in ["CP03-078", "CP03-P55", "CSD03b-014"] {
        let mut initial = position(card);
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] =
            json!([{"id":"source","card":card},{"id":"ally","card":"ready-deremas"}]);
        initial["players"]["P2"]["zones"]["field"][0]["state"] = json!({"acted":true});
        let mut game = game(&initial);
        native_activate(&mut game, card, 0);
        let acted = game.projection(View::Referee).unwrap();
        assert_eq!(acted["objects"]["source"]["acted"], true);
        assert_eq!(acted["objects"]["enemy"]["damage"], 1_u8);
        native_activate(&mut game, card, 1);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["source"]["acted"], false);
        assert_eq!(state["P1"]["pp"]["current"], 8_u8);
        for target in ["P2.leader", "enemy"] {
            assert_eq!(
                game.internal_rule_action(
                    &json!({"do":"attack","attacker":"source","target":target})
                )
                .unwrap()
                .outcome,
                "cannot-attack"
            );
        }
    }
}

#[test]
fn cp03_079_ward_heals_one_or_two_at_the_awakened_boundary() {
    for card in ["CP03-079", "CP03-P56", "CSD03b-015"] {
        for maximum in [6_u8, 7] {
            let mut initial = position(card);
            initial["players"]["P1"]["pp"] = json!({"current":maximum,"max":maximum});
            initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
            let mut game = game(&initial);
            play(&mut game, "source", 1);
            let state = game.projection(View::Referee).unwrap();
            assert!(has_keyword(&state, "source", "守護"));
            assert_eq!(
                state["P1"]["leader"]["life"],
                if maximum == 6 { 11_u8 } else { 12_u8 }
            );
        }
    }
}

#[test]
fn cp03_085_evolution_destroys_only_when_ten_shadow_paladin_cards_are_in_the_grave() {
    for card in ["CP03-085", "CP03-SL17"] {
        for count in [9_u8, 10] {
            let mut initial = evolution_position(card);
            initial["players"]["P1"]["zones"]["cemetery"] = json!(
                (0..count)
                    .map(|i| json!({"id":format!("old{i}"),"card":"ready-rin"}))
                    .collect::<Vec<_>>()
            );
            let game = evolved(&initial, card);
            let state = game.projection(View::Referee).unwrap();
            assert!(has_keyword(&state, "source", "疾走"));
            assert!(has_keyword(&state, "source", "ツインドライブ"));
            assert_eq!(
                state["P2"]["cemetery"],
                if count == 10 {
                    json!(["enemy"])
                } else {
                    json!([])
                }
            );
        }
    }
}

#[test]
fn cp03_074_cannot_choose_the_amulet_mode_without_an_amulet() {
    for card in ["CP03-074", "CP03-P51"] {
        let mut game = game(&position(card));
        let entered = game
            .play_internal_card("source", PlayerId { seat: 1 })
            .unwrap();
        finish(&mut game, entered, 2);
        if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
        let pending = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = &pending.domain else {
            panic!("pending fanfare");
        };
        let declaration = game
            .answer_internal_card(
                pending.actor,
                pending.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap();
        let CardPlay::Awaiting(question) = declaration else {
            panic!("printed choice");
        };
        let InputDomain::Modes { modes, .. } = &question.domain else {
            panic!("mode choice");
        };
        assert_eq!(modes, &vec![ModeId(2)]);
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Modes(vec![ModeId(1)]),
        )
        .unwrap_err();
        let resolved = game
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Modes(vec![ModeId(2)]),
            )
            .unwrap();
        finish(&mut game, resolved, 2);
        settle(&mut game, 2);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
            18_u8
        );
    }
}

#[test]
fn cp03_078_cannot_pay_act_without_another_kagero_follower() {
    for card in ["CP03-078", "CP03-P55", "CSD03b-014"] {
        let mut initial = position(card);
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] =
            json!([{"id":"source","card":card},{"id":"wrong","card":"ready-mio"}]);
        let mut game = game(&initial);
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
            .unwrap_err();
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["source"]["acted"], false);
        assert_eq!(state["objects"]["enemy"]["damage"], 0_u8);
    }
}

#[test]
fn cp03_069_can_select_two_enemies_or_no_enemy_after_paying_three_materials() {
    for card in ["CP03-069", "CP03-P47"] {
        for selected in [0_u8, 2] {
            let mut initial = position(card);
            initial["players"]["P1"]["zones"]["cemetery"] = json!([{"id":"a","card":"ready-deremas"},{"id":"b","card":"ready-deremas"},{"id":"c","card":"ready-deremas"}]);
            initial["players"]["P2"]["zones"]["field"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":"enemy2","card":"BP01-037"}));
            let mut game = game(&initial);
            play(&mut game, "source", u64::from(selected != 0));
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["P1"]["banish"], json!(["a", "b", "c"]));
            assert_eq!(
                state["P2"]["cemetery"].as_array().unwrap().len(),
                usize::from(selected)
            );
            assert_eq!(
                state["P2"]["field"].as_array().unwrap().len(),
                2 - usize::from(selected)
            );
        }
    }
}

#[test]
fn cp03_068_refusing_or_lacking_kagero_material_never_draws() {
    for card in ["CP03-068", "CP03-P46"] {
        for has_material in [false, true] {
            let mut initial = position(card);
            initial["players"]["P1"]["zones"]["hand"].as_array_mut().unwrap().push(json!({"id":"material","card":if has_material {"ready-deremas"} else {"ready-mio"}}));
            let mut game = game(&initial);
            play(&mut game, "source", if has_material { 3 } else { 1 });
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["P1"]["hand"], json!(["material"]));
            assert_eq!(state["P1"]["cemetery"], json!([]));
            assert_eq!(state["P1"]["deck"].as_array().unwrap().len(), 5);
        }
    }
}

#[test]
fn cp03_069_cannot_pay_with_two_kagero_and_a_wrong_trait_card() {
    for card in ["CP03-069", "CP03-P47"] {
        let mut initial = position(card);
        initial["players"]["P1"]["zones"]["cemetery"] = json!([{"id":"a","card":"ready-deremas"},{"id":"b","card":"ready-deremas"},{"id":"wrong","card":"BP01-171"}]);
        let mut game = game(&initial);
        play(&mut game, "source", 1);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["cemetery"], json!(["a", "b", "wrong"]));
        assert_eq!(state["P1"]["banish"], json!([]));
        assert_eq!(state["P2"]["field"], json!(["enemy"]));
    }
}

#[test]
fn cp03_078_already_standing_can_pay_two_and_still_cannot_attack() {
    for card in ["CP03-078", "CP03-P55", "CSD03b-014"] {
        let mut initial = position(card);
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":card}]);
        initial["players"]["P2"]["zones"]["field"][0]["state"] = json!({"acted":true});
        let mut game = game(&initial);
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["source"]["acted"],
            false
        );
        native_activate(&mut game, card, 1);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["source"]["acted"], false);
        assert_eq!(state["P1"]["pp"]["current"], 8_u8);
        for target in ["P2.leader", "enemy"] {
            assert_eq!(
                game.internal_rule_action(
                    &json!({"do":"attack","attacker":"source","target":target})
                )
                .unwrap()
                .outcome,
                "cannot-attack"
            );
        }
    }
}

#[test]
fn cp03_085_evolution_without_an_enemy_keeps_the_printed_keywords() {
    for card in ["CP03-085", "CP03-SL17"] {
        let mut initial = evolution_position(card);
        initial["players"]["P2"]["zones"]["field"] = json!([]);
        let game = evolved(&initial, card);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["evolution"], json!(["form"]));
        assert_eq!(state["P1"]["field"], json!(["source"]));
        assert!(has_keyword(&state, "source", "疾走"));
        assert!(has_keyword(&state, "source", "ツインドライブ"));
    }
}
