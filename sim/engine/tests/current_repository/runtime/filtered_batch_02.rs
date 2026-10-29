#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate printed card scenarios."
)]
use super::*;

fn rules_game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "filtered-count-rules",
    )
    .unwrap()
}

fn finish(game: &mut Game, mut play: CardPlay) {
    while let CardPlay::Awaiting(question) = play {
        let answer = if matches!(&question.domain, InputDomain::WardEntry { .. }) {
            Answer::WardEntry(false)
        } else if matches!(&question.domain, InputDomain::OptionalCost) {
            Answer::OptionalCost(true)
        } else if let InputDomain::CostMaterials(domain) = &question.domain {
            Answer::CostMaterials(domain.members.iter().take(1).map(|m| m.handle).collect())
        } else {
            let InputDomain::Targets(domain) = &question.domain else {
                panic!("unexpected printed choice: {:?}", question.domain);
            };
            Answer::Targets(domain.members.iter().take(1).map(|m| m.handle).collect())
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

fn maisha_game(initial: &Value) -> Game {
    use std::sync::OnceLock;
    static SELECTION: OnceLock<SelectedEffects> = OnceLock::new();
    let selection = SELECTION.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        let mut face = snapshot.lines().map(|line|serde_json::from_str::<Value>(line).unwrap())
            .find(|card|card["number"]=="BP01-037").unwrap()["faces"][0].clone();
        face["name"] = json!("粛清の英雄・メイシア");
        face["text"] = Value::Null;
        face["sections"] = json!([]);
        face["traits"] = json!([]);
        face["power"] = json!("2");
        face["hp"] = json!("3");
        snapshot.push_str(&json!({"number":"count-companion","faces":[face]}).to_string());
        snapshot.push('\n');
        input["documents"].as_array_mut().unwrap().push(json!(["count-companion.yaml","format: 1\nkind: effect_set\ncards:\n  count-companion:\n    status: complete\n    abilities: []\n"]));
        Catalog::from_current_selection(&snapshot,&keywords,&input).unwrap()
    });
    let mut game = Game::from_selected_effects(
        selection.clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "count-companion",
    )
    .unwrap();
    game.load_internal_card(
        "count-companion",
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
fn filtered_batch_02_bp07_026_checks_printed_effect() {
    for card in ["BP07-026", "BP07-P08"] {
        for count in [1, 2] {
            let mut initial = position(card, "BP01-029", count, false);
            initial["players"]["P1"]["zones"]["hand"] = json!([]);
            initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP07-025"}]);
            initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":card}]);
            let mut game = rules_game(&initial);
            assert_eq!(
                game.internal_rule_action(
                    &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
                )
                .unwrap()
                .outcome,
                "resolved"
            );
            trigger(&mut game);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["P2"]["cemetery"]
                    .as_array()
                    .is_some_and(|cards| cards.contains(&json!("enemy"))),
                count >= 2
            );
            if count < 2 {
                assert_eq!(state["objects"]["enemy"]["damage"], 2_u8);
            }
        }
    }
}

#[test]
fn filtered_batch_02_bp07_113_checks_printed_effect() {
    for card in ["BP07-113", "BP07-P33"] {
        for count in [0, 2] {
            let mut initial = position(card, "BP01-002", count, false);
            initial["players"]["P1"]["zones"]["field"] =
                json!([{"id":"maisha","card":"count-companion"}]);
            let mut game = maisha_game(&initial);
            let play = game
                .play_internal_card("source", PlayerId { seat: 1 })
                .unwrap();
            finish(&mut game, play);
            let state = game.projection(View::Referee).unwrap();
            assert!(
                state["P2"]["cemetery"]
                    .as_array()
                    .is_some_and(|cards| cards.contains(&json!("enemy"))),
                "the printed destruction moves the enemy"
            );
            assert_eq!(
                state["objects"]["maisha"]["power"],
                2_u64 + u64::try_from(count).unwrap()
            );
        }
    }
}

#[test]
fn filtered_batch_02_bp09_104_checks_printed_effect() {
    for card in ["BP09-104", "BP09-SL20", "BP09-U07"] {
        for (count, amulet_count) in [(4_usize, 4_usize), (5, 4), (4, 5), (5, 5)] {
            let mut initial = position(card, "BP01-015", count, false);
            initial["players"]["P1"]["zones"]["hand"] = json!([]);
            initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP09-103"}]);
            initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":card}]);
            for i in 0..amulet_count {
                initial["players"]["P1"]["zones"]["cemetery"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!({"id":format!("amulet-{i}"),"card":"BP01-016"}));
            }
            let mut game = rules_game(&initial);
            assert_eq!(
                game.internal_rule_action(
                    &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
                )
                .unwrap()
                .outcome,
                "resolved"
            );
            trigger(&mut game);
            game.resume_internal_card().unwrap();
            trigger(&mut game);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["P1"]["leader"]["life"],
                if count >= 5 { 15_u8 } else { 10_u8 }
            );
            assert_eq!(
                state["objects"]["enemy"]["damage"],
                if amulet_count >= 5 { 5_u8 } else { 0_u8 }
            );
            let drawn = usize::from(count >= 5) + usize::from(amulet_count >= 5);
            assert_eq!(state["P1"]["hand"].as_array().unwrap().len(), drawn);
            assert_eq!(state["P1"]["deck"].as_array().unwrap().len(), 3 - drawn);
        }
    }
}

#[test]
fn filtered_batch_02_bp13_032_checks_printed_effect() {
    for card in ["BP13-032", "SD07-019"] {
        for count in [3, 4] {
            let initial = position(card, "BP13-032", count, true);
            let mut game = rules_game(&initial);
            let play = game
                .play_internal_card("source", PlayerId { seat: 1 })
                .unwrap();
            finish(&mut game, play);
            trigger(&mut game);
            game.resume_internal_card().unwrap();
            game.internal_rule_action(&json!({"do":"end-phase"}))
                .unwrap();
            trigger(&mut game);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["P1"]["hand"], json!(["top"]));
            assert_eq!(state["P1"]["deck"].as_array().unwrap().len(), 2_usize);
            assert_eq!(
                state["P1"]["leader"]["life"],
                if count + 1 >= 5 { 11_u8 } else { 10_u8 }
            );
        }
    }
}

