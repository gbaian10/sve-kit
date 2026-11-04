#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate printed card scenarios."
)]
use super::*;
use sve_engine::game::internal::SearchAnswer;
fn append_fixture_card(snapshot: &mut String, input: &mut Value, number: &str, face: &Value) {
    snapshot.push_str(&json!({"number": number, "faces": [face]}).to_string());
    snapshot.push('\n');
    let document = json!({
        "format": 1_u8,
        "kind": "effect_set",
        "cards": {number: {"status": "complete", "abilities": []}}
    });
    input["documents"]
        .as_array_mut()
        .unwrap()
        .push(json!([format!("{number}.yaml"), document.to_string()]));
}
fn fixture() -> &'static SelectedEffects {
    static SELECTED: OnceLock<SelectedEffects> = OnceLock::new();
    SELECTED.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = producer_inputs();
        // Minimal companions isolate search filters, current costs, and ordinary evolution.
        for (number, name, kind, traits) in [
            (
                "ready-deremas",
                "deremas companion",
                "フォロワー",
                vec!["デレマス"],
            ),
            ("ready-rin", "fixture 渋谷凛", "フォロワー", vec![]),
            ("ready-mio", "fixture 本田未央", "フォロワー", vec![]),
            ("ready-amulet", "plain amulet", "アミュレット", vec![]),
        ] {
            let follower = kind == "フォロワー";
            let face = json!({
                "name": name, "card_class": "ニュートラル", "card_type": kind,
                "cost": "4", "power": if follower {"3"} else {"-"},
                "hp": if follower {"4"} else {"-"}, "traits": traits,
                "text": null, "sections": []
            });
            append_fixture_card(&mut snapshot, &mut input, number, &face);
        }
        for form in ["CP02-024", "CP02-054", "CP02-058", "CP02-067", "CP02-078"] {
            let name = snapshot
                .lines()
                .map(|line| serde_json::from_str::<Value>(line).unwrap())
                .find(|row| row["number"] == form)
                .unwrap()["faces"][0]["name"]
                .clone();
            let number = format!("ready-base-{form}");
            let face = json!({
                "name": name, "card_class": "ニュートラル", "card_type": "フォロワー",
                "cost": "0", "power": "3", "hp": "4", "traits": [],
                "text": null, "sections": []
            });
            append_fixture_card(&mut snapshot, &mut input, &number, &face);
        }
        repository::selection_with_rules_from(&snapshot, &keywords, &input)
    })
}
fn position(card: &str) -> Value {
    let mut initial = draw_position();
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
        "ready-cp02-2",
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
                    .first()
                    .map(|m| vec![m.handle])
                    .unwrap_or_default(),
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
            InputDomain::Abilities(_)
            | InputDomain::ResourceEntryPayment { .. }
            | InputDomain::Quick { .. }
            | InputDomain::CostMaterials(_)
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
fn has_keyword(state: &Value, object: &str, keyword: &str) -> bool {
    state["objects"][object]["keywords"]
        .as_array()
        .unwrap()
        .contains(&json!(keyword))
}

#[test]
fn cp02_024_searches_either_name_fragment_and_discounts_the_ex_card() {
    for card in ["CP02-024", "CP02-P18"] {
        for chosen in ["ready-rin", "ready-mio"] {
            let mut initial = evolution_position(card);
            initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"wrong","card":"BP01-171"},{"id":"found","card":chosen},{"id":"tail","card":"BP01-171"}]);
            let mut game = evolved(&initial, card);
            assert_eq!(
                game.projection(View::Referee).unwrap()["P1"]["ex"],
                json!(["found"])
            );
            play(&mut game, "found", 1);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["P1"]["pp"]["current"], 7_u8);
            assert!(
                state["P1"]["field"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("found"))
            );
        }
    }
}

#[test]
fn cp02_054_searches_a_low_cost_spell_or_amulet_then_discounts_it_in_ex() {
    for card in ["CP02-054", "CP02-SL12", "CP02-SP07", "CP02-U07"] {
        for chosen in ["BP01-065", "BP01-149"] {
            let mut initial = evolution_position(card);
            initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"wrong","card":"BP01-171"},{"id":"expensive","card":"ready-amulet"},{"id":"found","card":chosen},{"id":"draw1","card":"BP01-171"},{"id":"draw2","card":"BP01-171"}]);
            let mut game = evolved(&initial, card);
            assert_eq!(
                game.projection(View::Referee).unwrap()["P1"]["ex"],
                json!(["found"])
            );
            play(&mut game, "found", 1);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["P1"]["pp"]["current"], 9_u8);
            assert_eq!(state["P1"]["ex"], json!([]));
            if chosen == "BP01-149" {
                assert!(has_keyword(&state, "source", "守護"));
            } else {
                // Searching shuffles, so the printed draw guarantees a count rather than identities.
                assert_eq!(state["P1"]["hand"].as_array().unwrap().len(), 2);
            }
        }
    }
}

