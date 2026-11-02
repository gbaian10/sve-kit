#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate the printed grant scenarios."
)]
use super::*;
use sve_engine::game::internal::SearchAnswer;

const fn actor() -> PlayerId {
    PlayerId { seat: 1 }
}
fn activated(number: &str) -> AbilityId {
    selection().current[number]
        .program
        .card()
        .abilities
        .iter()
        .find(|ability| ability.node.kind == AbilityKind::Activated)
        .unwrap()
        .node
        .id
}

fn rules_game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "light-grants",
    )
    .unwrap()
}

fn pass_quick(game: &mut Game) {
    for _ in 0_u8..4 {
        finish_checks(game);
        if game.internal_pending_choice().unwrap().is_some() {
            return;
        }
        let Some(question) = game.internal_quick_choice().unwrap() else {
            return;
        };
        game.answer_internal_card(question.actor, question.token, Answer::Quick(None))
            .unwrap();
    }
    panic!("Quick window did not close");
}

fn position(card: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-037","state":{"acted":true}}]);
    initial
}
fn finish_checks(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}
fn finish(game: &mut Game, mut step: CardPlay, target: Option<&str>) {
    while let CardPlay::Awaiting(q) = step {
        let answer = if matches!(q.domain, InputDomain::OptionalCost) {
            Answer::OptionalCost(true)
        } else if matches!(q.domain, InputDomain::WardEntry { .. }) {
            Answer::WardEntry(false)
        } else if let InputDomain::Targets(d) = &q.domain {
            let member = d
                .members
                .iter()
                .find(|m| target.is_none_or(|target| m.card.as_deref() == Some(target)))
                .unwrap();
            Answer::Targets(vec![member.handle])
        } else if let InputDomain::CostMaterials(d) = &q.domain {
            Answer::CostMaterials(
                d.members
                    .iter()
                    .take(usize::try_from(d.cardinality.maximum.unwrap().0).unwrap())
                    .map(|m| m.handle)
                    .collect(),
            )
        } else if let InputDomain::Search(d) = &q.domain {
            Answer::Search(SearchAnswer::Ungrouped(
                d.cards
                    .iter()
                    .take(usize::try_from(d.cardinality.maximum.unwrap().0).unwrap())
                    .map(|m| m.handle)
                    .collect(),
            ))
        } else {
            panic!("unexpected printed decision");
        };
        step = game.answer_internal_card(q.actor, q.token, answer).unwrap();
    }
}
fn trigger(game: &mut Game, line: u64, target: Option<&str>) {
    let q = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &q.domain else {
        panic!("printed pending ability");
    };
    let chosen = options.iter().find(|o| o.line == line).unwrap();
    let step = game
        .answer_internal_card(q.actor, q.token, Answer::Pending(chosen.handle))
        .unwrap();
    finish(game, step, target);
    finish_checks(game);
}
fn play(game: &mut Game, target: Option<&str>) {
    let step = game.play_internal_card("source", actor()).unwrap();
    finish(game, step, target);
    finish_checks(game);
}
fn keyword(view: &Value, object: &str, keyword: &str) -> bool {
    view["objects"][object]["keywords"]
        .as_array()
        .unwrap()
        .contains(&json!(keyword))
}
fn own_followers() -> Selector {
    let mut selector = subjects();
    if let Selector::Zone { players, .. } = &mut selector {
        players.relation = PlayerRelation::SelfPlayer;
        players.cardinality = Cardinality {
            minimum: Nat(1),
            maximum: Some(Nat(1)),
        };
    }
    Selector::Difference {
        input: Box::new(selector),
        except: Box::new(Selector::Objects {
            objects: vec![ObjectOperand::SourceHolder {}],
        }),
    }
}
fn damage_own() -> Action {
    Action::Damage {
        recipients: RecipientSet::Objects {
            objects: own_followers(),
        },
        amount: DamageAmount::Uniform {
            amount: Expr::Literal(Int(2)),
        },
        flags: DamageFlags::Ability,
        source: ObjectView::SourceInformation {},
        batch: BatchPolicy::Instruction {},
    }
}
fn produce(game: &mut Game) {
    let step = game
        .activate_internal_card("producer", AbilityId(1), actor())
        .unwrap();
    finish(game, step, None);
    finish_checks(game);
}

