#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate the printed scenarios."
)]
use super::*;
use sve_engine::game::internal::{DecisionView, SearchAnswer};

const PLAYER: PlayerId = PlayerId { seat: 1 };

fn rules_game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "live-power",
    )
    .unwrap()
}

fn position(hand: &str, field: Value) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":hand}]);
    initial["players"]["P1"]["zones"]["field"] = field;
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"enemy","card":"CP03-013","state":{"hp":20_u8,"max_hp":20_u8}}
    ]);
    initial
}

fn handle(question: &DecisionView, card: &str) -> u64 {
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("expected targets, got {:?}", question.domain);
    };
    domain
        .members
        .iter()
        .find(|member| member.card.as_deref() == Some(card))
        .unwrap_or_else(|| panic!("{card} must be a target"))
        .handle
}

/// Answers mode and target questions in order, choosing the given card for each target.
fn declare(game: &mut Game, mut step: CardPlay, modes: &[u64], targets: &[&str]) -> CardPlay {
    let mut remaining = targets.iter();
    while let CardPlay::Awaiting(question) = &step {
        let answer = match &question.domain {
            InputDomain::Modes { .. } => Answer::Modes(modes.iter().map(|id| ModeId(*id)).collect()),
            InputDomain::Targets(_) => {
                let card = remaining.next().unwrap_or_else(|| panic!("unexpected target"));
                Answer::Targets(vec![handle(question, card)])
            }
            _ => break,
        };
        step = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    step
}

fn enemy_damage(game: &Game) -> u64 {
    game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"]
        .as_u64()
        .unwrap()
}

#[test]
fn bp17_090_damage_reads_the_buffed_ally_attack() {
    let mut game = rules_game(&position(
        "BP17-090",
        json!([{"id":"ally","card":"BP01-037"}]),
    ));
    let step = game.play_internal_card("source", PLAYER).unwrap();
    resolved(declare(&mut game, step, &[], &["BP01-037", "CP03-013"]));
    // セージコマンダー is 5/5; the +1/+1 resolves first, so 6 damage, not the declared 5.
    assert_eq!(enemy_damage(&game), 6);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["ally"]["power"], 6_u8);
}

#[test]
fn bp13_008_second_mode_damages_by_the_buffed_secca_attack() {
    for printing in ["BP13-008", "PR-360"] {
        let mut game = rules_game(&position(
            printing,
            json!([{"id":"secca","card":"BP13-002"}]),
        ));
        let step = game.play_internal_card("source", PLAYER).unwrap();
        resolved(declare(&mut game, step, &[2], &["BP13-002", "CP03-013"]));
        // 九火石炎・セッカ is 3/3 and gets +2/+2 before the damage is read.
        assert_eq!(enemy_damage(&game), 5, "{printing}");
    }
}

#[test]
fn bp13_008_both_modes_stack_before_the_damage_reads_the_attack() {
    let mut initial = position("BP13-008", json!([{"id":"secca","card":"BP13-002"}]));
    initial["players"]["P1"]["zones"]["banish"] = json!(
        (0_u8..9)
            .map(|i| json!({"id":format!("banished{i}"),"card":"BP01-037"}))
            .collect::<Vec<_>>()
    );
    let mut game = rules_game(&initial);
    let step = game.play_internal_card("source", PLAYER).unwrap();
    resolved(declare(
        &mut game,
        step,
        &[1, 2],
        &["BP13-002", "BP13-002", "CP03-013"],
    ));
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["secca"]["power"], 7_u8);
    assert!(
        state["objects"]["secca"]["keywords"]
            .as_array()
            .unwrap()
            .contains(&json!("疾走"))
    );
    assert_eq!(enemy_damage(&game), 7);
}

fn ability(card: &str, index: usize) -> AbilityId {
    selection_with_rules().current[card]
        .program
        .card()
        .abilities[index]
        .node
        .id
}