#[test]
fn cp02_058_destroys_its_target_and_hits_its_leader_only_at_maximum_pp_ten() {
    for card in ["CP02-058", "CP02-P46"] {
        for maximum in [9_u8, 10] {
            let mut initial = evolution_position(card);
            initial["players"]["P1"]["pp"] = json!({"current":maximum,"max":maximum});
            let game = evolved(&initial, card);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["P2"]["cemetery"], json!(["enemy"]));
            assert_eq!(
                state["P2"]["leader"]["life"],
                if maximum == 10 { 17_u8 } else { 20_u8 }
            );
        }
    }
}

#[test]
fn cp02_063_counts_other_deremas_followers_for_leader_damage() {
    for card in ["CP02-063", "CP02-P51", "CSD02c-013"] {
        for n in [0_u8, 2] {
            let mut initial = position(card);
            initial["players"]["P1"]["zones"]["field"] = json!(
                (0..n)
                    .map(|i| json!({"id":format!("ally{i}"),"card":"ready-deremas"}))
                    .collect::<Vec<_>>()
            );
            initial["players"]["P1"]["zones"]["field"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":"wrong","card":"BP01-171"}));
            let mut game = game(&initial);
            play(&mut game, "source", 1);
            assert_eq!(
                game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
                20_u8.saturating_sub(n)
            );
        }
    }
}

#[test]
fn cp02_064_three_deremas_followers_allow_rush_and_attacking_heals_two() {
    for card in ["CP02-064", "CP02-P52", "CSD02a-015"] {
        let mut initial = position(card);
        initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
        initial["players"]["P1"]["zones"]["field"] =
            json!([{"id":"ally1","card":"ready-deremas"},{"id":"ally2","card":"ready-deremas"}]);
        initial["players"]["P2"]["zones"]["field"][0]["state"] =
            json!({"power":0_u8,"hp":20_u8,"max_hp":20_u8,"acted":true});
        let mut game = game(&initial);
        play(&mut game, "source", 1);
        let entered = game.projection(View::Referee).unwrap();
        assert_eq!(entered["objects"]["source"]["power"], 3_u8);
        assert!(has_keyword(&entered, "source", "突進"));
        assert_eq!(
            game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"enemy"}))
                .unwrap()
                .outcome,
            "resolved"
        );
        settle(&mut game, 1);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
            12_u8
        );
    }
}

#[test]
fn cp02_065_chooses_between_an_amulet_target_and_drawing() {
    for card in ["CP02-065", "CP02-P53", "CSD02a-016"] {
        for mode in [1_u64, 2] {
            let mut initial = position(card);
            initial["players"]["P2"]["zones"]["field"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":"amulet","card":"ready-amulet"}));
            let mut game = game(&initial);
            play(&mut game, "source", mode);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["P1"]["pp"]["current"], 6_u8);
            assert_eq!(
                state["P1"]["hand"],
                if mode == 2 {
                    json!(["draw0"])
                } else {
                    json!([])
                }
            );
            assert_eq!(
                state["P2"]["cemetery"],
                if mode == 1 {
                    json!(["amulet"])
                } else {
                    json!([])
                }
            );
        }
    }
}

#[test]
fn cp02_067_evolution_uses_maximum_pp_not_remaining_pp_for_damage() {
    for card in ["CP02-067", "CP02-P55", "CSD02b-017"] {
        let mut initial = evolution_position(card);
        initial["players"]["P1"]["pp"] = json!({"current":7_u8,"max":7_u8});
        initial["players"]["P2"]["zones"]["field"][0]["state"] = json!({"hp":20_u8,"max_hp":20_u8});
        let game = evolved(&initial, card);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["pp"]["current"], 6_u8);
        assert_eq!(state["objects"]["enemy"]["damage"], 7_u8);
    }
}

