#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate printed card scenarios."
)]
use super::*;
use sve_engine::game::internal::SearchAnswer;
fn fixture() -> &'static SelectedEffects {
    static SELECTED: OnceLock<SelectedEffects> = OnceLock::new();
    SELECTED.get_or_init(|| {
  let (mut snapshot, keywords, mut input) = producer_inputs();
  // Minimal companions isolate evolution and the printed Princess follower filter.
  for (number, name, kind, traits) in [
   ("ready-base-PCS01-049", "プリンセスナイト", "フォロワー", vec![]),
   ("ready-princess", "eligible follower", "フォロワー", vec!["プリコネ"]),
   ("ready-wrong", "plain follower", "フォロワー", vec![]),
   ("ready-amulet", "princess amulet", "アミュレット", vec!["プリコネ"]),
  ] {
   snapshot.push_str(&json!({"number":number,"faces":[{"name":name,"card_class":"ニュートラル","card_type":kind,"cost":"0","power":if kind=="フォロワー" {"2"} else {"-"},"hp":if kind=="フォロワー" {"20"} else {"-"},"traits":traits,"text":"","sections":[]}]}).to_string()); snapshot.push('\n');
   input["documents"].as_array_mut().unwrap().push(json!([format!("{number}.yaml"),json!({"format":1_u8,"kind":"effect_set","cards":{number:{"status":"complete","abilities":[]}}}).to_string()]));
  }
  repository::selection_with_rules_from(&snapshot,&keywords,&input)
 })
}
fn position() -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["sep"] = json!(0_u8);
    initial["players"]["P1"]["zones"]["deck"] = json!(
        (0_u8..5)
            .map(|i| json!({"id":format!("draw{i}"),"card":"BP01-171"}))
            .collect::<Vec<_>>()
    );
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"ready-princess"}]);
    initial
}
fn game(initial: &Value) -> Game {
    let mut game = Game::from_selected_effects(
        fixture().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "ready-pcs01-1",
    )
    .unwrap();
    let empty = WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![],
    })
    .unwrap();
    for card in ["ready-princess", "ready-wrong", "ready-amulet"] {
        game.load_internal_card(card, &empty).unwrap();
    }
    game
}
fn finish(game: &mut Game, mut step: CardPlay, mode: u64) {
    while let CardPlay::Awaiting(question) = step {
        let answer = match &question.domain {
            InputDomain::Modes { .. } => Answer::Modes(vec![ModeId(mode)]),
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            InputDomain::OptionalCost => Answer::OptionalCost(mode != 3),
            InputDomain::Targets(domain) => {
                assert_eq!(
                    domain.members.len(),
                    1,
                    "only the other allied Princess follower is a formal target"
                );
                Answer::Targets(
                    if mode == 2 {
                        domain.members.last()
                    } else {
                        domain.members.first()
                    }
                    .map(|m| vec![m.handle])
                    .unwrap_or_default(),
                )
            }
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
fn evolution_position(form: &str) -> Value {
    let mut initial = position();
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
fn pcs01_049_evolution_buffs_only_another_princess_follower_and_grants_ward() {
    let form = "PCS01-049";
    let mut initial = evolution_position(form);
    initial["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .extend([
            json!({"id":"ally","card":"ready-princess"}),
            json!({"id":"wrong","card":"ready-wrong"}),
            json!({"id":"amulet","card":"ready-amulet"}),
        ]);
    let game = evolved(&initial, form);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["source"]["evolved"], json!(true));
    assert_eq!(state["objects"]["ally"]["power"], json!(12_u8));
    assert_eq!(state["objects"]["ally"]["hp"], json!(30_u8));
    assert!(has_keyword(&state, "ally", "守護"));
    for id in ["source", "wrong", "amulet", "enemy"] {
        assert!(
            !has_keyword(&state, id, "守護"),
            "only another allied Princess follower gains ward: {id}"
        );
    }
    assert_eq!(state["objects"]["source"]["power"], json!(3_u8));
    assert_eq!(state["objects"]["wrong"]["power"], json!(2_u8));
    assert_eq!(state["objects"]["enemy"]["power"], json!(2_u8));
    assert_eq!(state["P1"]["pp"]["current"], json!(9_u8));
}
