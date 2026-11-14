#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate these printed card scenarios."
)]
use super::*;
use sve_engine::game::internal::SearchAnswer;

const fn actor() -> PlayerId {
    PlayerId { seat: 1 }
}
fn position(card: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"draw","card":"BP01-171"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    initial
}
fn game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "wire-sol-g-ready13",
    )
    .unwrap()
}
fn finish(game: &mut Game, mut play: CardPlay) {
    while let CardPlay::Awaiting(q) = play {
        let answer = match &q.domain {
            InputDomain::Targets(d) => Answer::Targets(
                d.members
                    .iter()
                    .take(
                        usize::try_from(d.cardinality.maximum.unwrap_or(d.cardinality.minimum).0)
                            .unwrap(),
                    )
                    .map(|m| m.handle)
                    .collect(),
            ),
            InputDomain::CostMaterials(d) => Answer::CostMaterials(
                d.members
                    .iter()
                    .take(
                        usize::try_from(d.cardinality.maximum.unwrap_or(d.cardinality.minimum).0)
                            .unwrap(),
                    )
                    .map(|m| m.handle)
                    .collect(),
            ),
            InputDomain::Search(d) => Answer::Search(SearchAnswer::Ungrouped(
                d.cards
                    .iter()
                    .take(
                        usize::try_from(d.cardinality.maximum.unwrap_or(d.cardinality.minimum).0)
                            .unwrap(),
                    )
                    .map(|m| m.handle)
                    .collect(),
            )),
            InputDomain::Order(d) => Answer::Order(d.iter().map(|m| m.handle).collect()),
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            InputDomain::OptionalCost => Answer::OptionalCost(true),
            InputDomain::DiceReroll { .. } => panic!("unexpected dice decision"),
            InputDomain::Abilities(_)
            | InputDomain::ResourceEntryPayment { .. }
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
            | InputDomain::Modes { .. }
            | InputDomain::TokenCapacity { .. } => panic!("Unexpected card decision"),
        };
        play = game.answer_internal_card(q.actor, q.token, answer).unwrap();
    }
}
fn settle(game: &mut Game) {
    loop {
        if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
        let Some(q) = game.internal_pending_choice().unwrap() else {
            break;
        };
        let InputDomain::Pending(options) = &q.domain else {
            panic!("Expected a pending ability")
        };
        let play = game
            .answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
            .unwrap();
        finish(game, play);
    }
}
fn play(game: &mut Game, id: &str) {
    let step = game.play_internal_card(id, actor()).unwrap();
    finish(game, step);
    settle(game);
}
fn activation(card: &str) -> AbilityId {
    selection_with_rules().current[card]
        .program
        .card()
        .abilities
        .iter()
        .find(|a| {
            a.node.kind == AbilityKind::Activated
                && matches!(a.node.source.origin, AbilityOrigin::Printed { .. })
        })
        .unwrap()
        .node
        .id
}
fn activate(game: &mut Game, card: &str) {
    let step = game
        .activate_internal_card("source", activation(card), actor())
        .unwrap();
    finish(game, step);
    settle(game);
}

#[test]
fn bp19_077_empty_hand_searches_paracelise_with_a_turn_discount() {
    for card in ["BP19-077", "BP19-SL21"] {
        let mut initial = position(card);
        initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"found","card":"BP14-072"}]);
        let mut board = game(&initial);
        play(&mut board, "source");
        assert_eq!(
            board.projection(View::Referee).unwrap()["P1"]["ex"],
            json!(["found"])
        );
        play(&mut board, "found");
        assert_eq!(
            board.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
            8_u8
        );
    }
}