#[test]
fn bp06_018_all_printings_count_only_face_up_evolve_followers_and_stand_on_the_granted_attack() {
    for printing in ["BP06-018", "BP06-SL04", "PR-212"] {
        for count in [2_u8, 3, 7] {
            let mut initial = position(printing);
            initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037","state":{"acted":true}},{"id":"other","card":"BP01-037","state":{"acted":true}}]);
            let mut cards = (0..count)
                .map(|n| json!({"id":format!("used-{n}"),"card":"BP02-119","face_up":true}))
                .collect::<Vec<_>>();
            cards.push(json!({"id":"down","card":"BP02-119","face_up":false}));
            cards.push(json!({"id":"ordinary","card":"BP01-037","face_up":true}));
            initial["players"]["P1"]["zones"]["evolve_deck"] = json!(cards);
            let mut game = rules_game(&initial);
            let before = game.projection(View::Referee).unwrap();
            play(&mut game, None);
            trigger(&mut game, 2, None);
            let buffed = game.projection(View::Referee).unwrap();
            assert_eq!(
                buffed["objects"]["source"]["power"]
                    .as_i64()
                    .unwrap()
                    .checked_sub(before["objects"]["source"]["power"].as_i64().unwrap())
                    .unwrap(),
                i64::from(count)
            );
            assert_eq!(keyword(&buffed, "source", "指定攻撃"), count >= 3);
            game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"enemy"}))
                .unwrap();
            if count == 7 {
                trigger(&mut game, 2, None);
                assert_eq!(
                    game.projection(View::Referee).unwrap()["objects"]["source"]["acted"],
                    false
                );
                pass_quick(&mut game);
                game.internal_rule_action(
                    &json!({"do":"attack","attacker":"source","target":"other"}),
                )
                .unwrap();
                assert!(
                    game.internal_pending_choice().unwrap().is_none(),
                    "the same granted attack cannot stand it twice in one turn"
                );
            } else {
                assert!(game.internal_pending_choice().unwrap().is_none());
            }
        }
    }
}

#[test]
fn bp12_029_and_030_grant_the_real_azord_attack_stats_and_the_evolved_printing_searches_its_real_card()
 {
    for evolved in [false, true] {
        let mut initial = position("BP01-037");
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] =
            json!([{"id":"source","card":"BP12-029"},{"id":"azord","card":"BP08-025"}]);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP12-030"}]);
        initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"found","card":"BP08-025"}]);
        let mut without_provider = initial.clone();
        without_provider["players"]["P1"]["zones"]["field"] =
            json!([{"id":"azord","card":"BP08-025"}]);
        assert!(!keyword(
            &rules_game(&without_provider)
                .projection(View::Referee)
                .unwrap(),
            "azord",
            "疾走"
        ));
        let mut game = rules_game(&initial);
        if evolved {
            game.internal_rule_action(
                &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}}),
            )
            .unwrap();
            trigger(&mut game, 3, None);
            assert_eq!(
                game.projection(View::Referee).unwrap()["P1"]["hand"],
                json!(["found"])
            );
        }
        let before = game.projection(View::Referee).unwrap();
        assert!(keyword(&before, "azord", "疾走"));
        game.internal_rule_action(&json!({"do":"attack","attacker":"azord","target":"P2.leader"}))
            .unwrap();
        trigger(&mut game, if evolved { 2 } else { 3 }, None);
        let after = game.projection(View::Referee).unwrap();
        for stat in ["power", "hp"] {
            assert_eq!(
                after["objects"]["azord"][stat]
                    .as_i64()
                    .unwrap()
                    .checked_sub(before["objects"]["azord"][stat].as_i64().unwrap())
                    .unwrap(),
                2
            );
        }
    }
}

