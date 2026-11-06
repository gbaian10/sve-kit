#![expect(
    clippy::panic,
    reason = "Unexpected choices invalidate the printed-card scenarios."
)]
use super::*;

const ACTOR: PlayerId = PlayerId { seat: 1 };

fn position(card: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"first","card":"BP01-171"}, {"id":"second","card":"BP01-171"}
    ]);
    initial
}

fn checks(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}

fn finish(game: &mut Game, mut step: CardPlay) {
    while let CardPlay::Awaiting(question) = step {
        let answer = if let InputDomain::Targets(domain) = &question.domain {
            Answer::Targets(
                domain
                    .members
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
            )
        } else if let InputDomain::CostMaterials(domain) = &question.domain {
            Answer::CostMaterials(
                domain
                    .members
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
            )
        } else if matches!(question.domain, InputDomain::OptionalCost) {
            Answer::OptionalCost(true)
        } else if matches!(question.domain, InputDomain::WardEntry { .. }) {
            Answer::WardEntry(false)
        } else if matches!(question.domain, InputDomain::Modes { .. }) {
            Answer::Modes(vec![ModeId(1)])
        } else {
            panic!("unexpected choice")
        };
        step = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    checks(game);
}

fn play(game: &mut Game, id: &str) {
    let step = game.play_internal_card(id, ACTOR).unwrap();
    finish(game, step);
}

fn trigger(game: &mut Game, card: &str) {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("expected pending ability")
    };
    let chosen = options
        .iter()
        .filter(|option| option.card == card)
        .min_by_key(|option| option.line)
        .unwrap();
    let step = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(chosen.handle),
        )
        .unwrap();
    finish(game, step);
}

fn activate(game: &mut Game, card: &str) {
    let ability = selection().current[card]
        .program
        .card()
        .abilities
        .iter()
        .find(|ability| {
            ability.node.kind == AbilityKind::Activated
                && !matches!(
                    &ability.node.source.origin,
                    AbilityOrigin::Keyword {
                        keyword: KeywordId::Evolution,
                        ..
                    }
                )
        })
        .unwrap()
        .node
        .id;
    let step = game
        .activate_internal_card("source", ability, ACTOR)
        .unwrap();
    finish(game, step);
}

#[test]
fn summon_circle_creates_both_golems_and_buffs_field_and_ex_without_buffing_the_enemy() {
    let mut initial = position("BP09-045");
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"source","card":"BP09-045"}, {"id":"fuel","card":"BP01-171"}
    ]);
    let mut game = game(&initial);
    play(&mut game, "source");
    trigger(&mut game, "BP09-045");
    trigger(&mut game, "BP09-045");
    let state = game.projection(View::Referee).unwrap();
    let defense = state["P1"]["field"]
        .as_array()
        .unwrap()
        .iter()
        .find(|id| id.as_str() != Some("source"))
        .unwrap()
        .as_str()
        .unwrap();
    let attack = state["P1"]["ex"][0].as_str().unwrap();
    assert_eq!(state["objects"][defense]["power"], 3_u8);
    assert_eq!(state["objects"][defense]["hp"], 3_u8);
    assert_eq!(state["objects"][attack]["power"], 4_u8);
    assert_eq!(state["objects"][attack]["hp"], 2_u8);
    assert_eq!(state["objects"]["enemy"]["power"], 5_u8);
    assert_eq!(state["P1"]["cemetery"], json!(["fuel"]));
}

#[test]
fn breath_fairy_dancer_buffs_other_fairies_and_the_ex_bonus_survives_playing_the_fairy() {
    let mut initial = position("BP01-011");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"field-fairy","card":"BP01-T03"}]);
    initial["players"]["P1"]["zones"]["ex"] = json!([
        {"id":"ex-fairy","card":"BP01-T03"}, {"id":"other","card":"BP01-171"}
    ]);
    let mut game = game(&initial);
    play(&mut game, "source");
    trigger(&mut game, "BP01-011");
    let buffed = game.projection(View::Referee).unwrap();
    for id in ["field-fairy", "ex-fairy"] {
        assert_eq!(buffed["objects"][id]["power"], 2_u8);
        assert_eq!(buffed["objects"][id]["hp"], 2_u8);
    }
    assert_eq!(buffed["objects"]["source"]["power"], 2_u8);
    assert_eq!(buffed["objects"]["source"]["hp"], 3_u8);
    play(&mut game, "ex-fairy");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["ex-fairy"]["power"], 2_u8);
    assert_eq!(state["objects"]["ex-fairy"]["hp"], 2_u8);
    assert_eq!(state["P1"]["ex"], json!(["other"]));
}

