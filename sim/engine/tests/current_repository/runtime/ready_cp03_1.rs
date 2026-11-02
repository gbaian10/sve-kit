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
   ("ready-deremas","aquaforce companion","フォロワー",vec!["アクアフォース"]),
   ("ready-rin","ヴァンガード material","フォロワー",vec!["ヴァンガード","ロイヤルパラディン"]),
   ("ready-mio","ブラスター・ブレード","フォロワー",vec![]),
   ("ready-amulet","plain amulet","アミュレット",vec![]),
  ] {
   snapshot.push_str(&json!({"number":number,"faces":[{"name":name,"card_class":"ニュートラル","card_type":kind,"cost":"4","power":if kind=="フォロワー" {"3"} else {"-"},"hp":if kind=="フォロワー" {"4"} else {"-"},"traits":traits,"text":null,"sections":[]}]}).to_string());snapshot.push('\n');
   input["documents"].as_array_mut().unwrap().push(json!([format!("{number}.yaml"),json!({"format":1_u8,"kind":"effect_set","cards":{number:{"status":"complete","abilities":[]}}}).to_string()]));
  }
  let form = "CP03-002";
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
        "ready-cp03-1",
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
            InputDomain::OptionalCost => Answer::OptionalCost(true),
            InputDomain::Targets(domain) => Answer::Targets(
                domain
                    .members
                    .first()
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
fn finish_attack(game: &mut Game) {
    loop {
        settle(game, 1);
        let question = game
            .internal_decision(View::Referee)
            .filter(|question| matches!(question.domain, InputDomain::Quick { .. }))
            .map_or_else(|| game.internal_quick_choice().unwrap(), Some);
        let Some(question) = question else { break };
        let step = game
            .answer_internal_card(question.actor, question.token, Answer::Quick(None))
            .unwrap();
        finish(game, step, 1);
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
fn cp03_002_leader_damage_triggers_destruction_and_two_draws() {
    for card in ["CP03-002", "CP03-SL02", "PR-286"] {
        let initial = evolution_position(card);
        let mut game = evolved(&initial, card);
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"attack","attacker":"source","target":"P2.leader"})
            )
            .unwrap()
            .outcome,
            "resolved"
        );
        finish_attack(&mut game);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P2"]["leader"]["life"], 16_u8);
        assert_eq!(state["P2"]["cemetery"], json!(["enemy"]));
        assert_eq!(state["P1"]["hand"], json!(["draw2", "draw3"]));
        assert_eq!(state["P1"]["deck"], json!(["draw4", "draw0", "draw1"]));
    }
}

#[test]
fn cp03_011_requires_another_aquaforce_follower_to_grant_rush_and_assail() {
    for card in ["CP03-011", "PR-273"] {
        for ally in [false, true] {
            let mut initial = position(card);
            if ally {
                initial["players"]["P1"]["zones"]["field"] =
                    json!([{"id":"ally","card":"ready-deremas"}]);
            }
            let mut game = game(&initial);
            play(&mut game, "source", 1);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(has_keyword(&state, "source", "突進"), ally);
            assert_eq!(has_keyword(&state, "source", "指定攻撃"), ally);
        }
    }
}

#[test]
fn cp03_013_lastword_searches_its_own_printed_name() {
    for card in ["CP03-013", "PR-274"] {
        let mut initial = position(card);
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":card}]);
        initial["players"]["P1"]["zones"]["deck"] =
            json!([{"id":"wrong","card":"BP01-171"},{"id":"found","card":"CP03-013"}]);
        let mut game = with_selected_producer(
            &initial,
            Action::Destroy {
                objects: subjects(),
            },
            fixture(),
        );
        load_companions(&mut game);
        activate(&mut game, "P1");
        settle(&mut game, 1);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["hand"],
            json!(["found"])
        );
    }
}

#[test]
fn cp03_017_fanfare_can_act_or_stand_the_chosen_enemy() {
    for card in ["CP03-017", "CP03-P11"] {
        for mode in [1_u64, 2] {
            let mut initial = position(card);
            initial["players"]["P2"]["zones"]["field"][0]["state"] = json!({"acted":mode==2});
            let mut game = game(&initial);
            play(&mut game, "source", mode);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["objects"]["enemy"]["acted"], mode == 1);
            assert!(has_keyword(&state, "source", "突進"));
        }
    }
}

#[test]
fn cp03_020_revives_only_a_one_cost_aquaforce_follower() {
    for card in ["CP03-020", "CP03-P14"] {
        let mut initial = position(card);
        initial["players"]["P1"]["zones"]["cemetery"] = json!([{"id":"wrong","card":"BP01-171"},{"id":"expensive","card":"ready-deremas"},{"id":"found","card":"CP03-011"}]);
        let mut game = game(&initial);
        play(&mut game, "source", 1);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["field"], json!(["found"]));
        assert_eq!(
            state["P1"]["cemetery"],
            json!(["wrong", "expensive", "source"])
        );
        assert!(!has_keyword(&state, "found", "突進"));
    }
}

