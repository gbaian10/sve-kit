#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate printed card scenarios."
)]
use super::*;
use sve_engine::game::internal::SearchAnswer;

fn finish_mode(game: &mut Game, mut play: CardPlay, mode: u64) {
    while let CardPlay::Awaiting(question) = play {
        let answer = if matches!(&question.domain, InputDomain::WardEntry { .. }) {
            Answer::WardEntry(false)
        } else if matches!(&question.domain, InputDomain::OptionalCost) {
            Answer::OptionalCost(true)
        } else if let InputDomain::CostMaterials(domain) = &question.domain {
            Answer::CostMaterials(
                domain
                    .members
                    .iter()
                    .take(usize::try_from(domain.cardinality.minimum.0).unwrap())
                    .map(|m| m.handle)
                    .collect(),
            )
        } else if let InputDomain::Search(domain) = &question.domain {
            Answer::Search(SearchAnswer::Ungrouped(
                domain
                    .cards
                    .iter()
                    .find(|card| card.card == "BP01-016")
                    .or_else(|| domain.cards.last())
                    .into_iter()
                    .map(|c| c.handle)
                    .collect(),
            ))
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
                        domain
                            .cardinality
                            .maximum
                            .map_or(1_usize, |n| usize::try_from(n.0).unwrap()),
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

fn trigger_mode(game: &mut Game, mode: u64) {
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
    finish_mode(game, play, mode);
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
fn trigger(game: &mut Game) {
    trigger_mode(game, 1);
}
fn ex_position(card: &str, material: &str, count: usize) -> Value {
    let mut initial = position(card, material, 0, false);
    initial["players"]["P1"]["zones"]["ex"] = json!(
        (0..count)
            .map(|i| json!({"id":format!("own-ex-{i}"),"card":material}))
            .collect::<Vec<_>>()
    );
    initial["players"]["P2"]["zones"]["ex"] = json!([{"id":"opponent-ex","card":material}]);
    initial
}
fn play_and_trigger(game: &mut Game, mode: u64) {
    let play = game
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap();
    finish(game, play);
    trigger_mode(game, mode);
}
fn batch_game(initial: &Value) -> Game {
    use std::sync::OnceLock;
    static SELECTED: OnceLock<SelectedEffects> = OnceLock::new();
    let selected = SELECTED.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        let mut face = snapshot.lines().map(|line| serde_json::from_str::<Value>(line).unwrap())
            .find(|card| card["number"] == "BP01-037").unwrap()["faces"][0].clone();
        face["name"] = json!("counter-fairy");
        face["card_type"] = json!("フォロワー");
        face["card_class"] = json!("ニュートラル");
        face["traits"] = json!(["妖精"]);
        face["text"] = Value::Null;
        face["sections"] = json!([]);
        face["cost"] = json!("1");
        face["power"] = json!("1");
        face["hp"] = json!("3");
        snapshot.push_str(&json!({"number":"count-fairy","faces":[face]}).to_string());
        snapshot.push('\n');
        input["documents"].as_array_mut().unwrap().push(json!([
            "count-fairy.yaml",
            "format: 1\nkind: effect_set\ncards:\n  count-fairy:\n    status: complete\n    abilities: []\n"
        ]));
        Catalog::from_current_selection(&snapshot, &keywords, &input).unwrap()
    });
    let mut game = Game::from_selected_effects(
        selected.clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "ex-fairy-material",
    )
    .unwrap();
    game.load_internal_card(
        "count-fairy",
        &WellFormedCardProgram::check(CardProgram {
            abilities: vec![],
            card_play_costs: vec![],
        })
        .unwrap(),
    )
    .unwrap();
    game
}

fn evolution_position(card: &str, base: &str, material: &str, count: usize) -> Value {
    let mut initial = ex_position(card, material, count);
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":base}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":card}]);
    initial
}
fn evolve_and_trigger(game: &mut Game) {
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
        )
        .unwrap()
        .outcome,
        "resolved",
        "the selected printed evolution must resolve before its trigger"
    );
    trigger(game);
}

