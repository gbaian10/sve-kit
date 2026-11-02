#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate printed card scenarios."
)]
use super::*;
use sve_engine::game::internal::{OrderCard, SearchAnswer};
fn fixture() -> &'static SelectedEffects {
    static SELECTED: OnceLock<SelectedEffects> = OnceLock::new();
    SELECTED.get_or_init(|| {
  let (mut snapshot,keywords,mut input)=producer_inputs();
  // Minimal companions isolate search filters, current costs, and ordinary evolution.
  for (number,name,kind,traits) in [
   ("ready-deremas","プリコネ companion","フォロワー",vec!["プリコネ","カルミナ","ディアボロス"]),
   ("ready-rin","ヴァンガード material","フォロワー",vec!["プリコネ"]),
   ("ready-mio","イタズラ宣言","スペル",vec![]),
   ("ready-amulet","plain amulet","アミュレット",vec![]),
  ] {
   snapshot.push_str(&json!({"number":number,"faces":[{"name":name,"card_class":"ニュートラル","card_type":kind,"cost":"4","power":if kind=="フォロワー" {"3"} else {"-"},"hp":if kind=="フォロワー" {"4"} else {"-"},"traits":traits,"text":null,"sections":[]}]}).to_string());snapshot.push('\n');
   input["documents"].as_array_mut().unwrap().push(json!([format!("{number}.yaml"),json!({"format":1_u8,"kind":"effect_set","cards":{number:{"status":"complete","abilities":[]}}}).to_string()]));
  }
  let form = "CP04-100";
   let name=snapshot.lines().map(|line|serde_json::from_str::<Value>(line).unwrap()).find(|row|row["number"]==form).unwrap()["faces"][0]["name"].clone();
   let number=format!("ready-base-{form}");
   snapshot.push_str(&json!({"number":number,"faces":[{"name":name,"card_class":"ニュートラル","card_type":"フォロワー","cost":"0","power":"3","hp":"4","traits":[],"text":null,"sections":[]}]}).to_string());snapshot.push('\n');
   input["documents"].as_array_mut().unwrap().push(json!([format!("{number}.yaml"),json!({"format":1_u8,"kind":"effect_set","cards":{number:{"status":"complete","abilities":[]}}}).to_string()]));
  repository::selection_with_rules_from(&snapshot,&keywords,&input)
 })
}
fn position(card: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["construction"] = json!("title");
    initial["players"]["P1"]["title_code"] = json!("princess_connect_re_dive");
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    initial["players"]["P1"]["sep"] = json!(0_u8);
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
        "ready-cp04-3",
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
                    .iter()
                    .take(if mode == 4 && domain.cardinality.minimum == Nat(0) {
                        0
                    } else if mode == 5 {
                        2
                    } else {
                        1
                    })
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
            InputDomain::CostMaterials(domain) => Answer::CostMaterials(
                domain
                    .members
                    .iter()
                    .take(usize::try_from(domain.cardinality.minimum.0).unwrap())
                    .map(|m| m.handle)
                    .collect(),
            ),
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
fn has_keyword(state: &Value, object: &str, keyword: &str) -> bool {
    state["objects"][object]["keywords"]
        .as_array()
        .unwrap()
        .contains(&json!(keyword))
}

fn finish_attack(game: &mut Game, mode: u64) {
    loop {
        settle(game, mode);
        let question = game
            .internal_decision(View::Referee)
            .filter(|question| matches!(question.domain, InputDomain::Quick { .. }))
            .map_or_else(|| game.internal_quick_choice().unwrap(), Some);
        let Some(question) = question else { break };
        let step = game
            .answer_internal_card(question.actor, question.token, Answer::Quick(None))
            .unwrap();
        finish(game, step, mode);
    }
}
#[test]
fn cp04_090_damages_then_may_mill_the_observed_princess_card() {
    for card in ["CP04-090", "CP04-P70", "PCS01-038"] {
        for princess in [false, true] {
            let mut initial = position(card);
            initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"seen","card":if princess {"ready-deremas"} else {"BP01-171"}},{"id":"tail","card":"BP01-037"}]);
            let mut game = game(&initial);
            play(&mut game, "source", 1);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["objects"]["enemy"]["damage"], 2_u8);
            assert_eq!(
                state["P1"]["deck"],
                if princess {
                    json!(["tail"])
                } else {
                    json!(["seen", "tail"])
                }
            );
            assert_eq!(
                state["P1"]["cemetery"],
                if princess {
                    json!(["seen", "source"])
                } else {
                    json!(["source"])
                }
            );
        }
    }
}