#[test]
fn cp03_035_discards_vanguard_then_searches_blaster_blade() {
    for card in ["CP03-035", "CP03-P23", "CSD03a-012"] {
        let mut initial = position(card);
        initial["players"]["P1"]["zones"]["hand"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"material","card":"ready-rin"}));
        initial["players"]["P1"]["zones"]["deck"] =
            json!([{"id":"wrong","card":"BP01-171"},{"id":"found","card":"ready-mio"}]);
        let mut game = game(&initial);
        play(&mut game, "source", 1);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["hand"], json!(["found"]));
        assert_eq!(state["P1"]["cemetery"], json!(["material"]));
    }
}

#[test]
fn cp03_036_discards_royal_paladin_before_drawing_one() {
    for card in ["CP03-036", "CP03-P24", "CSD03a-013"] {
        let mut initial = position(card);
        initial["players"]["P1"]["zones"]["hand"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"material","card":"ready-rin"}));
        let mut game = game(&initial);
        play(&mut game, "source", 1);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["hand"], json!(["draw0"]));
        assert_eq!(state["P1"]["cemetery"], json!(["material"]));
    }
}

#[test]
fn cp03_043_top_banish_reaches_five_before_the_draw_condition() {
    for card in ["CP03-043", "CP03-SL09"] {
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
                state["P1"]["hand"],
                if count == 4 {
                    json!(["draw1"])
                } else {
                    json!([])
                }
            );
        }
    }
}

#[test]
fn cp03_002_attacking_a_follower_does_not_trigger_its_leader_damage_ability() {
    for card in ["CP03-002", "CP03-SL02", "PR-286"] {
        let mut initial = evolution_position(card);
        initial["players"]["P2"]["zones"]["field"][0]["state"] = json!({"acted":true});
        let mut game = evolved(&initial, card);
        assert_eq!(
            game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"enemy"}))
                .unwrap()
                .outcome,
            "resolved"
        );
        finish_attack(&mut game);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P2"]["leader"]["life"], 20_u8);
        assert_eq!(state["P2"]["field"], json!(["enemy"]));
        assert_eq!(state["objects"]["enemy"]["damage"], 4_u8);
        assert_eq!(state["P1"]["hand"], json!([]));
        assert_eq!(
            state["P1"]["deck"],
            json!(["draw2", "draw3", "draw4", "draw0", "draw1"])
        );
    }
}

#[test]
fn cp03_020_revived_fanfare_sees_the_other_aquaforce_follower() {
    for card in ["CP03-020", "CP03-P14"] {
        let mut initial = position(card);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"ally","card":"ready-deremas"}]);
        initial["players"]["P1"]["zones"]["cemetery"] =
            json!([{"id":"expensive","card":"ready-deremas"},{"id":"found","card":"CP03-011"}]);
        let mut game = game(&initial);
        play(&mut game, "source", 1);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["field"], json!(["ally", "found"]));
        assert_eq!(state["P1"]["cemetery"], json!(["expensive", "source"]));
        assert!(has_keyword(&state, "found", "突進"));
        assert!(has_keyword(&state, "found", "指定攻撃"));
    }
}

#[test]
fn cp03_017_can_choose_no_target_and_still_enter() {
    for card in ["CP03-017", "CP03-P11"] {
        let mut initial = position(card);
        initial["players"]["P2"]["zones"]["field"] = json!([]);
        let mut game = game(&initial);
        play(&mut game, "source", 2);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["field"], json!(["source"]));
        assert!(has_keyword(&state, "source", "突進"));
        assert_eq!(state["P2"]["field"], json!([]));
    }
}

#[test]
fn cp03_017_declares_its_target_before_the_resolution_alternative() {
    for card in ["CP03-017", "CP03-P11"] {
        let mut initial = position(card);
        initial["players"]["P2"]["zones"]["field"][0]["state"] = json!({"acted":true});
        let mut game = game(&initial);
        let entry = game
            .play_internal_card("source", PlayerId { seat: 1 })
            .unwrap();
        finish(&mut game, entry, 1);
        if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
        let pending = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = &pending.domain else {
            panic!("the actual fanfare occurrence must be pending");
        };
        let declaration = game
            .answer_internal_card(
                pending.actor,
                pending.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap();
        let CardPlay::Awaiting(target) = declaration else {
            panic!("the formal target is selected before choosing act or stand");
        };
        let InputDomain::Targets(domain) = &target.domain else {
            panic!("this printed alternative is not a choice-keyword parameter");
        };
        let resolution = game
            .answer_internal_card(
                target.actor,
                target.token,
                Answer::Targets(vec![domain.members[0].handle]),
            )
            .unwrap();
        let CardPlay::Awaiting(alternative) = resolution else {
            panic!("act or stand is a resolution alternative");
        };
        assert!(matches!(alternative.domain, InputDomain::Modes { .. }));
        let completed = game
            .answer_internal_card(
                alternative.actor,
                alternative.token,
                Answer::Modes(vec![ModeId(2)]),
            )
            .unwrap();
        finish(&mut game, completed, 2);
        settle(&mut game, 2);
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["enemy"]["acted"],
            false
        );
    }
}
