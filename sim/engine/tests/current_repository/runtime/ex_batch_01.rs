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
            Answer::CostMaterials(
                domain
                    .members
                    .iter()
                    .take(usize::try_from(domain.cardinality.minimum.0).unwrap())
                    .map(|m| m.handle)
                    .collect(),
            )
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
        let (mut snapshot,keywords,mut input)=inputs().clone();
        let base=snapshot.lines().map(|line|serde_json::from_str::<Value>(line).unwrap()).find(|card|card["number"]=="BP01-037").unwrap()["faces"][0].clone();
        for (number,name,kind,traits) in [
            ("count-ghost","counter-ゴースト","フォロワー",json!([])),
            ("count-ghost-spell","counter-ゴースト","スペル",json!([])),
            ("count-vehicle","counter-vehicle","アミュレット",json!(["乗物"])),
            ("count-celina","メルティングシュガー・セリーナ","フォロワー",json!([])),
        ] {
            let mut face=base.clone();face["name"]=json!(name);face["card_type"]=json!(kind);face["card_class"]=json!("ニュートラル");face["traits"]=traits;face["text"]=Value::Null;face["sections"]=json!([]);face["cost"]=json!("1");face["power"]=json!(if kind=="フォロワー"{"1"}else{"-"});face["hp"]=json!(if kind=="フォロワー"{"3"}else{"-"});
            snapshot.push_str(&json!({"number":number,"faces":[face]}).to_string());snapshot.push('\n');
            input["documents"].as_array_mut().unwrap().push(json!([format!("{number}.yaml"),format!("format: 1\nkind: effect_set\ncards:\n  {number}:\n    status: complete\n    abilities: []\n")]));
        }
        Catalog::from_current_selection(&snapshot,&keywords,&input).unwrap()
    });
    let mut game = Game::from_selected_effects(
        selected.clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "ex-materials",
    )
    .unwrap();
    for number in [
        "count-ghost",
        "count-ghost-spell",
        "count-vehicle",
        "count-celina",
    ] {
        game.load_internal_card(
            number,
            &WellFormedCardProgram::check(CardProgram {
                abilities: vec![],
                card_play_costs: vec![],
            })
            .unwrap(),
        )
        .unwrap();
    }
    game
}

#[test]
fn ex_batch_01_bp07_094_checks_printed_effect() {
    for card in ["BP07-094", "BP07-P27"] {
        for count in [1_usize, 2] {
            let mut initial = ex_position(card, "BP07-T02", count);
            initial["players"]["P1"]["zones"]["hand"] = json!([]);
            initial["players"]["P1"]["zones"]["field"] =
                json!([{"id":"source","card":card,"state":{"entered_this_turn":true}}]);
            let mut game = batch_game(&initial);
            assert_eq!(
                game.internal_rule_action(
                    &json!({"do":"attack","attacker":"source","target":"P2.leader"})
                )
                .unwrap()
                .outcome,
                "resolved"
            );
            trigger(&mut game);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["P1"]["ex"].as_array().unwrap().len(), count + 1);
            assert_eq!(
                state["objects"]["source"]["power"],
                1_u64 + u64::from(count >= 2)
            );
            let id = state["P1"]["ex"][count].as_str().unwrap();
            assert_eq!(state["objects"][id]["name"], "リペアモード");
        }
    }
}

#[test]
fn ex_batch_01_bp07_095_checks_printed_effect() {
    for card in ["BP07-095", "BP07-P28"] {
        for count in [0_usize, 1] {
            let mut initial = ex_position(card, "BP07-T02", count);
            initial["players"]["P1"]["zones"]["hand"] = json!([]);
            initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP07-094"}]);
            initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":card}]);
            let mut game = batch_game(&initial);
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
            assert_eq!(state["P1"]["pp"]["current"], 9_u8);
            assert_eq!(state["P1"]["ex"].as_array().unwrap().len(), count + 1);
            let id = state["P1"]["ex"][count].as_str().unwrap();
            assert_eq!(state["objects"][id]["name"], "リペアモード");
            assert!(
                state["objects"]["source"]["keywords"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("疾走"))
            );
            game.resume_internal_card().unwrap();
            assert_eq!(
                game.internal_rule_action(
                    &json!({"do":"attack","attacker":"source","target":"P2.leader"})
                )
                .unwrap()
                .outcome,
                "resolved"
            );
            trigger(&mut game);
            let after_attack = game.projection(View::Referee).unwrap();
            assert_eq!(
                after_attack["P1"]["ex"].as_array().unwrap().len(),
                count + 2
            );
            assert_eq!(
                after_attack["objects"]["source"]["power"],
                2_u64 + u64::from(count == 1)
            );
        }
    }
}

#[test]
fn ex_batch_01_bp10_091_checks_printed_effect() {
    for card in ["BP10-091", "BP10-P20"] {
        for present in [false, true] {
            let mut initial = ex_position(card, "count-ghost-spell", 1);
            if present {
                initial["players"]["P1"]["zones"]["ex"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!({"id":"matching","card":"count-ghost"}));
            }
            initial["players"]["P2"]["zones"]["ex"][0]["card"] = json!("count-ghost");
            let mut game = batch_game(&initial);
            let play = game
                .play_internal_card("source", PlayerId { seat: 1 })
                .unwrap();
            finish(&mut game, play);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["objects"]["enemy"]["damage"], 4_u8);
            assert_eq!(state["P2"]["leader"]["life"], 18_u8);
            assert_eq!(
                state["P1"]["pp"]["current"],
                if present { 9_u8 } else { 6_u8 }
            );
        }
    }
}