#[test]
fn cp04_099_and_100_grant_one_of_three_keywords_to_another_follower() {
    for card in ["CP04-099", "CP04-P75", "CP04-100", "CP04-P76"] {
        for mode in [1_u64, 2, 3] {
            let is_evolved = matches!(card, "CP04-100" | "CP04-P76");
            let base = if is_evolved { "CP04-099" } else { card };
            let mut initial = position(base);
            initial["players"]["P1"]["zones"]["hand"] = json!([]);
            initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":base},{"id":"ally","card":"ready-deremas"},{"id":"material","card":"ready-amulet"}]);
            if is_evolved {
                initial["players"]["P1"]["zones"]["evolve_deck"] =
                    json!([{"id":"form","card":card}]);
            }
            let mut game = game(&initial);
            if is_evolved {
                assert_eq!(game.internal_rule_action(&json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})).unwrap().outcome,"resolved");
                settle(&mut game, 1);
            }
            let definition = fixture().current[card]
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
            let activation = game
                .activate_internal_card("source", definition, PlayerId { seat: 1 })
                .unwrap();
            finish(&mut game, activation, mode);
            settle(&mut game, mode);
            let state = game.projection(View::Referee).unwrap();
            let expected = match mode {
                1 => "突進",
                2 => "必殺",
                3 => "守護",
                _ => panic!("known printed alternative"),
            };
            assert!(has_keyword(&state, "ally", expected));
            assert!(!has_keyword(&state, "source", expected));
            assert_eq!(state["objects"]["material"]["acted"], true);
            assert_eq!(state["objects"]["source"]["acted"], false);
        }
    }
}

#[test]
fn cp04_100_evolution_searches_a_carmina_card() {
    for card in ["CP04-100", "CP04-P76"] {
        let mut initial = evolution_position(card);
        initial["players"]["P1"]["zones"]["deck"] =
            json!([{"id":"wrong","card":"BP01-037"},{"id":"searched","card":"ready-deremas"}]);
        let game = evolved(&initial, card);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["hand"],
            json!(["searched"])
        );
    }
}

#[test]
fn cp04_103_searches_the_named_prank_spell() {
    for card in ["CP04-103", "CP04-P79"] {
        let mut initial = position(card);
        initial["players"]["P1"]["zones"]["deck"] =
            json!([{"id":"wrong","card":"BP01-037"},{"id":"searched","card":"ready-mio"}]);
        let mut game = game(&initial);
        play(&mut game, "source", 1);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["hand"],
            json!(["searched"])
        );
    }
}

#[test]
fn cp04_pr19_ward_follower_acts_to_damage_a_selected_enemy() {
    for card in ["CP04-PR19", "CP04-PR31", "CP04-T05"] {
        let mut initial = position(card);
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":card}]);
        let mut game = game(&initial);
        native_activate(&mut game, card, 0);
        let state = game.projection(View::Referee).unwrap();
        assert!(has_keyword(&state, "source", "守護"));
        assert_eq!(state["objects"]["source"]["acted"], true);
        assert_eq!(state["objects"]["enemy"]["damage"], 1_u8);
    }
}

#[test]
fn cp04_pr24_lastword_buffs_a_diabolos_after_actual_combat_death() {
    for card in ["CP04-PR24", "CP04-PR36", "CP04-T10"] {
        let mut initial = position(card);
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] =
            json!([{"id":"source","card":card},{"id":"ally","card":"ready-deremas"}]);
        initial["players"]["P2"]["zones"]["field"][0]["state"] = json!({"acted":true});
        let mut game = game(&initial);
        assert!(has_keyword(
            &game.projection(View::Referee).unwrap(),
            "source",
            "突進"
        ));
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"enemy"}))
            .unwrap();
        finish_attack(&mut game, 1);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["cemetery"], json!([]));
        assert_eq!(state["P1"]["field"], json!(["ally"]));
        assert_eq!(state["objects"]["ally"]["power"], 4_u8);
    }
}

#[test]
fn cp04_090_can_keep_a_princess_card_and_requires_an_enemy_target() {
    let mut initial = position("CP04-090");
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"seen","card":"ready-deremas"}]);
    let mut accepting_game = game(&initial);
    play(&mut accepting_game, "source", 4);
    let accepted = accepting_game.projection(View::Referee).unwrap();
    assert_eq!(accepted["P1"]["deck"], json!(["seen"]));
    assert_eq!(accepted["P1"]["cemetery"], json!(["source"]));
    initial["players"]["P2"]["zones"]["field"] = json!([]);
    let mut no_target = game(&initial);
    let rejected = no_target
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap_err();
    assert!(matches!(rejected, EngineFailure::IllegalPlay(_)));
    let state = no_target.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["source"]));
    assert_eq!(state["P1"]["deck"], json!(["seen"]));
    assert_eq!(state["P1"]["cemetery"], json!([]));
}

