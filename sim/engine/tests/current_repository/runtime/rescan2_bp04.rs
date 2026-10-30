use super::*;

mod first {
    #![expect(
        clippy::panic,
        reason = "Unexpected decisions invalidate the printed card scenarios."
    )]
    use super::*;
    use sve_engine::game::internal::SearchAnswer;

    const fn actor() -> PlayerId {
        PlayerId { seat: 1 }
    }

    fn position(card: &str) -> Value {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
        for seat in ["P1", "P2"] {
            initial["players"][seat]["zones"]["deck"] = json!([{"id":format!("{seat}-first"),"card":"BP01-171"},{"id":format!("{seat}-second"),"card":"BP01-171"}]);
        }
        initial["players"]["P2"]["zones"]["field"] =
            json!([{"id":"enemy","card":"BP01-037","state":{"acted":true}}]);
        initial
    }

    fn field(card: &str) -> Value {
        let mut initial = position(card);
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":card}]);
        initial
    }

    fn finish_checks(game: &mut Game) {
        while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
    }

    fn finish(game: &mut Game, mut step: CardPlay) {
        while let CardPlay::Awaiting(q) = step {
            let answer = if matches!(&q.domain, InputDomain::WardEntry { .. }) {
                Answer::WardEntry(false)
            } else if matches!(&q.domain, InputDomain::OptionalCost) {
                Answer::OptionalCost(true)
            } else if let InputDomain::Targets(d) = &q.domain {
                Answer::Targets(
                    d.members
                        .iter()
                        .take(usize::try_from(d.cardinality.maximum.unwrap().0).unwrap())
                        .map(|m| m.handle)
                        .collect(),
                )
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
            } else if let InputDomain::Order(cards) = &q.domain {
                Answer::Order(cards.iter().rev().map(|m| m.handle).collect())
            } else {
                panic!("unexpected decision")
            };
            step = game.answer_internal_card(q.actor, q.token, answer).unwrap();
        }
    }

    fn play(game: &mut Game, id: &str) {
        let step = game.play_internal_card(id, actor()).unwrap();
        finish(game, step);
    }

    fn trigger(game: &mut Game) {
        let q = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = &q.domain else {
            panic!("pending printed ability")
        };
        assert_eq!(
            options.len(),
            1,
            "only the intended printed ability is pending"
        );
        let step = game
            .answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
            .unwrap();
        finish(game, step);
        finish_checks(game);
    }

    fn evolve(game: &mut Game, cost: u8) {
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":cost}})
            )
            .unwrap()
            .outcome,
            "resolved",
            "the real base evolves for its printed PP cost"
        );
    }

    fn quick_pass(game: &mut Game) {
        let q = game.internal_quick_choice().unwrap().unwrap();
        let step = game
            .answer_internal_card(q.actor, q.token, Answer::Quick(None))
            .unwrap();
        finish(game, step);
    }

    fn evolution(base: &str, form: &str) -> Value {
        let mut initial = field(base);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
        initial
    }

    fn pay_life(game: &mut Game) {
        play(game, "payer");
        trigger(game);
    }

    #[test]
    fn bp04_080_fanfare_replaces_four_damage_with_eight_after_real_life_loss() {
        for crimson in [false, true] {
            let mut initial = position("BP04-080");
            initial["players"]["P1"]["zones"]["hand"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":"payer","card":"BP01-118"}));
            let mut game = game(&initial);
            if crimson {
                pay_life(&mut game);
            }
            play(&mut game, "source");
            trigger(&mut game);
            let s = game.projection(View::Referee).unwrap();
            if crimson {
                assert_eq!(
                    s["P2"]["cemetery"],
                    json!(["enemy"]),
                    "eight damage kills the five-health target"
                );
            } else {
                assert_eq!(
                    s["objects"]["enemy"]["damage"], 4_u8,
                    "without Crimson the Fanfare deals exactly four"
                );
            }
            assert_eq!(
                s["P2"]["leader"]["life"],
                if crimson { 19_u8 } else { 20_u8 },
                "the life-loss setup damages both leaders once; this Fanfare adds no leader damage"
            );
        }
    }

    #[test]
    fn bp04_081_real_evolution_heals_only_in_crimson_and_has_storm() {
        for crimson in [false, true] {
            let mut initial = evolution("BP04-080", "BP04-081");
            initial["players"]["P1"]["zones"]["field"][0]["state"]["entered_this_turn"] =
                json!(true);
            initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"payer","card":"BP01-118"}]);
            let mut game = game(&initial);
            if crimson {
                pay_life(&mut game);
            }
            evolve(&mut game, 3);
            trigger(&mut game);
            assert_eq!(
                game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
                if crimson { 24_u8 } else { 20_u8 },
                "the five-life evolution heal requires actual life loss this turn"
            );
            assert_eq!(
                game.internal_rule_action(
                    &json!({"do":"attack","attacker":"source","target":"P2.leader"})
                )
                .unwrap()
                .outcome,
                "resolved",
                "printed Storm permits the evolved source to attack the leader on entry"
            );
        }
    }

    fn commander_damage(card: &str) {
        let mut initial = position(card);
        initial["players"]["P1"]["zones"]["field"] =
            json!([{"id":"commander","card":"BP04-030"},{"id":"decoy","card":"BP01-171"}]);
        initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"},{"id":"second","card":"BP01-037"},{"id":"amulet","card":"BP19-108"}]);
        let mut game = game(&initial);
        let CardPlay::Awaiting(q) = game.play_internal_card("source", actor()).unwrap() else {
            panic!("formal Commander target")
        };
        let InputDomain::Targets(d) = &q.domain else {
            panic!("Commander target domain")
        };
        assert_eq!(
            d.members.len(),
            1,
            "only the allied Commander is a legal power source"
        );
        let step = game
            .answer_internal_card(q.actor, q.token, Answer::Targets(vec![d.members[0].handle]))
            .unwrap();
        finish(&mut game, step);
        finish_checks(&mut game);
        let s = game.projection(View::Referee).unwrap();
        for id in ["enemy", "second"] {
            assert_eq!(
                s["objects"][id]["damage"], 3_u8,
                "the selected Commander's power damages every opposing follower"
            );
        }
        assert_eq!(
            s["objects"]["commander"]["damage"], 0_u8,
            "the allied Commander receives no sweep damage"
        );
        assert_eq!(
            s["P1"]["field"],
            json!(["commander", "decoy"]),
            "allied followers remain on the field"
        );
        assert_eq!(
            s["P2"]["field"],
            json!(["enemy", "second", "amulet"]),
            "the opposing amulet is not a follower recipient"
        );
    }

    #[test]
    fn bp04_026_selected_commander_supplies_the_enemy_follower_sweep() {
        commander_damage("BP04-026");
    }
    #[test]
    fn bp04_027_selected_commander_supplies_the_enemy_follower_sweep() {
        commander_damage("BP04-027");
    }

    #[test]
    fn bp04_019_combo_follows_a_lethally_damaged_targets_leader() {
        for prior in [0_usize, 2] {
            let mut initial = position("BP04-019");
            for n in 0..prior {
                initial["players"]["P1"]["zones"]["hand"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!({"id":format!("prior{n}"),"card":"BP01-171"}));
            }
            let mut game = game(&initial);
            for n in 0..prior {
                play(&mut game, &format!("prior{n}"));
                finish_checks(&mut game);
            }
            play(&mut game, "source");
            finish_checks(&mut game);
            let s = game.projection(View::Referee).unwrap();
            assert_eq!(
                s["P2"]["cemetery"],
                json!(["enemy"]),
                "five damage kills the selected follower in either case"
            );
            assert_eq!(
                s["P2"]["leader"]["life"],
                if prior == 2 { 17_u8 } else { 20_u8 },
                "Combo three adds leader damage using the selected follower's controller"
            );
        }
    }

    #[test]
    fn bp04_033_attack_damages_its_selected_follower_and_that_leader() {
        let mut game = game(&field("BP04-033"));
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"attack","attacker":"source","target":"P2.leader"})
            )
            .unwrap()
            .outcome,
            "resolved",
            "the standing follower declares a leader attack"
        );
        trigger(&mut game);
        let before = game.projection(View::Referee).unwrap();
        assert_eq!(
            before["objects"]["enemy"]["damage"], 1_u8,
            "the attack-time ability damages the selected follower"
        );
        assert_eq!(
            before["P2"]["leader"]["life"], 19_u8,
            "the attack-time ability also damages that follower's leader"
        );
        quick_pass(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
            17_u8,
            "ordinary attack damage follows the triggered damage"
        );
    }

    #[test]
    fn bp04_099_damages_other_followers_then_buffs_only_surviving_allies() {
        let mut initial = position("BP04-099");
        initial["players"]["P1"]["zones"]["field"] =
            json!([{"id":"ally","card":"BP01-037"},{"id":"amulet","card":"BP19-108"}]);
        let mut game = game(&initial);
        play(&mut game, "source");
        trigger(&mut game);
        let s = game.projection(View::Referee).unwrap();
        assert_eq!(
            s["objects"]["source"]["damage"], 0_u8,
            "other excludes the Fanfare source from damage"
        );
        assert_eq!(
            s["objects"]["source"]["power"], 5_u8,
            "other excludes the source from the power bonus"
        );
        assert_eq!(
            s["objects"]["ally"]["damage"], 2_u8,
            "the allied follower takes the symmetric sweep"
        );
        assert_eq!(
            s["objects"]["ally"]["power"], 7_u8,
            "the surviving allied follower gains two power"
        );
        assert_eq!(
            s["objects"]["enemy"]["damage"], 2_u8,
            "the opposing follower takes two damage"
        );
        assert_eq!(
            s["objects"]["enemy"]["power"], 5_u8,
            "the opponent receives no power bonus"
        );
    }

    #[test]
    fn bp04_012_fanfare_and_lastword_share_the_printed_line_but_fire_separately() {
        let mut initial = position("BP04-012");
        initial["players"]["P1"]["zones"]["hand"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"removal","card":"BP01-116"}));
        let mut game = game(&initial);
        play(&mut game, "source");
        trigger(&mut game);
        let CardPlay::Awaiting(q) = game.play_internal_card("removal", actor()).unwrap() else {
            panic!("Soul Conversion target")
        };
        let InputDomain::Targets(d) = &q.domain else {
            panic!("own follower target")
        };
        let step = game
            .answer_internal_card(q.actor, q.token, Answer::Targets(vec![d.members[0].handle]))
            .unwrap();
        finish(&mut game, step);
        finish_checks(&mut game);
        trigger(&mut game);
        let s = game.projection(View::Referee).unwrap();
        assert_eq!(
            s["objects"]["enemy"]["damage"], 2_u8,
            "Fanfare and Lastword each deal one to opposing followers"
        );
        assert_eq!(
            s["P2"]["leader"]["life"], 18_u8,
            "the two printed triggers each damage the opposing leader"
        );
        assert_eq!(
            s["P1"]["leader"]["life"], 20_u8,
            "neither trigger damages its controller"
        );
    }

    #[test]
    fn bp04_120_evolves_normally_then_pays_four_for_its_enemy_only_sweep() {
        let mut game = game(&evolution("BP04-119", "BP04-120"));
        evolve(&mut game, 2);
        trigger(&mut game);
        let s = game.projection(View::Referee).unwrap();
        assert_eq!(
            s["P1"]["pp"]["current"], 4_u8,
            "normal evolution and the paid trigger cost two plus four PP"
        );
        assert_eq!(
            s["P2"]["leader"]["life"], 16_u8,
            "the paid trigger damages opposing leaders by four"
        );
        assert_eq!(
            s["objects"]["enemy"]["damage"], 4_u8,
            "the paid trigger also damages opposing followers"
        );
        assert_eq!(
            s["P1"]["leader"]["life"], 20_u8,
            "the controller leader is excluded"
        );
    }

    #[test]
    fn bp04_030_juliet_discount_is_not_multiplied_and_end_heal_requires_juliet() {
        for count in [0_usize, 1, 2] {
            let mut initial = position("BP04-030");
            initial["players"]["P1"]["zones"]["field"] = json!([]);
            for n in 0..count {
                initial["players"]["P1"]["zones"]["field"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!({"id":format!("juliet{n}"),"card":"BP04-032"}));
            }
            let mut game = game(&initial);
            play(&mut game, "source");
            finish_checks(&mut game);
            assert_eq!(
                game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
                if count == 0 { 7_u8 } else { 9_u8 },
                "one or more Juliets reduce the play cost by exactly two"
            );
            assert_eq!(
                game.internal_rule_action(&json!({"do":"end-phase"}))
                    .unwrap()
                    .outcome,
                "resolved",
                "the controller enters its real End phase"
            );
            let q = game.internal_pending_choice().unwrap().unwrap();
            let InputDomain::Pending(options) = &q.domain else {
                panic!("pending End abilities")
            };
            let option = options.iter().find(|o| o.card == "BP04-030").unwrap();
            let step = game
                .answer_internal_card(q.actor, q.token, Answer::Pending(option.handle))
                .unwrap();
            finish(&mut game, step);
            finish_checks(&mut game);
            assert_eq!(
                game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
                if count == 0 { 20_u8 } else { 22_u8 },
                "End healing requires Juliet and does not multiply with her count"
            );
        }
    }

    #[test]
    fn bp04_051_fanfare_requires_an_actual_spell_in_own_cemetery() {
        for buried in ["BP01-171", "BP01-007"] {
            let mut initial = position("BP04-051");
            initial["players"]["P1"]["zones"]["cemetery"] = json!([{"id":"buried","card":buried}]);
            let mut game = game(&initial);
            play(&mut game, "source");
            trigger(&mut game);
            assert_eq!(
                game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
                if buried == "BP01-007" { 22_u8 } else { 20_u8 },
                "only an actual spell in the controller cemetery enables the heal"
            );
        }
    }
}

