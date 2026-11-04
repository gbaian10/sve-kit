#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate printed card scenarios."
)]
use super::*;
use sve_engine::game::internal::{DecisionView, SearchAnswer, SearchDomain};
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
   ("ready-uma7","ready-uma7","フォロワー","7",vec!["ウマ娘"]),
   ("ready-wrong","ready-wrong","フォロワー","1",vec![]),
   ("ready-amulet","ready-amulet","アミュレット","1",vec!["ウマ娘"]),
   ("ready-spell1","ready-spell1","スペル","1",vec!["ウマ娘"]),
   ("ready-spell2","ready-spell2","スペル","2",vec!["ウマ娘"]),
   ("ready-bnw","ready-bnw","フォロワー","2",vec!["BNW"]),
   ("ready-bnw2","ready-bnw2","フォロワー","2",vec!["BNW"]),
   ("ready-bnw3","ready-bnw3","フォロワー","2",vec!["BNW"]),
   ("ready-vodka","ウオッカ","フォロワー","2",vec!["ウマ娘"]),
  ] {
   snapshot.push_str(&json!({"number":number,"faces":[{"name":name,"card_class":"ニュートラル","card_type":kind,"cost":cost,"power":if kind=="フォロワー" {"2"} else {"-"},"hp":if kind=="フォロワー" {"20"} else {"-"},"traits":traits,"text":"","sections":[]}]}).to_string());snapshot.push('\n');
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
        "ready-ecp01-1",
    )
    .unwrap();
    load_companions(&mut game);
    game
}
fn grouped_uma_search_answer(domain: &SearchDomain, mode: u64) -> SearchAnswer {
    let groups = domain.groups.as_ref().unwrap();
    for (group, number) in groups.iter().zip(["ready-vodka", "ready-spell1"]) {
        assert_eq!(
            group.handles.len(),
            1,
            "each printed search group has one legal card"
        );
        assert_eq!(
            group.cardinality.minimum,
            Nat(0),
            "each conditional group can find no card"
        );
        assert_eq!(
            group.cardinality.maximum,
            Some(Nat(1)),
            "each printed group finds at most one card"
        );
        let offered = domain
            .cards
            .iter()
            .find(|m| m.handle == group.handles[0])
            .unwrap();
        assert_eq!(
            offered.card, number,
            "each group retains its printed name or type/cost filter"
        );
    }
    SearchAnswer::Grouped(
        groups
            .iter()
            .map(|g| if mode == 0 { vec![] } else { g.handles.clone() })
            .collect(),
    )
}
fn bnw_search_answer(
    game: &mut Game,
    question: &DecisionView,
    domain: &SearchDomain,
) -> SearchAnswer {
    let groups = domain.groups.as_ref().unwrap();
    assert_eq!(
        groups.len(),
        1,
        "the different-name BNW search has one printed group"
    );
    assert_eq!(
        domain.cards.len(),
        4,
        "only four BNW copies, with three distinct names, may match"
    );
    assert_eq!(
        domain.cardinality.maximum,
        Some(Nat(3)),
        "the printed search cap is three"
    );
    assert!(
        domain
            .cards
            .iter()
            .all(|m| matches!(m.card.as_str(), "ready-bnw" | "ready-bnw2" | "ready-bnw3")),
        "non-BNW cards cannot enter the search domain"
    );
    let duplicates = domain
        .cards
        .iter()
        .filter(|m| m.card == "ready-bnw")
        .map(|m| m.handle)
        .collect::<Vec<_>>();
    assert_eq!(
        duplicates.len(),
        2,
        "the negative answer selects two physical copies with the same name"
    );
    let before = game.projection(View::Referee).unwrap();
    let error = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Search(SearchAnswer::Grouped(vec![duplicates])),
        )
        .unwrap_err();
    assert!(
        error.to_string().contains("search names must differ"),
        "the duplicate answer is rejected by the distinct-name constraint"
    );
    assert_eq!(
        game.projection(View::Referee).unwrap(),
        before,
        "a rejected duplicate search leaves the position unchanged"
    );
    let selected = ["ready-bnw", "ready-bnw2", "ready-bnw3"]
        .into_iter()
        .map(|number| {
            domain
                .cards
                .iter()
                .find(|m| m.card == number)
                .unwrap()
                .handle
        })
        .collect();
    SearchAnswer::Grouped(vec![selected])
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
                    "only the printed eligible card may be offered"
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
            InputDomain::Search(domain) => Answer::Search(match domain.groups.as_ref() {
                Some(groups) if groups.len() == 2 => grouped_uma_search_answer(domain, mode),
                Some(_) => bnw_search_answer(game, &question, domain),
                None => {
                    assert_eq!(
                        domain.cards.len(),
                        1,
                        "the exact-cost Uma search offers only its eligible follower"
                    );
                    assert_eq!(
                        domain.cards[0].card, "ready-uma",
                        "wrong trait or original cost cannot be selected"
                    );
                    SearchAnswer::Ungrouped(domain.cards.iter().map(|m| m.handle).collect())
                }
            }),
            InputDomain::Order(cards) => {
                Answer::Order(cards.iter().rev().map(|m| m.handle).collect())
            }
            InputDomain::Modes { .. }
            | InputDomain::Abilities(_)
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
        "ready-uma7",
        "ready-wrong",
        "ready-amulet",
        "ready-spell1",
        "ready-spell2",
        "ready-bnw",
        "ready-bnw2",
        "ready-bnw3",
        "ready-vodka",
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
fn finish_attack(game: &mut Game) {
    loop {
        settle(game, 1);
        let question = game
            .internal_decision(View::Referee)
            .filter(|q| matches!(q.domain, InputDomain::Quick { .. }))
            .map_or_else(|| game.internal_quick_choice().unwrap(), Some);
        let Some(question) = question else { break };
        let step = game
            .answer_internal_card(question.actor, question.token, Answer::Quick(None))
            .unwrap();
        finish(game, step, 1);
    }
}
#[test]
fn ecp01_001_play_discount_counts_own_uma_cards_including_an_amulet() {
    for card in ["ECP01-001", "ECP01-SL01", "ECP01-SP01", "ECP01-SSP01"] {
        let mut initial = position(card, "hand");
        initial["players"]["P1"]["zones"]["field"] =
            json!([{"id":"ally","card":"ready-uma"},{"id":"amulet","card":"ready-amulet"}]);
        initial["players"]["P2"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"foreign","card":"ready-uma"}));
        let mut game = game(&initial);
        play(&mut game, 1);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
            8_u8
        );
    }
}

