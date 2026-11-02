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
        // Small companions isolate trait conditions and ordinary evolution from unrelated card abilities.
        for (number, name, traits) in [
            ("ready-deremas", "deremas companion", vec!["デレマス"]),
            ("ready-passion", "passion companion", vec!["デレマス", "パッション"]),
            ("ready-other", "other companion", vec![]),
            ("ready-base-061", "中野有香", vec!["デレマス", "キュート"]),
        ] {
            snapshot.push_str(&json!({"number":number,"faces":[{"name":name,"card_class":"ニュートラル","card_type":"フォロワー","cost":"0","power":"3","hp":"4","traits":traits,"text":null,"sections":[]}]}).to_string());
            snapshot.push('\n');
            input["documents"].as_array_mut().unwrap().push(json!([format!("{number}.yaml"),json!({"format":1_u8,"kind":"effect_set","cards":{number:{"status":"complete","abilities":[]}}}).to_string()]));
        }
        // A blank evolution resource keeps the printed payment test independent of the other face's abilities.
        let mut form = snapshot.lines().map(|line| serde_json::from_str::<Value>(line).unwrap()).find(|row| row["number"] == "CP02-058").unwrap()["faces"][0].clone();
        form["text"] = Value::Null;
        form["sections"] = json!([]);
        snapshot.push_str(&json!({"number":"ready-form-057","faces":[form]}).to_string());
        snapshot.push('\n');
        input["documents"].as_array_mut().unwrap().push(json!(["ready-form-057.yaml",json!({"format":1_u8,"kind":"effect_set","cards":{"ready-form-057":{"status":"complete","abilities":[]}}}).to_string()]));
        repository::selection_with_rules_from(&snapshot, &keywords, &input)
    })
}
fn position(card: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    initial["players"]["P1"]["zones"]["deck"] = json!(
        (0_u8..5)
            .map(|i| json!({"id":format!("draw{i}"),"card":"BP01-171"}))
            .collect::<Vec<_>>()
    );
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    initial
}
fn game(initial: &Value) -> Game {
    let mut game = Game::from_selected_effects(
        fixture().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "ready-cp02-1",
    )
    .unwrap();
    let empty = WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![],
    })
    .unwrap();
    for number in [
        "ready-deremas",
        "ready-passion",
        "ready-other",
        "ready-form-057",
    ] {
        game.load_internal_card(number, &empty).unwrap();
    }
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
            InputDomain::OptionalCost => Answer::OptionalCost(true),
            InputDomain::Targets(domain) => Answer::Targets(
                domain
                    .members
                    .first()
                    .map(|m| vec![m.handle])
                    .unwrap_or_default(),
            ),
            InputDomain::CostMaterials(domain) => Answer::CostMaterials(
                domain
                    .members
                    .iter()
                    .take(usize::try_from(domain.cardinality.minimum.0).unwrap())
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
fn allies(initial: &mut Value, number: &str, n: u8) {
    initial["players"]["P1"]["zones"]["field"] = json!(
        (0..n)
            .map(|i| json!({"id":format!("ally{i}"),"card":number}))
            .collect::<Vec<_>>()
    );
}
fn has_keyword(state: &Value, object: &str, keyword: &str) -> bool {
    state["objects"][object]["keywords"]
        .as_array()
        .unwrap()
        .contains(&json!(keyword))
}
fn evolve(initial: &Value, base: &str) -> Game {
    let mut game = game(initial);
    let mut tree = expanded::tree().clone();
    tree["cards"] = json!({base:tree["cards"]["synthetic-evolution"]});
    let program = lowerer().lower(&tree).unwrap().remove(base).unwrap();
    game.load_internal_card(base, &program).unwrap();
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
        )
        .unwrap()
        .outcome,
        "resolved",
        "the ordinary fixture evolution must resolve"
    );
    settle(&mut game, 1);
    game
}

#[test]
fn cp02_009_fanfare_requires_three_passion_followers() {
    for card in ["CP02-009", "CP02-P06"] {
        for n in [1_u8, 2] {
            let mut initial = position(card);
            allies(&mut initial, "ready-passion", n);
            let mut game = game(&initial);
            play(&mut game, 1);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["objects"]["enemy"]["damage"],
                if n == 2 { 2_u8 } else { 0_u8 }
            );
        }
    }
}

#[test]
fn cp02_012_grants_rush_only_to_another_deremas_follower() {
    for card in ["CP02-012", "CP02-P09", "CSD02b-012"] {
        let mut initial = position(card);
        initial["players"]["P1"]["zones"]["field"] =
            json!([{"id":"ally","card":"ready-deremas"},{"id":"other","card":"ready-other"}]);
        let mut game = game(&initial);
        play(&mut game, 1);
        let state = game.projection(View::Referee).unwrap();
        assert!(has_keyword(&state, "ally", "突進"));
        assert!(!has_keyword(&state, "other", "突進"));
        assert!(!has_keyword(&state, "source", "突進"));
    }
}