#[test]
fn eleanor_banishes_a_witch_to_draw_then_uses_the_five_and_ten_card_damage_thresholds() {
    for banished in [4_u8, 5, 9, 10] {
        let mut initial = position("BP17-039");
        initial["players"]["P1"]["zones"]["cemetery"] =
            json!([{"id":"material","card":"BP01-060"}]);
        initial["players"]["P1"]["zones"]["banish"] = json!(
            (0_u8..(banished - 1))
                .map(|i| json!({"id":format!("banished-{i}"),"card":"BP01-171"}))
                .collect::<Vec<_>>()
        );
        let mut game = game(&initial);
        play(&mut game, "source");
        trigger(&mut game, "BP17-039");
        let drawn = game.projection(View::Referee).unwrap();
        assert_eq!(drawn["P1"]["hand"], json!(["first"]));
        assert_eq!(
            drawn["P1"]["banish"].as_array().unwrap().len(),
            usize::from(banished)
        );
        assert_eq!(drawn["P1"]["cemetery"], json!([]));
        activate(&mut game, "BP17-039");
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["objects"]["enemy"]["damage"],
            if banished >= 10 {
                4_u8
            } else {
                u8::from(banished >= 5)
            }
        );
        assert_eq!(state["objects"]["source"]["acted"], true);
    }
}

#[test]
fn flower_fox_evolution_buffs_only_fairy_tokens_in_ex_when_brilliant_fairy_is_present() {
    let mut initial = position("BP22-005");
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"source","card":"BP22-005"}, {"id":"brilliant","card":"BP22-001"}
    ]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP22-006"}]);
    initial["players"]["P1"]["zones"]["ex"] = json!([
        {"id":"fairy","card":"BP22-T05"}, {"id":"printed-fairy","card":"BP01-011"}
    ]);
    let mut game = game(&initial);
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
        )
        .unwrap()
        .outcome,
        "resolved"
    );
    checks(&mut game);
    trigger(&mut game, "BP22-006");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["ex"].as_array().unwrap().len(), 3_usize);
    let wisp = state["P1"]["ex"][2].as_str().unwrap();
    for id in ["fairy", wisp] {
        assert_eq!(state["objects"][id]["power"], 2_u8);
        assert_eq!(state["objects"][id]["hp"], 2_u8);
    }
    assert_eq!(state["objects"]["printed-fairy"]["power"], 2_u8);
    assert_eq!(state["objects"]["printed-fairy"]["hp"], 3_u8);
}

#[test]
fn demon_discards_before_moving_and_buffing_the_follower_on_top_of_the_deck() {
    let mut initial = position("BP07-078");
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"source","card":"BP07-078"},{"id":"fuel","card":"BP01-171"}
    ]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"top","card":"BP01-037"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    trigger(&mut game, "BP07-078");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["cemetery"], json!(["fuel"]));
    assert_eq!(state["P1"]["ex"], json!(["top"]));
    assert_eq!(state["objects"]["top"]["power"], 6_u8);
    assert_eq!(state["objects"]["top"]["hp"], 6_u8);
}

#[test]
fn evil_collector_only_buffs_a_follower_among_the_cards_it_moves_to_ex() {
    for (top, follower) in [("BP01-037", true), ("BP01-170", false)] {
        let mut initial = position("BP20-090");
        initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"top","card":top}]);
        let mut game = game(&initial);
        play(&mut game, "source");
        trigger(&mut game, "BP20-090");
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["ex"], json!(["top"]));
        if follower {
            assert_eq!(state["objects"]["top"]["power"], 6_u8);
            assert_eq!(state["objects"]["top"]["hp"], 6_u8);
        } else {
            assert_eq!(state["objects"]["top"]["power"], 0_u8);
            assert_eq!(state["objects"]["top"]["hp"], 0_u8);
        }
    }
}