#[test]
fn bp13_033_all_printings_grant_attack_stats_only_at_five_real_levion_cards() {
    for printing in ["BP13-033", "SD07-020"] {
        for count in [4_u8, 5] {
            let mut initial = position("BP01-037");
            initial["players"]["P1"]["zones"]["hand"] = json!([]);
            initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":printing}]);
            initial["players"]["P1"]["zones"]["cemetery"] = json!(
                (0..count)
                    .map(|n| json!({"id":format!("dead-{n}"),"card":"BP13-032"}))
                    .collect::<Vec<_>>()
            );
            let mut game = rules_game(&initial);
            let before = game.projection(View::Referee).unwrap();
            assert_eq!(keyword(&before, "source", "指定攻撃"), count == 5);
            game.internal_rule_action(
                &json!({"do":"attack","attacker":"source","target":"P2.leader"}),
            )
            .unwrap();
            if count == 5 {
                trigger(&mut game, 2, None);
                let after = game.projection(View::Referee).unwrap();
                assert_eq!(
                    after["objects"]["source"]["power"]
                        .as_i64()
                        .unwrap()
                        .checked_sub(before["objects"]["source"]["power"].as_i64().unwrap())
                        .unwrap(),
                    2
                );
                assert_eq!(
                    after["objects"]["source"]["hp"]
                        .as_i64()
                        .unwrap()
                        .checked_sub(before["objects"]["source"]["hp"].as_i64().unwrap())
                        .unwrap(),
                    1
                );
            } else {
                assert!(game.internal_pending_choice().unwrap().is_none());
            }
        }
    }
}

#[test]
fn bp18_017_buffs_the_other_follower_and_grants_rush_with_a_turn_shield() {
    let mut initial = position("BP18-017");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"ally","card":"BP01-037"}]);
    let mut game = with_producer(&initial, damage_own());
    play(&mut game, None);
    trigger(&mut game, 1, Some("BP01-037"));
    let before = game.projection(View::Referee).unwrap();
    assert_eq!(before["objects"]["ally"]["power"], 6_u8);
    assert!(keyword(&before, "ally", "突進"));
    assert!(!keyword(&before, "source", "突進"));
    produce(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["ally"]["hp"],
        before["objects"]["ally"]["hp"]
    );
}

#[test]
fn sirius_all_six_printings_count_the_entering_uma_and_only_then_grant_the_real_draw_discard_attack()
 {
    for printing in [
        "BP22-R20",
        "BP22-SL32",
        "ECP01-013",
        "ECP01-SL08",
        "ECP01-SP08",
        "ECP01-SSP08",
    ] {
        for count in [2_u8, 3] {
            let mut initial = position(printing);
            initial["players"]["P1"]["zones"]["field"] = json!(
                (0..count.checked_sub(1).unwrap())
                    .map(|n| json!({"id":format!("uma-{n}"),"card":"ECP01-008"}))
                    .collect::<Vec<_>>()
            );
            initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"drawn","card":"BP01-037"}]);
            let mut game = rules_game(&initial);
            let before = game.projection(View::Referee).unwrap();
            play(&mut game, None);
            trigger(&mut game, 3, None);
            let buffed = game.projection(View::Referee).unwrap();
            assert_eq!(
                buffed["objects"]["source"]["hp"]
                    .as_i64()
                    .unwrap()
                    .checked_sub(before["objects"]["source"]["hp"].as_i64().unwrap())
                    .unwrap(),
                i64::from(count == 3)
            );
            game.internal_rule_action(
                &json!({"do":"attack","attacker":"source","target":"P2.leader"}),
            )
            .unwrap();
            if count == 3 {
                trigger(&mut game, 3, None);
                let after = game.projection(View::Referee).unwrap();
                assert_eq!(after["P1"]["hand"], json!([]));
                assert!(
                    after["P1"]["cemetery"]
                        .as_array()
                        .unwrap()
                        .contains(&json!("drawn"))
                );
            } else {
                assert!(game.internal_pending_choice().unwrap().is_none());
            }
        }
    }
}