#[test]
fn cp02_078_evolution_can_damage_two_followers_in_one_batch() {
    for card in ["CP02-078", "CP02-P63"] {
        let mut initial = evolution_position(card);
        initial["players"]["P2"]["zones"]["field"] =
            json!([{"id":"enemy0","card":"BP01-037"},{"id":"enemy1","card":"BP01-037"}]);
        let mut game = prepare_evolution(&initial, card);
        if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
        let pending = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = &pending.domain else {
            panic!("evolution trigger");
        };
        let CardPlay::Awaiting(targets) = game
            .answer_internal_card(
                pending.actor,
                pending.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap()
        else {
            panic!("two printed targets");
        };
        let InputDomain::Targets(domain) = &targets.domain else {
            panic!("follower targets");
        };
        assert_eq!(domain.members.len(), 2);
        let step = game
            .answer_internal_card(
                targets.actor,
                targets.token,
                Answer::Targets(domain.members.iter().map(|m| m.handle).collect()),
            )
            .unwrap();
        finish(&mut game, step, 1);
        settle(&mut game, 1);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P2"]["field"], json!([]));
        assert_eq!(state["P2"]["cemetery"].as_array().unwrap().len(), 2);
    }
}

const fn self_damage() -> Action {
    Action::Damage {
        recipients: RecipientSet::Leaders {
            players: PlayerSet {
                relation: PlayerRelation::SelfPlayer,
                relative_to: PlayerRef::Controller {},
                text_quantifier: TextQuantifier::All,
                cardinality: Cardinality {
                    minimum: Nat(1),
                    maximum: Some(Nat(1)),
                },
            },
        },
        amount: DamageAmount::Uniform {
            amount: Expr::Literal(Int(1)),
        },
        flags: DamageFlags::Ability,
        source: ObjectView::SourceInformation {},
        batch: BatchPolicy::Instruction {},
    }
}
#[test]
fn cp02_079_destroys_then_uses_real_turn_life_loss_for_leader_damage() {
    for card in ["CP02-079", "CP02-P64"] {
        for crimson in [false, true] {
            let initial = position(card);
            let mut game = with_selected_producer(&initial, self_damage(), fixture());
            load_companions(&mut game);
            if crimson {
                activate(&mut game, "P1");
                settle(&mut game, 1);
            }
            play(&mut game, "source", 1);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["P2"]["cemetery"], json!(["enemy"]));
            assert_eq!(
                state["P2"]["leader"]["life"],
                if crimson { 18_u8 } else { 20_u8 }
            );
        }
    }
}

#[test]
fn cp02_064_two_deremas_followers_do_not_gain_power_or_rush() {
    for card in ["CP02-064", "CP02-P52", "CSD02a-015"] {
        let mut initial = position(card);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"ally","card":"ready-deremas"}]);
        let mut game = game(&initial);
        play(&mut game, "source", 1);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["source"]["power"], 2_u8);
        assert!(!has_keyword(&state, "source", "突進"));
    }
}

#[test]
fn cp02_058_without_a_follower_target_cannot_damage_the_leader() {
    for card in ["CP02-058", "CP02-P46"] {
        let mut initial = evolution_position(card);
        initial["players"]["P2"]["zones"]["field"] = json!([]);
        let game = evolved(&initial, card);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P2"]["leader"]["life"], 20_u8);
        assert_eq!(state["objects"]["source"]["evolved_with"], "form");
    }
}

#[test]
fn cp02_065_without_an_amulet_can_only_choose_drawing() {
    for card in ["CP02-065", "CP02-P53", "CSD02a-016"] {
        let initial = position(card);
        let mut game = game(&initial);
        let step = game
            .play_internal_card("source", PlayerId { seat: 1 })
            .unwrap();
        finish(&mut game, step, 2);
        if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
        let pending = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = &pending.domain else {
            panic!("fanfare pending");
        };
        let CardPlay::Awaiting(question) = game
            .answer_internal_card(
                pending.actor,
                pending.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap()
        else {
            panic!("mode decision");
        };
        let InputDomain::Modes { modes, .. } = &question.domain else {
            panic!("printed modes");
        };
        assert_eq!(modes.as_slice(), &[ModeId(2)]);
        let before = game.projection(View::Referee).unwrap();
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Modes(vec![ModeId(1)]),
        )
        .expect_err("a mode without its required amulet target must be rejected");
        assert_eq!(game.projection(View::Referee).unwrap(), before);
        let drawing = game
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Modes(vec![ModeId(2)]),
            )
            .unwrap();
        finish(&mut game, drawing, 2);
        settle(&mut game, 2);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["hand"],
            json!(["draw0"])
        );
    }
}

#[test]
fn cp02_078_evolution_allows_zero_or_one_follower_target() {
    for card in ["CP02-078", "CP02-P63"] {
        for count in [0_u8, 1] {
            let mut initial = evolution_position(card);
            initial["players"]["P2"]["zones"]["field"] = json!(
                (0..count)
                    .map(|i| json!({"id":format!("enemy{i}"),"card":"BP01-037"}))
                    .collect::<Vec<_>>()
            );
            let game = evolved(&initial, card);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["P2"]["field"], json!([]));
            assert_eq!(
                state["P2"]["cemetery"].as_array().unwrap().len(),
                usize::from(count)
            );
        }
    }
}
