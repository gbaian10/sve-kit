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
  // Minimal companions isolate printed search, type and original-cost predicates.
  for (number,name,kind,cost,traits) in [
   ("ready-uma","ready-uma","フォロワー","1",vec!["ウマ娘"]),
   ("ready-uma2","ready-uma2","フォロワー","2",vec!["ウマ娘"]),
   ("ready-uma3","ready-uma3","フォロワー","3",vec!["ウマ娘"]),
   ("ready-uma4","ready-uma4","フォロワー","4",vec!["ウマ娘"]),
   ("ready-wrong","ready-wrong","フォロワー","1",vec![]),
   ("ready-amulet","ready-amulet","アミュレット","1",vec!["ウマ娘"]),
  ] {
   snapshot.push_str(&json!({"number":number,"faces":[{"name":name,"card_class":"ニュートラル","card_type":kind,"cost":cost,"power":if kind=="フォロワー" {"2"} else {"-"},"hp":if kind=="フォロワー" {"20"} else {"-"},"traits":traits,"text":"","sections":[]}]}).to_string());snapshot.push('\n');
   input["documents"].as_array_mut().unwrap().push(json!([format!("{number}.yaml"),json!({"format":1_u8,"kind":"effect_set","cards":{number:{"status":"complete","abilities":[]}}}).to_string()]));
  }
  for (number,name,kind,cost) in [("ready-passion","パッションチャンピオーナ！","アミュレット","7"),("ready-base","サトノクラウン","フォロワー","0"),("ready-hishi","ヒシミラクル","フォロワー","0")] {
   snapshot.push_str(&json!({"number":number,"faces":[{"name":name,"card_class":"ニュートラル","card_type":kind,"cost":cost,"power":if kind=="フォロワー" {"3"} else {"-"},"hp":if kind=="フォロワー" {"3"} else {"-"},"traits":["ウマ娘"],"text":"","sections":[]}]}).to_string());snapshot.push('\n');
   input["documents"].as_array_mut().unwrap().push(json!([format!("{number}.yaml"),json!({"format":1_u8,"kind":"effect_set","cards":{number:{"status":"complete","abilities":[]}}}).to_string()]));
  }
  repository::selection_with_rules_from(&snapshot,&keywords,&input)
 })
}
fn position(card: &str, zone: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    initial["players"]["P1"]["sep"] = json!(0_u8);
    initial["players"]["P1"]["zones"][zone] = json!([{"id":"source","card":card}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!(
        (0_u8..3)
            .map(|i| json!({"id":format!("carrot{i}"),"card":"CSD01-032"}))
            .collect::<Vec<_>>()
    );
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
        "ready-ecp01-2",
    )
    .unwrap();
    load_companions(&mut game);
    game
}
fn finish(game: &mut Game, mut step: CardPlay, mode: u64) -> CardPlay {
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
                let expected = if domain.cardinality.maximum == Some(Nat(2)) {
                    2
                } else {
                    1
                };
                assert_eq!(
                    domain.members.len(),
                    expected,
                    "only printed eligible cards may be offered"
                );
                Answer::Targets(domain.members.iter().map(|m| m.handle).collect())
            }
            InputDomain::CostMaterials(domain) => {
                assert_eq!(
                    domain.members.len(),
                    usize::try_from(domain.cardinality.minimum.0).unwrap(),
                    "only the printed legal grave materials may be offered"
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
            InputDomain::Search(domain) => {
                assert_eq!(
                    domain.cards.len(),
                    1,
                    "only the single printed eligible deck card may be offered"
                );
                Answer::Search(SearchAnswer::Ungrouped(
                    domain.cards.iter().map(|m| m.handle).collect(),
                ))
            }
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
    step
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
        "ready-uma",
        "ready-uma2",
        "ready-uma3",
        "ready-uma4",
        "ready-wrong",
        "ready-amulet",
        "ready-passion",
    ] {
        game.load_internal_card(number, &empty).unwrap();
    }
}

fn native_activate(game: &mut Game, card: &str, index: usize) -> Activation {
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
    let activation = resolved(finish(game, step, 1));
    settle(game, 1);
    activation
}

fn keyword(state: &Value, id: &str, name: &str) -> bool {
    state["objects"][id]["keywords"]
        .as_array()
        .unwrap()
        .contains(&json!(name))
}
fn feed(game: &mut Game, card: &str, index: usize) -> Activation {
    let ability = fixture().current[card]
        .program
        .card()
        .abilities
        .iter()
        .filter(|a| {
            matches!(
                a.node.source.origin,
                AbilityOrigin::Keyword {
                    keyword: KeywordId::Meal,
                    ..
                }
            )
        })
        .nth(index)
        .unwrap()
        .node
        .id;
    let CardPlay::Awaiting(question) = game
        .activate_internal_card("source", ability, PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("printed meal payment")
    };
    let InputDomain::ResourceEntryPayment { materials, .. } = &question.domain else {
        panic!("printed meal resource domain")
    };
    let handles = materials
        .members
        .iter()
        .take(usize::try_from(materials.cardinality.minimum.0).unwrap())
        .map(|m| m.handle)
        .collect();
    let activation = resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::ResourceEntryPayment {
                materials: handles,
                use_ep: false,
            },
        )
        .unwrap(),
    );
    settle(game, 1);
    activation
}
#[test]
fn ecp01_034_storm_follower_recruits_the_named_amulet_and_really_races() {
    for card in ["ECP01-034", "ECP01-P18"] {
        let mut initial = position(card, "hand");
        initial["players"]["P1"]["zones"]["deck"] =
            json!([{"id":"passion","card":"ready-passion"},{"id":"wrong","card":"ready-amulet"}]);
        let mut game = game(&initial);
        play(&mut game, 1);
        let state = game.projection(View::Referee).unwrap();
        assert!(
            state["P1"]["field"]
                .as_array()
                .unwrap()
                .contains(&json!("passion")),
            "the named amulet enters the field"
        );
        assert!(
            keyword(&state, "source", "疾走"),
            "printed Storm remains active"
        );
        let activation = feed(&mut game, card, 0);
        assert_eq!(
            activation
                .progress
                .facts
                .iter()
                .filter(|f| matches!(f, Fact::Raced(_)))
                .count(),
            1
        );
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
            2_u8
        );
    }
}