#[test]
fn bp13_001_cemetery_activation_advances_bp13_002_which_finds_the_resolve() {
    let pairs = [
        ("BP13-001", "BP13-002"),
        ("BP13-SL01", "BP13-SP01"),
        ("BP13-U01", "BP13-SL02"),
        ("BP22-R01", "BP22-R02"),
        ("BP13-001", "BP18-SP03"),
    ];
    for (base, form) in pairs {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["cemetery"] = json!([
            {"id":"source","card":base},
            {"id":"fodder","card":"BP01-037"},
            {"id":"fodder_b","card":"BP01-037"}
        ]);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"incoming","card":form}]);
        initial["players"]["P1"]["zones"]["deck"] = json!([
            {"id":"resolve","card":"BP13-008"},
            {"id":"draw","card":"BP01-037"}
        ]);
        let mut game = rules_game(&initial);
        let mut step = game
            .activate_internal_card("source", ability(base, 1), PLAYER)
            .unwrap();
        let mut searched = false;
        while let CardPlay::Awaiting(question) = &step {
            let answer = match &question.domain {
                InputDomain::CostMaterials(domain) => {
                    Answer::CostMaterials(domain.members.iter().map(|member| member.handle).collect())
                }
                InputDomain::Targets(domain) => Answer::Targets(vec![domain.members[0].handle]),
                InputDomain::Modes { .. } => Answer::Modes(vec![ModeId(1)]),
                InputDomain::Search(domain) => {
                    searched = true;
                    Answer::Search(SearchAnswer::Ungrouped(vec![domain.cards[0].handle]))
                }
                other => panic!("unexpected question {other:?}"),
            };
            step = game
                .answer_internal_card(question.actor, question.token, answer)
                .unwrap();
        }
        while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
        if let Some(pending) = game.internal_pending_choice().unwrap() {
            let InputDomain::Pending(options) = &pending.domain else {
                panic!("expected the advanced follower's fanfare");
            };
            let mut fanfare = game
                .answer_internal_card(pending.actor, pending.token, Answer::Pending(options[0].handle))
                .unwrap();
            while let CardPlay::Awaiting(question) = &fanfare {
                let answer = match &question.domain {
                    InputDomain::Modes { .. } => Answer::Modes(vec![ModeId(1)]),
                    InputDomain::Search(domain) => {
                        searched = true;
                        Answer::Search(SearchAnswer::Ungrouped(vec![domain.cards[0].handle]))
                    }
                    other => panic!("unexpected question {other:?}"),
                };
                fanfare = game
                    .answer_internal_card(question.actor, question.token, answer)
                    .unwrap();
            }
        }
        assert!(searched, "{form} searches 九尾の決意");
        let state = game.projection(View::Referee).unwrap();
        assert!(state["P1"]["field"].as_array().unwrap().contains(&json!("incoming")));
        assert!(state["P1"]["hand"].as_array().unwrap().contains(&json!("resolve")));
        assert!(state["P1"]["banish"].as_array().unwrap().contains(&json!("source")));
    }
}

#[test]
fn bp07_007_combo_five_damages_every_enemy_by_the_buffed_latika_attack() {
    for printing in ["BP07-007", "BP07-P01", "PR-342"] {
        for combo in [false, true] {
            let mut initial = position(printing, json!([{"id":"latika","card":"BP07-001"}]));
            let plays = if combo { 4_u8 } else { 3_u8 };
            let mut hand = vec![json!({"id":"source","card":printing})];
            hand.extend((0_u8..plays).map(|i| json!({"id":format!("play{i}"),"card":"BP01-171"})));
            initial["players"]["P1"]["zones"]["hand"] = json!(hand);
            initial["players"]["P2"]["zones"]["field"] = json!([
                {"id":"enemy","card":"CP03-013","state":{"hp":20_u8,"max_hp":20_u8}},
                {"id":"enemy_b","card":"CP03-013","state":{"hp":20_u8,"max_hp":20_u8}}
            ]);
            let mut game = rules_game(&initial);
            for i in 0_u8..plays {
                assert!(
                    matches!(
                        game.play_internal_card(&format!("play{i}"), PLAYER)
                            .unwrap(),
                        CardPlay::Entered(_)
                    ),
                    "each earlier card counts toward the combo"
                );
                while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
                    game.resume_internal_card().unwrap();
                }
            }
            let step = game.play_internal_card("source", PLAYER).unwrap();
            resolved(declare(&mut game, step, &[], &["BP07-001"]));
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["objects"]["latika"]["power"], 6_u8);
            // Latika is 5/5 and gets +1/+1 first; combo 4 deals nothing.
            let expected = if combo { 6_u8 } else { 0_u8 };
            for id in ["enemy", "enemy_b"] {
                assert_eq!(state["objects"][id]["damage"], expected, "{printing} combo {combo}");
            }
        }
    }
}
