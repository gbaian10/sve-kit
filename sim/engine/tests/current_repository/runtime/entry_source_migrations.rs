#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate the entry and recipient migration scenarios."
)]
use super::*;
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
        for (number,name,kind,power,hp,traits) in [
            ("block-enemy","標的","フォロワー","1","30",vec![]),
            ("block-yokai","別の妖怪","フォロワー","1","20",vec!["妖怪"]),
            ("block-fighter","武闘竜人","フォロワー","4","20",vec!["武闘竜人"]),
            ("block-amulet","計数アミュレット","アミュレット","-","-",vec![]),
            ("block-base-bp08","神の盾・ブローディア","フォロワー","1","10",vec![]),
            ("block-base-bp17","万緑の回帰・ラティカ","フォロワー","1","10",vec![]),
        ] {
            snapshot.push_str(&json!({"number":number,"faces":[{"name":name,"card_class":"ニュートラル","card_type":kind,"traits":traits,"cost":"1","power":power,"hp":hp,"text":null,"sections":[]}]}).to_string());snapshot.push('\n');
            input["documents"].as_array_mut().unwrap().push(json!([format!("{number}.yaml"),json!({"format":1_u8,"kind":"effect_set","cards":{number:{"status":"complete","abilities":[]}}}).to_string()]));
            let program=if number.starts_with("block-base-") {
                let mut tree=expanded::tree().clone();tree["cards"]=json!({number:tree["cards"]["synthetic-evolution"]});
                lowerer().lower(&tree).unwrap().remove(number).unwrap()
            }else{WellFormedCardProgram::check(CardProgram{abilities:vec![],card_play_costs:vec![]}).unwrap()};
            programs.push((number.to_owned(),program));
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
        "block-migrations",
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
        json!([{"id":"enemy","card":"block-enemy","owner":"P1"}]);
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
fn pending_line(game: &mut Game, line: u64, modes: &[u64]) -> CardPlay {
    if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("printed pending ability");
    };
    let option = options.iter().find(|option| option.line == line).unwrap();
    let play = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(option.handle),
        )
        .unwrap();
    finish(game, play, modes)
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
#[test]
fn bp21_104_draws_once_and_only_nonhand_entry_adds_two_power_and_health() {
    for zone in ["hand", "ex"] {
        let mut game = enter(&position("BP21-104", zone), zone);
        settle(&mut game, &[1]);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["hand"].as_array().unwrap().len(), 1);
        assert_eq!(
            state["objects"]["source"]["power"],
            if zone == "hand" { 4_u8 } else { 6 }
        );
        assert_eq!(
            state["objects"]["source"]["hp"],
            if zone == "hand" { 4_u8 } else { 6 }
        );
    }
}
#[test]
fn bp21_067_nonhand_fanfare_evolves_into_the_real_bane_assail_form() {
    for zone in ["hand", "ex"] {
        let mut initial = position("BP21-067", zone);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP21-068"}]);
        let mut game = enter(&initial, zone);
        settle(&mut game, &[1]);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["source"]["evolved"], zone != "hand");
        if zone == "ex" {
            assert_eq!(
                game.internal_rule_action(
                    &json!({"do":"attack","attacker":"source","target":"enemy"})
                )
                .unwrap()
                .outcome,
                "resolved"
            );
            resolved(pending_line(&mut game, 2, &[1]));
            assert_eq!(
                game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
                18_u8
            );
        }
    }
}
#[test]
fn bp11_076_evolution_head_only_works_for_the_cemetery_entry_generation() {
    for zone in ["hand", "ex", "cemetery"] {
        let mut initial = position("BP11-076", zone);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP11-077"}]);
        let mut game = enter(&initial, zone);
        settle(&mut game, &[1]);
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
            )
            .unwrap()
            .outcome,
            if zone == "cemetery" {
                "resolved"
            } else {
                "cannot-evolve"
            }
        );
    }
}
#[test]
fn bp16_078_cemetery_entry_can_choose_both_vehicle_healing_and_mill() {
    for zone in ["hand", "cemetery"] {
        let mut game = enter(&position("BP16-078", zone), zone);
        settle(&mut game, if zone == "cemetery" { &[1, 2] } else { &[2] });
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["cemetery"].as_array().unwrap().len(), 2);
        assert_eq!(
            state["P1"]["leader"]["life"],
            if zone == "cemetery" { 11_u8 } else { 10 }
        );
        assert_eq!(
            state["P1"]["field"].as_array().unwrap().len(),
            if zone == "cemetery" { 3 } else { 1 }
        );
    }
}
#[test]
fn bp12_072_ability_entry_evolves_and_the_real_form_creates_its_machine_and_mills() {
    for zone in ["hand", "cemetery"] {
        let mut initial = position("BP12-072", zone);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP12-073"}]);
        let mut game = enter(&initial, zone);
        settle(&mut game, &[1]);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["source"]["evolved"], zone == "cemetery");
        assert_eq!(
            state["P1"]["cemetery"].as_array().unwrap().len(),
            usize::from(zone == "cemetery")
        );
        assert_eq!(
            state["P1"]["field"].as_array().unwrap().len(),
            if zone == "cemetery" { 3 } else { 1 }
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
fn cp04_061_union_burst_attack_hits_the_follower_and_its_leader_with_the_max_pp_replacement() {
    for maximum in [9_u8, 10] {
        let mut initial = position("CP04-061", "field");
        initial["players"]["P1"]["pp"] = json!({"current":maximum,"max":maximum});
        let mut game = build(&initial);
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"attack","attacker":"source","target":"P2.leader"})
            )
            .unwrap()
            .outcome,
            "resolved"
        );
        let damage = if maximum == 10 { 4_u8 } else { 2 };
        combined(
            &resolved(pending_line(&mut game, 1, &[1])),
            u64::from(damage),
        );
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["enemy"]["damage"], damage);
        assert_eq!(
            state["P2"]["leader"]["life"],
            20_u8.checked_sub(damage).unwrap()
        );
        assert_eq!(state["P1"]["leader"]["life"], 10_u8);
    }
}
#[test]
fn bp08_088_ward_evolution_scales_one_damage_batch_with_own_amulets() {
    for amulets in [0_u8, 2] {
        let mut initial = position("block-base-bp08", "field");
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP08-088"}]);
        for i in 0..amulets {
            initial["players"]["P1"]["zones"]["field"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":format!("amulet{i}"),"card":"block-amulet"}));
        }
        let mut game = build(&initial);
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
            )
            .unwrap()
            .outcome,
            "resolved"
        );
        let result = resolved(pending_line(&mut game, 2, &[1]));
        if amulets > 0 {
            combined(&result, u64::from(amulets.checked_mul(2).unwrap()));
        }
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["objects"]["enemy"]["damage"],
            amulets.checked_mul(2).unwrap()
        );
        assert_eq!(
            state["P2"]["leader"]["life"],
            20_u8.checked_sub(amulets.checked_mul(2).unwrap()).unwrap()
        );
    }
}
#[test]
fn bp17_003_super_evolution_counts_newly_created_trees_and_keeps_the_two_damage_instructions_distinct()
 {
    let mut initial = position("block-base-bp17", "field");
    initial["turn"]["elapsed_turns"]["P1"] = json!(7_u8);
    initial["players"]["P1"]["sep"] = json!(1_u8);
    initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"tree","card":"BP07-T03"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP17-003"}]);
    let mut game = build(&initial);
    assert_eq!(game.internal_rule_action(&json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8,"sep":1_u8}})).unwrap().outcome,"resolved");
    resolved(pending_line(&mut game, 1, &[1]));
    game.resume_internal_card().unwrap();
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
        20_u8
    );
    combined(&resolved(pending_line(&mut game, 2, &[1])), 3);
    game.resume_internal_card().unwrap();
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["ex"].as_array().unwrap().len(), 3);
    assert_eq!(state["objects"]["enemy"]["damage"], 5_u8);
    assert_eq!(state["P2"]["leader"]["life"], 17_u8);
}
#[test]
fn bp18_060_observes_a_four_power_fighter_attack_and_retains_its_look_fanfare() {
    let mut initial = position("BP18-060", "field");
    initial["players"]["P2"]["zones"]["field"][0]["state"]["acted"] = json!(true);
    initial["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"fighter","card":"block-fighter"}));
    let mut game = build(&initial);
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"fighter","target":"enemy"}))
            .unwrap()
            .outcome,
        "resolved"
    );
    combined(&resolved(pending_line(&mut game, 1, &[1])), 1);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["damage"], 1_u8);
    assert_eq!(state["P2"]["leader"]["life"], 19_u8);
    let mut searched = position("BP18-060", "hand");
    searched["players"]["P1"]["zones"]["deck"] = json!([{"id":"fighter","card":"block-fighter"},{"id":"wrong","card":"BP01-037"},{"id":"untouched","card":"BP01-037"}]);
    let mut entered = enter(&searched, "hand");
    settle(&mut entered, &[1]);
    let after = entered.projection(View::Referee).unwrap();
    assert_eq!(after["P1"]["hand"], json!(["fighter"]));
    assert_eq!(after["P1"]["deck"], json!(["untouched", "wrong"]));
}
#[test]
fn bp15_079_creates_foxes_and_observes_another_yokai_departing() {
    let mut entered = enter(&position("BP15-079", "hand"), "hand");
    settle(&mut entered, &[1]);
    let after = entered.projection(View::Referee).unwrap();
    assert_eq!(after["P1"]["field"].as_array().unwrap().len(), 4);
    for id in after["P1"]["field"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .filter(|id| *id != "source")
    {
        assert_eq!(after["objects"][id]["power"], 2_u8);
    }
    let mut initial = position("BP15-079", "field");
    initial["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"yokai","card":"block-yokai"}));
    let binding = Binding::new(50, ScopeId(1));
    let objects = Selector::Zone {
        players: PlayerSet {
            relation: PlayerRelation::SelfPlayer,
            text_quantifier: TextQuantifier::Implicit,
            relative_to: PlayerRef::Controller {},
            cardinality: Cardinality {
                minimum: Nat(1),
                maximum: Some(Nat(1)),
            },
        },
        zone: ZoneKind::Field,
        view: SelectionView::Current {},
        binding,
        predicate: Box::new(Expr::Compute(Box::new(Predicate::NamesIntersect {
            left: Expr::Live(Box::new(NamesRead::Names {
                object: ObjectView::Current {
                    object: Box::new(ObjectOperand::Bound { binding }),
                },
            })),
            right: Expr::Literal(vec![CardName("別の妖怪".into())]),
        }))),
    };
    let action = Action::Move {
        objects,
        movement: MoveSpec {
            to: MoveDestination::Owners {
                zone: OwnerZone::Cemetery,
            },
            position: None,
            order: None,
            initial_posture: None,
            entry_effects: vec![],
        },
        reason: MoveReason::Put,
    };
    let mut game = with_selected_producer(&initial, action, &prepared().selected);
    load(&mut game);
    activate(&mut game, "P1");
    game.resume_internal_card().unwrap();
    combined(&resolved(pending_line(&mut game, 1, &[1])), 1);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
        19_u8
    );
}

#[test]
fn cp04_nozomi_sections_pay_public_amulets_and_damage_the_target_controller_in_one_batch() {
    for card in ["CP04-093", "CP04-SP12a"] {
        let mut initial = position(card, "hand");
        initial["players"]["P1"]["zones"]["field"] = json!([
            {"id":"a1","card":"block-amulet"},
            {"id":"a2","card":"block-amulet"},
            {"id":"a3","card":"block-amulet"}
        ]);
        initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"found","card":"block-amulet"}]);
        let mut game = enter(&initial, "hand");
        settle(&mut game, &[1]);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["hand"],
            json!(["found"])
        );
        for line in [1_u32, 2] {
            let definition = prepared().selected.current[card]
                .program
                .card()
                .abilities
                .iter()
                .find(|a| a.node.kind == AbilityKind::Activated && a.source.line == line)
                .unwrap()
                .node
                .id;
            let play = game
                .activate_internal_card("source", definition, PlayerId { seat: 1 })
                .unwrap();
            let activation = resolved(finish(&mut game, play, &[1]));
            if line == 2 {
                combined(&activation, 3);
            }
            settle(&mut game, &[1]);
        }
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["cemetery"], json!(["a1", "a2", "a3"]));
        assert_eq!(state["objects"]["enemy"]["damage"], 4_u8);
        assert_eq!(state["P2"]["leader"]["life"], 17_u8);
        assert_eq!(state["P1"]["leader"]["life"], 10_u8);
        assert_eq!(state["objects"]["source"]["acted"], true);
    }
}
#[test]
fn csd02c_003_lesson_and_two_passion_hand_materials_pay_one_complete_fanfare() {
    let mut initial = position("CSD02c-003", "hand");
    initial["players"]["P1"]["zones"]["ex"] = json!(
        (0..3_u8)
            .map(|i| json!({"id":format!("lesson{i}"),"card":"CP02-T01"}))
            .collect::<Vec<_>>()
    );
    initial["players"]["P1"]["zones"]["hand"]
        .as_array_mut()
        .unwrap()
        .extend([
            json!({"id":"fuel1","card":"CSD02c-005"}),
            json!({"id":"fuel2","card":"CSD02c-005"}),
        ]);
    let mut game = enter(&initial, "hand");
    settle(&mut game, &[1]);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["damage"], 6_u8);
    assert_eq!(state["P1"]["hand"].as_array().unwrap().len(), 2);
    assert_eq!(state["P1"]["ex"], json!([]));
    assert_eq!(state["P1"]["cemetery"].as_array().unwrap().len(), 2);
}