#[test]
fn ex_batch_01_bp11_087_checks_printed_effect() {
    for card in ["BP11-087", "BP11-SL17"] {
        for present in [false, true] {
            let initial = ex_position(card, "BP11-087", usize::from(present));
            let mut game = batch_game(&initial);
            play_and_trigger(&mut game, 1);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["objects"]["enemy"]["damage"], 3_u8);
            assert_eq!(
                state["P2"]["leader"]["life"],
                20_u64 - 2_u64 * u64::from(present)
            );
        }
    }
}

#[test]
fn ex_batch_01_bp11_088_checks_printed_effect() {
    for card in ["BP11-088", "BP11-SL18"] {
        for present in [false, true] {
            let mut initial = ex_position(card, "BP11-087", usize::from(present));
            initial["players"]["P1"]["zones"]["hand"] = json!([]);
            initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP11-087"}]);
            initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":card}]);
            let mut game = batch_game(&initial);
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
            assert_eq!(state["objects"]["enemy"]["damage"], 3_u8);
            assert_eq!(
                state["P2"]["leader"]["life"],
                20_u64 - 2_u64 * u64::from(present)
            );
        }
    }
}

#[test]
fn ex_batch_01_bp11_089_checks_printed_effect() {
    for card in ["BP11-089", "BP11-P21"] {
        for count in [0_usize, 1] {
            let mut initial = ex_position(card, "BP01-016", count);
            initial["players"]["P1"]["zones"]["field"] = json!([{"id":"amulet","card":"BP01-016"}]);
            let mut game = batch_game(&initial);
            play_and_trigger(&mut game, 1);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["P2"]["leader"]["life"],
                20_u64 - 2_u64 * u64::from(count == 1)
            );
        }
    }
}

#[test]
fn ex_batch_01_bp11_096_checks_printed_effect() {
    for card in ["BP11-096", "BP11-P23"] {
        for present in [false, true] {
            for discount in [false, true] {
                let mut initial = ex_position(card, "BP11-087", usize::from(present));
                if discount {
                    initial["players"]["P1"]["zones"]["field"] =
                        json!([{"id":"celina","card":"count-celina"}]);
                }
                let mut game = batch_game(&initial);
                let play = game
                    .play_internal_card("source", PlayerId { seat: 1 })
                    .unwrap();
                finish(&mut game, play);
                let state = game.projection(View::Referee).unwrap();
                assert_eq!(state["objects"]["enemy"]["damage"], 4_u8);
                assert_eq!(state["P2"]["leader"]["life"], 20_u64 - u64::from(present));
                assert_eq!(state["P1"]["pp"]["current"], 8_u64 + u64::from(discount));
            }
        }
    }
}

#[test]
fn ex_batch_01_bp11_109_checks_printed_effect() {
    for card in ["BP11-109", "BP11-P25", "EBD03-017"] {
        for zone in ["none", "field", "ex"] {
            let mut initial = ex_position(card, "count-vehicle", 0);
            if zone != "none" {
                initial["players"]["P1"]["zones"][zone] =
                    json!([{"id":"vehicle","card":"count-vehicle"}]);
            }
            let mut game = batch_game(&initial);
            play_and_trigger(&mut game, 1);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["P1"]["leader"]["life"],
                10_u64 + 2_u64 * u64::from(zone != "none")
            );
        }
    }
}

#[test]
fn ex_batch_01_bp13_084_checks_printed_effect() {
    for card in ["BP13-084"] {
        for present in [false, true] {
            let mut initial = ex_position(card, "count-ghost-spell", 1);
            if present {
                initial["players"]["P1"]["zones"]["ex"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!({"id":"matching","card":"count-ghost"}));
            }
            initial["players"]["P2"]["zones"]["ex"][0]["card"] = json!("count-ghost");
            let mut game = batch_game(&initial);
            let play = game
                .play_internal_card("source", PlayerId { seat: 1 })
                .unwrap();
            finish(&mut game, play);
            trigger(&mut game);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["P1"]["hand"].as_array().unwrap().len(),
                usize::from(present)
            );
        }
    }
}

#[test]
fn ex_batch_01_bp14_010_checks_printed_effect() {
    for card in ["BP14-010", "PR-370"] {
        for count in [1_usize, 2] {
            let mut game = batch_game(&ex_position(card, "BP08-T09", count));
            play_and_trigger(&mut game, 1);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["P1"]["leader"]["life"],
                10_u64 + u64::from(count == 2)
            );
            assert_eq!(state["P1"]["ex"].as_array().unwrap().len(), count + 1);
            let id = state["P1"]["ex"][count].as_str().unwrap();
            assert_eq!(state["objects"][id]["name"], "操り人形");
        }
    }
}

#[test]
fn ex_batch_01_bp17_009_checks_printed_effect() {
    for card in ["BP17-009", "BP17-P02"] {
        for count in [1_usize, 2] {
            let mut game = batch_game(&ex_position(card, "BP07-T03", count));
            play_and_trigger(&mut game, 1);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["P1"]["leader"]["life"],
                10_u64 + u64::from(count == 2)
            );
            assert_eq!(state["P1"]["ex"].as_array().unwrap().len(), count + 1);
            let id = state["P1"]["ex"][count].as_str().unwrap();
            assert_eq!(state["objects"][id]["name"], "ナテラの大樹");
        }
    }
}