#[test]
fn bp21_006_evolution_returns_another_academy_follower_and_hits_the_enemy() {
    for card in ["BP21-006", "BP21-P02"] {
        let mut initial = position(card);
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] =
            json!([{"id":"source","card":"BP21-005"},{"id":"ally","card":"BP21-005"}]);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":card}]);
        let mut board = game(&initial);
        assert_eq!(
            board
                .internal_rule_action(
                    &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
                )
                .unwrap()
                .outcome,
            "resolved"
        );
        settle(&mut board);
        let view = board.projection(View::Referee).unwrap();
        assert_eq!(view["P1"]["hand"], json!(["ally"]));
        assert_eq!(view["objects"]["enemy"]["damage"], 3_u8);
    }
}
#[test]
fn bp10_108_creates_a_temporary_tiger_and_heals_by_the_chosen_original_cost() {
    let mut initial = position("BP10-108");
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    let mut board = game(&initial);
    play(&mut board, "source");
    let before = board.projection(View::Referee).unwrap();
    let tiger = before["P1"]["field"]
        .as_array()
        .unwrap()
        .iter()
        .find(|id| id.as_str() != Some("source"))
        .unwrap()
        .as_str()
        .unwrap()
        .to_owned();
    activate(&mut board, "BP10-108");
    assert_eq!(
        board.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        14_u8
    );
    assert_eq!(
        board
            .internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap()
            .outcome,
        "resolved"
    );
    settle(&mut board);
    assert!(
        !board.projection(View::Referee).unwrap()["P1"]["field"]
            .as_array()
            .unwrap()
            .contains(&json!(tiger))
    );
}
#[test]
fn bp21_011_departure_buffs_an_academy_or_beast_follower() {
    for card in ["BP21-011", "PR-569"] {
        let mut initial = position(card);
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P2"]["zones"]["field"] =
            json!([{"id":"enemy","card":"BP01-037","state":{"acted":true}}]);
        initial["players"]["P1"]["zones"]["field"] =
            json!([{"id":"tamer","card":card},{"id":"ally","card":"BP21-005"}]);
        let mut board = game(&initial);
        assert_eq!(
            board
                .internal_rule_action(&json!({"do":"attack","attacker":"tamer","target":"enemy"}))
                .unwrap()
                .outcome,
            "resolved"
        );
        loop {
            settle(&mut board);
            let Some(q) = board.internal_quick_choice().unwrap() else {
                break;
            };
            let step = board
                .answer_internal_card(q.actor, q.token, Answer::Quick(None))
                .unwrap();
            finish(&mut board, step);
        }
        settle(&mut board);
        let view = board.projection(View::Referee).unwrap();
        assert_eq!(view["P1"]["cemetery"], json!(["tamer"]));
        assert_eq!(view["objects"]["ally"]["power"], 3_u8);
        assert_eq!(view["objects"]["ally"]["hp"], 3_u8);
    }
}
#[test]
fn ecp01_050_race_places_a_cheap_umamusume_from_hand_after_its_fanfare_draw() {
    for card in ["ECP01-050", "ECP01-P26"] {
        let mut initial = position(card);
        initial["players"]["P1"]["zones"]["evolve_deck"] =
            json!([{"id":"carrot","card":"CSD01-032"}]);
        initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"ally","card":"CP01-006"}]);
        let mut board = game(&initial);
        play(&mut board, "source");
        assert_eq!(
            board.projection(View::Referee).unwrap()["P1"]["hand"],
            json!(["ally"])
        );
        meal_race::feed(&mut board, card, 0, false);
        settle(&mut board);
        let view = board.projection(View::Referee).unwrap();
        assert_eq!(view["P1"]["field"], json!(["source", "ally"]));
        assert_eq!(view["objects"]["source"]["power"], 4_u8);
    }
}
#[test]
fn bp10_072_end_phase_searches_only_while_an_evolved_follower_is_present() {
    for evolved in [false, true] {
        let mut initial = position("BP10-072");
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] = if evolved {
            json!([{"id":"source","card":"BP10-072"},{"id":"ally","card":"BP01-144"}])
        } else {
            json!([{"id":"source","card":"BP10-072"}])
        };
        initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"found","card":"BP10-072"}]);
        if evolved {
            initial["players"]["P1"]["zones"]["evolve_deck"] =
                json!([{"id":"form","card":"BP01-145"}]);
        }
        let mut board = game(&initial);
        if evolved {
            let evolution =
                json!({"do":"evolve","source":"ally","evolve_card":"form","pay":{"pp":1_u8}});
            assert_eq!(
                board.internal_rule_action(&evolution).unwrap().outcome,
                "resolved"
            );
            settle(&mut board);
        }
        assert_eq!(
            board
                .internal_rule_action(&json!({"do":"end-phase"}))
                .unwrap()
                .outcome,
            "resolved"
        );
        settle(&mut board);
        assert_eq!(
            board.projection(View::Referee).unwrap()["P1"]["hand"],
            if evolved { json!(["found"]) } else { json!([]) }
        );
    }
}