#[test]
fn cp04_pr24_lastword_can_buff_two_diabolos_or_resolve_without_one() {
    for count in [0_u8, 2] {
        let mut initial = position("CP04-PR24");
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        let mut field = vec![json!({"id":"source","card":"CP04-PR24"})];
        field.extend((0..count).map(|i| json!({"id":format!("ally{i}"),"card":"ready-deremas"})));
        initial["players"]["P1"]["zones"]["field"] = json!(field);
        initial["players"]["P2"]["zones"]["field"][0]["state"] = json!({"acted":true});
        let mut game = game(&initial);
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"enemy"}))
            .unwrap();
        finish_attack(&mut game, 5);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P1"]["field"].as_array().unwrap().len(),
            usize::from(count)
        );
        assert_eq!(state["P1"]["cemetery"], json!([]));
        for i in 0..count {
            assert_eq!(state["objects"][format!("ally{i}")]["power"], 4_u8);
        }
    }
}

fn grant_position(card: &str, ally: bool, material: bool) -> Game {
    let evolved = matches!(card, "CP04-100" | "CP04-P76");
    let base = if evolved { "CP04-099" } else { card };
    let mut initial = position(base);
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    let mut field = vec![json!({"id":"source","card":base})];
    if ally {
        field.push(json!({"id":"ally","card":"ready-deremas"}));
    }
    if material {
        field.push(json!({"id":"material","card":"ready-amulet"}));
    }
    initial["players"]["P1"]["zones"]["field"] = json!(field);
    if evolved {
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":card}]);
    }
    let mut game = game(&initial);
    if evolved {
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
            )
            .unwrap()
            .outcome,
            "resolved",
            "the real base must evolve before its form's activated ability is used"
        );
        settle(&mut game, 1);
    }
    game
}

#[test]
fn cp04_099_and_100_require_both_an_amulet_and_another_follower() {
    for card in ["CP04-099", "CP04-100"] {
        for (ally, material) in [(true, false), (false, true)] {
            let mut game = grant_position(card, ally, material);
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
            let before = game.projection(View::Referee).unwrap();
            let attempt = (|| {
                let mut step =
                    game.activate_internal_card("source", ability, PlayerId { seat: 1 })?;
                while let CardPlay::Awaiting(question) = step {
                    let answer = match &question.domain {
                        InputDomain::Targets(domain) => Answer::Targets(
                            domain.members.iter().take(1).map(|m| m.handle).collect(),
                        ),
                        InputDomain::CostMaterials(domain) => Answer::CostMaterials(
                            domain
                                .members
                                .iter()
                                .take(usize::try_from(domain.cardinality.minimum.0).unwrap())
                                .map(|m| m.handle)
                                .collect(),
                        ),
                        InputDomain::ResourceEntryPayment { .. }
                        | InputDomain::WardEntry { .. }
                        | InputDomain::Quick { .. }
                        | InputDomain::OptionalCost
                        | InputDomain::BeforeDraw
                        | InputDomain::Pending(_)
                        | InputDomain::TriggerQuota(_)
                        | InputDomain::Integer(_)
                        | InputDomain::Order(_)
                        | InputDomain::DamageReplacements(_)
                        | InputDomain::DriveTrigger { .. }
                        | InputDomain::Search(_)
                        | InputDomain::SearchPartition(_)
                        | InputDomain::Distribution(_)
                        | InputDomain::SearchReplacement
                        | InputDomain::TokenCapacity { .. }
                        | InputDomain::Modes { .. } => {
                            panic!("unexpected rejected activation parameter")
                        }
                    };
                    step = game.answer_internal_card(question.actor, question.token, answer)?;
                }
                Ok::<CardPlay, EngineFailure>(step)
            })();
            let rejected = attempt.unwrap_err();
            assert!(matches!(
                rejected,
                EngineFailure::IllegalPlay(_) | EngineFailure::Invalid(_)
            ));
            assert_eq!(game.projection(View::Referee).unwrap(), before);
        }
    }
}

#[test]
fn cp04_100_union_burst_counts_even_when_no_carmina_can_be_found() {
    let mut initial = position("CP04-099");
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"CP04-099"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"CP04-100"}]);
    let mut game = game(&initial);
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
        )
        .unwrap()
        .outcome,
        "resolved"
    );
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("evolution trigger")
    };
    let CardPlay::Awaiting(search) = game
        .answer_internal_card(
            pending.actor,
            pending.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap()
    else {
        panic!("search decision")
    };
    let InputDomain::Search(domain) = &search.domain else {
        panic!("search domain")
    };
    assert_eq!(domain.cards, Vec::<OrderCard>::new());
    let activation = resolved(
        game.answer_internal_card(
            search.actor,
            search.token,
            Answer::Search(SearchAnswer::Ungrouped(vec![])),
        )
        .unwrap(),
    );
    assert_eq!(
        activation
            .progress
            .facts
            .iter()
            .filter(|fact| matches!(
                fact,
                Fact::AbilityPlayed {
                    qualification: AbilityQualification::UnionBurst,
                    ..
                }
            ))
            .count(),
        1
    );
    settle(&mut game, 1);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!([]));
    assert_eq!(state["P1"]["deck"].as_array().unwrap().len(), 5);
}
