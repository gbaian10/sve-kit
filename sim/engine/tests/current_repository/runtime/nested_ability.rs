#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected questions invalidate the printed ability-play scenarios."
)]
use super::union_burst_batch1::{finish, pending};
use super::*;
use sve_engine::EngineFailure;
use sve_engine::game::internal::DecisionView;

fn real_game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "nested-ability-real",
    )
    .unwrap()
}
fn ameth_position(card: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["construction"] = json!("title");
    initial["players"]["P1"]["title_code"] = json!("princess_connect_re_dive");
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"source","card":"CP04-113"},
        {"id":"hatsune0","card":"CP04-051"},{"id":"hatsune1","card":"CP04-051"}
    ]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":card}]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-037","state":{"power":10_u8,"hp":20_u8,"max_hp":20_u8}}]);
    initial
}

fn aliases(representative: &str) -> Vec<String> {
    let selected = selection_with_rules();
    let identity = &selected.current[representative].card_id;
    selected
        .current
        .iter()
        .filter(|(_, card)| &card.card_id == identity)
        .map(|(number, _)| number.clone())
        .collect()
}
fn settle(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}
fn activate(game: &mut Game, holder: &str, number: &str) -> CardPlay {
    let definition = selection_with_rules().current[number]
        .program
        .card()
        .abilities
        .iter()
        .find(|entry| entry.node.kind == AbilityKind::Activated)
        .unwrap()
        .node
        .id;
    game.activate_internal_card(holder, definition, PlayerId { seat: 1 })
        .unwrap()
}
fn question(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(question) = play else {
        panic!("expected a printed decision");
    };
    question
}
fn complete(game: &mut Game, mut play: CardPlay) -> Activation {
    while let CardPlay::Awaiting(question) = play {
        let answer = match &question.domain {
            InputDomain::Abilities(options) => {
                assert_eq!(
                    options.len(),
                    1,
                    "the selected holder has one applicable printed ability"
                );
                Answer::Ability(options[0].handle)
            }
            InputDomain::OptionalCost => Answer::OptionalCost(true),
            InputDomain::CostMaterials(domain) => Answer::CostMaterials(
                domain
                    .members
                    .iter()
                    .take(usize::try_from(domain.cardinality.minimum.0).unwrap())
                    .map(|member| member.handle)
                    .collect(),
            ),
            InputDomain::Targets(domain) => Answer::Targets(
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
            ),
            InputDomain::Order(cards) => {
                Answer::Order(cards.iter().rev().map(|card| card.handle).collect())
            }
            _ => panic!("unexpected printed decision"),
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    resolved(play)
}

#[test]
fn jiemon_all_printings_pay_the_inner_gold_coins_and_keep_its_choice_order_discount_and_aura() {
    for card in aliases("BP14-023") {
        for coins in [1_u8, 3] {
            let mut initial = draw_position();
            initial["players"]["P1"]["zones"]["hand"] = json!([]);
            initial["players"]["P1"]["zones"]["field"] = json!([
                {"id":"source","card":"BP14-022","state":{"evolved":true,"evolved_with":"form","power":4_u8,"hp":5_u8,"max_hp":5_u8}},
                {"id":"other","card":"BP14-030"}
            ]);
            initial["players"]["P1"]["zones"]["evolution"] = json!([{"id":"form","card":card}]);
            initial["players"]["P1"]["zones"]["ex"] = json!(
                (0..coins)
                    .map(|index| json!({"id":format!("coin{index}"),"card":"BP14-T02"}))
                    .collect::<Vec<_>>()
            );
            initial["players"]["P1"]["zones"]["deck"] = json!([
                {"id":"picked","card":"BP14-026"},{"id":"rest0","card":"BP01-171"},
                {"id":"rest1","card":"BP01-171"},{"id":"rest2","card":"BP01-171"},
                {"id":"rest3","card":"BP01-171"},{"id":"tail","card":"BP01-171"}
            ]);
            let mut game = Game::from_selected_effects(
                selection_with_rules().clone(),
                &initial,
                &Value::Null,
                &Value::Null,
                "jiemon-replay",
            )
            .unwrap();
            let play = activate(&mut game, "source", &card);
            let result = complete(&mut game, play);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["P1"]["pp"]["current"], 9_u8);
            assert!(
                state["objects"]["other"]["keywords"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("疾走"))
            );
            assert!(
                !result
                    .progress
                    .facts
                    .iter()
                    .any(|fact| matches!(fact, Fact::Evolved(_)))
            );
            if coins == 3 {
                assert_eq!(
                    state["P1"]["banish"],
                    json!([]),
                    "spent gold tokens leave the banish zone through the ordinary token cleanup"
                );
                assert_eq!(state["P1"]["ex"], json!(["coin2", "picked"]));
                assert_eq!(
                    state["P1"]["deck"],
                    json!(["tail", "rest3", "rest2", "rest1", "rest0"])
                );
                assert_eq!(
                    result
                        .progress
                        .facts
                        .iter()
                        .filter(|fact| matches!(fact, Fact::AbilityPlayed { .. }))
                        .count(),
                    2
                );
                settle(&mut game);
                assert!(matches!(
                    game.play_internal_card("picked", PlayerId { seat: 1 })
                        .unwrap(),
                    CardPlay::Entered(_)
                ));
                assert_eq!(
                    game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
                    9_u8,
                    "the picked three-cost card has the printed turn discount"
                );
            } else {
                assert_eq!(state["P1"]["ex"], json!(["coin0"]));
                assert_eq!(
                    state["P1"]["deck"],
                    json!(["picked", "rest0", "rest1", "rest2", "rest3", "tail"])
                );
                assert_eq!(
                    result
                        .progress
                        .facts
                        .iter()
                        .filter(|fact| matches!(fact, Fact::AbilityPlayed { .. }))
                        .count(),
                    1
                );
            }
        }
    }
}

#[test]
fn ameth_all_printings_exclude_her_own_ub_from_the_threshold_then_replay_an_already_acted_hatsune()
{
    for card in aliases("CP04-114") {
        for prior in [1_u8, 2] {
            let initial = ameth_position(&card);
            let mut game = real_game(&initial);
            for index in 0..prior {
                let play = activate(&mut game, &format!("hatsune{index}"), "CP04-051");
                resolved(finish(&mut game, play));
                settle(&mut game);
            }
            game.internal_rule_action(
                &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}}),
            )
            .unwrap();
            let modes = question(pending(&mut game, &card));
            assert!(matches!(modes.domain, InputDomain::Modes { .. }));
            let targets = question(
                game.answer_internal_card(modes.actor, modes.token, Answer::Modes(vec![ModeId(2)]))
                    .unwrap(),
            );
            let InputDomain::Targets(domain) = &targets.domain else {
                panic!("Ameth's Priconne target");
            };
            assert_eq!(domain.members.len(), 2, "Ameth's own name is excluded");
            let play = game
                .answer_internal_card(
                    targets.actor,
                    targets.token,
                    Answer::Targets(vec![domain.members[0].handle]),
                )
                .unwrap();
            let result = complete(&mut game, play);
            let state = game.projection(View::Referee).unwrap();
            let total = if prior == 2 { 4_u8 } else { 2_u8 };
            assert_eq!(
                state["semantic_state"]["counters_this_turn"]["P1"]["ub_activated"],
                total
            );
            assert_eq!(
                state["objects"]["enemy"]["damage"],
                if prior == 2 { 6_u8 } else { 2_u8 }
            );
            assert_eq!(
                state["P2"]["leader"]["life"],
                if prior == 2 { 14_u8 } else { 18_u8 }
            );
            assert_eq!(state["P1"]["pp"]["current"], 9_u8);
            assert_eq!(
                result
                    .progress
                    .facts
                    .iter()
                    .filter(|fact| matches!(fact, Fact::AbilityPlayed { .. }))
                    .count(),
                if prior == 2 { 2 } else { 1 }
            );
            assert_eq!(state["objects"]["hatsune0"]["acted"], true);
        }
    }
}

