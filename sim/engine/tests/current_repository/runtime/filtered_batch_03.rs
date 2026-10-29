#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate printed card scenarios."
)]
use super::*;

fn finish_mode(game: &mut Game, mut play: CardPlay, mode: u64) {
    while let CardPlay::Awaiting(question) = play {
        let answer = if matches!(&question.domain, InputDomain::WardEntry { .. }) {
            Answer::WardEntry(false)
        } else if matches!(&question.domain, InputDomain::OptionalCost) {
            Answer::OptionalCost(true)
        } else if let InputDomain::CostMaterials(domain) = &question.domain {
            Answer::CostMaterials(domain.members.iter().take(1).map(|m| m.handle).collect())
        } else if matches!(&question.domain, InputDomain::Modes { .. }) {
            Answer::Modes(vec![ModeId(mode)])
        } else {
            let InputDomain::Targets(domain) = &question.domain else {
                panic!("unexpected printed choice: {:?}", question.domain);
            };
            Answer::Targets(
                domain
                    .members
                    .iter()
                    .take(
                        usize::try_from(domain.cardinality.minimum.0)
                            .unwrap()
                            .max(1),
                    )
                    .map(|m| m.handle)
                    .collect(),
            )
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    assert!(
        !matches!(play, CardPlay::Cancelled),
        "the printed play has legal targets and payment"
    );
}

fn trigger(game: &mut Game) {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("pending trigger")
    };
    assert!(!options.is_empty(), "a printed trigger is pending");
    let play = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap();
    finish(game, play);
}

fn position(card: &str, material: &str, count: usize, payment: bool) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    if payment {
        initial["players"]["P1"]["zones"]["hand"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"payment","card":material}));
    }
    initial["players"]["P1"]["zones"]["cemetery"] = json!(
        (0..count)
            .map(|i| json!({"id":format!("grave-{i}"),"card":material}))
            .collect::<Vec<_>>()
    );
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"top","card":"BP01-037"},{"id":"next","card":"BP01-037"},{"id":"last","card":"BP01-037"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP04-117"}]);
    initial
}

fn finish(game: &mut Game, play: CardPlay) {
    finish_mode(game, play, 1);
}
fn rules_game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "filtered-batch-03",
    )
    .unwrap()
}

fn vampy_game(initial: &Value) -> Game {
    use std::sync::OnceLock;
    static SELECTION: OnceLock<SelectedEffects> = OnceLock::new();
    let selection = SELECTION.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        let mut face = snapshot.lines().map(|line|serde_json::from_str::<Value>(line).unwrap())
            .find(|card|card["number"]=="BP01-037").unwrap()["faces"][0].clone();
        face["name"] = json!("小さなヴァンピィ");
        face["text"] = Value::Null;
        face["sections"] = json!([]);
        face["traits"] = json!([]);
        face["power"] = json!("2");
        face["hp"] = json!("3");
        snapshot.push_str(&json!({"number":"vampy-companion","faces":[face]}).to_string());
        snapshot.push('\n');
        input["documents"].as_array_mut().unwrap().push(json!(["vampy-companion.yaml","format: 1\nkind: effect_set\ncards:\n  vampy-companion:\n    status: complete\n    abilities: []\n"]));
        Catalog::from_current_selection(&snapshot,&keywords,&input).unwrap()
    });
    let mut game = Game::from_selected_effects(
        selection.clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "vampy-companion",
    )
    .unwrap();
    game.load_internal_card(
        "vampy-companion",
        &WellFormedCardProgram::check(CardProgram {
            abilities: vec![],
            card_play_costs: vec![],
        })
        .unwrap(),
    )
    .unwrap();
    game
}

#[test]
fn filtered_batch_03_bp06_042_checks_printed_effect() {
    for card in ["BP06-042", "PR-132"] {
        for extra in [0_usize, 1] {
            let mut initial = position(card, "BP06-042", 3, false);
            initial["players"]["P1"]["zones"]["cemetery"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":"onmyoji-follower","card":"BP06-048"}));
            for i in 0..(2 + extra) {
                initial["players"]["P1"]["zones"]["cemetery"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!({"id":format!("other-{i}"),"card":"BP01-015"}));
            }
            let mut game = rules_game(&initial);
            let play = game
                .play_internal_card("source", PlayerId { seat: 1 })
                .unwrap();
            finish(&mut game, play);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["P1"]["hand"].as_array().unwrap().len(), extra);
            let field = state["P1"]["field"].as_array().unwrap();
            assert_eq!(field.len(), 1_usize);
            assert_eq!(state["objects"][field[0].as_str().unwrap()]["name"], "形代");
        }
    }
}

