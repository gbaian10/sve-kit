#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected public decisions invalidate these printed nested plays."
)]
use super::union_burst_batch1::pending;
use super::*;
use sve_engine::game::internal::SearchAnswer;

fn aliases(number: &str) -> Vec<String> {
    let selected = selection_with_rules();
    let identity = &selected.current[number].card_id;
    selected
        .current
        .iter()
        .filter(|(_, card)| &card.card_id == identity)
        .map(|(printing, _)| printing.clone())
        .collect()
}
fn position(card: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"source","card":card,"state":{"entered_this_turn":false}}]);
    initial["players"]["P1"]["zones"]["deck"] =
        json!([{"id":"drawn","card":"BP01-040"},{"id":"spare","card":"BP01-040"}]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-040","state":{"hp":20_u8,"acted":true}}]);
    initial
}
fn game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "nested-light",
    )
    .unwrap()
}
fn checks(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}
fn finish(game: &mut Game, mut play: CardPlay, mode: u64) -> CardPlay {
    while let CardPlay::Awaiting(question) = play {
        let answer = match &question.domain {
            InputDomain::OptionalCost => Answer::OptionalCost(true),
            InputDomain::Modes { modes, .. } => {
                assert!(
                    modes.contains(&ModeId(mode)),
                    "the printed mode must be legal"
                );
                Answer::Modes(vec![ModeId(mode)])
            }
            InputDomain::Targets(domain) | InputDomain::CostMaterials(domain) => {
                let count = usize::try_from(
                    domain
                        .cardinality
                        .maximum
                        .unwrap_or(domain.cardinality.minimum)
                        .0,
                )
                .unwrap()
                .min(domain.members.len());
                let handles = domain
                    .members
                    .iter()
                    .take(count)
                    .map(|member| member.handle)
                    .collect();
                if matches!(&question.domain, InputDomain::Targets(_)) {
                    Answer::Targets(handles)
                } else {
                    Answer::CostMaterials(handles)
                }
            }
            InputDomain::Search(domain) => {
                assert!(
                    domain.groups.is_none(),
                    "these printed searches have one ungrouped result"
                );
                let count = usize::try_from(
                    domain
                        .cardinality
                        .maximum
                        .unwrap_or(domain.cardinality.minimum)
                        .0,
                )
                .unwrap()
                .min(domain.cards.len());
                Answer::Search(SearchAnswer::Ungrouped(
                    domain
                        .cards
                        .iter()
                        .take(count)
                        .map(|member| member.handle)
                        .collect(),
                ))
            }
            _ => panic!("unexpected nested-light decision"),
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    play
}
fn trigger(game: &mut Game, card: &str, mode: u64) -> Activation {
    let play = pending(game, card);
    let outcome = resolved(finish(game, play, mode));
    checks(game);
    outcome
}
fn evolve(game: &mut Game, pp: u8) {
    let state = game.projection(View::Referee).unwrap();
    let base = state["objects"]["source"]["card"].as_str().unwrap();
    let definition = selection_with_rules().current[base]
        .program
        .card()
        .abilities
        .iter()
        .find(|ability| {
            matches!(
                ability.node.source.origin,
                AbilityOrigin::Keyword {
                    keyword: KeywordId::Evolution,
                    ..
                }
            )
        })
        .unwrap()
        .node
        .id;
    let result = game
        .internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","definition":definition.0,"pay":{"pp":pp}}),
        )
        .unwrap();
    assert_eq!(
        result.outcome, "resolved",
        "the real printed base must evolve"
    );
}
fn attack(game: &mut Game) {
    let result = game
        .internal_rule_action(&json!({"do":"attack","attacker":"source","target":"enemy"}))
        .unwrap();
    assert_eq!(
        result.outcome, "resolved",
        "the printed source must legally declare its attack"
    );
}

#[test]
fn amiros_replays_a_real_hero_then_its_other_hero_attack_condition_observes_that_follower() {
    for form in aliases("BP03-024") {
        let mut initial = position("BP03-023");
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
        initial["players"]["P1"]["zones"]["cemetery"] =
            json!([{"id":"returned","card":"BP03-023"}]);
        let mut game = game(&initial);
        evolve(&mut game, 3);
        let played = trigger(&mut game, &form, 1);
        let after = game.projection(View::Referee).unwrap();
        assert_eq!(after["P1"]["field"], json!(["source", "returned"]));
        assert_eq!(after["P1"]["pp"]["current"], 7_u8);
        assert_eq!(
            played
                .progress
                .facts
                .iter()
                .filter(|fact| matches!(fact, Fact::CardPlayed { .. }))
                .count(),
            1
        );
        attack(&mut game);
        trigger(&mut game, &form, 1);
        let attacked = game.projection(View::Referee).unwrap();
        assert_eq!(attacked["objects"]["source"]["power"], 4_u8);
        assert_eq!(attacked["objects"]["source"]["max_hp"], 4_u8);
    }
    let mut alone = game(&position("BP03-024"));
    attack(&mut alone);
    trigger(&mut alone, "BP03-024", 1);
    assert_eq!(
        alone.projection(View::Referee).unwrap()["objects"]["source"]["power"],
        3_u8
    );
}

#[test]
fn valdain_all_printings_choose_the_real_search_or_zero_cost_replay_and_keep_shadow_invasion_in_ex()
{
    for form in aliases("BP07-053") {
        for mode in [1_u64, 2] {
            let mut initial = position("BP07-052");
            initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
            initial["players"]["P1"]["zones"]["cemetery"] =
                json!([{"id":"shadow","card":"BP07-058"}]);
            initial["players"]["P1"]["zones"]["deck"] =
                json!([{"id":"found","card":"BP07-058"},{"id":"spare","card":"BP01-040"}]);
            let mut game = game(&initial);
            evolve(&mut game, 2);
            trigger(&mut game, &form, mode);
            let after = game.projection(View::Referee).unwrap();
            assert_eq!(after["objects"]["enemy"]["hp"], 16_u8);
            assert_eq!(after["P1"]["pp"]["current"], 8_u8);
            assert_eq!(
                after["P1"]["ex"],
                if mode == 1 {
                    json!(["found"])
                } else {
                    json!(["shadow"])
                }
            );
            assert_eq!(
                after["P1"]["cemetery"],
                if mode == 1 {
                    json!(["shadow"])
                } else {
                    json!([])
                }
            );
        }
    }
}

#[test]
fn maisha_base_printings_attack_and_replay_a_real_neutral_spell_without_paying_its_original_two() {
    for card in aliases("BP07-108") {
        let mut initial = position(&card);
        initial["players"]["P1"]["zones"]["cemetery"] = json!([{"id":"path","card":"BP01-170"}]);
        let mut game = game(&initial);
        attack(&mut game);
        trigger(&mut game, &card, 1);
        let after = game.projection(View::Referee).unwrap();
        assert_eq!(after["P1"]["pp"]["current"], 10_u8);
        assert_eq!(after["P1"]["hand"], json!(["drawn"]));
        assert_eq!(after["P1"]["cemetery"], json!(["path"]));
    }
}

#[test]
fn maisha_evolution_printings_pay_five_mill_five_gain_storm_and_replay_a_newly_milled_neutral_spell()
 {
    for form in aliases("BP07-109") {
        let mut initial = position("BP07-108");
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
        initial["players"]["P1"]["zones"]["deck"] = json!([
            {"id":"path","card":"BP01-170"},
            {"id":"m1","card":"BP01-040"},{"id":"m2","card":"BP01-040"},
            {"id":"m3","card":"BP01-040"},{"id":"m4","card":"BP01-040"},
            {"id":"drawn","card":"BP01-040"},{"id":"spare","card":"BP01-040"}
        ]);
        let mut game = game(&initial);
        evolve(&mut game, 2);
        trigger(&mut game, &form, 1);
        let before = game.projection(View::Referee).unwrap();
        assert_eq!(before["P1"]["pp"]["current"], 3_u8);
        assert_eq!(before["P1"]["cemetery"].as_array().unwrap().len(), 5);
        assert!(
            before["objects"]["source"]["keywords"]
                .as_array()
                .unwrap()
                .contains(&json!("疾走"))
        );
        attack(&mut game);
        trigger(&mut game, &form, 1);
        let after = game.projection(View::Referee).unwrap();
        assert_eq!(after["P1"]["pp"]["current"], 3_u8);
        assert_eq!(after["P1"]["hand"], json!(["drawn"]));
    }
}

#[test]
fn hina_all_printings_replay_the_real_demas_draw_spell_and_its_original_cost_condition() {
    for form in aliases("CP02-043") {
        for big in [false, true] {
            let mut initial = position("CP02-042");
            initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
            initial["players"]["P1"]["zones"]["cemetery"] =
                json!([{"id":"spell","card":"CP02-011"}]);
            if big {
                initial["players"]["P1"]["zones"]["field"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!({"id":"companion","card":"BP01-037"}));
            }
            let mut game = game(&initial);
            evolve(&mut game, 1);
            trigger(&mut game, &form, 1);
            let after = game.projection(View::Referee).unwrap();
            assert_eq!(after["P1"]["hand"], json!(["drawn"]));
            assert_eq!(after["P1"]["pp"]["current"], if big { 10_u8 } else { 9_u8 });
            assert_eq!(after["P1"]["cemetery"], json!(["spell"]));
        }
    }
}
