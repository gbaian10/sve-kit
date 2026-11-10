#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate the printed scenarios."
)]
use super::*;

const PLAYER: PlayerId = PlayerId { seat: 1 };

fn rules_game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "banish-select",
    )
    .unwrap()
}

/// Plays `source`, recording every single-choice domain and answering with `pick`.
fn play_and_settle(game: &mut Game, pick: &dyn Fn(&[Option<String>]) -> usize) -> Vec<Vec<String>> {
    let mut offered = Vec::new();
    let mut step = Some(game.play_internal_card("source", PLAYER).unwrap());
    loop {
        while let Some(CardPlay::Awaiting(question)) = &step {
            let InputDomain::Targets(domain) = &question.domain else {
                panic!("unexpected question {:?}", question.domain)
            };
            let cards: Vec<_> = domain
                .members
                .iter()
                .map(|member| member.card.clone())
                .collect();
            let mut sorted: Vec<_> = cards.iter().flatten().cloned().collect();
            sorted.sort();
            offered.push(sorted);
            let handle = domain.members[pick(&cards)].handle;
            step = Some(
                game.answer_internal_card(
                    question.actor,
                    question.token,
                    Answer::Targets(vec![handle]),
                )
                .unwrap(),
            );
        }
        while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
        let Some(pending) = game.internal_pending_choice().unwrap() else {
            return offered;
        };
        let InputDomain::Pending(options) = &pending.domain else {
            panic!("expected a trigger");
        };
        step = Some(
            game.answer_internal_card(
                pending.actor,
                pending.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap(),
        );
    }
}

fn index_of(cards: &[Option<String>], card: &str) -> usize {
    cards
        .iter()
        .position(|item| item.as_deref() == Some(card))
        .unwrap()
}

#[test]
fn cp03_057_returns_a_banished_pale_moon_card_and_banishes_a_hand_card() {
    for printing in ["CP03-057", "CP03-P40"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["hand"] = json!([
            {"id":"source","card":printing},
            {"id":"spare","card":"BP01-037"}
        ]);
        initial["players"]["P1"]["zones"]["banish"] = json!([
            {"id":"moon","card":"CP03-052"},
            {"id":"other","card":"BP01-037"}
        ]);
        let mut game = rules_game(&initial);
        // The banish choice offers only the Pale Moon card; the hand choice keeps it.
        let offered = play_and_settle(&mut game, &|cards| {
            cards
                .iter()
                .position(|card| card.as_deref() == Some("BP01-037"))
                .unwrap_or(0)
        });
        assert_eq!(
            offered[0],
            ["CP03-052"],
            "{printing}: only the Pale Moon card in banish"
        );
        let state = game.projection(View::Referee).unwrap();
        assert!(
            state["P1"]["hand"]
                .as_array()
                .unwrap()
                .contains(&json!("moon"))
        );
        let banish = state["P1"]["banish"].as_array().unwrap();
        assert!(banish.contains(&json!("spare")) && banish.contains(&json!("other")));
    }
}

#[test]
fn bp18_039_returns_any_banished_card_and_banishes_a_hand_card() {
    for printing in ["BP18-039", "BP18-SL09"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["hand"] = json!([
            {"id":"source","card":printing},
            {"id":"spare","card":"BP01-037"}
        ]);
        initial["players"]["P1"]["zones"]["banish"] = json!([{"id":"gone","card":"CP03-050"}]);
        let mut game = rules_game(&initial);
        let offered = play_and_settle(&mut game, &|cards| {
            cards
                .iter()
                .position(|card| card.as_deref() == Some("BP01-037"))
                .unwrap_or(0)
        });
        assert_eq!(offered[0], ["CP03-050"], "{printing}");
        let state = game.projection(View::Referee).unwrap();
        assert!(
            state["P1"]["hand"]
                .as_array()
                .unwrap()
                .contains(&json!("gone"))
        );
        assert_eq!(state["P1"]["banish"], json!(["spare"]));
        assert_eq!(state["objects"]["spare"]["face_up"], true);
    }
}

#[test]
fn cp03_049_puts_a_cheap_banished_pale_moon_follower_into_play() {
    for printing in ["CP03-049", "CP03-P32"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":printing}]);
        initial["players"]["P1"]["zones"]["banish"] = json!([
            {"id":"cheap","card":"CP03-052"},
            {"id":"costly","card":"CP03-050"},
            {"id":"hidden","card":"CP03-052","face_up":false}
        ]);
        initial["players"]["P2"]["zones"]["banish"] = json!([{"id":"foreign","card":"CP03-052"}]);
        let mut game = rules_game(&initial);
        let public = game.projection(View::P2).unwrap();
        assert!(public["objects"]["cheap"].is_object());
        assert!(public["objects"]["hidden"].is_null());
        assert_eq!(public["P1"]["banish_count"], 3_u8);
        let offered = play_and_settle(&mut game, &|cards| index_of(cards, "CP03-052"));
        assert_eq!(
            offered[0],
            ["CP03-052"],
            "{printing}: only original cost 2 or less"
        );
        let state = game.projection(View::Referee).unwrap();
        assert!(
            state["P1"]["field"]
                .as_array()
                .unwrap()
                .contains(&json!("cheap"))
        );
        assert_eq!(state["P1"]["banish"], json!(["costly", "hidden"]));
    }
}

fn settle_mode(game: &mut Game, mut step: Option<CardPlay>, mode: u64) {
    loop {
        while let Some(CardPlay::Awaiting(q)) = step {
            let answer = match &q.domain {
                InputDomain::Modes { .. } => Answer::Modes(vec![ModeId(mode)]),
                InputDomain::OptionalCost => Answer::OptionalCost(true),
                InputDomain::Targets(d) => {
                    Answer::Targets(d.members.iter().take(1).map(|m| m.handle).collect())
                }
                _ => panic!("unexpected banish selection decision"),
            };
            step = Some(game.answer_internal_card(q.actor, q.token, answer).unwrap());
        }
        while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
        let Some(q) = game.internal_pending_choice().unwrap() else {
            return;
        };
        let InputDomain::Pending(options) = &q.domain else {
            panic!("pending")
        };
        step = Some(
            game.answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
                .unwrap(),
        );
    }
}

#[test]
fn bp18_050_paid_mode_revives_from_banish_and_pays_an_extra_pp() {
    for card in ["BP18-050", "BP18-P22"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
        initial["players"]["P1"]["zones"]["banish"] = json!([
            {"id":"revived","card":"CP03-052"}, {"id":"expensive","card":"CP03-050"}
        ]);
        let mut game = rules_game(&initial);
        let step = game.play_internal_card("source", PLAYER).unwrap();
        settle_mode(&mut game, Some(step), 2);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["pp"]["current"], 6_u8);
        assert_eq!(state["P1"]["field"], json!(["revived"]));
        assert_eq!(state["P1"]["banish"], json!(["expensive"]));
    }
}

