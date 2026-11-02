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
fn end(game: &mut Game) {
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
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
fn bp05_112_all_printings_test_the_selected_name_and_grant_only_that_receiver_a_shield() {
    for printing in ["BP05-112", "BP05-P52"] {
        for selected_mael in [false, true] {
            let mut initial = position(printing);
            initial["players"]["P1"]["zones"]["field"] =
                json!([{"id":"mael","card":"BP05-104"},{"id":"ordinary","card":"BP01-037"}]);
            let mut game = with_producer(&initial, damage_own());
            let before = game.projection(View::Referee).unwrap();
            play(
                &mut game,
                Some(if selected_mael {
                    "BP05-104"
                } else {
                    "BP01-037"
                }),
            );
            let buffed = game.projection(View::Referee).unwrap();
            let holder = if selected_mael { "mael" } else { "ordinary" };
            assert_eq!(
                buffed["objects"][holder]["power"]
                    .as_i64()
                    .unwrap()
                    .checked_sub(before["objects"][holder]["power"].as_i64().unwrap())
                    .unwrap(),
                if selected_mael { 3 } else { 1 }
            );
            assert_eq!(keyword(&buffed, holder, "指定攻撃"), selected_mael);
            produce(&mut game);
            let after = game.projection(View::Referee).unwrap();
            assert_eq!(
                buffed["objects"][holder]["hp"]
                    .as_i64()
                    .unwrap()
                    .checked_sub(after["objects"][holder]["hp"].as_i64().unwrap())
                    .unwrap(),
                if selected_mael { 0 } else { 2 }
            );
        }
    }
}

#[test]
fn bp09_t02_all_printings_revive_a_real_witch_with_rush_then_destroy_that_new_generation_at_end() {
    for printing in ["BP09-T02", "BP21-T14"] {
        let mut initial = position(printing);
        initial["players"]["P1"]["zones"]["cemetery"] = json!([{"id":"revived","card":"BP08-042"},{"id":"wrong-class","card":"BP01-117"},{"id":"too-expensive","card":"BP02-038"}]);
        let mut game = game(&initial);
        let step = game.play_internal_card("source", actor()).unwrap();
        let CardPlay::Awaiting(question) = &step else {
            panic!("revival target");
        };
        let InputDomain::Targets(domain) = &question.domain else {
            panic!("revival targets");
        };
        assert_eq!(
            domain.members.len(),
            1,
            "wrong class and original cost four are excluded independently"
        );
        assert_eq!(domain.members[0].card.as_deref(), Some("BP08-042"));
        finish(&mut game, step, Some("BP08-042"));
        finish_checks(&mut game);
        let view = game.projection(View::Referee).unwrap();
        assert!(
            view["P1"]["field"]
                .as_array()
                .unwrap()
                .contains(&json!("revived"))
        );
        assert!(keyword(&view, "revived", "突進"));
        end(&mut game);
        trigger(&mut game, 1, None);
        assert!(
            game.projection(View::Referee).unwrap()["P1"]["cemetery"]
                .as_array()
                .unwrap()
                .contains(&json!("revived"))
        );
    }
}

#[test]
fn bp14_p30_all_printings_pay_with_the_amulet_then_the_commander_keeps_its_granted_attack() {
    for printing in ["BP14-P30", "BP14-T03"] {
        let mut initial = position("BP01-037");
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] =
            json!([{"id":"source","card":printing},{"id":"commander","card":"BP01-036"}]);
        let mut game = game(&initial);
        let activation = game
            .activate_internal_card("source", activated(printing), actor())
            .unwrap();
        finish(&mut game, activation, Some("BP01-036"));
        finish_checks(&mut game);
        let view = game.projection(View::Referee).unwrap();
        for zone in ["field", "cemetery"] {
            assert!(
                !view["P1"][zone]
                    .as_array()
                    .unwrap()
                    .contains(&json!("source")),
                "the token leaves the field as payment and is erased from the cemetery"
            );
        }
        assert_eq!(view["P1"]["pp"]["current"], 9_u8);
        game.internal_rule_action(
            &json!({"do":"attack","attacker":"commander","target":"P2.leader"}),
        )
        .unwrap();
        trigger(&mut game, 2, None);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
            18_u8
        );
    }
}

#[test]
fn bp17_055_all_printings_preserve_its_real_fanfare_and_create_the_complete_ex_curse() {
    for printing in ["BP17-055", "BP17-SL13", "BP17-U06"] {
        let mut game = game(&position(printing));
        play(&mut game, None);
        trigger(&mut game, 3, None);
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["P1"]["ex"].as_array().unwrap().len(), 1);
        let curse = view["P1"]["ex"][0].as_str().unwrap();
        assert_eq!(view["objects"][curse]["name"], "黒龍の呪い");
        assert!(keyword(&view, "source", "突進"));
        assert!(keyword(&view, "source", "指定攻撃"));
    }
}

