#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate printed card scenarios."
)]
use super::*;
fn fixture() -> &'static SelectedEffects {
    static SELECTED: OnceLock<SelectedEffects> = OnceLock::new();
    SELECTED.get_or_init(|| {
  let (mut snapshot, keywords, mut input)=producer_inputs();
  // Minimal companions isolate printed trait filters and payments from unrelated abilities.
  for (number, kind, traits, cost) in [
   ("ready-academy", "フォロワー", vec!["学院"], "2"),
   ("ready-wrong", "フォロワー", vec![], "1"),
   ("ready-academy-amulet", "アミュレット", vec!["学院"], "1"),
   ("ready-expensive", "フォロワー", vec!["学院"], "3"),
  ] {
   snapshot.push_str(&json!({"number":number,"faces":[{"name":number,"card_class":"ニュートラル","card_type":kind,"cost":cost,"power":if kind=="フォロワー" {"3"} else {"-"},"hp":if kind=="フォロワー" {"4"} else {"-"},"traits":traits,"text":null,"sections":[]}]}).to_string());snapshot.push('\n');
   input["documents"].as_array_mut().unwrap().push(json!([format!("{number}.yaml"),json!({"format":1_u8,"kind":"effect_set","cards":{number:{"status":"complete","abilities":[]}}}).to_string()]));
  }
  let form = "DSD01a-011";
   let name=snapshot.lines().map(|line|serde_json::from_str::<Value>(line).unwrap()).find(|row|row["number"]==form).unwrap()["faces"][0]["name"].clone();
   let number=format!("ready-base-{form}");
   snapshot.push_str(&json!({"number":number,"faces":[{"name":name,"card_class":"ニュートラル","card_type":"フォロワー","cost":"0","power":"3","hp":"4","traits":["学院"],"text":null,"sections":[]}]}).to_string());snapshot.push('\n');
   input["documents"].as_array_mut().unwrap().push(json!([format!("{number}.yaml"),json!({"format":1_u8,"kind":"effect_set","cards":{number:{"status":"complete","abilities":[]}}}).to_string()]));
  repository::selection_with_rules_from(&snapshot,&keywords,&input)
 })
}
fn position(card: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["construction"] = json!("class");
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
        "ready-dsd01a-1",
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
            InputDomain::CostMaterials(domain) => Answer::CostMaterials(
                domain
                    .members
                    .iter()
                    .take(usize::try_from(domain.cardinality.minimum.0).unwrap())
                    .map(|m| m.handle)
                    .collect(),
            ),
            InputDomain::Targets(domain) => {
                assert_eq!(
                    domain.members.len(),
                    2,
                    "only the two low-cost Academy cards are offered"
                );
                assert_eq!(
                    domain.cardinality.minimum.0, 0,
                    "choosing no observed cards must remain legal"
                );
                assert_eq!(
                    domain.cardinality.maximum.unwrap().0,
                    2,
                    "the printed upper bound is two observed cards"
                );
                assert!(
                    domain.members.iter().all(|member| matches!(
                        member.card.as_deref(),
                        Some("ready-academy" | "ready-academy-amulet")
                    )),
                    "the eligibility filter includes Academy cards of both permanent kinds"
                );
                Answer::Targets(domain.members.iter().map(|member| member.handle).collect())
            }
            InputDomain::Order(cards) => {
                Answer::Order(cards.iter().rev().map(|m| m.handle).collect())
            }
            InputDomain::Modes { .. }
            | InputDomain::Search(_)
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
    let evolved = game.projection(View::Referee).unwrap();
    assert_eq!(
        evolved["P1"]["pp"]["current"], 9_u8,
        "evolution spends one PP"
    );
    assert_eq!(
        evolved["objects"]["source"]["evolved_with"], "form",
        "the actual rule procedure associates the evolution card"
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
        "ready-academy",
        "ready-wrong",
        "ready-academy-amulet",
        "ready-expensive",
    ] {
        game.load_internal_card(number, &empty).unwrap();
    }
}

#[test]
fn dsd01a_011_evolution_places_two_low_cost_academy_cards_in_ex_with_turn_discounts() {
    for card in ["DSD01a-011", "DSD01a-P05"] {
        let mut initial = evolution_position(card);
        initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"pick0","card":"ready-academy"},{"id":"pick1","card":"ready-academy-amulet"},{"id":"expensive","card":"ready-expensive"},{"id":"wrong","card":"ready-wrong"},{"id":"tail","card":"ready-wrong"}]);
        let mut game = evolve_form(&initial, card, true);
        let chosen = game.projection(View::Referee).unwrap();
        assert_eq!(chosen["P1"]["ex"].as_array().unwrap().len(), 2);
        assert_eq!(chosen["P1"]["deck"], json!(["tail", "wrong", "expensive"]));
        for id in ["pick0", "pick1"] {
            let step = game.play_internal_card(id, PlayerId { seat: 1 }).unwrap();
            finish(&mut game, step, 1);
            settle(&mut game, 1);
        }
        let played = game.projection(View::Referee).unwrap();
        assert_eq!(played["P1"]["pp"]["current"], 9_u8);
        assert!(
            played["P1"]["field"]
                .as_array()
                .unwrap()
                .contains(&json!("pick0"))
                && played["P1"]["field"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("pick1")),
            "both eligible Academy cards play from EX for zero this turn"
        );
    }
}
