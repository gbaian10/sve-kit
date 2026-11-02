#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate printed card scenarios."
)]
use super::*;
use sve_engine::game::internal::SearchAnswer;
fn fixture() -> &'static SelectedEffects {
    static SELECTED: OnceLock<SelectedEffects> = OnceLock::new();
    SELECTED.get_or_init(|| {
  let (mut snapshot, keywords, mut input)=producer_inputs();
  // Minimal companions isolate printed trait filters and payments from unrelated abilities.
  for (number, kind, traits) in [
   ("ready-cute", "フォロワー", vec!["キュート"]),
   ("ready-wrong", "フォロワー", vec![]),
   ("ready-cute-amulet", "アミュレット", vec!["キュート"]),
  ] {
   snapshot.push_str(&json!({"number":number,"faces":[{"name":number,"card_class":"ニュートラル","card_type":kind,"cost":"1","power":if kind=="フォロワー" {"3"} else {"-"},"hp":if kind=="フォロワー" {"4"} else {"-"},"traits":traits,"text":null,"sections":[]}]}).to_string());snapshot.push('\n');
   input["documents"].as_array_mut().unwrap().push(json!([format!("{number}.yaml"),json!({"format":1_u8,"kind":"effect_set","cards":{number:{"status":"complete","abilities":[]}}}).to_string()]));
  }
  for form in ["CSD02a-008","CSD02a-010"] {
   let name=snapshot.lines().map(|line|serde_json::from_str::<Value>(line).unwrap()).find(|row|row["number"]==form).unwrap()["faces"][0]["name"].clone();
   let number=format!("ready-base-{form}");
   snapshot.push_str(&json!({"number":number,"faces":[{"name":name,"card_class":"ニュートラル","card_type":"フォロワー","cost":"0","power":"3","hp":"4","traits":["キュート"],"text":null,"sections":[]}]}).to_string());snapshot.push('\n');
   input["documents"].as_array_mut().unwrap().push(json!([format!("{number}.yaml"),json!({"format":1_u8,"kind":"effect_set","cards":{number:{"status":"complete","abilities":[]}}}).to_string()]));
  }
  repository::selection_with_rules_from(&snapshot,&keywords,&input)
 })
}
fn position(card: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["construction"] = json!("title");
    initial["players"]["P1"]["title_code"] = json!("idolmaster_cinderella_girls");
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
        "ready-csd02a-1",
    )
    .unwrap();
    let empty = WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![],
    })
    .unwrap();
    for number in ["ready-cute", "ready-wrong", "ready-cute-amulet"] {
        game.load_internal_card(number, &empty).unwrap();
    }
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
                    .first()
                    .map(|m| vec![m.handle])
                    .unwrap_or_default(),
            ),
            InputDomain::CostMaterials(domain) => Answer::CostMaterials(
                domain
                    .members
                    .iter()
                    .take(usize::try_from(domain.cardinality.minimum.0).unwrap())
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

fn has_keyword(state: &Value, object: &str, keyword: &str) -> bool {
    state["objects"][object]["keywords"]
        .as_array()
        .unwrap()
        .contains(&json!(keyword))
}
fn evolution_position(card: &str) -> Value {
    let mut initial = position(card);
    let base = format!("ready-base-{}", fixture().current[card].card_no);
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":base}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":card}]);
    initial
}
fn evolve_form(initial: &Value, card: &str, payment: bool) -> Game {
    let mut game = game(initial);
    let base = format!("ready-base-{}", fixture().current[card].card_no);
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
    settle(&mut game, if payment { 1 } else { 2 });
    game
}
fn evolve_base(game: &mut Game) {
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
        )
        .unwrap()
        .outcome,
        "resolved",
        "the printed base evolution must resolve"
    );
    settle(game, 1);
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["source"]["evolved_with"],
        "form",
        "the printed base evolution must use the admitted matching resource"
    );
}
#[test]
fn csd02a_007_buffs_another_cute_follower_and_can_evolve() {
    for card in ["CSD02a-007", "CSD02a-P04"] {
        let mut initial = position(card);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"ally","card":"ready-cute"},{"id":"wrong","card":"ready-wrong"},{"id":"amulet","card":"ready-cute-amulet"}]);
        initial["players"]["P1"]["zones"]["evolve_deck"] =
            json!([{"id":"form","card":"CSD02a-008"}]);
        let mut game = game(&initial);
        play(&mut game, 1);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["ally"]["hp"], 5_u8);
        assert_eq!(state["objects"]["wrong"]["hp"], 4_u8);
        assert_eq!(state["objects"]["source"]["hp"], 3_u8);
        evolve_base(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
            6_u8
        );
    }
}

#[test]
fn csd02a_008_evolution_pays_only_a_cute_hand_card_to_damage_and_draw() {
    for card in ["CSD02a-008", "CSD02a-P05"] {
        for payment in [false, true] {
            let mut initial = evolution_position(card);
            initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"material","card":"ready-cute-amulet"},{"id":"wrong","card":"ready-wrong"}]);
            let game = evolve_form(&initial, card, payment);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["objects"]["enemy"]["damage"],
                if payment { 3_u8 } else { 0_u8 }
            );
            assert_eq!(
                state["P1"]["cemetery"],
                if payment {
                    json!(["material"])
                } else {
                    json!([])
                }
            );
            assert_eq!(
                state["P1"]["hand"],
                if payment {
                    json!(["wrong", "draw0"])
                } else {
                    json!(["material", "wrong"])
                }
            );
            assert_eq!(state["P1"]["pp"]["current"], 9_u8);
        }
    }
}

