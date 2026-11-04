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
   ("ready-deremas","ブラスター・ダーク","フォロワー",vec!["シャドウパラディン"]),
   ("ready-rin","ヴァンガード material","フォロワー",vec!["ヴァンガード","シャドウパラディン","オラクルシンクタンク"]),
   ("ready-mio","ファントム・ブラスター・ドラゴン","フォロワー",vec![]),
   ("ready-amulet","plain amulet","アミュレット",vec![]),
  ] {
   snapshot.push_str(&json!({"number":number,"faces":[{"name":name,"card_class":"ニュートラル","card_type":kind,"cost":"4","power":if kind=="フォロワー" {"3"} else {"-"},"hp":if number=="ready-mio" {"6"} else if kind=="フォロワー" {"4"} else {"-"},"traits":traits,"text":null,"sections":[]}]}).to_string());snapshot.push('\n');
   input["documents"].as_array_mut().unwrap().push(json!([format!("{number}.yaml"),json!({"format":1_u8,"kind":"effect_set","cards":{number:{"status":"complete","abilities":[]}}}).to_string()]));
  }
  let form = "CP03-109";
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
        "ready-cp03-3",
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
                if mode == 2 {
                    domain.members.last()
                } else {
                    domain.members.first()
                }
                .map(|m| vec![m.handle])
                .unwrap_or_default(),
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
fn has_keyword(state: &Value, object: &str, keyword: &str) -> bool {
    state["objects"][object]["keywords"]
        .as_array()
        .unwrap()
        .contains(&json!(keyword))
}

#[test]
fn cp03_095_discards_vanguard_and_searches_one_of_two_printed_names() {
    for card in ["CP03-095", "CP03-P64"] {
        for searched in ["ready-mio", "ready-deremas"] {
            let mut initial = position(card);
            initial["players"]["P1"]["zones"]["hand"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":"material","card":"ready-rin"}));
            initial["players"]["P1"]["zones"]["deck"] =
                json!([{"id":"wrong","card":"BP01-171"},{"id":"found","card":searched}]);
            let mut game = game(&initial);
            play(&mut game, "source", 1);
            let state = game.projection(View::Referee).unwrap();
            assert!(has_keyword(&state, "source", "突進"));
            assert_eq!(state["P1"]["hand"], json!(["found"]));
            assert_eq!(state["P1"]["cemetery"], json!(["material"]));
        }
    }
}

#[test]
fn cp03_098_pays_a_hand_card_then_returns_the_previously_selected_grave_follower() {
    for card in ["CP03-098", "CP03-P67"] {
        let mut initial = position(card);
        initial["players"]["P1"]["zones"]["hand"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"material","card":"ready-rin"}));
        initial["players"]["P1"]["zones"]["cemetery"] =
            json!([{"id":"wrong","card":"BP01-171"},{"id":"found","card":"ready-deremas"}]);
        let mut game = game(&initial);
        play(&mut game, "source", 1);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["hand"], json!(["found"]));
        assert_eq!(state["P1"]["cemetery"], json!(["wrong", "material"]));
    }
}

#[test]
fn cp03_099_stands_a_shadow_paladin_at_its_own_end_only_with_ten_grave_cards() {
    for card in ["CP03-099", "CP03-P68"] {
        for count in [9_u8, 10] {
            let mut initial = position(card);
            initial["players"]["P1"]["zones"]["hand"] = json!([]);
            initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":card,"state":{"acted":true}},{"id":"ally","card":"ready-deremas","state":{"acted":true}}]);
            initial["players"]["P1"]["zones"]["cemetery"] = json!(
                (0..count)
                    .map(|i| json!({"id":format!("old{i}"),"card":"ready-deremas"}))
                    .collect::<Vec<_>>()
            );
            let mut game = game(&initial);
            game.internal_rule_action(&json!({"do":"end-phase"}))
                .unwrap();
            settle(&mut game, 1);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["objects"]["source"]["acted"], count == 9);
            assert_eq!(state["objects"]["ally"]["acted"], true);
        }
    }
}

#[test]
fn cp03_100_ward_heals_only_at_the_ten_shadow_paladin_grave_threshold() {
    for card in ["CP03-100", "CP03-P69"] {
        for count in [9_u8, 10] {
            let mut initial = position(card);
            initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
            initial["players"]["P1"]["zones"]["cemetery"] = json!(
                (0..count)
                    .map(|i| json!({"id":format!("old{i}"),"card":"ready-deremas"}))
                    .collect::<Vec<_>>()
            );
            let mut game = game(&initial);
            play(&mut game, "source", 1);
            let state = game.projection(View::Referee).unwrap();
            assert!(has_keyword(&state, "source", "守護"));
            assert_eq!(
                state["P1"]["leader"]["life"],
                if count == 10 { 13_u8 } else { 10_u8 }
            );
        }
    }
}

#[test]
fn cp03_102_destroys_the_enemy_and_returns_the_selected_shadow_paladin() {
    for card in ["CP03-102", "CP03-P71"] {
        let mut initial = position(card);
        initial["players"]["P1"]["zones"]["cemetery"] =
            json!([{"id":"wrong","card":"BP01-171"},{"id":"found","card":"ready-deremas"}]);
        let mut game = game(&initial);
        play(&mut game, "source", 1);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P2"]["cemetery"], json!(["enemy"]));
        assert_eq!(state["P1"]["hand"], json!(["found"]));
        assert_eq!(state["P1"]["cemetery"], json!(["wrong", "source"]));
    }
}