#[test]
fn ecp01_040_fanfare_destroys_damages_heals_and_mills_then_race_checks_ten_uma_cards() {
    for card in ["ECP01-040", "ECP01-SL20", "ECP01-SP20", "ECP01-SSP20"] {
        let mut fanfare = game(&position(card, "hand"));
        play(&mut fanfare, 1);
        let entered = fanfare.projection(View::Referee).unwrap();
        assert!(
            entered["P2"]["cemetery"]
                .as_array()
                .unwrap()
                .contains(&json!("enemy")),
            "fanfare destroys its formal target"
        );
        assert_eq!(entered["P2"]["leader"]["life"], 18_u8);
        assert_eq!(entered["P1"]["leader"]["life"], 12_u8);
        assert_eq!(entered["P1"]["cemetery"], json!(["draw0", "draw1"]));
        for count in [9_u8, 10] {
            let mut initial = position(card, "field");
            initial["players"]["P1"]["pp"]["current"] = json!(3_u8);
            initial["players"]["P1"]["zones"]["cemetery"]=json!((0..count).map(|i|json!({"id":format!("grave{i}"),"card":if i==0 {"ready-amulet"} else {"ready-uma"}})).collect::<Vec<_>>());
            let mut racing = game(&initial);
            feed(&mut racing, card, 0);
            let raced = racing.projection(View::Referee).unwrap();
            assert_eq!(raced["objects"]["source"]["power"], 5_u8);
            assert_eq!(raced["objects"]["source"]["hp"], 5_u8);
            assert_eq!(
                raced["P1"]["pp"]["current"],
                if count == 10 { 5_u8 } else { 2_u8 }
            );
        }
    }
}