#[test]
fn cp02_013_three_existing_deremas_followers_reduce_play_cost() {
    for card in ["CP02-013", "CP02-P10"] {
        for n in [2_u8, 3] {
            let mut initial = position(card);
            allies(&mut initial, "ready-deremas", n);
            let mut game = game(&initial);
            play(&mut game, 1);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["P1"]["pp"]["current"],
                if n == 3 { 9_u8 } else { 6_u8 }
            );
            assert!(has_keyword(&state, "source", "守護"));
        }
    }
}

#[test]
fn cp02_017_fanfare_and_paid_departure_each_deal_three() {
    for card in ["CP02-017", "CP02-P14", "CSD02b-014"] {
        let mut initial = position(card);
        initial["players"]["P1"]["zones"]["hand"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"discard","card":"BP01-171"}));
        let mut game = game(&initial);
        play(&mut game, 1);
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
            3_u8
        );
        let ability = fixture().current[card]
            .program
            .card()
            .abilities
            .iter()
            .find(|a| a.node.kind == AbilityKind::Activated)
            .unwrap()
            .node
            .id;
        let step = game
            .activate_internal_card("source", ability, PlayerId { seat: 1 })
            .unwrap();
        finish(&mut game, step, 1);
        settle(&mut game, 1);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["pp"]["current"], 6_u8);
        assert_eq!(state["P1"]["cemetery"], json!(["discard", "source"]));
        assert_eq!(state["P2"]["cemetery"], json!(["enemy"]));
    }
}

#[test]
fn cp02_026_fanfare_gains_two_health_at_three_deremas_followers() {
    for card in ["CP02-026", "CP02-P20"] {
        for n in [1_u8, 2] {
            let mut initial = position(card);
            allies(&mut initial, "ready-deremas", n);
            let mut game = game(&initial);
            play(&mut game, 1);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["objects"]["source"]["hp"],
                if n == 2 { 3_u8 } else { 1_u8 }
            );
            assert_eq!(state["objects"]["source"]["power"], 1_u8);
        }
    }
}

#[test]
fn cp02_029_fanfare_requires_three_deremas_followers() {
    for card in ["CP02-029", "CP02-P23"] {
        for n in [1_u8, 2] {
            let mut initial = position(card);
            allies(&mut initial, "ready-deremas", n);
            let mut game = game(&initial);
            play(&mut game, 1);
            assert_eq!(
                game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
                if n == 2 { 2_u8 } else { 0_u8 }
            );
        }
    }
}

#[test]
fn cp02_041_deals_five_and_draws_only_with_a_passion_follower() {
    for card in ["CP02-041", "CP02-P32"] {
        for n in [0_u8, 1] {
            let mut initial = position(card);
            allies(&mut initial, "ready-passion", n);
            let mut game = game(&initial);
            play(&mut game, 1);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["P2"]["cemetery"], json!(["enemy"]));
            assert_eq!(
                state["P1"]["hand"],
                if n == 1 { json!(["draw0"]) } else { json!([]) }
            );
            assert_eq!(state["P1"]["pp"]["current"], 7_u8);
        }
    }
}

#[test]
fn cp02_045_replaces_two_damage_with_three_at_three_deremas_followers() {
    for card in ["CP02-045", "CP02-P36", "PR-147"] {
        for n in [2_u8, 3] {
            let mut initial = position(card);
            allies(&mut initial, "ready-deremas", n);
            let mut game = game(&initial);
            play(&mut game, 1);
            assert_eq!(
                game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
                if n == 3 { 3_u8 } else { 2_u8 }
            );
        }
    }
}

#[test]
fn cp02_049_buffs_another_deremas_follower_only() {
    for card in ["CP02-049", "CP02-P40", "CSD02a-014", "PR-146"] {
        let mut initial = position(card);
        initial["players"]["P1"]["zones"]["field"] =
            json!([{"id":"ally","card":"ready-deremas"},{"id":"other","card":"ready-other"}]);
        let mut game = game(&initial);
        play(&mut game, 1);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["ally"]["power"], 4_u8);
        assert_eq!(state["objects"]["ally"]["hp"], 5_u8);
        assert_eq!(state["objects"]["other"]["power"], 3_u8);
    }
}

