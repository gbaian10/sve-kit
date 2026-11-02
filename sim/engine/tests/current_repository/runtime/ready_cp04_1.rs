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
   ("ready-deremas","ペコリーヌ companion","フォロワー",vec!["プリコネ","なかよし部"]),
   ("ready-rin","ヴァンガード material","フォロワー",vec!["プリコネ","NIGHTMARE"]),
   ("ready-mio","ブラスター・ブレード","フォロワー",vec![]),
   ("ready-amulet","plain amulet","アミュレット",vec![]),
  ] {
   snapshot.push_str(&json!({"number":number,"faces":[{"name":name,"card_class":"ニュートラル","card_type":kind,"cost":"4","power":if kind=="フォロワー" {"3"} else {"-"},"hp":if kind=="フォロワー" {"4"} else {"-"},"traits":traits,"text":null,"sections":[]}]}).to_string());snapshot.push('\n');
   input["documents"].as_array_mut().unwrap().push(json!([format!("{number}.yaml"),json!({"format":1_u8,"kind":"effect_set","cards":{number:{"status":"complete","abilities":[]}}}).to_string()]));
  }
  for (number,traits) in [
   ("ready-no-evolve",vec!["プリコネ"]),
   ("ready-non-princess",vec![]),
  ] {
   snapshot.push_str(&json!({"number":number,"faces":[{"name":number,"card_class":"ニュートラル","card_type":"フォロワー","cost":"1","power":"1","hp":"1","traits":traits,"text":null,"sections":[]}]}).to_string());snapshot.push('\n');
   input["documents"].as_array_mut().unwrap().push(json!([format!("{number}.yaml"),json!({"format":1_u8,"kind":"effect_set","cards":{number:{"status":"complete","abilities":[]}}}).to_string()]));
  }
  for form in ["CP04-002","CP04-042"] {
   let name=snapshot.lines().map(|line|serde_json::from_str::<Value>(line).unwrap()).find(|row|row["number"]==form).unwrap()["faces"][0]["name"].clone();
   let number=format!("ready-base-{form}");
   snapshot.push_str(&json!({"number":number,"faces":[{"name":name,"card_class":"ニュートラル","card_type":"フォロワー","cost":"0","power":"3","hp":"4","traits":[],"text":null,"sections":[]}]}).to_string());snapshot.push('\n');
   input["documents"].as_array_mut().unwrap().push(json!([format!("{number}.yaml"),json!({"format":1_u8,"kind":"effect_set","cards":{number:{"status":"complete","abilities":[]}}}).to_string()]));
  }
  repository::selection_with_rules_from(&snapshot,&keywords,&input)
 })
}
fn position(card: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["construction"] = json!("title");
    initial["players"]["P1"]["title_code"] = json!("princess_connect_re_dive");
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    initial["players"]["P1"]["sep"] = json!(0_u8);
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
    for number in [
        "ready-deremas",
        "ready-rin",
        "ready-mio",
        "ready-amulet",
        "ready-no-evolve",
    ] {
        game.load_internal_card(number, &empty).unwrap();
    }
    let mut tree = expanded::tree().clone();
    tree["cards"] = json!({"ready-non-princess":tree["cards"]["synthetic-evolution"]});
    let program = lowerer()
        .lower(&tree)
        .unwrap()
        .remove("ready-non-princess")
        .unwrap();
    game.load_internal_card("ready-non-princess", &program)
        .unwrap();
}
fn game(initial: &Value) -> Game {
    let mut game = Game::from_selected_effects(
        fixture().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "ready-cp04-1",
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
#[test]
fn cp04_002_super_evolution_triggers_both_printed_heads() {
    for card in [
        "CP04-002",
        "CP04-SL02",
        "CP04-SP01",
        "CP04-U01",
        "PCS01-002",
    ] {
        for super_evolution in [false, true] {
            let mut initial = evolution_position(card);
            initial["turn"]["elapsed_turns"] = json!({"P1":7_u8,"P2":6_u8});
            initial["players"]["P1"]["sep"] = json!(1_u8);
            let base = format!("ready-base-{}", fixture().current[card].card_no);
            let mut game = game(&initial);
            let mut tree = expanded::tree().clone();
            tree["cards"] = json!({base.clone():tree["cards"]["synthetic-evolution"]});
            let program = lowerer().lower(&tree).unwrap().remove(&base).unwrap();
            game.load_internal_card(&base, &program).unwrap();
            let pay = if super_evolution {
                json!({"pp":1_u8,"sep":1_u8})
            } else {
                json!({"pp":1_u8})
            };
            assert_eq!(
                game.internal_rule_action(
                    &json!({"do":"evolve","source":"source","evolve_card":"form","pay":pay})
                )
                .unwrap()
                .outcome,
                "resolved"
            );
            settle(&mut game, 1);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["P1"]["hand"].as_array().unwrap().len(),
                if super_evolution { 2 } else { 1 }
            );
            assert_eq!(
                state["P1"]["leader"]["life"],
                if super_evolution { 12_u8 } else { 10_u8 }
            );
        }
    }
}

#[test]
fn cp04_009_evolves_for_free_only_when_played_from_ex() {
    for card in ["CP04-009", "CP04-P05", "PCS01-004"] {
        for from_ex in [false, true] {
            let mut initial = position(card);
            initial["players"]["P1"]["zones"]["evolve_deck"] =
                json!([{"id":"form","card":"CP04-010"}]);
            if from_ex {
                initial["players"]["P1"]["zones"]["hand"] = json!([]);
                initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"source","card":card}]);
            }
            let mut game = game(&initial);
            play(&mut game, "source", 1);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["P1"]["evolution"],
                if from_ex { json!(["form"]) } else { json!([]) }
            );
            assert_eq!(state["P1"]["pp"]["current"], 9_u8);
        }
    }
}