#[test]
fn bp17_p33_all_printings_hurt_the_owner_only_before_awakening_at_its_actual_end() {
    for printing in ["BP17-P33", "BP17-T06"] {
        for awakened in [false, true] {
            let mut initial = position("BP01-037");
            initial["players"]["P1"]["zones"]["hand"] = json!([]);
            initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"curse","card":printing}]);
            initial["players"]["P1"]["pp"]["max"] = json!(if awakened { 7_u8 } else { 6_u8 });
            initial["players"]["P1"]["pp"]["current"] = json!(if awakened { 7_u8 } else { 6_u8 });
            let mut game = game(&initial);
            end(&mut game);
            trigger(&mut game, 1, None);
            assert_eq!(
                game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
                if awakened { 20_u8 } else { 19_u8 }
            );
        }
    }
}

#[test]
fn bp18_026_all_printings_destroy_only_with_three_enemy_cards_and_keep_the_granted_shield() {
    for printing in ["BP18-026", "PR-478"] {
        for crowded in [false, true] {
            let mut initial = position(printing);
            initial["players"]["P2"]["zones"]["field"] =
                json!([{"id":"enemy","card":"BP01-037"},{"id":"enemy2","card":"BP01-037"}]);
            if crowded {
                initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"},{"id":"enemy2","card":"BP01-037"},{"id":"enemy3","card":"BP01-037"}]);
            }
            let mut game = with_producer(&initial, damage_own());
            play(&mut game, None);
            trigger(&mut game, 2, Some("BP01-037"));
            let before = game.projection(View::Referee).unwrap();
            assert_eq!(
                before["P2"]["cemetery"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("enemy")),
                crowded
            );
            assert_eq!(keyword(&before, "source", "オーラ"), crowded);
            produce(&mut game);
            assert_eq!(
                before["objects"]["source"]["hp"]
                    .as_i64()
                    .unwrap()
                    .checked_sub(
                        game.projection(View::Referee).unwrap()["objects"]["source"]["hp"]
                            .as_i64()
                            .unwrap()
                    )
                    .unwrap(),
                if crowded { 0 } else { 2 },
                "the paragraph condition gates both destruction and the granted shield"
            );
        }
    }
}

#[test]
fn bp21_073_all_printings_revive_the_chosen_academy_then_the_granted_end_trigger_moves_it_to_cemetery()
 {
    for printing in ["BP21-073", "BP21-SL17"] {
        let mut initial = position(printing);
        initial["players"]["P1"]["zones"]["cemetery"] = json!([{"id":"revived","card":"BP08-042"}]);
        let mut game = game(&initial);
        play(&mut game, None);
        trigger(&mut game, 2, Some("BP08-042"));
        assert!(
            game.projection(View::Referee).unwrap()["P1"]["field"]
                .as_array()
                .unwrap()
                .contains(&json!("revived"))
        );
        end(&mut game);
        trigger(&mut game, 2, None);
        assert!(
            game.projection(View::Referee).unwrap()["P1"]["cemetery"]
                .as_array()
                .unwrap()
                .contains(&json!("revived"))
        );
    }
}

#[test]
fn cp04_045_all_printings_return_the_real_priconne_spell_then_its_granted_ex_trigger_cleans_up() {
    for printing in ["CP04-045", "CP04-P33", "PCS01-018"] {
        let mut initial = position(printing);
        initial["players"]["P1"]["zones"]["cemetery"] =
            json!([{"id":"recovered","card":"CP04-118"}]);
        let mut game = rules_game(&initial);
        play(&mut game, None);
        trigger(&mut game, 2, Some("CP04-118"));
        assert!(
            game.projection(View::Referee).unwrap()["P1"]["ex"]
                .as_array()
                .unwrap()
                .contains(&json!("recovered"))
        );
        end(&mut game);
        trigger(&mut game, 2, None);
        assert!(
            game.projection(View::Referee).unwrap()["P1"]["cemetery"]
                .as_array()
                .unwrap()
                .contains(&json!("recovered"))
        );
    }
}

#[test]
fn bp14_021_all_printings_leave_a_real_complete_spears_token_after_their_lastword() {
    for printing in ["BP14-021", "BP14-SL07", "BP14-U02"] {
        let mut initial = position("BP01-037");
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP14-020"}]);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":printing}]);
        let mut game = with_producer(
            &initial,
            Action::Destroy {
                objects: own_followers(),
            },
        );
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}}),
        )
        .unwrap();
        trigger(&mut game, 2, Some("BP01-037"));
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["enemy"]["hp"],
            3_u8
        );
        produce(&mut game);
        trigger(&mut game, 3, None);
        let after = game.projection(View::Referee).unwrap();
        assert!(
            after["P1"]["cemetery"]
                .as_array()
                .unwrap()
                .contains(&json!("source"))
        );
        let created = after["P1"]["field"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(Value::as_str)
            .find(|id| *id != "producer")
            .unwrap();
        assert_eq!(after["objects"][created]["name"], "炎将の二槍");
    }
}