#[test]
fn cp03_044_evolution_modes_revive_a_cheap_or_paid_pale_moon_follower() {
    for card in ["CP03-044", "CP03-SL10"] {
        for mode in [1_u64, 2] {
            let mut initial = draw_position();
            initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"CP03-043"}]);
            initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":card}]);
            initial["players"]["P1"]["zones"]["banish"] = json!([
                {"id":"revived","card":if mode == 1 {"CP03-052"} else {"CP03-050"}}
            ]);
            let mut game = rules_game(&initial);
            game.internal_rule_action(
                &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}}),
            )
            .unwrap();
            settle_mode(&mut game, None, mode);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["P1"]["field"], json!(["source", "revived"]));
            assert_eq!(
                state["P1"]["pp"]["current"],
                if mode == 1 { 9_u8 } else { 5_u8 }
            );
        }
    }
}

#[test]
fn cp03_060_ride_returns_a_banished_card_and_buffs_the_source() {
    for card in ["CP03-060", "PR-278"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":card}]);
        initial["players"]["P1"]["zones"]["banish"] = json!([{"id":"returned","card":"CP03-052"}]);
        initial["players"]["P1"]["zones"]["evolve_deck"] =
            json!([{"id":"point","card":"CSD03a-017"}]);
        let mut game = rules_game(&initial);
        let before = game.projection(View::Referee).unwrap();
        let id = selection_with_rules().current[card]
            .program
            .card()
            .abilities
            .iter()
            .find(|a| {
                matches!(
                    a.node.source.origin,
                    AbilityOrigin::Keyword {
                        keyword: KeywordId::Ride,
                        ..
                    }
                )
            })
            .unwrap()
            .node
            .id;
        let CardPlay::Awaiting(q) = game.activate_internal_card("source", id, PLAYER).unwrap()
        else {
            panic!("ride payment")
        };
        let step = game
            .answer_internal_card(
                q.actor,
                q.token,
                Answer::ResourceEntryPayment {
                    materials: vec![0],
                    use_ep: false,
                },
            )
            .unwrap();
        settle_mode(&mut game, Some(step), 1);
        let state = game.projection(View::Referee).unwrap();
        assert!(
            state["P1"]["hand"]
                .as_array()
                .unwrap()
                .contains(&json!("returned"))
        );
        assert_eq!(
            state["objects"]["source"]["power"].as_i64().unwrap(),
            before["objects"]["source"]["power"].as_i64().unwrap() + 1
        );
        assert_eq!(state["P1"]["pp"]["current"], 7_u8);
    }
}

