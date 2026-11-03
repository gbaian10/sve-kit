#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate the entry and recipient migration scenarios."
)]
use super::*;
use alloc::collections::BTreeSet;
use sve_engine::game::internal::{InstructionOutcome, SearchAnswer};

struct Prepared {
    selected: SelectedEffects,
    programs: Vec<(String, WellFormedCardProgram)>,
}
fn prepared() -> &'static Prepared {
    static READY: OnceLock<Prepared> = OnceLock::new();
    READY.get_or_init(||{
        let (mut snapshot,keywords,mut input)=producer_inputs();
        let mut programs=Vec::new();
        for (number,name,kind,cost,class) in [
            ("limit-cheap","cheap","フォロワー","1","ビショップ"),
            ("limit-edge","edge","フォロワー","3","ビショップ"),
            ("limit-high","high","フォロワー","4","ビショップ"),
            ("limit-wrong","wrong","フォロワー","1","ニュートラル"),
            ("limit-same","白翼の守護神・アイテール","フォロワー","1","ビショップ"),
            ("limit-amulet","amulet","アミュレット","1","ビショップ"),
            ("limit-dragon7","dragon7","フォロワー","7","ドラゴン"),
            ("limit-dragon6","dragon6","フォロワー","6","ドラゴン"),
            ("limit-other7","other7","フォロワー","7","ニュートラル"),
            ("limit-two","two","フォロワー","2","ニュートラル"),
            ("limit-three","three","フォロワー","3","ニュートラル"),
            ("limit-blade","ブラスター・ブレード","フォロワー","2","ニュートラル"),
            ("limit-dark","ブラスター・ダーク","フォロワー","2","ニュートラル"),
            ("limit-similar","ブラスター・ダーク亜種","フォロワー","2","ニュートラル"),
        ] {
            snapshot.push_str(&json!({"number":number,"faces":[{"name":name,"card_class":class,"card_type":kind,"traits":[],"cost":cost,"power":if kind=="アミュレット"{"-"}else{"1"},"hp":if kind=="アミュレット"{"-"}else{"30"},"text":null,"sections":[]}]}).to_string());snapshot.push('\n');
            input["documents"].as_array_mut().unwrap().push(json!([format!("{number}.yaml"),json!({"format":1_u8,"kind":"effect_set","cards":{number:{"status":"complete","abilities":[]}}}).to_string()]));
            programs.push((number.to_owned(),WellFormedCardProgram::check(CardProgram{abilities:vec![],card_play_costs:vec![]}).unwrap()));
        }
        Prepared{selected:repository::selection_with_rules_from(&snapshot,&keywords,&input),programs}
    })
}
fn load(game: &mut Game) {
    for (number, program) in &prepared().programs {
        game.load_internal_card(number, program).unwrap();
    }
}
fn build(initial: &Value) -> Game {
    let mut game = Game::from_selected_effects(
        prepared().selected.clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "dynamic-number",
    )
    .unwrap();
    load(&mut game);
    game
}
fn position(card: &str, zone: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["construction"] = json!("title");
    initial["players"]["P1"]["title_code"] = json!("princess_connect_re_dive");
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    initial["players"]["P1"]["zones"][zone] = json!([{"id":"source","card":card}]);
    initial["players"]["P1"]["zones"]["deck"] = json!(
        (0..6_u8)
            .map(|i| json!({"id":format!("draw{i}"),"card":"BP01-037"}))
            .collect::<Vec<_>>()
    );
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"limit-cheap","owner":"P1"}]);
    initial
}
fn finish(game: &mut Game, mut play: CardPlay, modes: &[u64]) -> CardPlay {
    while let CardPlay::Awaiting(question) = play {
        let answer = match &question.domain {
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            InputDomain::OptionalCost => Answer::OptionalCost(true),
            InputDomain::Modes { .. } => Answer::Modes(modes.iter().copied().map(ModeId).collect()),
            InputDomain::Targets(domain) | InputDomain::CostMaterials(domain) => {
                let n = domain
                    .cardinality
                    .maximum
                    .unwrap_or(domain.cardinality.minimum)
                    .0
                    .min(u64::try_from(domain.members.len()).unwrap());
                let handles = domain
                    .members
                    .iter()
                    .take(usize::try_from(n).unwrap())
                    .map(|member| member.handle)
                    .collect();
                if matches!(question.domain, InputDomain::Targets(_)) {
                    Answer::Targets(handles)
                } else {
                    Answer::CostMaterials(handles)
                }
            }
            InputDomain::Order(cards) => {
                Answer::Order(cards.iter().map(|member| member.handle).collect())
            }
            InputDomain::Search(domain) => Answer::Search(SearchAnswer::Ungrouped(
                domain
                    .cards
                    .iter()
                    .take(
                        usize::try_from(
                            domain
                                .cardinality
                                .maximum
                                .unwrap_or(domain.cardinality.minimum)
                                .0,
                        )
                        .unwrap(),
                    )
                    .map(|member| member.handle)
                    .collect(),
            )),
            _ => panic!("unsupported migration question"),
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    play
}
fn settle(game: &mut Game, modes: &[u64]) {
    loop {
        if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
        let Some(question) = game.internal_pending_choice().unwrap() else {
            break;
        };
        let InputDomain::Pending(options) = &question.domain else {
            panic!("printed pending ability");
        };
        let play = game
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap();
        finish(game, play, modes);
    }
}
fn enter(initial: &Value, zone: &str) -> Game {
    if zone == "hand" || zone == "ex" {
        let mut game = build(initial);
        let play = game
            .play_internal_card("source", PlayerId { seat: 1 })
            .unwrap();
        finish(&mut game, play, &[1]);
        if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
        game
    } else {
        let action = Action::Move {
            objects: subjects_in(ZoneKind::Cemetery),
            movement: MoveSpec {
                to: MoveDestination::Shared {
                    zone: ZoneRef {
                        player: PlayerRef::Controller {},
                        kind: ZoneKind::Field,
                    },
                },
                position: None,
                order: None,
                initial_posture: None,
                entry_effects: vec![],
            },
            reason: MoveReason::Put,
        };
        let mut game = with_selected_producer(initial, action, &prepared().selected);
        load(&mut game);
        let play = game
            .activate_internal_card("producer", AbilityId(1), PlayerId { seat: 1 })
            .unwrap();
        finish(&mut game, play, &[1]);
        game.resume_internal_card().unwrap();
        game
    }
}

fn pending_start(game: &mut Game, line: u32) -> CardPlay {
    if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("printed pending ability");
    };
    let option = options.iter().find(|o| o.line == u64::from(line)).unwrap();
    game.answer_internal_card(pending.actor, pending.token, Answer::Pending(option.handle))
        .unwrap()
}
#[test]
fn bp04_098_searches_below_current_maximum_pp_and_excludes_same_name_kind_and_class() {
    for maximum in [0_u8, 3, 4] {
        let mut initial = position("BP04-098", "cemetery");
        initial["players"]["P1"]["pp"] = json!({"current":maximum,"max":maximum});
        initial["players"]["P1"]["zones"]["deck"] = json!([
            {"id":"cheap","card":"limit-cheap"},{"id":"edge","card":"limit-edge"},
            {"id":"high","card":"limit-high"},{"id":"wrong","card":"limit-wrong"},
            {"id":"same","card":"limit-same"},{"id":"amulet","card":"limit-amulet"}
        ]);
        let mut game = enter(&initial, "cemetery");
        let CardPlay::Awaiting(question) = pending_start(&mut game, 2) else {
            panic!("search even with no candidate");
        };
        let InputDomain::Search(domain) = &question.domain else {
            panic!("private search");
        };
        let expected = match maximum {
            0 => vec![],
            3 => vec!["limit-cheap"],
            _ => vec!["limit-cheap", "limit-edge"],
        };
        let actual = domain
            .cards
            .iter()
            .map(|c| c.card.as_str())
            .collect::<BTreeSet<_>>();
        assert_eq!(
            actual,
            expected.into_iter().collect(),
            "strictly below maximum PP, independent of hidden deck contents"
        );
        let wanted = if maximum == 4 {
            "limit-edge"
        } else {
            "limit-cheap"
        };
        let chosen = domain
            .cards
            .iter()
            .find(|c| c.card == wanted)
            .map(|c| c.handle)
            .into_iter()
            .collect();
        let play = game
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Search(SearchAnswer::Ungrouped(chosen)),
            )
            .unwrap();
        finish(&mut game, play, &[1]);
        settle(&mut game, &[1]);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P1"]["field"].as_array().unwrap().len(),
            if maximum == 0 { 2 } else { 3 },
            "a missing result leaves the ability-entry field unchanged"
        );
    }
}
#[test]
fn bp07_090_and_real_091_create_both_repair_modes_then_filter_targets_by_own_ex_count() {
    for old in [0_u8, 2] {
        let mut initial = position("BP07-090", "hand");
        initial["players"]["P1"]["zones"]["ex"] = json!(
            (0..old)
                .map(|i| json!({"id":format!("old{i}"),"card":"limit-two"}))
                .collect::<Vec<_>>()
        );
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP07-091"}]);
        initial["players"]["P2"]["zones"]["field"] = json!([{ "id":"cheap","card":"limit-cheap"},{"id":"edge","card":"limit-edge"},{"id":"high","card":"limit-high"}]);
        let mut game = enter(&initial, "hand");
        settle(&mut game, &[1]);
        let before = game.projection(View::Referee).unwrap();
        assert_eq!(
            before["P1"]["ex"].as_array().unwrap().len(),
            usize::from(old) + 1
        );
        assert_eq!(before["P2"]["ex"].as_array().unwrap().len(), 1);
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
            )
            .unwrap()
            .outcome,
            "resolved"
        );
        let CardPlay::Awaiting(question) = pending_start(&mut game, 1) else {
            panic!("formal evolved target");
        };
        let InputDomain::Targets(domain) = &question.domain else {
            panic!("original cost target");
        };
        assert_eq!(domain.members.len(), if old == 0 { 1 } else { 2 });
        assert!(
            domain
                .members
                .iter()
                .all(|m| m.card.as_deref() != Some("limit-high")),
            "the cost above the own EX count is illegal"
        );
        let wanted = if old == 0 {
            "limit-cheap"
        } else {
            "limit-edge"
        };
        let target = domain
            .members
            .iter()
            .find(|m| m.card.as_deref() == Some(wanted))
            .unwrap()
            .handle;
        let play = game
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Targets(vec![target]),
            )
            .unwrap();
        resolved(finish(&mut game, play, &[1]));
        settle(&mut game, &[1]);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P2"]["cemetery"],
            json!([if old == 0 { "cheap" } else { "edge" }])
        );
    }
}
#[test]
fn bp06_019_live_other_follower_count_reduces_only_its_normal_evolution_payment() {
    for others in [0_u8, 1, 3] {
        let mut initial = position("BP06-019", "field");
        initial["players"]["P1"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .extend((0..others).map(|i| json!({"id":format!("ally{i}"),"card":"limit-two"})));
        if others < 3 {
            initial["players"]["P1"]["zones"]["field"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":"amulet","card":"limit-amulet"}));
        }
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP06-020"}]);
        let mut game = build(&initial);
        let cost = 3_u8.checked_sub(others).unwrap();
        let before = game.projection(View::Referee).unwrap();
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":cost+1}})
            )
            .unwrap()
            .outcome,
            "cannot-evolve"
        );
        assert_eq!(
            game.projection(View::Referee).unwrap(),
            before,
            "a wrong quote is atomic"
        );
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":cost}})
            )
            .unwrap()
            .outcome,
            "resolved"
        );
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
            10_u8.checked_sub(cost).unwrap()
        );
    }
}
#[test]
fn bp11_058_freezes_the_formal_upper_bound_from_only_qualifying_own_grave_cards() {
    for count in [0_u8, 1, 2] {
        let mut initial = position("BP11-058", "hand");
        let mut grave = vec![
            json!({"id":"wrong","card":"limit-other7"}),
            json!({"id":"low","card":"limit-dragon6"}),
        ];
        grave.extend((0..count).map(|i| json!({"id":format!("grave{i}"),"card":"limit-dragon7"})));
        initial["players"]["P1"]["zones"]["cemetery"] = json!(grave);
        initial["players"]["P2"]["zones"]["field"] = json!(
            (0..3_u8)
                .map(|i| json!({"id":format!("enemy{i}"),"card":"limit-two"}))
                .collect::<Vec<_>>()
        );
        let mut game = enter(&initial, "hand");
        let CardPlay::Awaiting(question) = pending_start(&mut game, 1) else {
            panic!("formal quantity target");
        };
        let InputDomain::Targets(domain) = &question.domain else {
            panic!("bounded objects");
        };
        assert_eq!(
            domain.cardinality,
            Cardinality {
                minimum: Nat(0),
                maximum: Some(Nat(u64::from(count)))
            }
        );
        let before = game.projection(View::Referee).unwrap();
        let too_many = domain
            .members
            .iter()
            .take(usize::from(count) + 1)
            .map(|m| m.handle)
            .collect();
        assert!(
            game.answer_internal_card(question.actor, question.token, Answer::Targets(too_many))
                .is_err(),
            "the printed public count bounds the answer"
        );
        assert_eq!(
            game.projection(View::Referee).unwrap(),
            before,
            "over-cap answers do not mutate the state"
        );
        let chosen = domain
            .members
            .iter()
            .take(usize::from(count))
            .map(|m| m.handle)
            .collect();
        let play = game
            .answer_internal_card(question.actor, question.token, Answer::Targets(chosen))
            .unwrap();
        resolved(finish(&mut game, play, &[1]));
        settle(&mut game, &[1]);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P2"]["banish"]
                .as_array()
                .unwrap()
                .len(),
            usize::from(count)
        );
    }
}
#[test]
fn bp11_058_hand_activation_pays_three_pp_source_and_another_expensive_dragon() {
    let mut initial = position("BP11-058", "hand");
    initial["players"]["P1"]["pp"] = json!({"current":8_u8,"max":8_u8});
    initial["players"]["P1"]["zones"]["hand"]
        .as_array_mut()
        .unwrap()
        .extend([
            json!({"id":"fuel","card":"limit-dragon7"}),
            json!({"id":"wrong","card":"limit-other7"}),
            json!({"id":"low","card":"limit-dragon6"}),
        ]);
    let mut game = build(&initial);
    let definition = prepared().selected.current["BP11-058"]
        .program
        .card()
        .abilities
        .iter()
        .find(|a| a.node.kind == AbilityKind::Activated && a.source.line == 2)
        .unwrap()
        .node
        .id;
    let CardPlay::Awaiting(question) = game
        .activate_internal_card("source", definition, PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("private material");
    };
    let InputDomain::CostMaterials(domain) = &question.domain else {
        panic!("private hand cost");
    };
    let fuel = domain
        .members
        .iter()
        .find(|m| m.card.as_deref() == Some("limit-dragon7"))
        .unwrap()
        .handle;
    assert!(
        domain
            .members
            .iter()
            .all(|m| !matches!(m.card.as_deref(), Some("limit-other7" | "limit-dragon6"))),
        "class and original cost are both required"
    );
    let play = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::CostMaterials(vec![fuel]),
        )
        .unwrap();
    resolved(finish(&mut game, play, &[1]));
    settle(&mut game, &[1]);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["pp"]["max"], 9_u8);
    assert_eq!(state["P1"]["cemetery"], json!(["source", "fuel"]));
    assert_eq!(state["P1"]["hand"].as_array().unwrap().len(), 3);
}
#[test]
fn bp22_074_counts_whole_tens_of_own_original_two_cost_grave_cards_for_modes_and_attack() {
    for count in [0_u8, 10, 20, 30] {
        let mut initial = position("BP22-074", "hand");
        let mut grave = (0..count)
            .map(|i| json!({"id":format!("two{i}"),"card":"limit-two"}))
            .collect::<Vec<_>>();
        grave.extend((0..10_u8).map(|i| json!({"id":format!("wrong{i}"),"card":"limit-three"})));
        initial["players"]["P1"]["zones"]["cemetery"] = json!(grave);
        initial["players"]["P2"]["zones"]["cemetery"] = json!(
            (0..10_u8)
                .map(|i| json!({"id":format!("opposing{i}"),"card":"limit-two"}))
                .collect::<Vec<_>>()
        );
        let mut field_position = initial.clone();
        field_position["players"]["P1"]["zones"]["hand"] = json!([]);
        field_position["players"]["P1"]["zones"]["field"] =
            json!([{"id":"source","card":"BP22-074"}]);
        let mut field_game = build(&field_position);
        assert_eq!(
            field_game
                .internal_rule_action(
                    &json!({"do":"attack","attacker":"source","target":"P2.leader"})
                )
                .unwrap()
                .outcome,
            if count == 0 {
                "cannot-attack"
            } else {
                "resolved"
            },
            "the printed grave threshold governs an established follower independently of entry and storm"
        );
        let mut game = enter(&initial, "hand");
        let play = pending_start(&mut game, 2);
        if count == 0 {
            resolved(play);
        } else {
            let CardPlay::Awaiting(question) = play else {
                panic!("dynamic mode declaration");
            };
            let InputDomain::Modes { count: limit, .. } = &question.domain else {
                panic!("whole-tens mode cap");
            };
            assert_eq!(*limit, Nat(if count == 10 { 1 } else { 2 }));
            let modes = if count == 10 {
                vec![ModeId(2)]
            } else {
                vec![ModeId(1), ModeId(2)]
            };
            let answer = game
                .answer_internal_card(question.actor, question.token, Answer::Modes(modes))
                .unwrap();
            resolved(finish(&mut game, answer, &[1, 2]));
        }
        settle(&mut game, &[1]);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P2"]["leader"]["life"],
            if count >= 20 { 19_u8 } else { 20 }
        );
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"attack","attacker":"source","target":"P2.leader"})
            )
            .unwrap()
            .outcome,
            if count == 0 {
                "cannot-attack"
            } else {
                "resolved"
            }
        );
    }
}
#[test]
fn cp03_022_counts_complete_pairs_of_the_two_exact_grave_names() {
    for (blades, darks) in [(0_u8, 2_u8), (3, 1), (3, 2)] {
        let mut initial = position("CP03-022", "hand");
        initial["players"]["P1"]["title_code"] = json!("cardfight_vanguard");
        let mut grave = (0..blades)
            .map(|i| json!({"id":format!("blade{i}"),"card":"limit-blade"}))
            .collect::<Vec<_>>();
        grave.extend((0..darks).map(|i| json!({"id":format!("dark{i}"),"card":"limit-dark"})));
        grave.extend((0..3_u8).map(|i| json!({"id":format!("similar{i}"),"card":"limit-similar"})));
        initial["players"]["P1"]["zones"]["cemetery"] = json!(grave);
        initial["players"]["P2"]["zones"]["field"] = json!(
            (0..3_u8)
                .map(|i| json!({"id":format!("enemy{i}"),"card":"limit-two"}))
                .collect::<Vec<_>>()
        );
        let mut game = enter(&initial, "hand");
        let CardPlay::Awaiting(question) = pending_start(&mut game, 3) else {
            panic!("paired target declaration");
        };
        let InputDomain::Targets(domain) = &question.domain else {
            panic!("formal objects");
        };
        let pairs = blades.min(darks);
        assert_eq!(
            domain.cardinality.maximum,
            Some(Nat(u64::from(pairs))),
            "uneven populations produce complete pairs, never total divided by two"
        );
        let chosen = domain
            .members
            .iter()
            .take(usize::from(pairs))
            .map(|m| m.handle)
            .collect();
        let play = game
            .answer_internal_card(question.actor, question.token, Answer::Targets(chosen))
            .unwrap();
        resolved(finish(&mut game, play, &[1]));
        settle(&mut game, &[1]);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P2"]["cemetery"]
                .as_array()
                .unwrap()
                .len(),
            usize::from(pairs)
        );
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"attack","attacker":"source","target":"P2.leader"})
            )
            .unwrap()
            .outcome,
            "resolved"
        );
        let attack = pending_start(&mut game, 2);
        resolved(finish(&mut game, attack, &[1]));
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["source"]["power"],
            if pairs == 0 { 4_u8 } else { 6 },
            "only the two complete names enable its printed attack bonus"
        );
    }
}
fn combined(activation: &Activation, amount: u64) {
    let damage = activation
        .progress
        .receipts
        .values()
        .filter_map(|receipt| {
            if let InstructionOutcome::Damage(outcome) = receipt {
                Some(outcome)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(
        damage.len(),
        1,
        "the selected follower and leader share one instruction"
    );
    assert_eq!(
        damage[0].actual,
        Nat(amount.checked_mul(2).unwrap()),
        "the receipt sums both recipients"
    );
    assert_eq!(
        activation
            .progress
            .facts
            .iter()
            .filter(|fact| matches!(fact, Fact::Damage(_)))
            .count(),
        2,
        "both recipients receive one damage fact"
    );
}
#[test]
fn bp18_046_banish_threshold_controls_both_recipients_and_second_mode_draws_before_banishing() {
    for (banished, mode) in [(9_u8, 1_u64), (10, 1), (0, 2)] {
        let mut initial = position("BP18-046", "hand");
        initial["players"]["P1"]["zones"]["banish"] = json!(
            (0..banished)
                .map(|i| json!({"id":format!("banished{i}"),"card":"BP01-037"}))
                .collect::<Vec<_>>()
        );
        let mut game = build(&initial);
        let play = game
            .play_internal_card("source", PlayerId { seat: 1 })
            .unwrap();
        let result = resolved(finish(&mut game, play, &[mode]));
        if banished == 10 {
            combined(&result, 2);
        }
        game.resume_internal_card().unwrap();
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P2"]["leader"]["life"],
            if banished == 10 { 18_u8 } else { 20 },
            "the selected follower controller receives its share in the same batch"
        );
        assert_eq!(
            state["objects"]["enemy"]["damage"],
            if banished == 10 { 2_u8 } else { 0 },
            "only the ten-card threshold applies damage"
        );
        if mode == 2 {
            assert_eq!(state["P1"]["hand"], json!(["draw0"]));
            assert_eq!(state["P1"]["banish"], json!(["draw1"]));
        }
    }
}
