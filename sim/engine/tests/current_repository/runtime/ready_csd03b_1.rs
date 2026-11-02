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
   ("ready-kagero", "フォロワー", vec!["かげろう"], "3"),
   ("ready-wrong", "フォロワー", vec![], "1"),
   ("ready-sturdy", "フォロワー", vec![], "1"),
   ("ready-kagero-amulet", "アミュレット", vec!["かげろう"], "1"),
  ] {
   snapshot.push_str(&json!({"number":number,"faces":[{"name":number,"card_class":"ニュートラル","card_type":kind,"cost":cost,"power":if kind=="フォロワー" {"3"} else {"-"},"hp":if kind=="フォロワー" {if number=="ready-sturdy" {"6"} else {"4"}} else {"-"},"traits":traits,"text":null,"sections":[]}]}).to_string());snapshot.push('\n');
   input["documents"].as_array_mut().unwrap().push(json!([format!("{number}.yaml"),json!({"format":1_u8,"kind":"effect_set","cards":{number:{"status":"complete","abilities":[]}}}).to_string()]));
  }
  repository::selection_with_rules_from(&snapshot,&keywords,&input)
 })
}
fn position(card: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["construction"] = json!("title");
    initial["players"]["P1"]["title_code"] = json!("cardfight_vanguard");
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
        "ready-csd03b-1",
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
            InputDomain::CostMaterials(domain) => {
                assert_eq!(
                    domain.members.len(),
                    4,
                    "only four own Kagero grave cards are offered"
                );
                Answer::CostMaterials(
                    domain
                        .members
                        .iter()
                        .take(usize::try_from(domain.cardinality.minimum.0).unwrap())
                        .map(|m| m.handle)
                        .collect(),
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
fn load_companions(game: &mut Game) {
    let empty = WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![],
    })
    .unwrap();
    for number in [
        "ready-kagero",
        "ready-wrong",
        "ready-sturdy",
        "ready-kagero-amulet",
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

#[test]
fn csd03b_003_acts_and_banishes_four_own_kagero_to_damage_all_opposing_characters() {
    for card in ["CSD03b-003", "CSD03b-SL03"] {
        let mut initial = position(card);
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] =
            json!([{"id":"source","card":card},{"id":"ally","card":"ready-wrong"}]);
        let mut materials = (0_u8..4)
            .map(|i| json!({"id":format!("material{i}"),"card":if i==3 {"ready-kagero-amulet"} else {"ready-kagero"}}))
            .collect::<Vec<_>>();
        materials.push(json!({"id":"wrong","card":"ready-wrong"}));
        initial["players"]["P1"]["zones"]["cemetery"] = json!(materials);
        initial["players"]["P2"]["zones"]["cemetery"] =
            json!([{"id":"foreign","card":"ready-kagero"}]);
        initial["players"]["P2"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"sturdy","card":"ready-sturdy"}));
        let mut game = game(&initial);
        native_activate(&mut game, card, 0);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["source"]["acted"], true);
        assert_eq!(state["P2"]["leader"]["life"], 16_u8);
        assert_eq!(state["objects"]["sturdy"]["hp"], 2_u8);
        assert!(
            state["objects"]["source"]["keywords"]
                .as_array()
                .unwrap()
                .contains(&json!("守護")),
            "printed Ward remains active"
        );
        assert_eq!(state["P1"]["leader"]["life"], 10_u8);
        assert!(
            state["P2"]["cemetery"]
                .as_array()
                .unwrap()
                .contains(&json!("enemy")),
            "four damage destroys the opposing four-health follower"
        );
        assert!(
            state["P1"]["field"]
                .as_array()
                .unwrap()
                .contains(&json!("ally")),
            "own followers are excluded from the damage"
        );
        let banished = state["P1"]["banish"].as_array().unwrap();
        for i in 0_u8..4 {
            assert!(
                banished.contains(&json!(format!("material{i}"))),
                "all four own Kagero cards pay the cost"
            );
        }
        assert!(
            state["P1"]["cemetery"]
                .as_array()
                .unwrap()
                .contains(&json!("wrong")),
            "the wrong trait cannot pay the cost"
        );
        assert!(
            state["P2"]["cemetery"]
                .as_array()
                .unwrap()
                .contains(&json!("foreign")),
            "opposing Kagero cannot pay the cost"
        );
    }
}