#[test]
fn cp04_011_pays_two_named_counters_to_heal_and_draw() {
    for card in ["CP04-011", "CP04-P07"] {
        let mut initial = position(card);
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] =
            json!([{"id":"source","card":card,"state":{"counters":{"かばばん君カウンター":2_u8}}}]);
        let mut game = game(&initial);
        native_activate(&mut game, card, 0);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["objects"]["source"]["counters"]["かばばん君カウンター"],
            0_u8
        );
        assert_eq!(state["P1"]["leader"]["life"], 12_u8);
        assert_eq!(state["P1"]["hand"], json!(["draw0"]));
    }
}

#[test]
fn cp04_011_gains_counters_from_nonhand_entry_and_its_own_end_phase() {
    for from_ex in [false, true] {
        let mut initial = position("CP04-011");
        if from_ex {
            initial["players"]["P1"]["zones"]["hand"] = json!([]);
            initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"source","card":"CP04-011"}]);
        }
        let mut game = game(&initial);
        play(&mut game, "source", 1);
        let counters = |state_game: &Game| {
            state_game.projection(View::Referee).unwrap()["objects"]["source"]["counters"]["かばばん君カウンター"].as_u64().unwrap_or(0)
        };
        assert_eq!(counters(&game), if from_ex { 2 } else { 0 });
        game.internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap();
        settle(&mut game, 1);
        assert_eq!(counters(&game), if from_ex { 3 } else { 1 });
    }
}

#[test]
fn cp04_027_fanfare_counts_itself_for_the_four_princess_followers() {
    for card in ["CP04-027", "CP04-P19"] {
        for other in [2_u8, 3] {
            let mut initial = position(card);
            initial["players"]["P1"]["zones"]["field"] = json!(
                (0..other)
                    .map(|i| json!({"id":format!("ally{i}"),"card":"ready-rin"}))
                    .collect::<Vec<_>>()
            );
            initial["players"]["P1"]["zones"]["evolve_deck"] =
                json!([{"id":"form","card":"CP04-028"}]);
            let mut game = game(&initial);
            play(&mut game, "source", 1);
            let state = game.projection(View::Referee).unwrap();
            assert!(has_keyword(&state, "source", "疾走"));
            assert_eq!(
                state["P1"]["evolution"],
                if other == 3 {
                    json!(["form"])
                } else {
                    json!([])
                }
            );
        }
    }
}