#[test]
fn ex_batch_02_bp07_089_checks_printed_effect() {
    for card in ["BP07-089", "PR-337"] {
        for count in [4_usize, 5] {
            let mut initial = ex_position(card, "BP07-T02", count);
            initial["players"]["P1"]["zones"]["hand"] = json!([]);
            initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":card}]);
            initial["players"]["P2"]["zones"]["field"][0]["state"] = json!({"acted":true});
            let mut game = batch_game(&initial);
            assert_eq!(
                game.internal_rule_action(
                    &json!({"do":"attack","attacker":"source","target":"enemy"})
                )
                .unwrap()
                .outcome,
                "resolved"
            );
            trigger(&mut game);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["P1"]["hand"].as_array().unwrap().len(),
                if count == 5 { 2_usize } else { 1_usize }
            );
        }
    }
}

#[test]
fn ex_batch_02_bp07_096_checks_printed_effect() {
    for card in ["BP07-096"] {
        for count in [4_usize, 5] {
            let mut initial = ex_position(card, "BP01-037", count);
            initial["players"]["P1"]["zones"]["deck"][2]["card"] = json!("BP01-016");
            let mut game = batch_game(&initial);
            play_and_trigger(&mut game, 1);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["P1"]["hand"],
                if count == 5 {
                    json!(["last"])
                } else {
                    json!(["top"])
                }
            );
        }
    }
}

#[test]
fn ex_batch_02_bp14_013_checks_printed_effect() {
    for card in ["BP14-013"] {
        for count in [2_usize, 3] {
            let mut game = batch_game(&evolution_position(card, "BP14-012", "BP01-037", count));
            evolve_and_trigger(&mut game);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["objects"]["enemy"]["damage"],
                if count == 3 { 3_u8 } else { 2_u8 }
            );
        }
    }
}

#[test]
fn ex_batch_02_bp15_003_checks_printed_effect() {
    for card in ["BP15-003", "BP15-SL03", "EBD01-004"] {
        for (count, ordinary) in [(2_usize, false), (2, true), (3, false)] {
            let mut initial = evolution_position(card, "BP15-002", "BP01-T03", count);
            if ordinary {
                initial["players"]["P1"]["zones"]["ex"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!({"id":"non-fairy","card":"BP01-037"}));
            }
            let mut game = batch_game(&initial);
            evolve_and_trigger(&mut game);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["P2"]["cemetery"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("enemy")),
                count + usize::from(ordinary) >= 3
            );
            assert_eq!(
                state["P2"]["leader"]["life"],
                20_u64 - 2_u64 * u64::from(count == 3)
            );
        }
    }
}

#[test]
fn ex_batch_02_bp17_065_checks_printed_effect() {
    for card in ["BP17-065"] {
        for from_ex in [false, true] {
            let mut initial = ex_position(card, "BP17-065", 3);
            initial["players"]["P1"]["zones"]["hand"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":"payment","card":"BP17-065"}));
            if from_ex {
                initial["players"]["P1"]["zones"]["hand"]
                    .as_array_mut()
                    .unwrap()
                    .remove(0);
                initial["players"]["P1"]["zones"]["ex"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!({"id":"source","card":card}));
            }
            let mut game = batch_game(&initial);
            play_and_trigger(&mut game, 1);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["P1"]["hand"], json!(["top", "next"]));
            assert!(
                state["P1"]["cemetery"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("payment"))
            );
            assert_eq!(
                state["P1"]["pp"]["current"],
                6_u64 + 3_u64 * u64::from(from_ex)
            );
        }
    }
}

#[test]
fn ex_batch_02_bp17_106_checks_printed_effect() {
    for card in ["BP17-106", "BP17-P25"] {
        for count in [1_usize, 2] {
            let mut game = batch_game(&ex_position(card, "BP07-T02", count));
            play_and_trigger(&mut game, 1);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["P1"]["ex"].as_array().unwrap().len(), count + 1);
            assert_eq!(
                state["P1"]["hand"].as_array().unwrap().len(),
                usize::from(count == 2)
            );
        }
    }
}