#[test]
fn ecp01_003_five_uma_cards_qualify_the_printed_act_buff_ward_and_heal() {
    for card in ["ECP01-003", "ECP01-SL03", "ECP01-SP03", "ECP01-SSP03"] {
        let mut initial = position(card, "field");
        initial["players"]["P1"]["zones"]["field"].as_array_mut().unwrap().extend((0_u8..4).map(|i|json!({"id":format!("ally{i}"),"card":if i==3 {"ready-amulet"} else {"ready-uma"}})));
        let mut game = game(&initial);
        native_activate(&mut game, card, 0);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["source"]["power"], 4_u8);
        assert_eq!(state["objects"]["source"]["hp"], 4_u8);
        assert_eq!(state["P1"]["leader"]["life"], 12_u8);
        assert!(
            keyword(&state, "source", "守護"),
            "the printed activation grants Ward"
        );
    }
}

#[test]
fn ecp01_005_real_race_recruits_a_low_cost_uma_and_buffs_the_source() {
    for card in ["ECP01-005", "ECP01-P01"] {
        let mut initial = position(card, "field");
        initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"recruit","card":"ready-uma2"},{"id":"expensive","card":"ready-uma3"},{"id":"wrong0","card":"ready-wrong"},{"id":"wrong1","card":"ready-wrong"},{"id":"wrong2","card":"ready-wrong"}]);
        let mut game = game(&initial);
        feed(&mut game, card, 0);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["source"]["power"], 2_u8);
        assert_eq!(state["P1"]["pp"]["current"], 9_u8);
        assert!(
            state["P1"]["field"]
                .as_array()
                .unwrap()
                .contains(&json!("recruit")),
            "the low-cost observed Uma enters the field"
        );
        assert_eq!(
            state["P1"]["deck"],
            json!(["wrong2", "wrong1", "wrong0", "expensive"])
        );
    }
}