#[test]
fn ecp01_041_fanfare_mills_and_racing_revives_a_low_cost_uma_before_growing() {
    for card in ["ECP01-041", "ECP01-P21"] {
        let mut initial = position(card, "hand");
        initial["players"]["P1"]["zones"]["cemetery"] = json!([{"id":"recover","card":"ready-uma3"},{"id":"expensive","card":"ready-uma4"},{"id":"wrong","card":"ready-wrong"},{"id":"amulet","card":"ready-amulet"}]);
        let mut game = game(&initial);
        play(&mut game, 1);
        assert!(
            game.projection(View::Referee).unwrap()["P1"]["cemetery"]
                .as_array()
                .unwrap()
                .contains(&json!("draw0")),
            "fanfare mills one card"
        );
        feed(&mut game, card, 0);
        let state = game.projection(View::Referee).unwrap();
        assert!(
            state["P1"]["field"]
                .as_array()
                .unwrap()
                .contains(&json!("recover")),
            "racing revives the original cost-three Uma follower"
        );
        assert_eq!(state["objects"]["source"]["power"], 4_u8);
        assert_eq!(state["objects"]["source"]["hp"], 4_u8);
        assert_eq!(state["P1"]["pp"]["current"], 6_u8);
    }
}

#[test]
fn ecp01_044_qualified_activation_acts_and_sacrifices_self_to_revive_exact_cost_two_uma() {
    for card in ["ECP01-044", "ECP01-P24"] {
        let mut initial = position(card, "hand");
        let mut grave = (0_u8..9)
            .map(|i| json!({"id":format!("grave{i}"),"card":"ready-uma"}))
            .collect::<Vec<_>>();
        grave.push(json!({"id":"recover","card":"ready-uma2"}));
        initial["players"]["P1"]["zones"]["cemetery"] = json!(grave);
        let mut game = game(&initial);
        play(&mut game, 1);
        let activation = native_activate(&mut game, card, 0);
        let state = game.projection(View::Referee).unwrap();
        assert!(
            state["P1"]["cemetery"]
                .as_array()
                .unwrap()
                .contains(&json!("source")),
            "the activated cost puts the source in the cemetery"
        );
        assert!(
            state["P1"]["field"]
                .as_array()
                .unwrap()
                .contains(&json!("recover")),
            "the exact-cost-two Uma follower enters the field"
        );
        // Changing zones resets posture; the cost fact proves the prior act.
        let posture = activation
            .progress
            .facts
            .iter()
            .position(|f| matches!(f, Fact::Posture(_)))
            .unwrap();
        let moved = activation
            .progress
            .facts
            .iter()
            .position(|f| matches!(f, Fact::Movement(_)))
            .unwrap();
        let played = activation
            .progress
            .facts
            .iter()
            .position(|f| matches!(f, Fact::AbilityPlayed { .. }))
            .unwrap();
        assert!(
            posture < moved && moved < played,
            "acting and self-movement must precede the accepted ability"
        );
        assert_eq!(state["objects"]["source"]["acted"], false);
        assert_eq!(state["P1"]["pp"]["current"], 9_u8);
        let mut below = position(card, "field");
        let mut nine = (0_u8..8)
            .map(|i| json!({"id":format!("grave{i}"),"card":"ready-uma"}))
            .collect::<Vec<_>>();
        nine.push(json!({"id":"recover","card":"ready-uma2"}));
        below["players"]["P1"]["zones"]["cemetery"] = json!(nine);
        let mut rejected = Game::from_selected_effects(
            fixture().clone(),
            &below,
            &Value::Null,
            &Value::Null,
            "ready-ecp01-2",
        )
        .unwrap();
        load_companions(&mut rejected);
        let ability = fixture().current[card]
            .program
            .card()
            .abilities
            .iter()
            .find(|a| {
                a.node.kind == AbilityKind::Activated
                    && matches!(a.node.source.origin, AbilityOrigin::Printed { .. })
            })
            .unwrap()
            .node
            .id;
        let before = rejected.projection(View::Referee).unwrap();
        assert!(
            rejected
                .activate_internal_card("source", ability, PlayerId { seat: 1 })
                .is_err(),
            "nine Uma cards cannot activate the ability"
        );
        assert_eq!(
            rejected.projection(View::Referee).unwrap(),
            before,
            "rejection must not act or sacrifice the source"
        );
    }
}