#[test]
fn filtered_batch_02_bp15_pr09_checks_printed_effect() {
    for card in ["BP15-PR09", "BP15-PR19", "BP20-T12"] {
        for count in [5, 6] {
            let initial = position(card, "BP01-006", count, false);
            let mut game = rules_game(&initial);
            let play = game
                .play_internal_card("source", PlayerId { seat: 1 })
                .unwrap();
            finish(&mut game, play);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["P2"]["leader"]["life"],
                if count >= 6 { 14_u8 } else { 20_u8 }
            );
        }
    }
}

#[test]
fn filtered_batch_02_bp16_045_checks_printed_effect() {
    for card in ["BP16-045", "BP16-P11", "PR-591"] {
        for count in [4, 5] {
            let initial = position(card, "BP16-045", count, false);
            let mut game = rules_game(&initial);
            let play = game
                .play_internal_card("source", PlayerId { seat: 1 })
                .unwrap();
            finish(&mut game, play);
            let state = game.projection(View::Referee).unwrap();
            if count >= 5 {
                let token = state["P1"]["ex"][0].as_str().unwrap();
                assert_eq!(state["objects"][token]["name"], "成長したですぅ！");
            }
            assert_eq!(state["P1"]["hand"], json!(["top"]));
            assert_eq!(
                state["P1"]["ex"].as_array().unwrap().len(),
                usize::from(count >= 5)
            );
        }
    }
}