#[test]
fn ecp01_043_all_printings_apply_the_granted_attack_only_at_twenty_real_uma_cards() {
    for printing in ["ECP01-043", "ECP01-P23"] {
        for count in [19_u8, 20] {
            let mut initial = position("BP01-037");
            initial["players"]["P1"]["zones"]["hand"] = json!([]);
            initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":printing}]);
            initial["players"]["P1"]["zones"]["cemetery"] = json!(
                (0..count)
                    .map(|n| json!({"id":format!("uma-{n}"),"card":"ECP01-008"}))
                    .collect::<Vec<_>>()
            );
            let mut game = rules_game(&initial);
            let before = game.projection(View::Referee).unwrap();
            assert_eq!(keyword(&before, "source", "疾走"), count == 20);
            game.internal_rule_action(
                &json!({"do":"attack","attacker":"source","target":"P2.leader"}),
            )
            .unwrap();
            if count == 20 {
                trigger(&mut game, 2, None);
                let after = game.projection(View::Referee).unwrap();
                for stat in ["power", "hp"] {
                    assert_eq!(
                        after["objects"]["source"][stat]
                            .as_i64()
                            .unwrap()
                            .checked_sub(before["objects"]["source"][stat].as_i64().unwrap())
                            .unwrap(),
                        3
                    );
                }
            } else {
                assert!(game.internal_pending_choice().unwrap().is_none());
            }
        }
    }
}

#[test]
fn both_uzuki_cards_and_all_printings_search_without_their_own_name_then_pay_lesson_once_to_discount_the_actual_ex_play()
 {
    for printing in ["ECP02-015", "ECP02-SL09", "ECP02-SP04", "ECP02-U04"] {
        for cute in [4_u8, 5] {
            let mut initial = position(printing);
            initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"same","card":printing},{"id":"discounted","card":"ECP02-013"},{"id":"other","card":"ECP02-008"}]);
            initial["players"]["P1"]["zones"]["ex"] =
                json!([{"id":"item1","card":"PR-155"},{"id":"item2","card":"PR-156"}]);
            initial["players"]["P1"]["zones"]["cemetery"] = json!(
                (0..cute)
                    .map(|n| json!({"id":format!("cute-{n}"),"card":"CP02-008"}))
                    .collect::<Vec<_>>()
            );
            let mut game = rules_game(&initial);
            play(&mut game, None);
            trigger(&mut game, 1, None);
            let searched = game.projection(View::Referee).unwrap();
            assert!(
                searched["P1"]["deck"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("same")),
                "same-name exclusion uses the actual source printing"
            );
            assert!(
                searched["P1"]["ex"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("discounted"))
            );
            assert_eq!(
                searched["P1"]["ex"].as_array().unwrap().len(),
                if cute == 5 { 4 } else { 3 }
            );
            let step = game
                .activate_internal_card("source", activated(printing), actor())
                .unwrap();
            finish(&mut game, step, Some("ECP02-013"));
            finish_checks(&mut game);
            let paid = game.projection(View::Referee).unwrap();
            assert_eq!(
                paid["P1"]["ex"].as_array().unwrap().len(),
                searched["P1"]["ex"]
                    .as_array()
                    .unwrap()
                    .len()
                    .checked_sub(1)
                    .unwrap()
            );
            let before_retry = format!("{game:?}");
            game.activate_internal_card("source", activated(printing), actor()).expect_err(
                "a second material and target remain, but the printed once-per-turn limit is used"
            );
            assert_eq!(format!("{game:?}"), before_retry);
            let play_step = game.play_internal_card("discounted", actor()).unwrap();
            finish(&mut game, play_step, None);
            let played = game.projection(View::Referee).unwrap();
            assert!(
                played["P1"]["field"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("discounted"))
            );
            assert_eq!(
                played["P1"]["pp"]["current"], paid["P1"]["pp"]["current"],
                "the granted -5 reaches the real payment and floors the two-PP card at zero"
            );
        }
    }
}

