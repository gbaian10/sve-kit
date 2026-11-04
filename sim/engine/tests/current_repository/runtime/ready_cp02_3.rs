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
   ("ready-deremas","deremas companion","フォロワー",vec!["デレマス"]),
   ("ready-rin","的場梨沙","フォロワー",vec!["デレマス","キュート"]),
   ("ready-mio","passion companion","フォロワー",vec!["デレマス","パッション"]),
   ("ready-amulet","cool companion","フォロワー",vec!["デレマス","クール"]),
  ] {
   snapshot.push_str(&json!({"number":number,"faces":[{"name":name,"card_class":"ニュートラル","card_type":kind,"cost":"4","power":if kind=="フォロワー" {"3"} else {"-"},"hp":if kind=="フォロワー" {"4"} else {"-"},"traits":traits,"text":null,"sections":[]}]}).to_string());snapshot.push('\n');
   input["documents"].as_array_mut().unwrap().push(json!([format!("{number}.yaml"),json!({"format":1_u8,"kind":"effect_set","cards":{number:{"status":"complete","abilities":[]}}}).to_string()]));
  }
  for form in ["CP02-088","CP02-100","CP02-SP12a"] {
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
        "ready-cp02-3",
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
            InputDomain::Search(domain) => Answer::Search(domain.groups.as_ref().map_or_else(
                || {
                    SearchAnswer::Ungrouped(
                        domain
                            .cards
                            .first()
                            .map(|m| vec![m.handle])
                            .unwrap_or_default(),
                    )
                },
                |groups| {
                    SearchAnswer::Grouped(
                        groups
                            .iter()
                            .map(|g| g.handles.first().map(|h| vec![*h]).unwrap_or_default())
                            .collect(),
                    )
                },
            )),
            InputDomain::Order(cards) => {
                Answer::Order(cards.iter().rev().map(|m| m.handle).collect())
            }
            InputDomain::Abilities(_)
            | InputDomain::ResourceEntryPayment { .. }
            | InputDomain::Quick { .. }
            | InputDomain::CostMaterials(_)
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
fn native_activate(game: &mut Game, card: &str) {
    let ability = fixture().current[card]
        .program
        .card()
        .abilities
        .iter()
        .find(|a| a.node.kind == AbilityKind::Activated)
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
fn cp02_080_ward_and_lastword_return_a_deremas_spell_from_the_grave() {
    for card in ["CP02-080", "CP02-P65"] {
        let mut initial = position(card);
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":card}]);
        initial["players"]["P1"]["zones"]["cemetery"] =
            json!([{"id":"wrong","card":"BP01-065"},{"id":"spell","card":"CP02-102"}]);
        let mut game = with_selected_producer(
            &initial,
            Action::Destroy {
                objects: subjects(),
            },
            fixture(),
        );
        load_companions(&mut game);
        assert!(has_keyword(
            &game.projection(View::Referee).unwrap(),
            "source",
            "守護"
        ));
        activate(&mut game, "P1");
        settle(&mut game, 1);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["hand"], json!(["spell"]));
        assert_eq!(
            state["P1"]["cemetery"],
            json!(["wrong", "source", "producer"])
        );
    }
}

#[test]
fn cp02_088_evolution_revives_a_follower_and_runs_its_fanfare() {
    for card in ["CP02-088", "CP02-SL18"] {
        let mut initial = evolution_position(card);
        initial["players"]["P1"]["zones"]["cemetery"] = json!([{"id":"expensive","card":"BP01-061"},{"id":"spell","card":"BP01-065"},{"id":"revived","card":"BP01-177"}]);
        initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
        let game = evolved(&initial, card);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["field"], json!(["source", "revived"]));
        assert_eq!(state["P1"]["cemetery"], json!(["expensive", "spell"]));
        assert_eq!(state["P1"]["leader"]["life"], 11_u8);
    }
}

#[test]
fn cp02_sp12_evolution_revives_from_the_grave() {
    for card in ["CP02-SP12a", "CP02-SP12b", "CP02-U12a", "CP02-U12b"] {
        let mut initial = evolution_position(card);
        initial["players"]["P1"]["zones"]["cemetery"] =
            json!([{"id":"expensive","card":"BP01-061"},{"id":"revived","card":"BP01-177"}]);
        initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
        let game = evolved(&initial, card);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["field"], json!(["source", "revived"]));
        assert_eq!(state["P1"]["cemetery"], json!(["expensive"]));
        assert_eq!(state["P1"]["leader"]["life"], 11_u8);
    }
}

#[test]
fn cp02_094_named_ally_grants_a_buff_and_storm() {
    for card in ["CP02-094", "CP02-P76"] {
        for named in [false, true] {
            let mut initial = position(card);
            if named {
                initial["players"]["P1"]["zones"]["field"] =
                    json!([{"id":"risa","card":"ready-rin"}]);
            }
            let mut game = game(&initial);
            play(&mut game, "source", 1);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["objects"]["source"]["power"],
                if named { 3_u8 } else { 2_u8 }
            );
            assert_eq!(
                state["objects"]["source"]["hp"],
                if named { 3_u8 } else { 2_u8 }
            );
            assert_eq!(has_keyword(&state, "source", "疾走"), named);
        }
    }
}