#[test]
fn ecp01_012_three_real_races_resolve_three_recruitments_and_three_stat_gains() {
    for card in ["ECP01-012", "ECP01-SL07", "ECP01-SP07", "ECP01-SSP07"] {
        let mut initial = position(card, "field");
        initial["players"]["P1"]["zones"]["deck"] = json!(
            (0_u8..3)
                .flat_map(|i| [
                    json!({"id":format!("recruit{i}"),"card":"ready-uma2"}),
                    json!({"id":format!("wrong{i}a"),"card":"ready-wrong"}),
                    json!({"id":format!("wrong{i}b"),"card":"ready-wrong"})
                ])
                .collect::<Vec<_>>()
        );
        let mut game = game(&initial);
        let result = feed(&mut game, card, 2);
        assert_eq!(
            result
                .progress
                .facts
                .iter()
                .filter(|f| matches!(f, Fact::Raced(_)))
                .count(),
            3
        );
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["source"]["power"], 6_u8);
        assert_eq!(state["objects"]["source"]["hp"], 6_u8);
        assert_eq!(state["P1"]["pp"]["current"], 7_u8);
        for i in 0_u8..3 {
            assert!(
                state["P1"]["field"]
                    .as_array()
                    .unwrap()
                    .contains(&json!(format!("recruit{i}"))),
                "each separately resolved race recruits its follower"
            );
        }
    }
}

#[test]
fn ecp01_016_fanfare_recruits_an_original_cost_one_uma_follower() {
    for card in ["ECP01-016", "ECP01-P08"] {
        let mut initial = position(card, "hand");
        initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"recruit","card":"ready-uma"},{"id":"expensive","card":"ready-uma2"},{"id":"wrong","card":"ready-wrong"}]);
        let mut game = game(&initial);
        play(&mut game, 1);
        let state = game.projection(View::Referee).unwrap();
        assert!(
            state["P1"]["field"]
                .as_array()
                .unwrap()
                .contains(&json!("recruit")),
            "the original cost-one Uma follower enters"
        );
        assert_eq!(state["P1"]["deck"].as_array().unwrap().len(), 2);
    }
}

#[test]
fn ecp01_017_race_searches_bnw_into_ex_with_a_three_point_turn_discount() {
    for card in ["ECP01-017", "ECP01-P09"] {
        let mut initial = position(card, "field");
        initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"wrong","card":"ready-wrong"},{"id":"bnw","card":"ready-bnw"},{"id":"duplicate","card":"ready-bnw"},{"id":"bnw2","card":"ready-bnw2"},{"id":"bnw3","card":"ready-bnw3"}]);
        let mut game = game(&initial);
        feed(&mut game, card, 0);
        let state = game.projection(View::Referee).unwrap();
        assert!(
            state["P1"]["ex"]
                .as_array()
                .unwrap()
                .contains(&json!("bnw")),
            "the searched BNW card enters EX"
        );
        assert!(
            keyword(&state, "source", "守護"),
            "printed Ward survives racing"
        );
        assert_eq!(state["P1"]["ex"].as_array().unwrap().len(), 3);
        for id in ["bnw", "bnw2", "bnw3"] {
            assert!(
                state["P1"]["ex"].as_array().unwrap().contains(&json!(id)),
                "three differently named BNW cards enter EX"
            );
        }
        let step = game
            .play_internal_card("bnw", PlayerId { seat: 1 })
            .unwrap();
        finish(&mut game, step, 1);
        settle(&mut game, 1);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
            9_u8
        );
    }
}

#[test]
fn ecp01_021_real_race_deals_four_and_grows_the_printed_follower() {
    for card in ["ECP01-021", "ECP01-SL11", "ECP01-SP11", "ECP01-SSP11"] {
        for find in [false, true] {
            let mut search_setup = position(card, "hand");
            search_setup["players"]["P1"]["zones"]["deck"] = json!([{"id":"wrong","card":"ready-wrong"},{"id":"amulet","card":"ready-amulet"},{"id":"vodka","card":"ready-vodka"},{"id":"spell","card":"ready-spell1"},{"id":"expensive","card":"ready-spell2"}]);
            let mut searching = game(&search_setup);
            play(&mut searching, u64::from(find));
            let found = searching.projection(View::Referee).unwrap();
            if find {
                assert_eq!(found["P1"]["hand"].as_array().unwrap().len(), 2);
                assert!(
                    found["P1"]["hand"]
                        .as_array()
                        .unwrap()
                        .contains(&json!("vodka"))
                        && found["P1"]["hand"]
                            .as_array()
                            .unwrap()
                            .contains(&json!("spell")),
                    "both printed groups may find their one matching card"
                );
            } else {
                assert_eq!(
                    found["P1"]["hand"],
                    json!([]),
                    "Q1829 permits declining both conditional finds"
                );
            }
            assert_eq!(
                found["P1"]["deck"].as_array().unwrap().len(),
                if find { 3 } else { 5 }
            );
            assert_eq!(found["P1"]["pp"]["current"], 6_u8);
        }
        let mut game = game(&position(card, "field"));
        feed(&mut game, card, 0);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["enemy"]["hp"], 16_u8);
        assert_eq!(state["objects"]["source"]["power"], 4_u8);
        assert_eq!(state["objects"]["source"]["hp"], 4_u8);
    }
}