mod second {
    #![expect(
        clippy::panic,
        reason = "Unexpected decisions invalidate the printed card scenarios."
    )]
    use super::*;
    use sve_engine::game::internal::SearchAnswer;

    const fn actor() -> PlayerId {
        PlayerId { seat: 1 }
    }

    fn position(card: &str) -> Value {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
        for seat in ["P1", "P2"] {
            initial["players"][seat]["zones"]["deck"] = json!([{"id":format!("{seat}-first"),"card":"BP01-171"},{"id":format!("{seat}-second"),"card":"BP01-171"}]);
        }
        initial["players"]["P2"]["zones"]["field"] =
            json!([{"id":"enemy","card":"BP01-037","state":{"acted":true}}]);
        initial
    }

    fn field(card: &str) -> Value {
        let mut initial = position(card);
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":card}]);
        initial
    }

    fn finish_checks(game: &mut Game) {
        while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
    }

    fn finish(game: &mut Game, mut step: CardPlay) {
        while let CardPlay::Awaiting(q) = step {
            let answer = if matches!(&q.domain, InputDomain::WardEntry { .. }) {
                Answer::WardEntry(false)
            } else if matches!(&q.domain, InputDomain::OptionalCost) {
                Answer::OptionalCost(true)
            } else if let InputDomain::Targets(d) = &q.domain {
                Answer::Targets(
                    d.members
                        .iter()
                        .take(usize::try_from(d.cardinality.maximum.unwrap().0).unwrap())
                        .map(|m| m.handle)
                        .collect(),
                )
            } else if let InputDomain::CostMaterials(d) = &q.domain {
                let mut members: Vec<_> = d.members.iter().collect();
                members.sort_by_key(|member| {
                    !matches!(member.card.as_deref(), Some("BP01-007" | "BP01-015"))
                });
                Answer::CostMaterials(
                    members
                        .into_iter()
                        .take(usize::try_from(d.cardinality.maximum.unwrap().0).unwrap())
                        .map(|member| member.handle)
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
            } else if let InputDomain::Order(cards) = &q.domain {
                Answer::Order(cards.iter().rev().map(|m| m.handle).collect())
            } else {
                panic!("unexpected decision")
            };
            step = game.answer_internal_card(q.actor, q.token, answer).unwrap();
        }
    }

    fn play(game: &mut Game, id: &str) {
        let step = game.play_internal_card(id, actor()).unwrap();
        finish(game, step);
    }

    fn trigger(game: &mut Game) {
        let q = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = &q.domain else {
            panic!("pending printed ability")
        };
        assert_eq!(
            options.len(),
            1,
            "only the intended printed ability is pending"
        );
        let step = game
            .answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
            .unwrap();
        finish(game, step);
        finish_checks(game);
    }

    fn evolve(game: &mut Game, cost: u8) {
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":cost}})
            )
            .unwrap()
            .outcome,
            "resolved",
            "the real base evolves for its printed PP cost"
        );
    }

    fn quick_pass(game: &mut Game) {
        let q = game.internal_quick_choice().unwrap().unwrap();
        let step = game
            .answer_internal_card(q.actor, q.token, Answer::Quick(None))
            .unwrap();
        finish(game, step);
    }

    fn evolution(base: &str, form: &str) -> Value {
        let mut initial = field(base);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
        initial
    }

    fn spirit_miko(card: &str) {
        for max in [6_u8, 7] {
            let mut initial = position(card);
            initial["players"]["P1"]["pp"] = json!({"current":max,"max":max});
            let mut game = game(&initial);
            play(&mut game, "source");
            trigger(&mut game);
            let s = game.projection(View::Referee).unwrap();
            assert_eq!(
                s["objects"]["source"]["power"],
                if max == 6 { 3_u8 } else { 4_u8 },
                "only Awakening grants the Fanfare power bonus"
            );
            assert_eq!(
                s["objects"]["source"]["hp"],
                if max == 6 { 4_u8 } else { 5_u8 },
                "only Awakening grants the Fanfare health bonus"
            );
            assert_eq!(
                s["P1"]["pp"]["max"],
                if max == 6 { 6_u8 } else { 8_u8 },
                "Q592 also gates the PP maximum increase behind Awakening"
            );
            assert_eq!(
                game.internal_rule_action(&json!({"do":"end-phase"}))
                    .unwrap()
                    .outcome,
                "resolved",
                "the controller enters its real End phase"
            );
            trigger(&mut game);
            assert_eq!(
                game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
                if max == 6 { 21_u8 } else { 22_u8 },
                "Awakening replaces the End heal of one with two"
            );
        }
    }

    #[test]
    fn bp04_057_awakening_gates_both_fanfare_bonuses_and_replaces_end_healing() {
        spirit_miko("BP04-057");
    }
    #[test]
    fn bp04_058_awakening_gates_both_fanfare_bonuses_and_replaces_end_healing() {
        spirit_miko("BP04-058");
    }

    #[test]
    fn bp04_077_damage_trigger_buffs_a_survivor_but_does_not_save_a_lethal_victim() {
        for lethal in [false, true] {
            let mut initial = field("BP04-077");
            initial["players"]["P1"]["zones"]["field"][0]["state"] =
                json!({"entered_this_turn":true,"damage":if lethal {6_u8}else{0_u8}});
            initial["players"]["P2"]["zones"]["field"] =
                json!([{"id":"enemy","card":"BP01-171","state":{"acted":true}}]);
            let mut game = game(&initial);
            assert_eq!(
                game.internal_rule_action(
                    &json!({"do":"attack","attacker":"source","target":"enemy"})
                )
                .unwrap()
                .outcome,
                "resolved",
                "printed Rush permits the entry-turn follower attack"
            );
            quick_pass(&mut game);
            trigger(&mut game);
            let s = game.projection(View::Referee).unwrap();
            if lethal {
                assert_eq!(
                    s["P1"]["cemetery"],
                    json!(["source"]),
                    "Q609 destroys the lethal victim before its delayed buff can help"
                );
            } else {
                assert_eq!(
                    s["objects"]["source"]["power"], 8_u8,
                    "the surviving damaged source gains two power"
                );
                assert_eq!(
                    s["objects"]["source"]["hp"], 7_u8,
                    "the survivor has five remaining health before the two-health bonus"
                );
                assert_eq!(
                    s["objects"]["source"]["damage"], 2_u8,
                    "the health bonus does not erase battle damage"
                );
            }
        }
    }

    #[test]
    fn bp04_025_fanfare_counts_itself_and_excludes_amulets_from_the_four_followers() {
        for all_followers in [false, true] {
            let mut initial = position("BP04-025");
            initial["players"]["P1"]["zones"]["field"] = json!([{"id":"a","card":"BP01-171"},{"id":"b","card":"BP01-171"},{"id":"c","card":if all_followers {"BP01-171"}else{"BP19-108"}}]);
            let mut game = game(&initial);
            play(&mut game, "source");
            trigger(&mut game);
            let s = game.projection(View::Referee).unwrap();
            assert_eq!(
                s["objects"]["source"]["power"],
                if all_followers { 3_u8 } else { 2_u8 },
                "Q577 includes the Fanfare source but excludes an amulet"
            );
            assert_eq!(
                s["objects"]["source"]["hp"],
                if all_followers { 3_u8 } else { 2_u8 },
                "the four-follower threshold grants exactly one health"
            );
        }
    }

    #[test]
    fn bp04_018_combo_three_buffs_the_follower_and_grants_entry_turn_storm() {
        for prior in [0_usize, 2] {
            let mut initial = position("BP04-018");
            for n in 0..prior {
                initial["players"]["P1"]["zones"]["hand"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!({"id":format!("prior{n}"),"card":"BP01-171"}));
            }
            let mut game = game(&initial);
            for n in 0..prior {
                play(&mut game, &format!("prior{n}"));
                finish_checks(&mut game);
            }
            play(&mut game, "source");
            trigger(&mut game);
            let s = game.projection(View::Referee).unwrap();
            assert_eq!(
                s["objects"]["source"]["power"],
                if prior == 2 { 4_u8 } else { 3_u8 },
                "the third card play enables the printed power bonus"
            );
            assert_eq!(
                s["objects"]["source"]["hp"],
                if prior == 2 { 5_u8 } else { 4_u8 },
                "the third card play enables the printed health bonus"
            );
            assert_eq!(
                game.internal_rule_action(
                    &json!({"do":"attack","attacker":"source","target":"P2.leader"})
                )
                .unwrap()
                .outcome,
                if prior == 2 {
                    "resolved"
                } else {
                    "cannot-attack"
                },
                "only Combo three grants Storm on the entry turn"
            );
        }
    }

    #[test]
    fn bp04_037_buffs_an_allied_follower_and_aura_blocks_a_real_enemy_spell() {
        let mut initial = position("BP04-037");
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"ally","card":"BP01-171"}]);
        initial["players"]["P2"]["zones"]["hand"] = json!([{"id":"removal","card":"BP02-120"}]);
        let mut game = game(&initial);
        play(&mut game, "source");
        finish_checks(&mut game);
        let s = game.projection(View::Referee).unwrap();
        assert_eq!(
            s["objects"]["ally"]["power"], 3_u8,
            "the selected follower gains one power"
        );
        assert_eq!(
            s["objects"]["ally"]["hp"], 4_u8,
            "the selected follower gains two health"
        );
        assert_eq!(
            game.internal_rule_action(&json!({"do":"end-phase"}))
                .unwrap()
                .outcome,
            "resolved",
            "ending the turn gives the opponent a real play window"
        );
        quick_pass(&mut game);
        let before = game.projection(View::Referee).unwrap();
        assert!(
            game.play_internal_card("removal", PlayerId { seat: 2 })
                .is_err(),
            "Aura leaves the opposing targeted spell without a legal follower"
        );
        assert_eq!(
            game.projection(View::Referee).unwrap(),
            before,
            "the rejected opposing play does not charge PP or move cards"
        );
    }

    fn remove_source(game: &mut Game) {
        let CardPlay::Awaiting(q) = game.play_internal_card("removal", actor()).unwrap() else {
            panic!("Soul Conversion target")
        };
        let InputDomain::Targets(d) = &q.domain else {
            panic!("one allied follower target")
        };
        assert_eq!(
            d.members.len(),
            1,
            "the scenario exposes only its intended source to Soul Conversion"
        );
        let step = game
            .answer_internal_card(q.actor, q.token, Answer::Targets(vec![d.members[0].handle]))
            .unwrap();
        finish(game, step);
        finish_checks(game);
    }

    #[test]
    fn bp04_129_fanfare_allows_negative_power_then_lastword_debuffs_after_source_departure() {
        let mut initial = position("BP04-129");
        initial["players"]["P1"]["zones"]["hand"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"removal","card":"BP01-116"}));
        initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-157"}]);
        let mut game = game(&initial);
        play(&mut game, "source");
        trigger(&mut game);
        let before = game.projection(View::Referee).unwrap();
        assert_eq!(
            before["objects"]["enemy"]["power"], -2_i8,
            "Q639 permits power below zero after the three-point debuff"
        );
        assert_eq!(
            before["objects"]["enemy"]["hp"], 2_u8,
            "the first debuff leaves the five-health target alive"
        );
        remove_source(&mut game);
        trigger(&mut game);
        let s = game.projection(View::Referee).unwrap();
        assert!(
            s["P1"]["cemetery"]
                .as_array()
                .unwrap()
                .contains(&json!("source")),
            "the actual source leaves before its Lastword resolves"
        );
        assert_eq!(
            s["P2"]["cemetery"],
            json!(["enemy"]),
            "the second printed debuff kills the remaining target"
        );
    }

    #[test]
    fn bp04_023_enemy_field_count_includes_amulets_and_printed_assail_targets_standing_followers() {
        for count in [2_usize, 3] {
            let mut initial = position("BP04-023");
            initial["players"]["P2"]["zones"]["field"] =
                json!([{"id":"enemy","card":"BP01-171"},{"id":"second","card":"BP01-171"}]);
            if count == 3 {
                initial["players"]["P2"]["zones"]["field"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!({"id":"amulet","card":"BP19-108"}));
            }
            let mut game = game(&initial);
            play(&mut game, "source");
            finish_checks(&mut game);
            assert_eq!(
                game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
                if count == 3 { 7_u8 } else { 5_u8 },
                "an opposing amulet can reach the three-card discount threshold"
            );
        }
        let mut initial = field("BP04-023");
        initial["players"]["P2"]["zones"]["field"][0]["state"]["acted"] = json!(false);
        let mut game = game(&initial);
        assert_eq!(
            game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"enemy"}))
                .unwrap()
                .outcome,
            "resolved",
            "printed Assail permits a standing opposing follower target"
        );
    }

    #[test]
    fn bp04_028_bane_kills_in_real_battle_and_its_normal_head_evolves_for_two() {
        let mut base = game(&field("BP04-028"));
        assert_eq!(
            base.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"enemy"}))
                .unwrap()
                .outcome,
            "resolved",
            "the unevolved Bane follower attacks a rested target"
        );
        quick_pass(&mut base);
        assert_eq!(
            base.projection(View::Referee).unwrap()["P2"]["cemetery"],
            json!(["enemy"]),
            "the base card's one power still kills the five-health follower through Bane"
        );
        let mut evolved = game(&evolution("BP04-028", "BP04-029"));
        evolve(&mut evolved, 2);
        assert_eq!(
            evolved.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
            8_u8,
            "the real normal evolution head charges two PP"
        );
    }

    #[test]
    fn bp04_055_fanfare_and_lastword_each_discard_a_spell_before_drawing() {
        let mut initial = position("BP04-055");
        initial["players"]["P1"]["zones"]["hand"]
            .as_array_mut()
            .unwrap()
            .extend([
                json!({"id":"spell-a","card":"BP01-007"}),
                json!({"id":"spell-b","card":"BP01-015"}),
                json!({"id":"removal","card":"BP01-116"}),
            ]);
        initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"d1","card":"BP01-171"},{"id":"d2","card":"BP01-171"},{"id":"d3","card":"BP01-171"},{"id":"d4","card":"BP01-171"},{"id":"tail","card":"BP01-171"}]);
        let mut game = game(&initial);
        play(&mut game, "source");
        trigger(&mut game);
        remove_source(&mut game);
        trigger(&mut game);
        let s = game.projection(View::Referee).unwrap();
        assert_eq!(
            s["P1"]["deck"],
            json!(["tail"]),
            "Fanfare, Soul Conversion and Lastword draw one plus two plus one cards"
        );
        let grave = s["P1"]["cemetery"].as_array().unwrap();
        assert!(
            grave.contains(&json!("spell-a")) && grave.contains(&json!("spell-b")),
            "each printed trigger pays a real spell from hand before its draw"
        );
    }

    #[test]
    fn bp04_110_pays_two_allied_amulets_then_discards_one_of_the_drawn_cards() {
        let mut initial = position("BP04-110");
        initial["players"]["P1"]["zones"]["field"] =
            json!([{"id":"a","card":"BP19-108"},{"id":"b","card":"BP19-108"}]);
        initial["players"]["P2"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"opposing-amulet","card":"BP19-108"}));
        initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"d1","card":"BP01-171"},{"id":"d2","card":"BP01-171"},{"id":"tail","card":"BP01-171"}]);
        let mut game = game(&initial);
        play(&mut game, "source");
        trigger(&mut game);
        let s = game.projection(View::Referee).unwrap();
        for id in ["a", "b"] {
            assert_eq!(
                s["objects"][id]["acted"], true,
                "both allied amulets pay the printed head cost"
            );
        }
        assert_eq!(
            s["objects"]["opposing-amulet"]["acted"], false,
            "an opponent amulet cannot pay this cost"
        );
        assert_eq!(
            s["P1"]["deck"],
            json!(["tail"]),
            "the effect draws exactly two cards before discarding"
        );
        assert_eq!(
            s["P1"]["hand"].as_array().unwrap().len(),
            1,
            "one of the two newly drawn cards remains in hand"
        );
        let grave = s["P1"]["cemetery"].as_array().unwrap();
        assert_eq!(
            grave.len(),
            1,
            "the resolution discard puts exactly one drawn card in the cemetery"
        );
        assert!(
            grave.contains(&json!("d1")) || grave.contains(&json!("d2")),
            "the discarded card was drawn by this Fanfare"
        );
    }

    #[test]
    fn bp04_067_declares_surcharge_before_targets_then_pays_or_restores_without_charging() {
        for (accepted, pp, quick) in [
            (false, 10_u8, false),
            (true, 10, false),
            (true, 9, false),
            (true, 10, true),
        ] {
            let mut initial = position("BP04-067");
            initial["players"]["P1"]["pp"] = json!({"current":pp,"max":10_u8});
            initial["players"]["P2"]["zones"]["field"] =
                json!([{"id":"enemy1","card":"BP01-037"},{"id":"enemy2","card":"BP01-037"}]);
            if quick {
                initial["turn"]["active"] = json!("P2");
            }
            let mut game = game(&initial);
            let original = game.projection(View::Referee).unwrap();
            let started = if quick {
                assert_eq!(
                    game.internal_rule_action(&json!({"do":"end-phase"}))
                        .unwrap()
                        .outcome,
                    "resolved",
                    "the opponent opens an actual End Quick window"
                );
                let q = game.internal_quick_choice().unwrap().unwrap();
                let InputDomain::Quick { options, .. } = &q.domain else {
                    panic!("Quick spell options")
                };
                let option = options.iter().find(|o| o.card == "BP04-067").unwrap();
                game.answer_internal_card(q.actor, q.token, Answer::Quick(Some(option.handle)))
                    .unwrap()
            } else {
                game.play_internal_card("source", actor()).unwrap()
            };
            let CardPlay::Awaiting(declare) = started else {
                panic!("optional surcharge declaration")
            };
            assert!(
                matches!(&declare.domain, InputDomain::OptionalCost),
                "the surcharge is declared before formal targets"
            );
            let CardPlay::Awaiting(target) = game
                .answer_internal_card(declare.actor, declare.token, Answer::OptionalCost(accepted))
                .unwrap()
            else {
                panic!("mandatory formal follower target")
            };
            let InputDomain::Targets(d) = &target.domain else {
                panic!("formal follower target domain")
            };
            assert_eq!(
                game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
                pp,
                "declaring the surcharge and opening targets do not pay PP yet"
            );
            let result = game.answer_internal_card(
                target.actor,
                target.token,
                Answer::Targets(vec![d.members[0].handle]),
            );
            if accepted && pp < 10 {
                assert!(result.is_err(), "the accepted surcharge needs ten total PP");
                assert_eq!(
                    game.projection(View::Referee).unwrap(),
                    original,
                    "insufficient surcharge restores the original hand, PP and authority state"
                );
                continue;
            }
            finish(&mut game, result.unwrap());
            finish_checks(&mut game);
            let s = game.projection(View::Referee).unwrap();
            assert_eq!(
                s["P1"]["pp"]["current"],
                if accepted { 0_u8 } else { 5_u8 },
                "the declaration selects the printed five or ten PP total cost"
            );
            assert_eq!(
                s["P2"]["field"],
                if accepted {
                    json!([])
                } else {
                    json!(["enemy2"])
                },
                "paying the surcharge replaces the single banish with a follower sweep"
            );
            assert_eq!(
                s["P2"]["banish"],
                if accepted {
                    json!(["enemy1", "enemy2"])
                } else {
                    json!(["enemy1"])
                },
                "the surcharge branch banishes every opposing follower after still selecting one formal target"
            );
        }
    }

    fn named(state: &Value, zone: &str, name: &str) -> usize {
        state["P1"][zone]
            .as_array()
            .unwrap()
            .iter()
            .filter(|id| state["objects"][id.as_str().unwrap()]["name"] == name)
            .count()
    }

    fn search_amulet_fanfare(game: &mut Game) {
        let q = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = &q.domain else {
            panic!("pending search Fanfare")
        };
        assert_eq!(
            options.len(),
            1,
            "only the printed search Fanfare is pending"
        );
        let CardPlay::Awaiting(search) = game
            .answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
            .unwrap()
        else {
            panic!("amulet search decision")
        };
        let InputDomain::Search(d) = &search.domain else {
            panic!("amulet search domain")
        };
        assert_eq!(
            d.cards.len(),
            1,
            "the search excludes the ordinary followers in the same deck"
        );
        let step = game
            .answer_internal_card(
                search.actor,
                search.token,
                Answer::Search(SearchAnswer::Ungrouped(vec![d.cards[0].handle])),
            )
            .unwrap();
        finish(game, step);
        finish_checks(game);
    }

    fn amulet_position(card: &str) -> Value {
        let mut initial = position(card);
        initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"wrong","card":"BP01-171"},{"id":"found","card":"BP19-108"},{"id":"tail","card":"BP01-171"}]);
        initial
    }

    #[test]
    fn bp04_014_fanfare_searches_only_an_amulet_from_a_mixed_deck() {
        let mut game = game(&amulet_position("BP04-014"));
        play(&mut game, "source");
        search_amulet_fanfare(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["hand"],
            json!(["found"]),
            "the printed Fanfare puts the only amulet into hand"
        );
    }

    #[test]
    fn bp04_031_modes_search_or_buff_only_an_allied_round_table_follower() {
        for mode in [1_u64, 2] {
            let mut initial = position("BP04-031");
            initial["players"]["P1"]["zones"]["field"] =
                json!([{"id":"wrong-ally","card":"BP01-171"},{"id":"ally","card":"BP04-035"}]);
            initial["players"]["P1"]["zones"]["deck"] =
                json!([{"id":"wrong","card":"BP01-171"},{"id":"found","card":"BP04-035"}]);
            let mut game = game(&initial);
            let CardPlay::Awaiting(q) = game.play_internal_card("source", actor()).unwrap() else {
                panic!("Round Table modes")
            };
            assert!(
                matches!(&q.domain, InputDomain::Modes { .. }),
                "the mode is declared before its own targets"
            );
            let CardPlay::Awaiting(choice) = game
                .answer_internal_card(q.actor, q.token, Answer::Modes(vec![ModeId(mode)]))
                .unwrap()
            else {
                panic!("mode-specific decision")
            };
            let answer = if mode == 1 {
                let InputDomain::Search(d) = &choice.domain else {
                    panic!("Round Table search")
                };
                assert_eq!(
                    d.cards.len(),
                    1,
                    "the search excludes non-Round-Table followers"
                );
                Answer::Search(SearchAnswer::Ungrouped(vec![d.cards[0].handle]))
            } else {
                let InputDomain::Targets(d) = &choice.domain else {
                    panic!("Round Table target")
                };
                assert_eq!(
                    d.members.len(),
                    1,
                    "only the allied Round Table follower is a legal buff target"
                );
                Answer::Targets(vec![d.members[0].handle])
            };
            let step = game
                .answer_internal_card(choice.actor, choice.token, answer)
                .unwrap();
            finish(&mut game, step);
            finish_checks(&mut game);
            let s = game.projection(View::Referee).unwrap();
            assert_eq!(
                s["P1"]["hand"],
                if mode == 1 {
                    json!(["found"])
                } else {
                    json!([])
                },
                "only the first mode searches a card into hand"
            );
            assert_eq!(
                s["objects"]["ally"]["power"],
                if mode == 2 { 3_u8 } else { 2_u8 },
                "only the second mode adds one power"
            );
            assert_eq!(
                s["objects"]["ally"]["hp"],
                if mode == 2 { 4_u8 } else { 2_u8 },
                "only the second mode adds two health"
            );
            assert_eq!(
                s["objects"]["wrong-ally"]["power"], 2_u8,
                "the non-Round-Table ally is not buffed"
            );
        }
    }

    #[test]
    fn bp04_074_awakened_fanfare_grants_storm_only_to_the_new_orca() {
        for max in [6_u8, 7] {
            let mut initial = position("BP04-074");
            initial["players"]["P1"]["pp"] = json!({"current":max,"max":max});
            initial["players"]["P1"]["zones"]["field"] =
                json!([{"id":"old","card":"BP02-T05","state":{"entered_this_turn":true}}]);
            let mut game = game(&initial);
            play(&mut game, "source");
            trigger(&mut game);
            let s = game.projection(View::Referee).unwrap();
            assert_eq!(
                named(&s, "field", "オルカ"),
                2,
                "the Fanfare creates one additional real Orca"
            );
            let made = s["P1"]["field"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(Value::as_str)
                .find(|id| *id != "old" && *id != "source")
                .unwrap()
                .to_owned();
            assert_eq!(
                game.internal_rule_action(
                    &json!({"do":"attack","attacker":"old","target":"P2.leader"})
                )
                .unwrap()
                .outcome,
                "cannot-attack",
                "the pre-existing Orca does not gain Storm"
            );
            assert_eq!(
                game.internal_rule_action(
                    &json!({"do":"attack","attacker":made,"target":"P2.leader"})
                )
                .unwrap()
                .outcome,
                if max == 7 {
                    "resolved"
                } else {
                    "cannot-attack"
                },
                "only Awakening grants Storm to the created Orca"
            );
        }
    }

    #[test]
    fn bp04_097_without_a_gorgon_creates_a_viper_that_uses_zero_power_bane_next_turn() {
        let mut game = game(&position("BP04-097"));
        play(&mut game, "source");
        finish_checks(&mut game);
        let created = game.projection(View::Referee).unwrap();
        assert_eq!(
            named(&created, "field", "毒蛇"),
            1,
            "the spell creates exactly one real Viper without a Gorgon"
        );
        assert_eq!(
            created["P1"]["pp"]["current"], 9_u8,
            "the ordinary spell play pays its printed one PP"
        );
        let made = created["P1"]["field"][0].as_str().unwrap().to_owned();
        assert_eq!(
            game.internal_rule_action(&json!({"do":"attack","attacker":made,"target":"enemy"}))
                .unwrap()
                .outcome,
            "cannot-attack",
            "without a Gorgon the new Viper does not gain entry-turn Rush"
        );
        assert_eq!(
            game.internal_rule_action(&json!({"do":"end-phase"}))
                .unwrap()
                .outcome,
            "resolved",
            "the Viper's controller ends the creation turn"
        );
        quick_pass(&mut game);
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"attack","attacker":"enemy","target":"P1.leader"})
            )
            .unwrap()
            .outcome,
            "resolved",
            "the opponent rests its follower through a real leader attack"
        );
        quick_pass(&mut game);
        assert_eq!(
            game.internal_rule_action(&json!({"do":"end-phase"}))
                .unwrap()
                .outcome,
            "resolved",
            "the opponent ends its turn normally"
        );
        quick_pass(&mut game);
        assert_eq!(
            game.internal_rule_action(&json!({"do":"attack","attacker":made,"target":"enemy"}))
                .unwrap()
                .outcome,
            "resolved",
            "on its next turn the Viper can attack the rested opposing follower"
        );
        quick_pass(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P2"]["cemetery"],
            json!(["enemy"]),
            "Q622 Bane destroys the battle opponent despite the Viper's zero power"
        );
    }

    #[test]
    fn bp04_011_real_evolution_does_not_count_as_a_play_and_combo_five_replaces_three() {
        for (prior, power) in [(1_usize, 2_u8), (2, 3), (4, 5)] {
            let mut initial = position("BP04-010");
            initial["players"]["P1"]["zones"]["evolve_deck"] =
                json!([{"id":"form","card":"BP04-011"}]);
            for n in 0..prior {
                initial["players"]["P1"]["zones"]["hand"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!({"id":format!("prior{n}"),"card":"BP01-171"}));
            }
            let mut game = game(&initial);
            play(&mut game, "source");
            finish_checks(&mut game);
            for n in 0..prior {
                play(&mut game, &format!("prior{n}"));
                finish_checks(&mut game);
            }
            evolve(&mut game, 1);
            trigger(&mut game);
            let s = game.projection(View::Referee).unwrap();
            assert_eq!(
                s["objects"]["source"]["power"], power,
                "Q563 evolution is not a card play; Combo five replaces the one-power bonus with three"
            );
            assert_eq!(
                s["objects"]["source"]["hp"], power,
                "the evolution trigger grants the matching health bonus"
            );
            assert_eq!(
                game.internal_rule_action(
                    &json!({"do":"attack","attacker":"source","target":"P2.leader"})
                )
                .unwrap()
                .outcome,
                "resolved",
                "printed Storm permits the evolved follower's entry-turn leader attack even below Combo three"
            );
        }
    }
}