#[test]
fn bp18_050_distinct_targets_reject_duplicates_and_share_field_capacity() {
    for (occupied, pay) in [(0_usize, true), (4, true), (0, false)] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP18-050"}]);
        initial["players"]["P1"]["zones"]["field"] = json!(
            (0..occupied)
                .map(|i| json!({"id":format!("existing-{i}"),"card":"BP01-037"}))
                .collect::<Vec<_>>()
        );
        initial["players"]["P1"]["zones"]["banish"] = json!([
            {"id":"first","card":"CP03-052"},
            {"id":"duplicate","card":"CP03-052"},
            {"id":"different","card":"CP03-058"}
        ]);
        let mut game = rules_game(&initial);
        let mut step = game.play_internal_card("source", PLAYER).unwrap();
        let mut capacity_offered = false;
        while let CardPlay::Awaiting(q) = step {
            let answer = match &q.domain {
                InputDomain::Modes { .. } => Answer::Modes(vec![ModeId(2)]),
                InputDomain::Targets(d) => {
                    let duplicates: Vec<_> = d
                        .members
                        .iter()
                        .filter(|m| m.card.as_deref() == Some("CP03-052"))
                        .map(|m| m.handle)
                        .collect();
                    assert_eq!(duplicates.len(), 2);
                    let before = game.projection(View::Referee).unwrap();
                    game.answer_internal_card(
                        q.actor,
                        q.token,
                        Answer::Targets(duplicates.clone()),
                    )
                    .unwrap_err();
                    assert_eq!(game.projection(View::Referee).unwrap(), before);
                    let other = d
                        .members
                        .iter()
                        .find(|m| m.card.as_deref() == Some("CP03-058"))
                        .unwrap()
                        .handle;
                    Answer::Targets(vec![duplicates[0], other])
                }
                InputDomain::OptionalCost => Answer::OptionalCost(pay),
                InputDomain::SearchPartition(d) => {
                    capacity_offered = true;
                    assert_eq!(d.cardinality.minimum, Nat(1));
                    assert_eq!(d.cards.len(), 2);
                    assert!(d.cards.iter().all(|c| c.object.is_some()));
                    Answer::SearchPartition(vec![d.cards[0].handle])
                }
                _ => panic!("unexpected paid-entry choice"),
            };
            step = game.answer_internal_card(q.actor, q.token, answer).unwrap();
        }
        settle_mode(&mut game, None, 1);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(capacity_offered, occupied == 4 && pay);
        let entered = if pay { (5 - occupied).min(2) } else { 0 };
        assert_eq!(
            state["P1"]["field"].as_array().unwrap().len(),
            occupied + entered
        );
        assert_eq!(state["P1"]["banish"].as_array().unwrap().len(), 3 - entered);
        assert_eq!(state["P1"]["pp"]["current"], if pay { 6_u8 } else { 7_u8 });
    }
}