#[test]
fn cerberus_chooses_one_arm_below_ten_graves_and_creates_both_at_necrocharge_ten() {
    for graves in [9_u8, 10] {
        let mut initial = position("BP01-101");
        initial["players"]["P1"]["zones"]["cemetery"] = json!(
            (0_u8..graves)
                .map(|i| json!({"id":format!("grave-{i}"),"card":"BP01-171"}))
                .collect::<Vec<_>>()
        );
        let mut game = game(&initial);
        play(&mut game, "source");
        if graves == 9 {
            let pending_question = game.internal_pending_choice().unwrap().unwrap();
            let InputDomain::Pending(options) = &pending_question.domain else {
                panic!("expected Cerberus Fanfare")
            };
            let pending_step = game
                .answer_internal_card(
                    pending_question.actor,
                    pending_question.token,
                    Answer::Pending(options[0].handle),
                )
                .unwrap();
            let CardPlay::Awaiting(mode_question) = pending_step else {
                panic!("one arm must be chosen below ten graves")
            };
            assert!(matches!(mode_question.domain, InputDomain::Modes { .. }));
            let step = game
                .answer_internal_card(
                    mode_question.actor,
                    mode_question.token,
                    Answer::Modes(vec![ModeId(2)]),
                )
                .unwrap();
            finish(&mut game, step);
        } else {
            trigger(&mut game, "BP01-101");
        }
        let state = game.projection(View::Referee).unwrap();
        let names: Vec<_> = state["P1"]["ex"]
            .as_array()
            .unwrap()
            .iter()
            .map(|id| state["objects"][id.as_str().unwrap()]["name"].clone())
            .collect();
        let expected = if graves == 9 {
            json!(["冥焔の左腕・ココ"])
        } else {
            json!(["冥焔の右腕・ミミ", "冥焔の左腕・ココ"])
        };
        assert_eq!(names, *expected.as_array().unwrap());
    }
}

#[test]
fn the_remnants_recover_another_named_assassin_without_offering_their_own_name() {
    let mut initial = position("BP17-035");
    initial["players"]["P1"]["zones"]["cemetery"] = json!([
        {"id":"same","card":"BP17-035"}, {"id":"assassin","card":"SD02-005"}
    ]);
    let mut game = game(&initial);
    play(&mut game, "source");
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("expected a Fanfare")
    };
    let step = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap();
    let CardPlay::Awaiting(target_question) = &step else {
        panic!("expected the cemetery target")
    };
    let InputDomain::Targets(domain) = &target_question.domain else {
        panic!("expected targets")
    };
    assert_eq!(domain.members.len(), 1_usize);
    assert_eq!(domain.members[0].card.as_deref(), Some("SD02-005"));
    finish(&mut game, step);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["cemetery"], json!(["same"]));
    assert_eq!(state["P1"]["ex"], json!(["assassin"]));
}

#[test]
fn golem_assault_uses_an_earth_rite_to_buff_both_new_golems_in_ex() {
    let mut initial = position("BP22-054");
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"soil","card":"BP03-T02","state":{"counters":{"stack_counter":1_u8}}}
    ]);
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    let ex = state["P1"]["ex"].as_array().unwrap();
    assert_eq!(ex.len(), 2_usize);
    let defense = ex[0].as_str().unwrap();
    let attack = ex[1].as_str().unwrap();
    assert_eq!(state["objects"][defense]["power"], 3_u8);
    assert_eq!(state["objects"][defense]["hp"], 4_u8);
    assert_eq!(state["objects"][attack]["power"], 4_u8);
    assert_eq!(state["objects"][attack]["hp"], 3_u8);
    assert_eq!(state["P1"]["field"], json!([]));
}

#[test]
fn marions_evolution_can_choose_and_buff_a_dragon_follower_in_ex() {
    let mut initial = position("BP07-056");
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP07-056"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP07-057"}]);
    initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"dragon","card":"BP01-079"}]);
    let mut game = game(&initial);
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
        )
        .unwrap()
        .outcome,
        "resolved"
    );
    checks(&mut game);
    trigger(&mut game, "BP07-057");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["dragon"]["power"], 4_u8);
    assert_eq!(state["objects"]["dragon"]["hp"], 3_u8);
    assert_eq!(state["P1"]["ex"], json!(["dragon"]));
}

#[test]
fn divine_smithing_targets_a_fairy_in_each_public_zone_and_draws_for_combo_three() {
    let mut initial = position("BP07-011");
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"source","card":"BP07-011"},{"id":"first-play","card":"BP01-171"},{"id":"second-play","card":"BP01-171"}
    ]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"field-fairy","card":"BP01-T03"}]);
    initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"ex-fairy","card":"BP01-T03"}]);
    let mut game = game(&initial);
    play(&mut game, "first-play");
    play(&mut game, "second-play");
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    for id in ["field-fairy", "ex-fairy"] {
        assert_eq!(state["objects"][id]["power"], 2_u8);
        assert_eq!(state["objects"][id]["hp"], 1_u8);
    }
    assert_eq!(state["P1"]["hand"], json!(["first"]));
    assert_eq!(state["objects"]["first-play"]["power"], 2_u8);
}