#[test]
fn filtered_batch_02_bp19_p34_checks_printed_effect() {
    for count in [4, 5, 9, 10] {
        let mut initial = position("BP19-P34", "BP19-001", count, false);
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] = json!([]);
        let mut game = with_producer(
            &initial,
            Action::Token {
                tokens: vec![TokenSpec {
                    template: TokenTemplateId("多頭の実験体".into()),
                    name: Expr::Literal(CardName("多頭の実験体".into())),
                    count: Expr::Literal(Nat(1)),
                }],
                destination: TokenDestination::Single {
                    zone: Expr::Literal(ZoneRef {
                        player: PlayerRef::Controller {},
                        kind: ZoneKind::Field,
                    }),
                },
                output: Binding::new(501, ScopeId(1)),
                initial_state: None,
                initial_posture: Posture::Standing,
                entry_effects: vec![],
            },
        );
        activate(&mut game, "P1");
        game.resume_internal_card().unwrap();
        trigger(&mut game);
        let state = game.projection(View::Referee).unwrap();
        let token = state["P1"]["field"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(Value::as_str)
            .find(|id| *id != "producer")
            .unwrap();
        assert_eq!(state["objects"][token]["card"], "BP19-P34");
        assert_eq!(
            state["objects"][token]["power"],
            if count >= 5 { 4_u8 } else { 2_u8 }
        );
        assert_eq!(
            state["objects"][token]["hp"],
            if count >= 5 { 4_u8 } else { 2_u8 }
        );
        for (keyword, minimum) in [("突進", 5), ("指定攻撃", 10), ("必殺", 10)] {
            assert_eq!(
                state["objects"][token]["keywords"]
                    .as_array()
                    .unwrap()
                    .contains(&json!(keyword)),
                count >= minimum
            );
        }
    }
}

#[test]
fn filtered_batch_02_cp01_060_checks_printed_effect() {
    for card in ["CP01-060", "CP01-P45", "CSD01-012", "PR-038"] {
        for count in [9, 10] {
            let initial = position(card, "CP01-060", count, false);
            let mut game = rules_game(&initial);
            let play = game
                .play_internal_card("source", PlayerId { seat: 1 })
                .unwrap();
            finish(&mut game, play);
            let state = game.projection(View::Referee).unwrap();
            assert!(
                state["P2"]["cemetery"]
                    .as_array()
                    .is_some_and(|cards| cards.contains(&json!("enemy"))),
                "the printed destruction moves the enemy"
            );
            assert_eq!(
                state["P1"]["pp"]["current"],
                if count >= 10 { 8_u8 } else { 6_u8 }
            );
        }
    }
}

#[test]
fn filtered_batch_02_cp02_033_checks_printed_effect() {
    for card in ["CP02-033", "CP02-P27"] {
        for count in [4, 5] {
            let mut initial = position(card, "CP02-014", count, false);
            initial["players"]["P1"]["zones"]["hand"] = json!([]);
            initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"CP02-032"}]);
            initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":card}]);
            let mut game = rules_game(&initial);
            assert_eq!(
                game.internal_rule_action(
                    &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
                )
                .unwrap()
                .outcome,
                "resolved"
            );
            trigger(&mut game);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["objects"]["enemy"]["damage"],
                if count >= 5 { 3_u8 } else { 2_u8 }
            );
        }
    }
}

#[test]
fn filtered_batch_02_cp02_098_checks_printed_effect() {
    for card in ["CP02-098", "CP02-P80", "CSD02c-017"] {
        for count in [4, 5] {
            let initial = position(card, "CP02-098", count, false);
            let mut game = rules_game(&initial);
            let play = game
                .play_internal_card("source", PlayerId { seat: 1 })
                .unwrap();
            finish(&mut game, play);
            trigger(&mut game);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["P1"]["leader"]["life"],
                if count >= 5 { 14_u8 } else { 12_u8 }
            );
        }
    }
}