#[test]
fn cp04_029_acts_to_damage_an_enemy_and_heal_one() {
    for card in ["CP04-029", "CP04-P21", "PCS01-011", "PR-506"] {
        let mut initial = position(card);
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":card}]);
        let mut game = game(&initial);
        native_activate(&mut game, card, 0);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["source"]["acted"], true);
        assert_eq!(state["objects"]["enemy"]["damage"], 1_u8);
        assert_eq!(state["P1"]["leader"]["life"], 11_u8);
    }
}

#[test]
fn cp04_029_searches_a_one_cost_princess_follower_with_evolution() {
    let mut initial = position("CP04-029");
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"wrong","card":"CP04-027"},
        {"id":"no-evolution","card":"ready-no-evolve"},
        {"id":"wrong-trait","card":"ready-non-princess"},
        {"id":"valid","card":"CP04-009"}
    ]);
    let mut game = game(&initial);
    play(&mut game, "source", 1);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["ex"], json!(["valid"]));
    let deck = state["P1"]["deck"].as_array().unwrap();
    assert_eq!(deck.len(), 3);
    for id in ["wrong", "no-evolution", "wrong-trait"] {
        assert!(deck.contains(&json!(id)));
    }
    assert!(has_keyword(&state, "source", "守護"));
}

#[test]
fn cp04_034_attack_deals_ability_damage_to_a_separate_enemy() {
    for card in ["CP04-034", "CP04-P26"] {
        let mut initial = position(card);
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":card}]);
        let mut game = game(&initial);
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"P2.leader"}))
            .unwrap();
        finish_attack(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
            1_u8
        );
    }
}

#[test]
fn cp04_034_has_rush_and_requires_another_nightmare_for_assail() {
    for companion in [false, true] {
        let mut initial = position("CP04-034");
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"CP04-034"}]);
        if companion {
            initial["players"]["P1"]["zones"]["field"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":"ally","card":"ready-rin"}));
        }
        let game = game(&initial);
        let state = game.projection(View::Referee).unwrap();
        assert!(has_keyword(&state, "source", "突進"));
        assert_eq!(has_keyword(&state, "source", "指定攻撃"), companion);
    }
}

#[test]
fn cp04_042_evolution_puts_one_observed_card_in_ex_and_orders_the_remainder() {
    for card in ["CP04-042", "CP04-P30"] {
        let game = evolved(&evolution_position(card), card);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["ex"], json!(["draw0"]));
        assert_eq!(
            state["P1"]["deck"],
            json!(["draw4", "draw3", "draw2", "draw1"])
        );
    }
}

#[test]
fn cp04_044_discards_a_princess_card_and_heals_only_with_a_friendship_follower() {
    for card in ["CP04-044", "CP04-P32"] {
        for friend in [false, true] {
            let mut initial = position(card);
            initial["players"]["P1"]["zones"]["hand"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":"material","card":"ready-rin"}));
            if friend {
                initial["players"]["P1"]["zones"]["field"] =
                    json!([{"id":"ally","card":"ready-deremas"}]);
            }
            let mut game = game(&initial);
            play(&mut game, "source", 1);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["P1"]["hand"], json!(["draw0", "draw1"]));
            assert_eq!(
                state["P1"]["leader"]["life"],
                if friend { 12_u8 } else { 10_u8 }
            );
            assert!(
                state["P1"]["cemetery"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("material"))
            );
        }
    }
}