#[test]
fn ex_batch_02_bp17_119_checks_printed_effect() {
    for card in ["BP17-119", "BP17-P28"] {
        for from_ex in [false, true] {
            let mut initial = ex_position(card, "BP07-089", 3);
            initial["players"]["P1"]["zones"]["hand"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":"payment","card":"BP07-089"}));
            if from_ex {
                initial["players"]["P1"]["zones"]["hand"]
                    .as_array_mut()
                    .unwrap()
                    .remove(0);
                initial["players"]["P1"]["zones"]["ex"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!({"id":"source","card":card}));
            }
            let mut game = batch_game(&initial);
            play_and_trigger(&mut game, 1);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["P1"]["hand"], json!(["top", "next"]));
            assert!(
                state["P1"]["cemetery"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("payment"))
            );
            assert_eq!(state["P1"]["pp"]["current"], 9_u64);
        }
    }
}

#[test]
fn ex_batch_02_bp12_016_checks_printed_effect() {
    for card in ["BP12-016"] {
        for count in [0_usize, 2] {
            let mut initial = ex_position(card, "BP01-T03", count);
            initial["players"]["P1"]["zones"]["ex"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":"not-token","card":"count-fairy"}));
            let mut game = batch_game(&initial);
            play_and_trigger(&mut game, 1);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["P1"]["ex"].as_array().unwrap().len(), count + 3);
            assert_eq!(state["objects"]["enemy"]["damage"], count + 2);
        }
    }
}

#[test]
fn ex_batch_02_bp21_018_checks_printed_effect() {
    for card in ["BP21-018", "PR-595"] {
        for count in [0_usize, 2] {
            let mut initial = ex_position(card, "BP01-T03", count);
            initial["players"]["P1"]["zones"]["ex"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":"ordinary-ex","card":"count-fairy"}));
            initial["players"]["P1"]["zones"]["field"] =
                json!([{"id":"ordinary-field","card":"count-fairy"}]);
            let mut game = batch_game(&initial);
            play_and_trigger(&mut game, 1);
            game.resume_internal_card().unwrap();
            let activation = game
                .activate_internal_card("source", AbilityId(4), PlayerId { seat: 1 })
                .unwrap();
            finish(&mut game, activation);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["objects"]["enemy"]["damage"], count + 1);
            assert!(
                state["P1"]["cemetery"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("source"))
            );
            assert_eq!(state["P1"]["field"].as_array().unwrap().len(), 2_usize);
        }
    }
}

#[test]
fn ex_batch_02_bp22_008_checks_printed_effect() {
    for card in ["BP22-008", "PR-592"] {
        for count in [0_usize, 3] {
            let mut game = batch_game(&ex_position(card, "BP01-T03", count));
            play_and_trigger(&mut game, 1);
            assert_eq!(
                game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
                count
            );
        }
    }
}

#[test]
fn ex_batch_02_bp22_011_checks_printed_effect() {
    for card in ["BP22-011", "PR-609"] {
        for count in [3_usize, 4] {
            let mut game = batch_game(&ex_position(card, "BP01-T03", count));
            play_and_trigger(&mut game, 1);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["P1"]["ex"].as_array().unwrap().len(), count + 1);
            assert_eq!(
                state["P1"]["leader"]["life"],
                10_u64 + u64::from(count == 4)
            );
            assert_eq!(
                state["P1"]["hand"].as_array().unwrap().len(),
                usize::from(count == 4)
            );
        }
    }
}

#[test]
fn ex_batch_02_bp22_058_checks_printed_effect() {
    for card in ["BP22-058", "BP22-SL16"] {
        for count in [1_usize, 2] {
            let mut game = batch_game(&ex_position(card, "BP22-058", count));
            play_and_trigger(&mut game, 1);
            assert_eq!(
                game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
                6_u64 + 2_u64 * u64::from(count == 2)
            );
        }
    }
}