#[test]
fn filtered_batch_02_cp03_091_checks_printed_effect() {
    for card in ["CP03-091", "CP03-P60"] {
        for count in [9, 10] {
            let initial = position(card, "CP03-091", count, false);
            let mut game = rules_game(&initial);
            let play = game
                .play_internal_card("source", PlayerId { seat: 1 })
                .unwrap();
            finish(&mut game, play);
            trigger(&mut game);
            game.resume_internal_card().unwrap();
            game.internal_rule_action(&json!({"do":"end-phase"}))
                .unwrap();
            trigger(&mut game);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["P2"]["cemetery"]
                    .as_array()
                    .is_some_and(|cards| cards.contains(&json!("enemy"))),
                count >= 10
            );
            assert_eq!(state["P1"]["leader"]["life"], 13_u8);
            assert_eq!(state["P1"]["deck"].as_array().unwrap().len(), 1_usize);
        }
    }
}

#[test]
fn filtered_batch_02_csd02a_004_checks_printed_effect() {
    for card in ["CSD02a-004", "CSD02a-P01"] {
        for count in [4, 5] {
            let initial = position(card, "CSD02a-004", count, false);
            let mut game = rules_game(&initial);
            let play = game
                .play_internal_card("source", PlayerId { seat: 1 })
                .unwrap();
            finish(&mut game, play);
            trigger(&mut game);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["objects"]["source"]["power"],
                if count >= 5 { 3_u8 } else { 2_u8 }
            );
            assert_eq!(
                state["objects"]["source"]["hp"],
                if count >= 5 { 4_u8 } else { 2_u8 }
            );
        }
    }
}

#[test]
fn filtered_batch_02_csd02c_004_checks_printed_effect() {
    for card in ["CSD02c-004", "CSD02c-P01"] {
        for count in [4, 5] {
            let mut initial = position(card, "CSD02c-004", count, false);
            initial["players"]["P1"]["zones"]["hand"] = json!([]);
            initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":card}]);
            let mut game = rules_game(&initial);
            let play = game
                .activate_internal_card("source", AbilityId(1), PlayerId { seat: 1 })
                .unwrap();
            finish(&mut game, play);
            let state = game.projection(View::Referee).unwrap();
            game.resume_internal_card().unwrap();
            let before_second = game.projection(View::Referee).unwrap();
            game.activate_internal_card("source", AbilityId(1), PlayerId { seat: 1 })
                .unwrap_err();
            assert_eq!(game.projection(View::Referee).unwrap(), before_second);
            assert_eq!(
                state["P1"]["hand"].as_array().unwrap().len(),
                usize::from(count >= 5)
            );
            assert_eq!(state["P1"]["pp"]["current"], 8_u8);
        }
    }
}

#[test]
fn filtered_batch_02_ecp02_032_checks_printed_effect() {
    for card in ["ECP02-032", "ECP02-P16"] {
        for count in [4, 5] {
            let initial = position(card, "ECP02-032", count, false);
            let mut game = rules_game(&initial);
            let play = game
                .play_internal_card("source", PlayerId { seat: 1 })
                .unwrap();
            finish(&mut game, play);
            trigger(&mut game);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["P1"]["leader"]["life"],
                if count >= 5 { 12_u8 } else { 10_u8 }
            );
        }
    }
}

#[test]
fn filtered_batch_02_ecp02_044_checks_printed_effect() {
    for card in ["ECP02-044", "ECP02-P23"] {
        for count in [4, 5] {
            let mut initial = position(card, "ECP02-044", count, false);
            initial["players"]["P1"]["zones"]["hand"] = json!([]);
            initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":card}]);
            let mut game = rules_game(&initial);
            game.internal_rule_action(&json!({"do":"end-phase"}))
                .unwrap();
            trigger(&mut game);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["objects"]["source"]["hp"],
                if count >= 5 { 4_u8 } else { 2_u8 }
            );
        }
    }
}