#[test]
fn cp02_095_real_haru_evolution_pays_one_and_heals_two() {
    for card in ["CP02-095", "CP02-P77"] {
        for base in ["CP02-094", "CP02-P76"] {
            let mut initial = position(card);
            initial["players"]["P1"]["zones"]["hand"] = json!([]);
            initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":base}]);
            initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":card}]);
            initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
            let mut game = game(&initial);
            assert_eq!(
                game.internal_rule_action(
                    &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
                )
                .unwrap()
                .outcome,
                "resolved"
            );
            settle(&mut game, 1);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["P1"]["leader"]["life"], 12_u8);
            assert_eq!(state["P1"]["pp"]["current"], 9_u8);
            assert_eq!(state["objects"]["source"]["evolved_with"], "form");
        }
    }
}

#[test]
fn cp02_097_storm_and_damage_count_only_other_passion_followers() {
    for card in ["CP02-097", "CP02-P79", "CSD02c-016", "PR-151"] {
        for n in [0_u8, 2] {
            let mut initial = position(card);
            initial["players"]["P1"]["zones"]["field"] = json!(
                (0..n)
                    .map(|i| json!({"id":format!("ally{i}"),"card":"ready-mio"}))
                    .collect::<Vec<_>>()
            );
            let mut game = game(&initial);
            play(&mut game, "source", 1);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["objects"]["enemy"]["damage"], n);
            assert!(has_keyword(&state, "source", "疾走"));
        }
    }
}

#[test]
fn cp02_100_evolution_searches_one_follower_per_trait_group() {
    for card in ["CP02-100", "CP02-P82"] {
        let mut initial = evolution_position(card);
        initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"wrong","card":"BP01-171"},{"id":"cool","card":"ready-amulet"},{"id":"passion","card":"ready-mio"}]);
        let game = evolved(&initial, card);
        let state = game.projection(View::Referee).unwrap();
        let mut hand = state["P1"]["hand"].as_array().unwrap().clone();
        hand.sort_by_key(|v| v.as_str().unwrap().to_owned());
        assert_eq!(hand, json!(["cool", "passion"]).as_array().unwrap().clone());
        assert_eq!(state["P1"]["deck"], json!(["wrong"]));
    }
}

#[test]
fn cp02_102_banishes_and_heals_only_with_a_cute_follower() {
    for card in ["CP02-102", "CP02-P84", "CSD02a-018"] {
        for cute in [false, true] {
            let mut initial = position(card);
            initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
            if cute {
                initial["players"]["P1"]["zones"]["field"] =
                    json!([{"id":"ally","card":"ready-rin"}]);
            }
            let mut game = game(&initial);
            play(&mut game, "source", 1);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["P2"]["banish"], json!(["enemy"]));
            assert_eq!(
                state["P1"]["leader"]["life"],
                if cute { 13_u8 } else { 10_u8 }
            );
        }
    }
}

#[test]
fn cp02_108_acts_itself_and_buffs_another_deremas_follower() {
    for card in ["CP02-108", "CP02-P88", "CSD02b-018"] {
        let mut initial = position(card);
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":card},{"id":"ally","card":"ready-deremas"},{"id":"wrong","card":"BP01-171"}]);
        let mut game = game(&initial);
        native_activate(&mut game, card);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["source"]["acted"], true);
        assert_eq!(state["objects"]["source"]["power"], 2_u8);
        assert_eq!(state["objects"]["ally"]["power"], 4_u8);
        assert_eq!(state["objects"]["wrong"]["power"], 2_u8);
    }
}

#[test]
fn cp02_100_searches_the_available_group_when_the_other_is_empty() {
    for card in ["CP02-100", "CP02-P82"] {
        let mut initial = evolution_position(card);
        initial["players"]["P1"]["zones"]["deck"] =
            json!([{"id":"wrong","card":"BP01-171"},{"id":"cool","card":"ready-amulet"}]);
        let game = evolved(&initial, card);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["hand"], json!(["cool"]));
        assert_eq!(state["P1"]["deck"], json!(["wrong"]));
    }
}
#[test]
fn cp02_102_cannot_heal_without_a_legal_enemy_target() {
    for card in ["CP02-102", "CP02-P84", "CSD02a-018"] {
        let mut initial = position(card);
        initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"ally","card":"ready-rin"}]);
        initial["players"]["P2"]["zones"]["field"] = json!([]);
        let mut game = game(&initial);
        let before = game.projection(View::Referee).unwrap();
        game.play_internal_card("source", PlayerId { seat: 1 })
            .expect_err("a formal enemy target is required before healing");
        assert_eq!(game.projection(View::Referee).unwrap(), before);
    }
}