/// KE-14: only an Onmyoji follower or a spell can be taken; an amulet (BP21-P21 is an
/// Alchemist amulet) and an ordinary follower among the looked-at cards are never offered.
/// No printed Onmyoji amulet exists; the conditions unit tests cover that combination.
#[test]
#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "Any other decision before the choice invalidates the scenario."
)]
fn bp20_p28_offers_only_onmyoji_followers_or_spells_from_the_looked_at_cards() {
    for (deck, offered) in [
        (["BP20-P28", "BP21-P21", "BP01-173"], "BP20-P28"),
        (["BP01-173", "BP21-012", "BP21-P21"], "BP21-012"),
    ] {
        let mut initial = position("BP20-P28");
        initial["players"]["P1"]["zones"]["hand"] =
            json!([{"id":"source","card":"BP20-P28"},{"id":"discard","card":"BP01-171"}]);
        initial["players"]["P1"]["zones"]["deck"] = Value::Array(
            deck.iter()
                .enumerate()
                .map(|(i, card)| json!({"id":format!("d{i}"),"card":card}))
                .collect(),
        );
        let mut board = game(&initial);
        let mut step = board.play_internal_card("source", actor()).unwrap();
        let candidates = loop {
            let CardPlay::Awaiting(q) = step else {
                // The fanfare waits as a pending ability after the follower enters.
                if board.internal_card_stage() == Some(ResumeStage::CheckTiming) {
                    board.resume_internal_card().unwrap();
                }
                let pending = board.internal_pending_choice().unwrap().unwrap();
                let InputDomain::Pending(options) = &pending.domain else {
                    panic!("BP20-P28 must offer a choice among the looked-at cards")
                };
                step = board
                    .answer_internal_card(
                        pending.actor,
                        pending.token,
                        Answer::Pending(options[0].handle),
                    )
                    .unwrap();
                continue;
            };
            let answer = match &q.domain {
                InputDomain::OptionalCost => Answer::OptionalCost(true),
                InputDomain::CostMaterials(d) => Answer::CostMaterials(vec![d.members[0].handle]),
                InputDomain::Targets(d)
                    if d.members.iter().any(|m| m.card.as_deref() == Some("BP01-171")) =>
                {
                    Answer::Targets(vec![d.members[0].handle])
                }
                InputDomain::Targets(d) => {
                    break d.members.iter().filter_map(|m| m.card.clone()).collect::<Vec<_>>();
                }
                other => panic!("unexpected decision before the choice: {other:?}"),
            };
            step = board.answer_internal_card(q.actor, q.token, answer).unwrap();
        };
        assert_eq!(candidates, vec![offered.to_owned()], "{deck:?}");
    }
}

#[test]
fn bp20_054_discards_then_finds_a_spell_and_counts_union_for_shikigami() {
    for card in ["BP20-054", "BP20-P28"] {
        for mixed in [false, true] {
            for count in [6_u8, 7] {
                let mut initial = position(card);
                initial["players"]["P1"]["zones"]["hand"] =
                    json!([{"id":"source","card":card},{"id":"discard","card":"BP01-171"}]);
                initial["players"]["P1"]["zones"]["deck"] =
                    json!([{"id":"spell","card":"BP06-042"},{"id":"other","card":"BP01-171"}]);
                initial["players"]["P1"]["zones"]["cemetery"] = json!(
                    (0..count)
                        .map(|i| {
                            let material = if mixed {
                                if i % 2 == 0 { "BP06-039" } else { "BP01-100" }
                            } else {
                                "BP06-042"
                            };
                            json!({"id":format!("material-{i}"),"card":material})
                        })
                        .collect::<Vec<_>>()
                );
                let mut board = game(&initial);
                play(&mut board, "source");
                assert_eq!(
                    board.projection(View::Referee).unwrap()["P1"]["hand"],
                    json!(["spell"])
                );
                let opponent = board.projection(View::P2).unwrap();
                let known = opponent["known_cards"].as_array().unwrap();
                assert!(known.iter().any(|item| item["id"] == "spell"));
                assert!(!known.iter().any(|item| item["id"] == "other"));
                let result = board.activate_internal_card("source", activation(card), actor());
                if count == 6 {
                    assert!(result.is_err());
                } else {
                    finish(&mut board, result.unwrap());
                    settle(&mut board);
                }
                let state = board.projection(View::Referee).unwrap();
                let field = state["P1"]["field"].as_array().unwrap();
                assert_eq!(field.len(), if count == 6 { 1 } else { 2 });
                if count == 7 {
                    let token = field
                        .iter()
                        .find(|id| id.as_str() != Some("source"))
                        .unwrap()
                        .as_str()
                        .unwrap();
                    assert_eq!(state["objects"][token]["card"], "BP06-T02");
                }
            }
        }
    }
}
