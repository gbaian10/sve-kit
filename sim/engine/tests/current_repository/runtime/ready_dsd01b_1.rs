#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate printed card scenarios."
)]
use super::*;
fn fixture() -> &'static SelectedEffects {
    static SELECTED: OnceLock<SelectedEffects> = OnceLock::new();
    SELECTED.get_or_init(|| {
  let (mut snapshot,keywords,mut input)=producer_inputs();
  // Minimal companions isolate printed filters, entry events and ordinary evolution.
  for (number,name,kind,class,traits,cost,power,hp) in [
   ("ready-warrior","ready-warrior","フォロワー","ドラゴン",vec!["武闘竜人"],"1","3","4"),
   ("ready-wrong","ready-wrong","フォロワー","ドラゴン",vec![],"1","3","4"),
   ("ready-warrior-amulet","ready-warrior-amulet","アミュレット","ドラゴン",vec!["武闘竜人"],"1","-","-"),
   ("ready-neutral","ready-neutral","フォロワー","ニュートラル",vec!["武闘竜人"],"1","3","4"),
   ("ready-form","片倉小十郎","フォロワー・エボルヴ","ドラゴン",vec!["武闘竜人"],"-","3","3"),
  ] {
   snapshot.push_str(&json!({"number":number,"faces":[{"name":name,"card_class":class,"card_type":kind,"cost":cost,"power":power,"hp":hp,"traits":traits,"text":"","sections":[]}]}).to_string());snapshot.push('\n');
   input["documents"].as_array_mut().unwrap().push(json!([format!("{number}.yaml"),json!({"format":1_u8,"kind":"effect_set","cards":{number:{"status":"complete","abilities":[]}}}).to_string()]));
  }
  repository::selection_with_rules_from(&snapshot,&keywords,&input)
 })
}
fn position(card: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    initial["players"]["P1"]["sep"] = json!(0_u8);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    initial["players"]["P1"]["zones"]["field"] = json!([]);
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
        "ready-dsd01b-1",
    )
    .unwrap();
    load_companions(&mut game);
    game
}
fn finish(game: &mut Game, mut step: CardPlay, mode: u64) {
    while let CardPlay::Awaiting(question) = step {
        let answer = match &question.domain {
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            InputDomain::OptionalCost => Answer::OptionalCost(mode == 1),
            InputDomain::Targets(domain) => {
                assert_eq!(
                    domain.members.len(),
                    1,
                    "only the printed eligible follower may be offered"
                );
                Answer::Targets(
                    domain
                        .members
                        .first()
                        .map(|member| vec![member.handle])
                        .unwrap_or_default(),
                )
            }
            InputDomain::Modes { .. }
            | InputDomain::CostMaterials(_)
            | InputDomain::Search(_)
            | InputDomain::Order(_)
            | InputDomain::MealPayment { .. }
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
        "ready-warrior",
        "ready-wrong",
        "ready-warrior-amulet",
        "ready-neutral",
        "ready-form",
    ] {
        game.load_internal_card(number, &empty).unwrap();
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
#[test]
fn dsd01b_003_counts_the_entering_source_and_a_warrior_amulet_for_storm() {
    for card in ["DSD01b-003", "DSD01b-SL03"] {
        for mode in 0_u8..3 {
            let mut initial = position(card);
            initial["players"]["P1"]["zones"]["field"] =
                json!([{"id":"ally","card":"ready-warrior"}]);
            if mode != 0 {
                initial["players"]["P1"]["zones"]["field"].as_array_mut().unwrap().push(json!({"id":"third","card":if mode==1 {"ready-warrior-amulet"} else {"ready-wrong"}}));
            }
            let mut game = game(&initial);
            play(&mut game, 1);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["objects"]["source"]["power"],
                if mode == 1 { 4_u8 } else { 2_u8 }
            );
            assert_eq!(
                state["objects"]["source"]["hp"],
                if mode == 1 { 4_u8 } else { 2_u8 }
            );
            assert_eq!(keyword(&state, "source", "疾走"), mode == 1);
            assert!(
                keyword(&state, "source", "必殺"),
                "printed Bane remains active"
            );
        }
    }
}
#[test]
fn dsd01b_010_buffs_another_warrior_only_when_awakened_and_can_really_evolve() {
    for card in ["DSD01b-010", "DSD01b-P03"] {
        for awakened in [false, true] {
            let mut initial = position(card);
            initial["players"]["P1"]["pp"] = json!({"current":if awakened {7_u8} else {6_u8},"max":if awakened {7_u8} else {6_u8}});
            initial["players"]["P1"]["zones"]["field"] = json!([{"id":"ally","card":"ready-warrior"},{"id":"amulet","card":"ready-warrior-amulet"},{"id":"wrong","card":"ready-wrong"}]);
            initial["players"]["P1"]["zones"]["evolve_deck"] =
                json!([{"id":"form","card":"ready-form"}]);
            let mut game = game(&initial);
            play(&mut game, 1);
            let before = game.projection(View::Referee).unwrap();
            assert_eq!(
                before["objects"]["ally"]["power"],
                if awakened { 4_u8 } else { 3_u8 }
            );
            assert_eq!(before["objects"]["source"]["power"], 2_u8);
            assert_eq!(
                game.internal_rule_action(
                    &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
                )
                .unwrap()
                .outcome,
                "resolved"
            );
            settle(&mut game, 1);
            let evolved = game.projection(View::Referee).unwrap();
            assert_eq!(evolved["objects"]["source"]["power"], 3_u8);
            assert_eq!(
                evolved["P1"]["pp"]["current"],
                if awakened { 4_u8 } else { 3_u8 }
            );
        }
    }
}
#[test]
fn dsd01b_013_warrior_amulet_meets_the_three_card_draw_threshold_and_keywords_remain() {
    for card in ["DSD01b-013", "DSD01b-P06"] {
        for threshold in [false, true] {
            let mut initial = position(card);
            initial["players"]["P1"]["zones"]["field"] =
                json!([{"id":"ally","card":"ready-warrior"}]);
            if threshold {
                initial["players"]["P1"]["zones"]["field"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!({"id":"amulet","card":"ready-warrior-amulet"}));
            }
            let mut game = game(&initial);
            play(&mut game, 1);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["P1"]["hand"].as_array().unwrap().len(),
                if threshold { 2 } else { 0 }
            );
            assert!(
                keyword(&state, "source", "突進") && keyword(&state, "source", "守護"),
                "printed Rush and Ward remain active"
            );
        }
    }
}
#[test]
fn dsd01b_token_real_warrior_entry_grows_thunder_and_qualifies_both_activated_abilities() {
    for card in ["DSD01b-T01", "PR-268"] {
        for initial_count in [3_u8, 4, 9] {
            let mut initial = position(card);
            initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":card,"state":{"counters":{"雷カウンター":initial_count}}},{"id":"neutral","card":"ready-neutral"}]);
            initial["players"]["P1"]["zones"]["hand"] =
                json!([{"id":"entrant","card":"ready-warrior"}]);
            let mut game = game(&initial);
            let step = game
                .play_internal_card("entrant", PlayerId { seat: 1 })
                .unwrap();
            finish(&mut game, step, 1);
            settle(&mut game, 1);
            assert_eq!(
                game.projection(View::Referee).unwrap()["objects"]["source"]["counters"]["雷カウンター"],
                initial_count + 1
            );
            if initial_count == 3 {
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
                let error = game
                    .activate_internal_card("source", ability, PlayerId { seat: 1 })
                    .unwrap_err();
                assert!(
                    matches!(
                        error,
                        EngineFailure::IllegalPlay(_) | EngineFailure::Invalid(_)
                    ),
                    "the real counter qualification must reject below five"
                );
                assert_eq!(game.projection(View::Referee).unwrap(), before);
            } else {
                if initial_count == 4 {
                    let second = fixture().current[card]
                        .program
                        .card()
                        .abilities
                        .iter()
                        .filter(|a| {
                            a.node.kind == AbilityKind::Activated
                                && matches!(a.node.source.origin, AbilityOrigin::Printed { .. })
                        })
                        .nth(1)
                        .unwrap()
                        .node
                        .id;
                    let before = game.projection(View::Referee).unwrap();
                    let error = game
                        .activate_internal_card("source", second, PlayerId { seat: 1 })
                        .unwrap_err();
                    assert!(
                        matches!(
                            error,
                            EngineFailure::IllegalPlay(_) | EngineFailure::Invalid(_)
                        ),
                        "five counters must not qualify the ten-counter activation"
                    );
                    assert_eq!(game.projection(View::Referee).unwrap(), before);
                }
                native_activate(&mut game, card, usize::from(initial_count != 4));
                let state = game.projection(View::Referee).unwrap();
                assert_eq!(state["objects"]["source"]["acted"], true);
                if initial_count == 4 {
                    assert_eq!(state["objects"]["entrant"]["power"], 5_u8);
                    assert_eq!(state["objects"]["neutral"]["power"], 3_u8);
                } else {
                    assert!(
                        state["P2"]["cemetery"]
                            .as_array()
                            .unwrap()
                            .contains(&json!("enemy")),
                        "the ten-counter ability destroys the target"
                    );
                    assert_eq!(state["P2"]["leader"]["life"], 17_u8);
                }
            }
        }
    }
}