#[test]
fn cp03_109_evolution_discards_oracle_and_banishes_a_five_or_less_hp_follower() {
    for card in ["CP03-109", "CP03-P73"] {
        let mut initial = evolution_position(card);
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"material","card":"ready-rin"}]);
        let game = evolved(&initial, card);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["cemetery"], json!(["material"]));
        assert_eq!(state["P2"]["banish"], json!(["enemy"]));
    }
}

#[test]
fn cp03_098_can_refuse_without_discarding_or_returning_the_grave_target() {
    for card in ["CP03-098", "CP03-P67"] {
        let mut initial = position(card);
        initial["players"]["P1"]["zones"]["hand"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"material","card":"ready-rin"}));
        initial["players"]["P1"]["zones"]["cemetery"] =
            json!([{"id":"found","card":"ready-deremas"}]);
        let mut game = game(&initial);
        play(&mut game, "source", 3);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["hand"], json!(["material"]));
        assert_eq!(state["P1"]["cemetery"], json!(["found"]));
    }
}

#[test]
fn cp03_102_can_destroy_an_enemy_without_selecting_a_grave_follower() {
    for card in ["CP03-102", "CP03-P71"] {
        let mut game = game(&position(card));
        play(&mut game, "source", 1);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P2"]["cemetery"], json!(["enemy"]));
        assert_eq!(state["P1"]["hand"], json!([]));
        assert_eq!(state["P1"]["cemetery"], json!(["source"]));
    }
}

#[test]
fn cp03_109_no_legal_hp_target_keeps_the_oracle_material_in_hand() {
    for card in ["CP03-109", "CP03-P73"] {
        let mut initial = evolution_position(card);
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"material","card":"ready-rin"}]);
        initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"ready-mio"}]);
        let game = evolved(&initial, card);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["hand"], json!(["material"]));
        assert_eq!(state["P1"]["cemetery"], json!([]));
        assert_eq!(state["P2"]["field"], json!(["enemy"]));
        assert_eq!(state["P2"]["banish"], json!([]));
        assert!(has_keyword(&state, "source", "シングルドライブ"));
    }
}

#[test]
fn cp03_098_cannot_return_the_hand_material_when_no_grave_target_exists() {
    for card in ["CP03-098", "CP03-P67"] {
        let mut initial = position(card);
        initial["players"]["P1"]["zones"]["hand"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"material","card":"ready-rin"}));
        let mut game = game(&initial);
        play(&mut game, "source", 1);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["hand"], json!(["material"]));
        assert_eq!(state["P1"]["cemetery"], json!([]));
    }
}

#[test]
fn cp03_099_can_stand_another_follower_only_at_its_own_end_phase() {
    for card in ["CP03-099", "CP03-P68"] {
        for active in ["P1", "P2"] {
            let mut initial = position(card);
            initial["turn"]["active"] = json!(active);
            initial["players"]["P1"]["zones"]["hand"] = json!([]);
            initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":card,"state":{"acted":true}},{"id":"ally","card":"ready-deremas","state":{"acted":true}}]);
            initial["players"]["P1"]["zones"]["cemetery"] = json!(
                (0_u8..10)
                    .map(|i| json!({"id":format!("old{i}"),"card":"ready-deremas"}))
                    .collect::<Vec<_>>()
            );
            let mut game = game(&initial);
            game.internal_rule_action(&json!({"do":"end-phase"}))
                .unwrap();
            settle(&mut game, 2);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["objects"]["source"]["acted"], true);
            assert_eq!(state["objects"]["ally"]["acted"], active == "P2");
        }
    }
}

#[test]
fn cp03_102_without_an_enemy_cannot_play_just_to_recover_a_grave_follower() {
    for card in ["CP03-102", "CP03-P71"] {
        let mut initial = position(card);
        initial["players"]["P2"]["zones"]["field"] = json!([]);
        initial["players"]["P1"]["zones"]["cemetery"] =
            json!([{"id":"recover","card":"ready-deremas"}]);
        let mut game = game(&initial);
        let before = game.projection(View::Referee).unwrap();
        game.play_internal_card("source", PlayerId { seat: 1 })
            .unwrap_err();
        assert_eq!(game.projection(View::Referee).unwrap(), before);
    }
}

#[test]
fn cp03_095_without_vanguard_material_does_not_search() {
    for card in ["CP03-095", "CP03-P64"] {
        let mut initial = position(card);
        initial["players"]["P1"]["zones"]["hand"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"wrong","card":"ready-deremas"}));
        initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"searched","card":"ready-mio"}]);
        let mut game = game(&initial);
        play(&mut game, "source", 1);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["hand"], json!(["wrong"]));
        assert_eq!(state["P1"]["cemetery"], json!([]));
        assert_eq!(state["P1"]["deck"], json!(["searched"]));
    }
}

#[test]
fn cp03_109_without_oracle_material_or_when_refusing_never_banishes() {
    for card in ["CP03-109", "CP03-P73"] {
        for has_oracle in [false, true] {
            let mut initial = evolution_position(card);
            initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"material","card":if has_oracle {"ready-rin"} else {"ready-deremas"}}]);
            let mut game = prepare_evolution(&initial, card);
            settle(&mut game, if has_oracle { 3 } else { 1 });
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["P1"]["hand"], json!(["material"]));
            assert_eq!(state["P1"]["cemetery"], json!([]));
            assert_eq!(state["P2"]["field"], json!(["enemy"]));
            assert_eq!(state["P2"]["banish"], json!([]));
        }
    }
}
