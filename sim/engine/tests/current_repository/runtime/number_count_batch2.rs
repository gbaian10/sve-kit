#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate these numeric consumer scenarios."
)]
use super::*;
const PLAYER: PlayerId = PlayerId { seat: 1 };

fn printings(number: &str) -> Vec<String> {
    let card_id = &selection().current[number].card_id;
    selection()
        .current
        .iter()
        .filter(|(_, card)| &card.card_id == card_id)
        .map(|(printing, _)| printing.clone())
        .collect()
}
fn position(number: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":number}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037","state":{"hp":15_u8,"max_hp":15_u8,"acted":true,"power":0_u8}}]);
    initial
}
fn new_game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "number-count-batch2",
    )
    .unwrap()
}
fn finish(game: &mut Game, mut play: CardPlay) {
    while let CardPlay::Awaiting(q) = play {
        let answer = match &q.domain {
            InputDomain::Targets(domain) => Answer::Targets(vec![
                domain
                    .members
                    .iter()
                    .find(|member| member.card.as_deref() == Some("BP01-037"))
                    .unwrap_or(&domain.members[0])
                    .handle,
            ]),
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            _ => panic!("unexpected numeric consumer decision"),
        };
        play = game.answer_internal_card(q.actor, q.token, answer).unwrap();
    }
    resolved(play);
}
fn trigger(game: &mut Game) {
    let q = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &q.domain else {
        panic!("printed trigger")
    };
    assert_eq!(options.len(), 1, "exactly one printed trigger is pending");
    let play = game
        .answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
        .unwrap();
    finish(game, play);
}
fn activate(game: &mut Game, number: &str, line: u32) {
    let definition = selection().current[number]
        .program
        .card()
        .abilities
        .iter()
        .find(|a| a.source.line == line)
        .unwrap()
        .node
        .id;
    let play = game
        .activate_internal_card("source", definition, PLAYER)
        .unwrap();
    finish(game, play);
}

#[test]
fn twin_fencer_counts_itself_and_allied_named_followers_only() {
    for printing in printings("BP20-034") {
        let mut initial = position(&printing);
        initial["players"]["P1"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"twin","card":printing}));
        initial["players"]["P1"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"other","card":"BP01-037"}));
        initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"outside","card":printing}]);
        initial["players"]["P2"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"opposing-twin","card":printing}));
        let mut game = new_game(&initial);
        activate(&mut game, &printing, 2);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["enemy"]["damage"], 2_u8);
        assert_eq!(state["objects"]["source"]["acted"], true);
        assert_eq!(state["P1"]["pp"]["current"], 10_u8);
    }
}

#[test]
fn excella_excludes_itself_silenced_storm_holders_and_other_zones_from_other_storm_count() {
    for printing in printings("BP21-079") {
        let mut initial = position(&printing);
        initial["players"]["P1"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .extend([
                json!({"id":"ally","card":printing}),
                json!({"id":"silenced","card":printing,"state":{"silenced":true}}),
                json!({"id":"ordinary","card":"BP01-037"}),
            ]);
        initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"outside","card":printing}]);
        initial["players"]["P2"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"opposing-storm","card":printing}));
        let mut game = new_game(&initial);
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"enemy"}))
            .unwrap();
        trigger(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["source"]["power"],
            2_u8
        );
    }
}

#[test]
fn ignis_attack_uses_current_power_for_every_enemy_follower() {
    for printing in printings("BP22-057") {
        let mut initial = position(&printing);
        initial["players"]["P1"]["zones"]["field"][0]["state"] = json!({"power":7_u8});
        initial["players"]["P1"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"ally","card":"BP01-037"}));
        initial["players"]["P2"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"second","card":"BP01-037","state":{"hp":15_u8,"max_hp":15_u8}}));
        let mut game = new_game(&initial);
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"enemy"}))
            .unwrap();
        trigger(&mut game);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["enemy"]["damage"], 7_u8);
        assert_eq!(state["objects"]["second"]["damage"], 7_u8);
        assert_eq!(state["objects"]["ally"]["damage"], 0_u8);
        assert_eq!(state["P2"]["leader"]["life"], 20_u8);
    }
}

#[test]
fn sutherland_reads_last_known_power_after_its_real_sacrifice_cost() {
    for printing in printings("CSD03b-010") {
        let mut initial = position(&printing);
        initial["players"]["P1"]["zones"]["field"][0]["state"] = json!({"power":5_u8});
        let mut game = new_game(&initial);
        activate(&mut game, &printing, 2);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["enemy"]["damage"], 5_u8);
        assert_eq!(state["P1"]["cemetery"], json!(["source"]));
        assert_eq!(state["P1"]["pp"]["current"], 10_u8);
    }
}

#[test]
fn night_horde_counts_its_created_bats_with_an_existing_allied_bat() {
    for printing in printings("SD05-006") {
        let mut initial = position(&printing);
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":printing}]);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"old-bat","card":"BP01-T15"}]);
        initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"outside","card":"BP01-T15"}]);
        let mut game = new_game(&initial);
        let play = game.play_internal_card("source", PLAYER).unwrap();
        finish(&mut game, play);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["enemy"]["damage"], 3_u8);
        assert_eq!(state["P1"]["field"].as_array().unwrap().len(), 3);
        assert_eq!(state["P1"]["pp"]["current"], 7_u8);
    }
}

mod distinct {
    use super::*;

    const PLAYER: PlayerId = PlayerId { seat: 1 };

