#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate the printed scenario."
)]
use super::*;
use alloc::collections::BTreeMap;
use std::{fs::read_to_string, path::PathBuf};
use sve_engine::catalog::EngineIdentityInput;

fn prepared() -> &'static (SelectedEffects, BTreeMap<String, WellFormedCardProgram>) {
    static READY: OnceLock<(SelectedEffects, BTreeMap<String, WellFormedCardProgram>)> =
        OnceLock::new();
    READY.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        let mut programs = BTreeMap::new();
        for (number, name, class, kind) in [
            (
                "class-base",
                "セラフィックレオ・ガルエル",
                "ニュートラル",
                "フォロワー",
            ),
            ("class-elf", "外部エルフ", "エルフ", "フォロワー"),
            ("class-royal", "外部ロイヤル", "ロイヤル", "フォロワー"),
            (
                "class-nightmare",
                "外部ナイトメア",
                "ナイトメア",
                "フォロワー",
            ),
            ("class-dragon", "外部ドラゴン", "ドラゴン", "フォロワー"),
            ("class-amulet", "外部エルフ護符", "エルフ", "アミュレット"),
        ] {
            let stat = if kind == "アミュレット" {
                "-"
            } else {
                "1"
            };
            let companion = json!({
                "number": number,
                "faces": [{
                    "name": name, "card_class": class, "card_type": kind,
                    "traits": [], "cost": "0", "power": stat, "hp": stat,
                    "text": null, "sections": []
                }]
            });
            snapshot.push_str(&companion.to_string());
            snapshot.push('\n');
            let document = json!({
                "format": 1_u8, "kind": "effect_set",
                "cards": {number: {"status": "complete", "abilities": []}}
            });
            input["documents"]
                .as_array_mut()
                .unwrap()
                .push(json!([format!("{number}.yaml"), document.to_string()]));
            let program = if number == "class-base" {
                // The real evolved programme remains intact while its base is not admitted.
                let mut tree = expanded::tree().clone();
                tree["cards"] = json!({number: tree["cards"]["synthetic-evolution"]});
                lowerer().lower(&tree).unwrap().remove(number).unwrap()
            } else {
                WellFormedCardProgram::check(CardProgram {
                    abilities: vec![],
                    card_play_costs: vec![],
                })
                .unwrap()
            };
            programs.insert(number.to_owned(), program);
        }
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../authored/rules/engine/index.yaml");
        let mut rules: Value = serde_saphyr::from_str(&read_to_string(path).unwrap()).unwrap();
        rules["input"] = rule_fixtures::rules(&snapshot, json!([]), json!([]))["input"].clone();
        let selected = Catalog::from_current_selection_with_rules(
            &snapshot,
            &keywords,
            &input,
            &rules.to_string(),
            &EngineIdentityInput::LegacyJp,
        )
        .unwrap();
        (selected, programs)
    })
}
fn fixture(initial: &Value) -> Game {
    let (selected, programs) = prepared();
    let mut game = Game::from_selected_effects(
        selected.clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "class-batch",
    )
    .unwrap();
    for (number, program) in programs {
        game.load_internal_card(number, program).unwrap();
    }
    game
}
fn trigger(game: &mut Game, card: &str) -> CardPlay {
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = pending.domain else {
        panic!("trigger")
    };
    let option = options.iter().find(|option| option.card == card).unwrap();
    game.answer_internal_card(pending.actor, pending.token, Answer::Pending(option.handle))
        .unwrap()
}
fn choose(game: &mut Game, play: CardPlay, card: &str) -> CardPlay {
    let CardPlay::Awaiting(question) = play else {
        panic!("target")
    };
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("target domain")
    };
    let selected = domain
        .members
        .iter()
        .find(|member| member.card.as_deref() == Some(card))
        .unwrap();
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::Targets(vec![selected.handle]),
    )
    .unwrap()
}
fn enter(game: &mut Game, card: &str) -> CardPlay {
    assert!(
        matches!(
            game.play_internal_card("source", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ),
        "the printed source enters"
    );
    trigger(game, card)
}
fn keyword(state: &Value, id: &str, name: &str) -> bool {
    state["objects"][id]["keywords"]
        .as_array()
        .unwrap()
        .contains(&json!(name))
}
#[test]
fn bp03_113_evolution_bounces_only_another_neutral_ally() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"class-base"},{"id":"neutral","card":"BP04-117"},{"id":"royal","card":"class-royal"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP03-113"}]);
    let mut game = fixture(&initial);
    game.internal_rule_action(
        &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}}),
    )
    .unwrap();
    let play = trigger(&mut game, "BP03-113");
    if let CardPlay::Awaiting(question) = &play {
        let InputDomain::Targets(domain) = &question.domain else {
            panic!("target")
        };
        assert_eq!(domain.members.len(), 1);
    } else {
        panic!("target")
    }
    resolved(choose(&mut game, play, "BP04-117"));
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["neutral"]));
    assert_eq!(state["P1"]["field"], json!(["source", "royal"]));
}
fn amulet_activation(number: &str) {
    for prior in [0_usize, 3] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["field"] =
            json!([{"id":"source","card":number},{"id":"elf","card":"class-elf"}]);
        initial["players"]["P1"]["zones"]["hand"] = json!(
            (0..prior)
                .map(|n| json!({"id":format!("prior{n}"),"card":"class-royal"}))
                .collect::<Vec<_>>()
        );
        let mut game = fixture(&initial);
        for n in 0..prior {
            assert!(
                matches!(
                    game.play_internal_card(&format!("prior{n}"), PlayerId { seat: 1 })
                        .unwrap(),
                    CardPlay::Entered(_)
                ),
                "the external zero-cost cards establish real Combo history"
            );
            game.resume_internal_card().unwrap();
        }
        let activated = selection().current[number]
            .program
            .card()
            .abilities
            .iter()
            .find(|ability| ability.node.kind == AbilityKind::Activated)
            .unwrap()
            .node
            .id;
        let activation = game
            .activate_internal_card("source", activated, PlayerId { seat: 1 })
            .unwrap();
        resolved(choose(&mut game, activation, "class-elf"));
        game.resume_internal_card().unwrap();
        let departure = trigger(&mut game, number);
        resolved(departure);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P1"]["cemetery"],
            json!(["source"]),
            "the source is sacrificed as a cost"
        );
        assert_eq!(state["P1"]["pp"]["current"], 9_u8, "one PP is paid");
        assert_eq!(
            state["objects"]["elf"]["hp"], 2_u8,
            "the Elf gains one life"
        );
        assert_eq!(
            state["objects"]["elf"]["power"],
            if prior == 3 { 2_u8 } else { 1_u8 },
            "only Combo three adds power"
        );
        let ex = state["P1"]["ex"].as_array().unwrap();
        assert_eq!(ex.len(), 1, "departure creates exactly one token in EX");
        assert_eq!(
            state["objects"][ex[0].as_str().unwrap()]["card"],
            "BP01-T02",
            "the created token is a Wisp"
        );
    }
}
#[test]
fn bp04_008_pays_acts_and_leaves_to_buff_an_elf_then_creates_wisp() {
    amulet_activation("BP04-008");
}
#[test]
fn bp04_009_pays_acts_and_leaves_to_buff_an_elf_then_creates_wisp() {
    amulet_activation("BP04-009");
}
#[test]
fn bp14_087_grants_storm_to_nightmare_only_when_the_play_leaves_no_hand() {
    for keep in [false, true] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["hand"] = if keep {
            json!([{"id":"source","card":"BP14-087"},{"id":"kept","card":"class-royal"}])
        } else {
            json!([{"id":"source","card":"BP14-087"}])
        };
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"nightmare","card":"class-nightmare"},{"id":"royal","card":"class-royal"}]);
        let mut game = fixture(&initial);
        let play = game
            .play_internal_card("source", PlayerId { seat: 1 })
            .unwrap();
        resolved(choose(&mut game, play, "class-nightmare"));
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["nightmare"]["power"], 2_u8);
        assert_eq!(state["objects"]["royal"]["power"], 1_u8);
        assert_eq!(keyword(&state, "nightmare", "疾走"), !keep);
    }
}
#[test]
fn bp16_071_always_declares_dragon_target_but_grants_storm_only_at_maximum_ten_pp() {
    for maximum in [9_u8, 10] {
        let mut initial = draw_position();
        initial["players"]["P1"]["pp"] = json!({"current":maximum,"max":maximum});
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP16-071"}]);
        initial["players"]["P1"]["zones"]["field"] =
            json!([{"id":"dragon","card":"class-dragon"},{"id":"royal","card":"class-royal"}]);
        let mut game = fixture(&initial);
        let play = enter(&mut game, "BP16-071");
        resolved(choose(&mut game, play, "class-dragon"));
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(keyword(&state, "dragon", "疾走"), maximum == 10);
        assert!(!keyword(&state, "royal", "疾走"));
    }
}
#[test]
fn bp18_019_returns_an_elf_amulet_and_damages_only_its_other_formal_target() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP18-019"}]);
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"amulet","card":"class-amulet"},{"id":"royal","card":"class-royal"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP04-117"}]);
    let mut game = fixture(&initial);
    let play = game
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap();
    let play = choose(&mut game, play, "class-amulet");
    resolved(choose(&mut game, play, "BP04-117"));
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["amulet"]));
    assert_eq!(state["P1"]["field"], json!(["royal"]));
    assert_eq!(state["objects"]["enemy"]["damage"], 3_u8);
}