#[test]
fn filtered_batch_03_bp06_043_checks_printed_effect() {
    for card in ["BP06-043"] {
        for extra in [0_usize, 1] {
            let mut initial = position(card, "BP06-042", 3, false);
            initial["players"]["P1"]["zones"]["cemetery"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":"onmyoji-follower","card":"BP06-048"}));
            for i in 0..(2 + extra) {
                initial["players"]["P1"]["zones"]["cemetery"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!({"id":format!("other-{i}"),"card":"BP01-015"}));
            }
            let mut game = rules_game(&initial);
            let play = game
                .play_internal_card("source", PlayerId { seat: 1 })
                .unwrap();
            finish(&mut game, play);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["P1"]["hand"].as_array().unwrap().len(), extra);
            let field = state["P1"]["field"].as_array().unwrap();
            assert_eq!(field.len(), 1_usize);
            assert_eq!(state["objects"][field[0].as_str().unwrap()]["name"], "形代");
        }
    }
}

#[test]
fn filtered_batch_03_bp09_075_checks_printed_effect() {
    for card in ["BP09-075", "PR-230", "PR-488"] {
        for count in [4_usize, 5] {
            for named in [false, true] {
                let mut initial = position(card, card, count, false);
                if named {
                    initial["players"]["P1"]["zones"]["cemetery"]
                        .as_array_mut()
                        .unwrap()
                        .push(json!({"id":"vampy","card":"vampy-companion"}));
                }
                let mut game = vampy_game(&initial);
                let play = game
                    .play_internal_card("source", PlayerId { seat: 1 })
                    .unwrap();
                finish(&mut game, play);
                let state = game.projection(View::Referee).unwrap();
                assert_eq!(
                    state["P1"]["hand"].as_array().unwrap().len(),
                    usize::from(named)
                );
                assert_eq!(
                    state["P1"]["pp"]["current"],
                    if count >= 5 { 10_u8 } else { 8_u8 }
                );
                assert_eq!(state["P1"]["field"].as_array().unwrap().len(), 2_usize);
                for id in state["P1"]["field"].as_array().unwrap() {
                    assert_eq!(
                        state["objects"][id.as_str().unwrap()]["name"],
                        "フォレストバット"
                    );
                }
            }
        }
    }
}

#[test]
fn filtered_batch_03_bp13_042_checks_printed_effect() {
    for card in ["BP13-042", "BP13-P09"] {
        for count in [3_usize, 4] {
            let mut game = rules_game(&position(card, card, count, true));
            let play = game
                .play_internal_card("source", PlayerId { seat: 1 })
                .unwrap();
            finish(&mut game, play);
            trigger(&mut game);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["P1"]["hand"], json!(["top"]));
            assert_eq!(
                state["P1"]["ex"].as_array().unwrap().len(),
                usize::from(count == 4)
            );
            if count == 4 {
                let id = state["P1"]["ex"][0].as_str().unwrap();
                assert_eq!(state["objects"][id]["name"], "フェイタルバーン");
            }
            game.resume_internal_card().unwrap();
            let activation = game
                .activate_internal_card("source", AbilityId(4), PlayerId { seat: 1 })
                .unwrap();
            finish(&mut game, activation);
            let after = game.projection(View::Referee).unwrap();
            assert_eq!(after["objects"]["enemy"]["damage"], 1_u8);
            assert_eq!(after["objects"]["source"]["acted"], true);
        }
    }
}

#[test]
fn filtered_batch_03_bp13_p30_checks_printed_effect() {
    for card in ["BP13-P30", "BP13-T02"] {
        for mode in [1_u64, 2] {
            for count in [14_usize, 15] {
                let mut initial = position(card, "BP13-042", count, false);
                initial["players"]["P2"]["zones"]["field"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!({"id":"enemy-two","card":"BP04-117"}));
                let mut game = rules_game(&initial);
                let play = game
                    .play_internal_card("source", PlayerId { seat: 1 })
                    .unwrap();
                finish_mode(&mut game, play, mode);
                let state = game.projection(View::Referee).unwrap();
                assert_eq!(
                    state["objects"]["enemy"]["damage"],
                    if mode == 1 || count >= 15 { 4_u8 } else { 0_u8 }
                );
                assert_eq!(
                    state["objects"]["enemy-two"]["damage"],
                    if mode == 2 && count >= 15 { 4_u8 } else { 0_u8 }
                );
            }
        }
    }
}

#[test]
fn filtered_batch_03_bp18_096_checks_printed_effect() {
    for card in ["BP18-096"] {
        for count in [4_usize, 5] {
            for matching in [false, true] {
                let mut initial = position(card, card, count, false);
                if matching {
                    initial["players"]["P1"]["zones"]["deck"][0]["card"] = json!(card);
                }
                let mut game = rules_game(&initial);
                let play = game
                    .play_internal_card("source", PlayerId { seat: 1 })
                    .unwrap();
                finish(&mut game, play);
                let state = game.projection(View::Referee).unwrap();
                assert_eq!(
                    state["P1"]["hand"].as_array().unwrap().len(),
                    usize::from(matching)
                );
                assert_eq!(
                    state["P1"]["deck"].as_array().unwrap().len(),
                    3 - usize::from(matching)
                );
                let field = state["P1"]["field"].as_array().unwrap();
                assert_eq!(field.len(), 1_usize);
                let bat = &state["objects"][field[0].as_str().unwrap()];
                assert_eq!(bat["name"], "フォレストバット");
                assert_eq!(bat["power"], if count >= 5 { 3_u8 } else { 1_u8 });
                for keyword in ["突進", "指定攻撃"] {
                    assert_eq!(
                        bat["keywords"]
                            .as_array()
                            .unwrap()
                            .contains(&json!(keyword)),
                        count >= 5
                    );
                }
            }
        }
    }
}