#[test]
fn ameths_first_mode_gives_ward_without_replaying_an_ub() {
    let initial = ameth_position("CP04-114");
    let mut game = real_game(&initial);
    game.internal_rule_action(
        &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}}),
    )
    .unwrap();
    let modes = question(pending(&mut game, "CP04-114"));
    let play = game
        .answer_internal_card(modes.actor, modes.token, Answer::Modes(vec![ModeId(1)]))
        .unwrap();
    complete(&mut game, play);
    let state = game.projection(View::Referee).unwrap();
    assert!(
        state["objects"]["hatsune0"]["keywords"]
            .as_array()
            .unwrap()
            .contains(&json!("守護"))
    );
    assert_eq!(
        state["semantic_state"]["counters_this_turn"]["P1"]["ub_activated"],
        1_u8
    );
}

#[test]
fn ameth_all_printings_replay_akari_after_her_normal_limit_and_share_the_inferred_usage_record() {
    for card in aliases("CP04-114") {
        for normally_used in [false, true] {
            let mut initial = ameth_position(&card);
            initial["players"]["P1"]["zones"]["field"]
                .as_array_mut()
                .unwrap()
                .extend([
                    json!({"id":"akari","card":"CP04-083"}),
                    json!({"id":"student","card":"PCS01-035"}),
                ]);
            let mut game = real_game(&initial);
            if normally_used {
                let play = activate(&mut game, "akari", "CP04-083");
                complete(&mut game, play);
                settle(&mut game);
            }
            for index in 0..if normally_used { 1_u8 } else { 2 } {
                let play = activate(&mut game, &format!("hatsune{index}"), "CP04-051");
                complete(&mut game, play);
                settle(&mut game);
            }
            let before = game.projection(View::Referee).unwrap()["objects"]["student"]["power"]
                .as_i64()
                .unwrap();
            game.internal_rule_action(
                &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}}),
            )
            .unwrap();
            let modes = question(pending(&mut game, &card));
            let targets = question(
                game.answer_internal_card(modes.actor, modes.token, Answer::Modes(vec![ModeId(2)]))
                    .unwrap(),
            );
            let InputDomain::Targets(domain) = &targets.domain else {
                panic!("Ameth's formal follower target")
            };
            let akari = domain
                .members
                .iter()
                .find(|member| member.card.as_deref() == Some("CP04-083"))
                .unwrap()
                .handle;
            let selected = game
                .answer_internal_card(targets.actor, targets.token, Answer::Targets(vec![akari]))
                .unwrap();
            let result = complete(&mut game, selected);
            let after = game.projection(View::Referee).unwrap();
            assert_eq!(after["objects"]["student"]["power"], before + 1);
            assert_eq!(
                after["semantic_state"]["counters_this_turn"]["P1"]["ub_activated"],
                4_u8
            );
            assert_eq!(after["P1"]["pp"]["current"], 9_u8);
            assert_eq!(
                result
                    .progress
                    .facts
                    .iter()
                    .filter(|fact| matches!(fact, Fact::AbilityPlayed { .. }))
                    .count(),
                2
            );
            settle(&mut game);
            assert!(game.internal_pending_choice().unwrap().is_none());
            let ready = game.projection(View::Referee).unwrap();
            let definition = selection_with_rules().current["CP04-083"]
                .program
                .card()
                .abilities
                .iter()
                .find(|entry| entry.node.kind == AbilityKind::Activated)
                .unwrap()
                .node
                .id;
            // R-0016 counts the effect play even when no normal play preceded it.
            let error = game
                .activate_internal_card("akari", definition, PlayerId { seat: 1 })
                .unwrap_err();
            assert!(
                matches!(error, EngineFailure::IllegalPlay(message) if message == "activated ability quota exhausted"),
                "Akari's normal play must fail for quota in both usage orders"
            );
            assert_eq!(game.projection(View::Referee).unwrap(), ready);
        }
    }
}
