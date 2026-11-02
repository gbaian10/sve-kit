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
   ("ready-royal", "フォロワー", vec!["ロイヤルパラディン"], "3"),
   ("ready-wrong", "フォロワー", vec![], "1"),
   ("ready-royal-amulet", "アミュレット", vec!["ロイヤルパラディン"], "1"),
   ("ready-blaster", "フォロワー", vec!["ロイヤルパラディン"], "2"),
  ] {
   snapshot.push_str(&json!({"number":number,"faces":[{"name":if number=="ready-blaster" {"ブラスター・ブレード"} else {number},"card_class":"ニュートラル","card_type":kind,"cost":cost,"power":if kind=="フォロワー" {"3"} else {"-"},"hp":if kind=="フォロワー" {"4"} else {"-"},"traits":traits,"text":null,"sections":[]}]}).to_string());snapshot.push('\n');
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
        "ready-csd03a-1",
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
                    1,
                    "only the printed Royal Paladin hand material may be offered"
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
        "ready-royal",
        "ready-wrong",
        "ready-royal-amulet",
        "ready-blaster",
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
fn csd03a_005_three_pp_and_act_reanimate_only_the_named_grave_card() {
    for card in ["CSD03a-005", "CSD03a-P01"] {
        let mut initial = position(card);
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":card}]);
        initial["players"]["P1"]["zones"]["cemetery"] =
            json!([{"id":"blaster","card":"ready-blaster"},{"id":"wrong","card":"ready-royal"}]);
        initial["players"]["P2"]["zones"]["cemetery"] =
            json!([{"id":"foreign","card":"ready-blaster"}]);
        let mut game = game(&initial);
        native_activate(&mut game, card, 0);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["pp"]["current"], 7_u8);
        assert!(
            state["P1"]["field"]
                .as_array()
                .unwrap()
                .contains(&json!("blaster")),
            "the named grave follower enters the field"
        );
        assert!(
            state["P1"]["cemetery"]
                .as_array()
                .unwrap()
                .contains(&json!("wrong")),
            "a different Royal Paladin name is not eligible"
        );
        assert_eq!(state["objects"]["source"]["acted"], true);
    }
}
#[test]
fn csd03a_010_optional_discard_recovers_only_a_preexisting_royal_grave_follower() {
    for card in ["CSD03a-010", "CSD03a-P06"] {
        for paid in [true, false] {
            let mut initial = position(card);
            initial["players"]["P1"]["zones"]["hand"]
                .as_array_mut()
                .unwrap()
                .extend([
                    json!({"id":"material","card":"ready-royal"}),
                    json!({"id":"wrong-hand","card":"ready-wrong"}),
                ]);
            initial["players"]["P1"]["zones"]["cemetery"] = json!([{"id":"recover","card":"ready-royal"},{"id":"amulet","card":"ready-royal-amulet"},{"id":"wrong","card":"ready-wrong"}]);
            initial["players"]["P2"]["zones"]["cemetery"] =
                json!([{"id":"foreign","card":"ready-royal"}]);
            let mut game = game(&initial);
            play(&mut game, if paid { 1 } else { 2 });
            let state = game.projection(View::Referee).unwrap();
            assert!(
                state["objects"]["source"]["keywords"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("守護")),
                "the printed Ward keyword remains active"
            );
            let hand = state["P1"]["hand"].as_array().unwrap();
            let grave = state["P1"]["cemetery"].as_array().unwrap();
            assert_eq!(hand.contains(&json!("recover")), paid);
            assert_eq!(grave.contains(&json!("material")), paid);
            assert!(
                grave.contains(&json!("amulet")) && grave.contains(&json!("wrong")),
                "kind and trait negatives remain in the grave"
            );
            assert!(
                hand.contains(&json!("wrong-hand")),
                "the wrong hand trait cannot pay the cost"
            );
        }
    }
}