#[test]
fn ecp01_047_actual_evolution_places_two_uma_card_kinds_in_ex_and_orders_the_rest() {
    for card in ["ECP01-047", "ECP01-SL22", "ECP01-SP22", "ECP01-SSP22"] {
        let mut initial = position(card, "field");
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"ready-base"}]);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":card}]);
        initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"pick0","card":"ready-uma"},{"id":"pick1","card":"ready-amulet"},{"id":"wrong0","card":"ready-wrong"},{"id":"wrong1","card":"ready-wrong"},{"id":"tail","card":"ready-wrong"}]);
        let mut game = game(&initial);
        let mut tree = expanded::tree().clone();
        tree["cards"] = json!({"ready-base":tree["cards"]["synthetic-evolution"]});
        let program = lowerer()
            .lower(&tree)
            .unwrap()
            .remove("ready-base")
            .unwrap();
        game.load_internal_card("ready-base", &program).unwrap();
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
        assert_eq!(state["P1"]["ex"], json!(["pick0", "pick1"]));
        assert_eq!(state["P1"]["deck"], json!(["tail", "wrong1", "wrong0"]));
        assert_eq!(state["P1"]["pp"]["current"], 9_u8);
    }
}

#[test]
fn ecp01_056_spell_searches_an_uma_follower_and_excludes_an_amulet() {
    for card in ["ECP01-056", "ECP01-SL26"] {
        let mut initial = position(card, "hand");
        initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"recover","card":"ready-uma"},{"id":"wrong","card":"ready-wrong"},{"id":"amulet","card":"ready-amulet"}]);
        let mut game = game(&initial);
        play(&mut game, 1);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["hand"], json!(["recover"]));
        assert_eq!(state["P1"]["pp"]["current"], 8_u8);
        assert!(
            state["P1"]["cemetery"]
                .as_array()
                .unwrap()
                .contains(&json!("source")),
            "the resolved spell enters the cemetery"
        );
    }
}

#[test]
fn ecp01_038_actual_evolution_discards_two_uma_and_only_nine_face_up_carrots_mill_twenty() {
    for card in ["ECP01-038", "ECP01-SL18", "ECP01-SP18", "ECP01-SSP18"] {
        for face_up in [8_u8, 9] {
            let mut initial = position(card, "field");
            initial["players"]["P1"]["zones"]["field"] =
                json!([{"id":"source","card":"ready-hishi"}]);
            initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"material0","card":"ready-uma"},{"id":"material1","card":"ready-amulet"},{"id":"wrong","card":"ready-wrong"}]);
            let mut resources = (0_u8..9)
                .map(|i| json!({"id":format!("carrot{i}"),"card":"CSD01-032","face_up":i<face_up}))
                .collect::<Vec<_>>();
            resources.push(json!({"id":"form","card":card}));
            initial["players"]["P1"]["zones"]["evolve_deck"] = json!(resources);
            initial["players"]["P2"]["zones"]["deck"] = json!(
                (0_u8..21)
                    .map(|i| json!({"id":format!("opponent{i}"),"card":"ready-wrong"}))
                    .collect::<Vec<_>>()
            );
            let mut game = game(&initial);
            let mut tree = expanded::tree().clone();
            tree["cards"] = json!({"ready-hishi":tree["cards"]["synthetic-evolution"]});
            let program = lowerer()
                .lower(&tree)
                .unwrap()
                .remove("ready-hishi")
                .unwrap();
            game.load_internal_card("ready-hishi", &program).unwrap();
            assert_eq!(
                game.internal_rule_action(
                    &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
                )
                .unwrap()
                .outcome,
                "resolved"
            );
            settle(&mut game, 1);
            let after_evolution = game.projection(View::Referee).unwrap();
            assert_eq!(
                after_evolution["P1"]["hand"],
                json!(["wrong", "draw0", "draw1"])
            );
            assert_eq!(
                after_evolution["P1"]["cemetery"],
                json!(["material0", "material1"])
            );
            game.internal_rule_action(&json!({"do":"end-phase"}))
                .unwrap();
            settle(&mut game, 1);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["P2"]["cemetery"].as_array().unwrap().len(),
                if face_up == 9 { 20 } else { 0 }
            );
            assert_eq!(
                state["P2"]["deck"].as_array().unwrap().len(),
                if face_up == 9 { 1 } else { 21 }
            );
            assert_eq!(
                state["P1"]["deck"].as_array().unwrap().len(),
                3,
                "only opponents are milled"
            );
        }
    }
}