    #[derive(Clone, Copy)]
    enum Reward {
        JointDamage,
        DrawDamage,
        Heal,
    }

    fn printings(number: &str) -> Vec<String> {
        let card_id = &selection().current[number].card_id;
        selection()
            .current
            .iter()
            .filter(|(_, card)| &card.card_id == card_id)
            .map(|(printing, _)| printing.clone())
            .collect()
    }

    fn initial(number: &str, qualified: bool) -> Value {
        let mut initial = draw_position();
        initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
        initial["players"]["P1"]["zones"]["field"] = json!([
            {"id":"source","card":number},
            {"id":"knight","card":"BP17-T11"},
            {"id":"steel","card":"BP17-T12"},
            {"id":"last","card":if qualified {"BP17-T13"} else {"BP01-T05"}}
        ]);
        initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"outside","card":"BP17-T13"}]);
        initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"drawn","card":"BP01-037"}]);
        initial["players"]["P2"]["zones"]["field"] = json!([
            {"id":"enemy","card":"BP01-037","state":{"hp":12_u8,"max_hp":12_u8,"damage":0_u8}},
            {"id":"opposing-guard","card":"BP17-T13"}
        ]);
        initial
    }

    fn finish(game: &mut Game, play: CardPlay) {
        finish_result(game, play).unwrap();
    }

    fn target_answer(domain: &InputDomain) -> Answer {
        let InputDomain::Targets(domain) = domain else {
            panic!("formal follower target")
        };
        let handle = domain
            .members
            .iter()
            .find(|member| member.card.as_deref() == Some("BP01-037"))
            .unwrap()
            .handle;
        Answer::Targets(vec![handle])
    }

    fn finish_result(game: &mut Game, mut play: CardPlay) -> Result<(), EngineFailure> {
        while let CardPlay::Awaiting(question) = play {
            let answer = target_answer(&question.domain);
            play = game.answer_internal_card(question.actor, question.token, answer)?;
        }
        resolved(play);
        Ok(())
    }

    fn check(number: &str, reward: Reward) {
        for printing in printings(number) {
            for qualified in [false, true] {
                let mut game = Game::from_selected_effects(
                    selection_with_rules().clone(),
                    &initial(&printing, qualified),
                    &Value::Null,
                    &Value::Null,
                    "distinct-consumer",
                )
                .unwrap();
                let definition = selection().current[&printing]
                    .program
                    .card()
                    .abilities
                    .iter()
                    .find(|a| a.node.kind == AbilityKind::Activated)
                    .unwrap()
                    .node
                    .id;
                if !qualified {
                    let before = game.projection(View::Referee).unwrap();
                    assert!(
                        matches!(
                            game.activate_internal_card("source", definition, PLAYER)
                                .and_then(|attempt| finish_result(&mut game, attempt))
                                .unwrap_err(),
                            EngineFailure::IllegalPlay(_)
                        ),
                        "three distinct allied field token names are required before activation"
                    );
                    let after = game.projection(View::Referee).unwrap();
                    assert_eq!(
                        after["P1"], before["P1"],
                        "a rejected activation preserves all controller resources"
                    );
                    assert_eq!(
                        after["objects"]["enemy"], before["objects"]["enemy"],
                        "a rejected activation cannot damage its target"
                    );
                    continue;
                }
                let play = game
                    .activate_internal_card("source", definition, PLAYER)
                    .unwrap();
                finish(&mut game, play);
                game.resume_internal_card().unwrap();
                let after = game.projection(View::Referee).unwrap();
                match reward {
                    Reward::JointDamage => {
                        assert_eq!(
                            after["objects"]["enemy"]["damage"], 3_u8,
                            "the selected follower takes the printed three damage"
                        );
                        assert_eq!(
                            after["P2"]["leader"]["life"], 17_u8,
                            "the same selected follower's leader also takes three damage"
                        );
                    }
                    Reward::DrawDamage => {
                        assert_eq!(
                            after["objects"]["enemy"]["damage"], 5_u8,
                            "three distinct soldier token names unlock the printed five damage"
                        );
                        assert_eq!(
                            after["P1"]["hand"],
                            json!(["drawn"]),
                            "the same successful activation also draws one real card"
                        );
                    }
                    Reward::Heal => assert_eq!(
                        after["P1"]["leader"]["life"], 12_u8,
                        "three distinct soldier token names unlock the printed two life"
                    ),
                }
                assert_eq!(
                    after["P1"]["pp"]["current"], 10_u8,
                    "the printed zero-PP activation preserves PP"
                );
                assert!(
                    matches!(
                        game.activate_internal_card("source", definition, PLAYER)
                            .and_then(|attempt| finish_result(&mut game, attempt))
                            .unwrap_err(),
                        EngineFailure::IllegalPlay(_)
                    ),
                    "the printed once-per-turn limit rejects a second activation"
                );
                assert_eq!(
                    game.projection(View::Referee).unwrap()["P1"],
                    after["P1"],
                    "the once-per-turn rejection preserves controller resources"
                );
            }
        }
    }

    #[test]
    fn luminous_knight_requires_three_distinct_soldier_token_names_and_only_activates_once() {
        check("BP16-028", Reward::JointDamage);
    }

    #[test]
    fn luminous_mage_damages_and_draws_only_with_three_distinct_soldier_token_names() {
        check("BP16-029", Reward::DrawDamage);
    }
    #[test]
    fn lyrical_healer_excludes_duplicate_names_opposing_tokens_and_ex_from_its_gate() {
        check("BP16-034", Reward::Heal);
    }
}