mod cemetery {
    #![expect(
        clippy::panic,
        reason = "Unexpected decisions invalidate the printed card scenarios."
    )]
    use super::*;
    use sve_engine::game::internal::SearchAnswer;

    const fn actor() -> PlayerId {
        PlayerId { seat: 1 }
    }

    fn position(card: &str) -> Value {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
        for seat in ["P1", "P2"] {
            initial["players"][seat]["zones"]["deck"] = json!([{"id":format!("{seat}-first"),"card":"BP01-171"},{"id":format!("{seat}-second"),"card":"BP01-171"}]);
        }
        initial["players"]["P2"]["zones"]["field"] =
            json!([{"id":"enemy","card":"BP01-037","state":{"acted":true}}]);
        initial
    }

    fn finish_checks(game: &mut Game) {
        while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
    }

    fn finish(game: &mut Game, mut step: CardPlay) {
        while let CardPlay::Awaiting(q) = step {
            let answer = if matches!(&q.domain, InputDomain::WardEntry { .. }) {
                Answer::WardEntry(false)
            } else if matches!(&q.domain, InputDomain::OptionalCost) {
                Answer::OptionalCost(true)
            } else if let InputDomain::Targets(d) = &q.domain {
                Answer::Targets(
                    d.members
                        .iter()
                        .take(usize::try_from(d.cardinality.maximum.unwrap().0).unwrap())
                        .map(|m| m.handle)
                        .collect(),
                )
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
            } else if let InputDomain::Order(cards) = &q.domain {
                Answer::Order(cards.iter().rev().map(|m| m.handle).collect())
            } else {
                panic!("unexpected decision")
            };
            step = game.answer_internal_card(q.actor, q.token, answer).unwrap();
        }
    }

    fn play(game: &mut Game, id: &str) {
        let step = game.play_internal_card(id, actor()).unwrap();
        finish(game, step);
    }

    fn trigger(game: &mut Game) {
        let q = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = &q.domain else {
            panic!("pending printed ability")
        };
        assert_eq!(
            options.len(),
            1,
            "only the intended printed ability is pending"
        );
        let step = game
            .answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
            .unwrap();
        finish(game, step);
        finish_checks(game);
    }

    fn grave_amulet(card: &str) {
        let mut initial = position(card);
        initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"returned","card":"BP01-117"},{"id":"spell","card":"BP01-116"},{"id":"tail","card":"BP01-171"}]);
        initial["players"]["P1"]["zones"]["cemetery"] =
            json!([{"id":"wrong-trait","card":"BP01-171"}]);
        initial["players"]["P2"]["zones"]["cemetery"] = json!([{"id":"foreign","card":"BP01-117"}]);
        let mut game = game(&initial);
        play(&mut game, "source");
        trigger(&mut game);
        let ability = selection().current[card]
            .program
            .card()
            .abilities
            .iter()
            .find(|a| a.node.kind == AbilityKind::Activated)
            .unwrap()
            .node
            .id;
        let CardPlay::Awaiting(q) = game
            .activate_internal_card("source", ability, actor())
            .unwrap()
        else {
            panic!("formal cemetery target")
        };
        let InputDomain::Targets(d) = &q.domain else {
            panic!("own Dead follower")
        };
        assert_eq!(
            d.members.len(),
            1,
            "owner, card kind and Dead trait exclude every other cemetery card"
        );
        assert_eq!(
            d.members[0].card.as_deref(),
            Some("BP01-117"),
            "the declared target is the real self-milled Dead follower"
        );
        let before = game.projection(View::Referee).unwrap();
        assert_eq!(
            before["P1"]["field"],
            json!(["source"]),
            "the amulet stays on the field until its declared target is answered"
        );
        assert_eq!(
            before["P1"]["pp"]["current"], 9_u8,
            "declaration precedes payment"
        );
        let step = game
            .answer_internal_card(q.actor, q.token, Answer::Targets(vec![d.members[0].handle]))
            .unwrap();
        finish(&mut game, step);
        finish_checks(&mut game);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P1"]["hand"],
            json!(["returned"]),
            "the selected cemetery follower returns to hand"
        );
        assert_eq!(
            state["P1"]["cemetery"],
            json!(["wrong-trait", "spell", "source"]),
            "the returned follower leaves while the amulet pays its own burial cost"
        );
        assert_eq!(
            state["P1"]["pp"]["current"], 7_u8,
            "activation pays exactly two PP after the printed one PP play cost"
        );
        assert_eq!(
            state["P2"]["cemetery"],
            json!(["foreign"]),
            "the opponent's cemetery is unchanged"
        );
        assert_eq!(
            state["P1"]["deck"],
            json!(["tail"]),
            "the Fanfare mills exactly two cards and preserves the untouched tail"
        );
    }

    #[test]
    fn bp04_090_real_self_mill_provides_the_declared_dead_follower_before_sacrificing_the_amulet() {
        grave_amulet("BP04-090");
    }

    #[test]
    fn bp04_091_its_distinct_card_identity_has_the_same_formal_cemetery_payment_order() {
        grave_amulet("BP04-091");
    }
}