#[test]
fn bp17_079_all_printings_discard_two_machine_cards_then_replay_the_same_ex_card_for_one_pp() {
    for printing in ["BP17-079", "BP17-P18"] {
        let mut initial = position("BP01-037");
        initial["players"]["P1"]["zones"]["hand"] =
            json!([{"id":"first","card":"BP07-079"},{"id":"second","card":"BP07-079"}]);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":printing}]);
        let mut game = with_producer(
            &initial,
            Action::Destroy {
                objects: own_followers(),
            },
        );
        produce(&mut game);
        trigger(&mut game, 3, None);
        let returned = game.projection(View::Referee).unwrap();
        assert_eq!(returned["P1"]["ex"], json!(["source"]));
        assert_eq!(returned["P1"]["hand"], json!([]));
        assert!(
            returned["P1"]["cemetery"]
                .as_array()
                .unwrap()
                .contains(&json!("first"))
        );
        assert!(
            returned["P1"]["cemetery"]
                .as_array()
                .unwrap()
                .contains(&json!("second"))
        );
        play(&mut game, None);
        trigger(&mut game, 2, None);
        let replayed = game.projection(View::Referee).unwrap();
        assert_eq!(
            replayed["P1"]["pp"]["current"], 9_u8,
            "the real payment applies the granted -1 once"
        );
        assert!(
            keyword(&replayed, "source", "指定攻撃"),
            "the complete Fanfare distinguishes EX entry from hand entry"
        );
    }
}

#[test]
fn bp18_080_and_081_all_printings_pay_the_real_discard_create_a_bat_then_super_evolve_its_field_aura()
 {
    for (base, form) in [
        ("BP18-080", "BP18-081"),
        ("BP18-SL20", "BP18-SL21"),
        ("PR-479", "PR-480"),
    ] {
        let mut initial = position(base);
        initial["turn"]["elapsed_turns"] = json!({"P1":7_u8,"P2":6_u8});
        initial["players"]["P1"]["sep"] = json!(1_u8);
        initial["players"]["P1"]["zones"]["hand"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"discard","card":"BP02-069"}));
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"bat","card":"BP01-T15"}]);
        initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"drawn","card":"BP01-037"}]);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
        let mut game = game(&initial);
        play(&mut game, None);
        trigger(&mut game, 2, None);
        let created = game.projection(View::Referee).unwrap();
        assert_eq!(created["P1"]["hand"], json!(["drawn"]));
        assert!(
            created["P1"]["cemetery"]
                .as_array()
                .unwrap()
                .contains(&json!("discard"))
        );
        assert_eq!(created["P1"]["ex"].as_array().unwrap().len(), 1);
        assert!(!keyword(&created, "bat", "疾走"));
        game.internal_rule_action(&json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8,"sep":1_u8}})).unwrap();
        trigger(&mut game, 1, Some("BP01-037"));
        trigger(&mut game, 2, None);
        let evolved = game.projection(View::Referee).unwrap();
        assert_eq!(evolved["objects"]["enemy"]["hp"], 3_u8);
        assert!(keyword(&evolved, "bat", "疾走"));
        assert!(keyword(&evolved, "bat", "必殺"));
        let ex = evolved["P1"]["ex"][0].as_str().unwrap();
        assert!(
            !keyword(&evolved, ex, "疾走"),
            "the granted field aura does not grant keywords to EX cards"
        );
    }
}

#[test]
fn bp21_021_all_printings_super_evolve_and_reduce_damage_only_to_academy_followers() {
    for form in ["BP21-021", "BP21-SL07"] {
        let mut initial = position("BP01-037");
        initial["turn"]["elapsed_turns"] = json!({"P1":7_u8,"P2":6_u8});
        initial["players"]["P1"]["sep"] = json!(1_u8);
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP21-020"},{"id":"academy","card":"BP08-042"},{"id":"other","card":"BP01-037"}]);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
        initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"searched","card":"BP08-042"}]);
        let mut game = with_producer(&initial, damage_own());
        game.internal_rule_action(&json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8,"sep":1_u8}})).unwrap();
        trigger(&mut game, 1, None);
        trigger(&mut game, 2, None);
        assert!(
            game.projection(View::Referee).unwrap()["P1"]["field"]
                .as_array()
                .unwrap()
                .contains(&json!("searched")),
            "the printed evolution search places the complete Academy card on the field"
        );
        let before = game.projection(View::Referee).unwrap();
        produce(&mut game);
        let after = game.projection(View::Referee).unwrap();
        assert_eq!(
            after["objects"]["academy"]["hp"],
            before["objects"]["academy"]["hp"]
        );
        assert_eq!(
            before["objects"]["other"]["hp"]
                .as_i64()
                .unwrap()
                .checked_sub(after["objects"]["other"]["hp"].as_i64().unwrap())
                .unwrap(),
            2
        );
    }
}