#[test]
fn ecp01_043_all_printings_pay_the_real_meal_then_race_damage_buff_and_mill() {
    for printing in ["ECP01-043", "ECP01-P23"] {
        let mut initial = position("BP01-037");
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":printing}]);
        initial["players"]["P1"]["zones"]["evolve_deck"] =
            json!([{"id":"carrot","card":"CSD01-032"}]);
        initial["players"]["P1"]["zones"]["deck"] =
            json!([{"id":"milled1","card":"BP01-037"},{"id":"milled2","card":"BP01-037"}]);
        let mut game = rules_game(&initial);
        let before = game.projection(View::Referee).unwrap();
        let activation = meal_race::feed(&mut game, printing, 0, false);
        assert!(
            activation
                .progress
                .facts
                .iter()
                .any(|fact| matches!(fact, Fact::Raced(_)))
        );
        finish_checks(&mut game);
        trigger(&mut game, 3, Some("BP01-037"));
        let after = game.projection(View::Referee).unwrap();
        assert_eq!(after["P1"]["pp"]["current"], 9_u8);
        assert_eq!(after["P1"]["race"], json!(["carrot"]));
        assert_eq!(after["objects"]["enemy"]["hp"], 2_u8);
        for stat in ["power", "hp"] {
            assert_eq!(
                after["objects"]["source"][stat]
                    .as_i64()
                    .unwrap()
                    .checked_sub(before["objects"]["source"][stat].as_i64().unwrap())
                    .unwrap(),
                1
            );
        }
        assert_eq!(after["P1"]["cemetery"], json!(["milled1", "milled2"]));
    }
}

#[test]
fn csd03b_overlord_all_printings_stand_once_after_combat_damage_only_when_awakened() {
    for (base, evolved) in [
        ("CSD03b-001", "CSD03b-002"),
        ("CSD03b-SL01", "CSD03b-SL02"),
        ("CSD03b-001", "PR-288"),
    ] {
        for awakened in [false, true] {
            let mut initial = position(base);
            initial["players"]["P1"]["zones"]["hand"] = json!([]);
            initial["players"]["P1"]["pp"] =
                json!({"current":10_u8,"max":if awakened {10_u8} else {6_u8}});
            initial["players"]["P1"]["zones"]["field"] = json!([
                {"id":"source","card":base}, {"id":"ally","card":"CSD03b-001"}
            ]);
            initial["players"]["P1"]["zones"]["evolve_deck"] =
                json!([{"id":"form","card":evolved}]);
            initial["players"]["P1"]["zones"]["deck"] = json!([
                {"id":"drive1","card":"BP01-037"}, {"id":"drive2","card":"BP01-037"}
            ]);
            initial["players"]["P2"]["zones"]["field"] = json!([
                {"id":"enemy","card":"BP01-037","state":{"acted":true,"power":1_u8,"hp":15_u8,"max_hp":15_u8}}
            ]);
            let mut game = rules_game(&initial);
            assert_eq!(
                keyword(&game.projection(View::Referee).unwrap(), "source", "疾走"),
                awakened
            );
            game.internal_rule_action(
                &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}}),
            )
            .unwrap();
            trigger(&mut game, 3, Some("BP01-037"));
            assert_eq!(
                game.projection(View::Referee).unwrap()["objects"]["source"]["acted"],
                false
            );
            assert_eq!(
                game.projection(View::Referee).unwrap()["objects"]["enemy"]["hp"],
                10_u8
            );
            assert!(game.internal_pending_choice().unwrap().is_none());
            game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"enemy"}))
                .unwrap();
            trigger(&mut game, 1, None);
            pass_quick(&mut game);
            if awakened {
                trigger(&mut game, 3, None);
                assert_eq!(
                    game.projection(View::Referee).unwrap()["objects"]["source"]["acted"],
                    false
                );
                game.internal_rule_action(
                    &json!({"do":"attack","attacker":"source","target":"enemy"}),
                )
                .unwrap();
                trigger(&mut game, 1, None);
                pass_quick(&mut game);
            }
            assert!(game.internal_pending_choice().unwrap().is_none());
            assert_eq!(
                game.projection(View::Referee).unwrap()["objects"]["source"]["acted"],
                true
            );
        }
    }
}