#[test]
fn cp02_051_can_destroy_two_opposing_followers() {
    for card in ["CP02-051", "CP02-P42", "CSD02b-015"] {
        let mut initial = position(card);
        initial["players"]["P2"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"enemy2","card":"BP01-037"}));
        let mut game = game(&initial);
        let CardPlay::Awaiting(question) = game
            .play_internal_card("source", PlayerId { seat: 1 })
            .unwrap()
        else {
            panic!("formal spell targets");
        };
        let InputDomain::Targets(domain) = &question.domain else {
            panic!("two followers");
        };
        assert_eq!(domain.members.len(), 2);
        let step = game
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Targets(domain.members.iter().map(|m| m.handle).collect()),
            )
            .unwrap();
        finish(&mut game, step, 1);
        settle(&mut game, 1);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P2"]["field"], json!([]));
        assert_eq!(state["P2"]["cemetery"].as_array().unwrap().len(), 2);
        assert_eq!(state["P1"]["pp"]["current"], 5_u8);
    }
}

#[test]
fn cp02_057_fanfare_increases_maximum_pp_without_refilling_current_pp() {
    for card in ["CP02-057", "CP02-P45"] {
        let mut initial = position(card);
        initial["players"]["P1"]["pp"] = json!({"current":9_u8,"max":9_u8});
        let mut game = game(&initial);
        play(&mut game, 1);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["pp"],
            json!({"current":3_u8,"max":10_u8})
        );
    }
}

#[test]
fn cp02_061_evolution_replaces_one_damage_with_four_at_maximum_pp_ten() {
    for card in ["CP02-061", "CP02-P49"] {
        for maximum in [9_u8, 10] {
            let mut initial = position("ready-base-061");
            initial["players"]["P1"]["pp"] = json!({"current":maximum,"max":maximum});
            initial["players"]["P1"]["zones"]["hand"] = json!([]);
            initial["players"]["P1"]["zones"]["field"] =
                json!([{"id":"source","card":"ready-base-061"}]);
            initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":card}]);
            let game = evolve(&initial, "ready-base-061");
            assert_eq!(
                game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
                if maximum == 10 { 4_u8 } else { 1_u8 }
            );
        }
    }
}

#[test]
fn cp02_041_without_a_required_target_cannot_pay_or_draw() {
    for card in ["CP02-041", "CP02-P32"] {
        let mut initial = position(card);
        initial["players"]["P2"]["zones"]["field"] = json!([]);
        allies(&mut initial, "ready-passion", 1);
        let mut game = game(&initial);
        assert!(matches!(
            game.play_internal_card("source", PlayerId { seat: 1 }),
            Err(EngineFailure::IllegalPlay(_))
        ));
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["pp"]["current"], 10_u8);
        assert_eq!(state["P1"]["hand"], json!(["source"]));
        assert_eq!(state["P1"]["deck"].as_array().unwrap().len(), 5);
    }
}

#[test]
fn cp02_057_selects_pp_or_lesson_evolution_payment_and_rejects_short_payment() {
    for card in ["CP02-057", "CP02-P45"] {
        for line in [1_u32, 2] {
            let mut initial = position(card);
            initial["players"]["P1"]["zones"]["hand"] = json!([]);
            initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":card}]);
            initial["players"]["P1"]["zones"]["evolve_deck"] =
                json!([{"id":"form","card":"ready-form-057"}]);
            initial["players"]["P1"]["zones"]["ex"] = json!(
                (0_u8..3)
                    .map(|i| json!({"id":format!("magic{i}"),"card":"CP02-T01"}))
                    .collect::<Vec<_>>()
            );
            let definition = fixture().current[card]
                .program
                .card()
                .abilities
                .iter()
                .find(|a| {
                    a.source.line == line
                        && matches!(
                            a.node.source.origin,
                            AbilityOrigin::Keyword {
                                keyword: KeywordId::Evolution,
                                ..
                            }
                        )
                })
                .unwrap()
                .node
                .id;
            let mut game = game(&initial);
            let mut decision = json!({"do":"evolve","source":"source","evolve_card":"form","definition":definition.0,"pay":{"pp":u8::from(line==1)},"costs":{"materials":if line==1 {json!([])} else {json!(["magic0","magic1"])}}});
            assert_eq!(
                game.internal_rule_action(&decision).unwrap().outcome,
                "cannot-evolve"
            );
            let refused = game.projection(View::Referee).unwrap();
            assert_eq!(refused["P1"]["pp"]["current"], 10_u8);
            assert_eq!(refused["P1"]["ex"].as_array().unwrap().len(), 3);
            if line == 1 {
                decision["pay"]["pp"] = json!(2_u8);
            } else {
                decision["costs"]["materials"] = json!(["magic0", "magic1", "magic2"]);
            }
            assert_eq!(
                game.internal_rule_action(&decision).unwrap().outcome,
                "resolved"
            );
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["objects"]["source"]["evolved_with"], "form");
            assert_eq!(
                state["P1"]["pp"]["current"],
                if line == 1 { 8_u8 } else { 10_u8 }
            );
            assert_eq!(
                state["P1"]["ex"].as_array().unwrap().len(),
                if line == 1 { 3 } else { 0 }
            );
        }
    }
}