#[test]
fn bp22_023_and_024_all_printings_pay_the_printed_evolution_and_shield_the_chosen_ally_in_combat() {
    for (base, form) in [("BP22-023", "BP22-024"), ("PR-596", "PR-597")] {
        let mut initial = position(base);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"ally","card":"BP01-037"}]);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
        let mut game = game(&initial);
        play(&mut game, None);
        trigger(&mut game, 2, None);
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":2_u8}}),
        )
        .unwrap();
        trigger(&mut game, 2, Some("BP01-037"));
        let before = game.projection(View::Referee).unwrap();
        assert_eq!(before["P1"]["pp"]["current"], 7_u8);
        assert!(keyword(&before, "source", "指定攻撃"));
        game.internal_rule_action(&json!({"do":"attack","attacker":"ally","target":"enemy"}))
            .unwrap();
        pass_quick(&mut game);
        let after = game.projection(View::Referee).unwrap();
        assert_eq!(
            after["objects"]["ally"]["hp"], before["objects"]["ally"]["hp"],
            "the printed evolution grants combat prevention to the chosen ally"
        );
        assert!(
            after["P2"]["cemetery"]
                .as_array()
                .unwrap()
                .contains(&json!("enemy")),
            "the enemy dies to the ordinary combat damage"
        );
    }
}

#[test]
fn bp17_p33_all_printings_require_awakening_and_the_complete_spell_increases_its_chosen_rowens_damage()
 {
    for printing in ["BP17-P33", "BP17-T06"] {
        for awakened in [false, true] {
            let mut initial = position("BP01-037");
            initial["players"]["P1"]["zones"]["hand"] = json!([]);
            initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"source","card":printing}]);
            initial["players"]["P1"]["zones"]["field"] = json!([{"id":"rowen","card":"BP17-055"}]);
            initial["players"]["P1"]["pp"]["max"] = json!(if awakened { 7_u8 } else { 6_u8 });
            initial["players"]["P1"]["pp"]["current"] = json!(if awakened { 7_u8 } else { 6_u8 });
            let mut game = game(&initial);
            if !awakened {
                let before = format!("{game:?}");
                game.play_internal_card("source", actor()).unwrap_err();
                assert_eq!(
                    format!("{game:?}"),
                    before,
                    "failed printed restriction does not consume the token"
                );
                continue;
            }
            play(&mut game, Some("BP17-055"));
            game.internal_rule_action(&json!({"do":"attack","attacker":"rowen","target":"enemy"}))
                .unwrap();
            trigger(&mut game, 2, Some("BP01-037"));
            let after = game.projection(View::Referee).unwrap();
            assert_eq!(
                after["objects"]["enemy"]["hp"], 2_u8,
                "the printed 2 damage becomes 3"
            );
            assert_eq!(
                after["P2"]["leader"]["life"], 18_u8,
                "the awakened printed 1 damage becomes 2"
            );
        }
    }
}

#[test]
fn bp17_055_all_printings_pay_four_pp_for_the_complete_activated_damage() {
    for printing in ["BP17-055", "BP17-SL13", "BP17-U06"] {
        let mut initial = position("BP01-037");
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":printing}]);
        let mut game = game(&initial);
        let step = game
            .activate_internal_card("source", activated(printing), actor())
            .unwrap();
        finish(&mut game, step, Some("BP01-037"));
        finish_checks(&mut game);
        let after = game.projection(View::Referee).unwrap();
        assert_eq!(after["P1"]["pp"]["current"], 6_u8);
        assert_eq!(after["objects"]["enemy"]["hp"], 2_u8);
        assert_eq!(after["P2"]["leader"]["life"], 19_u8);
    }
}

#[test]
fn bp17_079_all_printings_grant_rush_and_bane_only_while_another_complete_machine_follower_is_present()
 {
    for printing in ["BP17-079", "BP17-P18"] {
        for other in [false, true] {
            let mut initial = position("BP01-037");
            initial["players"]["P1"]["zones"]["hand"] = json!([]);
            initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":printing}]);
            if other {
                initial["players"]["P1"]["zones"]["field"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!({"id":"machine","card":"BP07-044"}));
            }
            let game = game(&initial);
            let view = game.projection(View::Referee).unwrap();
            for name in ["突進", "必殺"] {
                assert_eq!(keyword(&view, "source", name), other);
            }
        }
    }
}