#[test]
fn csd02a_010_evolution_can_pay_three_pp_to_revive_only_a_cute_follower() {
    for card in ["CSD02a-010", "CSD02a-P07"] {
        for payment in [false, true] {
            let mut initial = evolution_position(card);
            initial["players"]["P1"]["zones"]["cemetery"] = json!([{"id":"wrong","card":"ready-wrong"},{"id":"amulet","card":"ready-cute-amulet"},{"id":"revive","card":"ready-cute"}]);
            let game = evolve_form(&initial, card, payment);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["P1"]["field"],
                if payment {
                    json!(["source", "revive"])
                } else {
                    json!(["source"])
                }
            );
            assert_eq!(
                state["P1"]["cemetery"],
                if payment {
                    json!(["wrong", "amulet"])
                } else {
                    json!(["wrong", "amulet", "revive"])
                }
            );
            assert_eq!(
                state["P1"]["pp"]["current"],
                if payment { 6_u8 } else { 9_u8 }
            );
            assert!(
                has_keyword(&state, "source", "守護"),
                "the evolved form retains printed Ward"
            );
        }
    }
}

#[test]
fn csd02a_011_optional_cute_discard_gains_two_health_draws_and_retains_ward() {
    for card in ["CSD02a-011", "CSD02a-P08"] {
        for payment in [false, true] {
            let mut initial = position(card);
            initial["players"]["P1"]["zones"]["hand"]
                .as_array_mut()
                .unwrap()
                .extend([
                    json!({"id":"material","card":"ready-cute-amulet"}),
                    json!({"id":"wrong","card":"ready-wrong"}),
                ]);
            let mut game = game(&initial);
            play(&mut game, if payment { 1 } else { 2 });
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["objects"]["source"]["hp"],
                if payment { 5_u8 } else { 3_u8 }
            );
            assert_eq!(state["objects"]["source"]["power"], 3_u8);
            assert_eq!(
                state["P1"]["cemetery"],
                if payment {
                    json!(["material"])
                } else {
                    json!([])
                }
            );
            assert_eq!(
                state["P1"]["hand"],
                if payment {
                    json!(["wrong", "draw0"])
                } else {
                    json!(["material", "wrong"])
                }
            );
            assert!(
                has_keyword(&state, "source", "守護"),
                "printed Ward is unconditional"
            );
        }
    }
}

#[test]
fn csd02a_008_missing_a_target_or_cute_material_does_not_discard_or_draw() {
    for card in ["CSD02a-008", "CSD02a-P05"] {
        for missing_target in [false, true] {
            let mut initial = evolution_position(card);
            if missing_target {
                initial["players"]["P2"]["zones"]["field"] = json!([]);
            }
            initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"material","card":if missing_target {"ready-cute"} else {"ready-wrong"}}]);
            let game = evolve_form(&initial, card, true);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["P1"]["hand"], json!(["material"]));
            assert_eq!(state["P1"]["cemetery"], json!([]));
            assert_eq!(state["P1"]["deck"].as_array().unwrap().len(), 5);
            assert_eq!(state["P1"]["pp"]["current"], 9_u8);
            if !missing_target {
                assert_eq!(state["objects"]["enemy"]["damage"], 0_u8);
            }
        }
    }
}

#[test]
fn csd02a_010_missing_a_revive_target_or_pp_cannot_consume_payment() {
    for card in ["CSD02a-010", "CSD02a-P07"] {
        for missing_target in [false, true] {
            let mut initial = evolution_position(card);
            let available = if missing_target { 10_u8 } else { 3_u8 };
            initial["players"]["P1"]["pp"]["current"] = json!(available);
            initial["players"]["P1"]["zones"]["cemetery"] = if missing_target {
                json!([{"id":"wrong","card":"ready-wrong"},{"id":"amulet","card":"ready-cute-amulet"}])
            } else {
                json!([{"id":"revive","card":"ready-cute"}])
            };
            let game = evolve_form(&initial, card, true);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["P1"]["field"], json!(["source"]));
            assert_eq!(state["P1"]["pp"]["current"], available - 1);
            assert_eq!(
                state["P1"]["cemetery"],
                if missing_target {
                    json!(["wrong", "amulet"])
                } else {
                    json!(["revive"])
                }
            );
        }
    }
}

#[test]
fn csd02a_011_a_non_cute_hand_card_cannot_pay_for_health_or_a_draw() {
    for card in ["CSD02a-011", "CSD02a-P08"] {
        let mut initial = position(card);
        initial["players"]["P1"]["zones"]["hand"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"wrong","card":"ready-wrong"}));
        let mut game = game(&initial);
        play(&mut game, 1);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["hand"], json!(["wrong"]));
        assert_eq!(state["P1"]["cemetery"], json!([]));
        assert_eq!(state["objects"]["source"]["hp"], 3_u8);
        assert_eq!(state["P1"]["pp"]["current"], 7_u8);
        assert!(
            has_keyword(&state, "source", "守護"),
            "lack of a material cannot remove printed Ward"
        );
    }
}

#[test]
fn csd02a_007_enters_without_buffing_itself_when_no_other_cute_follower_exists() {
    for card in ["CSD02a-007", "CSD02a-P04"] {
        let mut initial = position(card);
        initial["players"]["P1"]["zones"]["field"] =
            json!([{"id":"wrong","card":"ready-wrong"},{"id":"amulet","card":"ready-cute-amulet"}]);
        let mut game = game(&initial);
        play(&mut game, 1);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["field"], json!(["wrong", "amulet", "source"]));
        assert_eq!(state["objects"]["source"]["hp"], 3_u8);
        assert_eq!(state["objects"]["wrong"]["hp"], 4_u8);
        assert_eq!(state["P1"]["pp"]["current"], 7_u8);
    }
}