#[test]
fn ecp01_022_attack_banishes_three_printed_storm_uma_to_damage_every_opposing_follower() {
    for card in ["ECP01-022", "ECP01-SL12", "ECP01-SP12", "ECP01-SSP12"] {
        let mut initial = position(card, "field");
        initial["players"]["P1"]["zones"]["cemetery"] = json!([{"id":"storm0","card":"CP01-001"},{"id":"storm1","card":"CP01-001"},{"id":"storm2","card":"CP01-001"},{"id":"wrong","card":"ready-uma"}]);
        initial["players"]["P2"]["zones"]["cemetery"] = json!([{"id":"foreign","card":"CP01-001"}]);
        initial["players"]["P2"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"enemy2","card":"ready-wrong"}));
        let mut game = game(&initial);
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"attack","attacker":"source","target":"P2.leader"})
            )
            .unwrap()
            .outcome,
            "resolved"
        );
        finish_attack(&mut game);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["enemy"]["hp"], 16_u8);
        assert_eq!(state["objects"]["enemy2"]["hp"], 16_u8);
        for i in 0_u8..3 {
            assert!(
                state["P1"]["banish"]
                    .as_array()
                    .unwrap()
                    .contains(&json!(format!("storm{i}"))),
                "all three own original Storm Uma pay the cost"
            );
        }
        assert!(
            state["P1"]["cemetery"]
                .as_array()
                .unwrap()
                .contains(&json!("wrong")),
            "a non-Storm Uma cannot pay"
        );
    }
}

#[test]
fn ecp01_025_high_original_cost_uma_changes_fanfare_damage_from_one_to_three() {
    for card in ["ECP01-025", "ECP01-P13"] {
        for high in [false, true] {
            let mut initial = position(card, "hand");
            initial["players"]["P1"]["zones"]["field"] =
                json!([{"id":"ally","card":if high {"ready-uma4"} else {"ready-uma3"}}]);
            let mut game = game(&initial);
            play(&mut game, 1);
            assert_eq!(
                game.projection(View::Referee).unwrap()["objects"]["enemy"]["hp"],
                if high { 17_u8 } else { 19_u8 }
            );
        }
    }
}

#[test]
fn ecp01_026_five_uma_spells_make_the_play_free_and_recover_only_a_cost_one_spell() {
    for card in ["ECP01-026", "ECP01-P14"] {
        let mut initial = position(card, "hand");
        let mut grave = (0_u8..4)
            .map(|i| json!({"id":format!("other{i}"),"card":"ready-spell2"}))
            .collect::<Vec<_>>();
        grave.push(json!({"id":"recover","card":"ready-spell1"}));
        initial["players"]["P1"]["zones"]["cemetery"] = json!(grave);
        let mut game = game(&initial);
        play(&mut game, 1);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["pp"]["current"], 10_u8);
        assert!(
            state["P1"]["ex"]
                .as_array()
                .unwrap()
                .contains(&json!("recover")),
            "the cost-one Uma spell is recovered into EX"
        );
        assert_eq!(state["P1"]["cemetery"].as_array().unwrap().len(), 4);
    }
}

#[test]
fn ecp01_032_fanfare_heals_and_raises_max_pp_then_the_awakened_activation_recovers_a_high_cost_uma()
{
    for card in ["ECP01-032", "ECP01-P16"] {
        let mut initial = position(card, "hand");
        initial["players"]["P1"]["pp"] = json!({"current":7_u8,"max":7_u8});
        initial["players"]["P1"]["zones"]["cemetery"] =
            json!([{"id":"recover","card":"ready-uma7"},{"id":"low","card":"ready-uma4"}]);
        let mut game = game(&initial);
        play(&mut game, 1);
        native_activate(&mut game, card, 0);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["leader"]["life"], 13_u8);
        assert_eq!(state["P1"]["pp"]["max"], 8_u8);
        assert!(
            state["P1"]["hand"]
                .as_array()
                .unwrap()
                .contains(&json!("recover")),
            "the awakened printed activation returns an original cost-seven Uma"
        );
        assert!(
            keyword(&state, "source", "守護"),
            "printed Ward remains active"
        );
    }
}