#[test]
fn filtered_batch_03_bp21_pr10_checks_printed_effect() {
    for card in ["BP21-PR10", "DSD01a-009", "DSD01a-P03"] {
        for count in [4_usize, 5] {
            let mut game = rules_game(&position(card, "BP21-PR10", count, false));
            let play = game
                .play_internal_card("source", PlayerId { seat: 1 })
                .unwrap();
            finish(&mut game, play);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["objects"]["enemy"]["damage"],
                if count >= 5 { 3_u8 } else { 2_u8 }
            );
            assert_eq!(state["P2"]["leader"]["life"], 20_u8);
        }
    }
}

#[test]
fn filtered_batch_03_dsd01a_002_checks_printed_effect() {
    for card in [
        "DSD01a-002",
        "DSD01a-SL02",
        "SP01-021",
        "SP01-SL21",
        "SP01-SP14",
        "SP01-U12",
    ] {
        for count in [9_usize, 10] {
            let mut game = rules_game(&position(card, "DSD01a-002", count, false));
            let play = game
                .play_internal_card("source", PlayerId { seat: 1 })
                .unwrap();
            finish(&mut game, play);
            trigger(&mut game);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["objects"]["enemy"]["damage"],
                if count >= 10 { 4_u8 } else { 0_u8 }
            );
            assert_eq!(
                state["P2"]["leader"]["life"],
                if count >= 10 { 19_u8 } else { 20_u8 }
            );
        }
    }
}

#[test]
fn filtered_batch_03_dsd01a_006_checks_printed_effect() {
    for card in ["DSD01a-006", "DSD01a-P01"] {
        for count in [4_usize, 5] {
            let mut initial = position(card, card, count, false);
            initial["players"]["P2"]["zones"]["field"][0]["state"] = json!({"acted":true});
            let mut game = rules_game(&initial);
            let play = game
                .play_internal_card("source", PlayerId { seat: 1 })
                .unwrap();
            finish(&mut game, play);
            trigger(&mut game);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["objects"]["source"]["power"],
                if count >= 5 { 4_u8 } else { 3_u8 }
            );
            assert_eq!(
                state["objects"]["source"]["hp"],
                if count >= 5 { 3_u8 } else { 2_u8 }
            );
            assert_eq!(
                state["objects"]["source"]["keywords"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("指定攻撃")),
                count >= 5
            );
            game.resume_internal_card().unwrap();
            assert_eq!(
                game.internal_rule_action(
                    &json!({"do":"attack","attacker":"source","target":"enemy"})
                )
                .unwrap()
                .outcome,
                "resolved"
            );
            trigger(&mut game);
            let after_attack = game.projection(View::Referee).unwrap();
            assert_eq!(after_attack["P1"]["ex"].as_array().unwrap().len(), 1_usize);
            let id = after_attack["P1"]["ex"][0].as_str().unwrap();
            assert_eq!(after_attack["objects"][id]["name"], "マナリアの魔弾");
        }
    }
}

#[test]
fn filtered_batch_03_dsd01a_t01_checks_printed_effect() {
    for card in ["DSD01a-T01", "PR-267"] {
        for count in [9_usize, 10] {
            let mut game = rules_game(&position(card, "DSD01a-002", count, false));
            let play = game
                .play_internal_card("source", PlayerId { seat: 1 })
                .unwrap();
            finish(&mut game, play);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["objects"]["enemy"]["damage"], 3_u8);
            assert_eq!(
                state["P2"]["leader"]["life"],
                if count >= 10 { 18_u8 } else { 20_u8 }
            );
        }
    }
}

#[test]
fn filtered_batch_03_ecp01_027_checks_printed_effect() {
    for card in ["ECP01-027", "ECP01-P15"] {
        for count in [4_usize, 5] {
            for matching in [false, true] {
                let mut initial = position(card, card, count, false);
                if matching {
                    initial["players"]["P1"]["zones"]["deck"][0]["card"] = json!(card);
                }
                let mut game = rules_game(&initial);
                let play = game
                    .play_internal_card("source", PlayerId { seat: 1 })
                    .unwrap();
                finish(&mut game, play);
                let state = game.projection(View::Referee).unwrap();
                assert_eq!(
                    state["P1"]["leader"]["life"],
                    if count >= 5 { 12_u8 } else { 10_u8 }
                );
                assert_eq!(
                    state["P1"]["hand"].as_array().unwrap().len(),
                    usize::from(matching)
                );
                assert_eq!(
                    state["P1"]["deck"].as_array().unwrap().len(),
                    3 - usize::from(matching)
                );
            }
        }
    }
}