#[test]
fn cp04_011_and_029_cannot_activate_without_the_printed_cost_or_target() {
    for card in ["CP04-011", "CP04-029"] {
        let mut initial = position(card);
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] =
            json!([{"id":"source","card":card,"state":{"counters":{"かばばん君カウンター":1_u8}}}]);
        if card == "CP04-029" {
            initial["players"]["P2"]["zones"]["field"] = json!([]);
        }
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
        let before = game.projection(View::Referee).unwrap();
        game.activate_internal_card("source", ability, PlayerId { seat: 1 })
            .unwrap_err();
        assert_eq!(game.projection(View::Referee).unwrap(), before);
    }
}

#[test]
fn cp04_042_may_decline_ex_and_order_all_four_observed_cards() {
    let mut game = prepare_evolution(&evolution_position("CP04-042"), "CP04-042");
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("evolution trigger")
    };
    let CardPlay::Awaiting(choice) = game
        .answer_internal_card(
            pending.actor,
            pending.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap()
    else {
        panic!("optional observed card")
    };
    assert!(matches!(choice.domain, InputDomain::Targets(_)));
    let step = game
        .answer_internal_card(choice.actor, choice.token, Answer::Targets(vec![]))
        .unwrap();
    finish(&mut game, step, 1);
    settle(&mut game, 1);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["ex"], json!([]));
    assert_eq!(
        state["P1"]["deck"],
        json!(["draw4", "draw3", "draw2", "draw1", "draw0"])
    );
}

#[test]
fn cp04_044_can_decline_payment_and_cannot_discard_a_nonprincess_card() {
    for material in [None, Some("ready-rin"), Some("ready-mio")] {
        let mut initial = position("CP04-044");
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"ally","card":"ready-deremas"}]);
        if let Some(card) = material {
            initial["players"]["P1"]["zones"]["hand"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":"material","card":card}));
        }
        let mut game = game(&initial);
        let before = game.projection(View::Referee).unwrap();
        let CardPlay::Awaiting(mut choice) = game
            .play_internal_card("source", PlayerId { seat: 1 })
            .unwrap()
        else {
            panic!("optional payment")
        };
        assert!(matches!(choice.domain, InputDomain::OptionalCost));
        if material != Some("ready-rin") {
            game.answer_internal_card(choice.actor, choice.token, Answer::OptionalCost(true))
                .unwrap_err();
            let state = game.projection(View::Referee).unwrap();
            for key in ["hand", "cemetery", "leader", "pp"] {
                assert_eq!(state["P1"][key], before["P1"][key]);
            }
            let CardPlay::Awaiting(retry) = game
                .play_internal_card("source", PlayerId { seat: 1 })
                .unwrap()
            else {
                panic!("optional payment retry")
            };
            choice = retry;
        }
        let step = game
            .answer_internal_card(choice.actor, choice.token, Answer::OptionalCost(false))
            .unwrap();
        finish(&mut game, step, 1);
        settle(&mut game, 1);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["leader"]["life"], 10_u8);
        assert_eq!(
            state["P1"]["hand"],
            if material.is_some() {
                json!(["material"])
            } else {
                json!([])
            }
        );
        assert_eq!(state["P1"]["cemetery"], json!(["source"]));
    }
}

#[test]
fn cp04_011_does_not_gain_a_counter_during_the_opponents_end_phase() {
    let mut initial = position("CP04-011");
    initial["turn"]["active"] = json!("P2");
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"CP04-011","state":{"counters":{"かばばん君カウンター":2_u8}}}]);
    let mut game = game(&initial);
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    settle(&mut game, 1);
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["source"]["counters"]["かばばん君カウンター"],
        2_u8
    );
}

#[test]
fn cp04_009_and_027_can_pay_two_pp_for_ordinary_evolution() {
    for (base, form) in [("CP04-009", "CP04-010"), ("CP04-027", "CP04-028")] {
        let mut initial = position(base);
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":base}]);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
        let mut game = game(&initial);
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":2_u8}})
            )
            .unwrap()
            .outcome,
            "resolved"
        );
        settle(&mut game, 1);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["pp"]["current"], 8_u8);
        assert_eq!(state["P1"]["evolution"], json!(["form"]));
    }
}
