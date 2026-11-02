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
  for (number, kind, traits, cost) in [
   ("ready-passion", "フォロワー", vec!["パッション"], "3"),
   ("ready-wrong", "フォロワー", vec![], "1"),
   ("ready-passion-amulet", "アミュレット", vec!["パッション"], "1"),
   ("ready-expensive", "フォロワー", vec!["パッション"], "4"),
  ] {
   snapshot.push_str(&json!({"number":number,"faces":[{"name":number,"card_class":"ニュートラル","card_type":kind,"cost":cost,"power":if kind=="フォロワー" {"3"} else {"-"},"hp":if kind=="フォロワー" {"4"} else {"-"},"traits":traits,"text":null,"sections":[]}]}).to_string());snapshot.push('\n');
   input["documents"].as_array_mut().unwrap().push(json!([format!("{number}.yaml"),json!({"format":1_u8,"kind":"effect_set","cards":{number:{"status":"complete","abilities":[]}}}).to_string()]));
  }
  for form in ["CSD02c-008","CSD02c-011"] {
   let name=snapshot.lines().map(|line|serde_json::from_str::<Value>(line).unwrap()).find(|row|row["number"]==form).unwrap()["faces"][0]["name"].clone();
   let number=format!("ready-base-{form}");
   snapshot.push_str(&json!({"number":number,"faces":[{"name":name,"card_class":"ニュートラル","card_type":"フォロワー","cost":"0","power":"3","hp":"4","traits":["パッション"],"text":null,"sections":[]}]}).to_string());snapshot.push('\n');
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
        "ready-csd02c-1",
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
            InputDomain::Targets(domain) => {
                assert_eq!(
                    domain.members.len(),
                    1,
                    "only the printed eligible cemetery follower may be offered"
                );
                Answer::Targets(
                    domain
                        .members
                        .first()
                        .map(|member| vec![member.handle])
                        .unwrap_or_default(),
                )
            }
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

fn load_companions(game: &mut Game) {
    let empty = WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![],
    })
    .unwrap();
    for number in [
        "ready-passion",
        "ready-wrong",
        "ready-passion-amulet",
        "ready-expensive",
    ] {
        game.load_internal_card(number, &empty).unwrap();
    }
}

#[test]
fn csd02c_008_optional_passion_discard_draws_two_only_when_paid() {
    for card in ["CSD02c-008", "CSD02c-P05"] {
        for mode in 0_u8..3 {
            let mut initial = evolution_position(card);
            initial["players"]["P1"]["zones"]["hand"] = if mode == 2 {
                json!([{"id":"wrong","card":"ready-wrong"}])
            } else {
                json!([{"id":"material","card":"ready-passion"},{"id":"wrong","card":"ready-wrong"}])
            };
            let game = evolve_form(&initial, card, mode != 1);
            let state = game.projection(View::Referee).unwrap();
            let hand = state["P1"]["hand"].as_array().unwrap();
            assert_eq!(
                hand.len(),
                match mode {
                    0 => 3,
                    1 => 2,
                    _ => 1,
                }
            );
            assert_eq!(
                state["P1"]["cemetery"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("material")),
                mode == 0
            );
            assert!(
                hand.contains(&json!("wrong")),
                "the wrong trait must never be paid as Passion"
            );
            assert_eq!(
                state["P1"]["deck"].as_array().unwrap().len(),
                if mode == 0 { 3 } else { 5 }
            );
        }
    }
}
#[test]
fn csd02c_011_evolution_reanimates_only_a_low_cost_passion_follower() {
    for card in ["CSD02c-011", "CSD02c-P08"] {
        let mut initial = evolution_position(card);
        initial["players"]["P2"]["zones"]["cemetery"] =
            json!([{"id":"opposing","card":"ready-passion"}]);
        initial["players"]["P1"]["zones"]["cemetery"] = json!([{"id":"eligible","card":"ready-passion"},{"id":"wrong","card":"ready-wrong"},{"id":"amulet","card":"ready-passion-amulet"},{"id":"expensive","card":"ready-expensive"}]);
        let game = evolve_form(&initial, card, true);
        let state = game.projection(View::Referee).unwrap();
        assert!(
            state["P1"]["field"]
                .as_array()
                .unwrap()
                .contains(&json!("eligible")),
            "the selected grave follower enters the field"
        );
        for rejected in ["wrong", "amulet", "expensive"] {
            assert!(
                state["P1"]["cemetery"]
                    .as_array()
                    .unwrap()
                    .contains(&json!(rejected)),
                "wrong trait, kind and cost stay in the cemetery"
            );
        }
    }
}
