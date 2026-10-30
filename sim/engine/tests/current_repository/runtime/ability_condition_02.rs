#![expect(
    clippy::panic,
    reason = "Unexpected questions invalidate the printed scenarios."
)]
use super::*;

fn batch_selection() -> &'static SelectedEffects {
    static SELECTED: OnceLock<SelectedEffects> = OnceLock::new();
    SELECTED.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        let prints = snapshot.lines().map(|line| serde_json::from_str::<Value>(line).unwrap()).collect::<Vec<_>>();
        for (number, base, name, kind, traits) in [
            ("gate-machine", "BP01-037", "machine material", "フォロワー", vec!["機械"]),
            ("gate-sea", "BP01-037", "sea material", "フォロワー", vec!["海洋"]),
            ("gate-academy", "BP01-037", "academy material", "フォロワー", vec!["学院"]),
            ("gate-commander", "BP01-037", "commander material", "アミュレット", vec!["指揮官"]),
            ("gate-amulet", "BP01-037", "amulet material", "アミュレット", vec![]),
            ("gate-normal", "BP01-037", "ordinary material", "フォロワー", vec![]),
            ("gate-god", "BP01-037", "ゴッド・オブ・カース", "フォロワー", vec![]),
            ("gate-lumiore", "BP01-037", "金色の威信・リュミオール", "フォロワー", vec![]),
            ("form-dealer", "BP17-028", "ヴァローナディーラー", "フォロワー・エボルヴ", vec![]),
            ("form-dance", "BP22-061", "黒白の乱舞・ノール＆ブラン", "フォロワー・エボルヴ", vec![]),
        ] {
            let mut face = prints.iter().find(|entry| entry["number"] == base).unwrap()["faces"][0].clone();
            face["name"] = json!(name);
            face["card_type"] = json!(kind);
            face["traits"] = json!(traits);
            face["text"] = Value::Null;
            face["sections"] = json!([]);
            if base == "BP01-037" {
                face["hp"] = json!("10");
                face["power"] = json!("5");
            }
            if kind == "アミュレット" {
                face["hp"] = json!("-");
                face["power"] = json!("-");
            }
            snapshot.push_str(&json!({"number":number,"faces":[face]}).to_string());
            snapshot.push('\n');
            input["documents"].as_array_mut().unwrap().push(json!([
                format!("{number}.yaml"),
                format!("format: 1\nkind: effect_set\ncards:\n  {number}:\n    status: complete\n    abilities: []\n")
            ]));
        }
        Catalog::from_current_selection(&snapshot, &keywords, &input).unwrap()
    })
}

fn batch_game(value: &Value) -> Game {
    let mut game = Game::from_selected_effects(
        batch_selection().clone(),
        value,
        &Value::Null,
        &Value::Null,
        "ability-gates",
    )
    .unwrap();
    let empty = WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![],
    })
    .unwrap();
    for number in [
        "gate-machine",
        "gate-sea",
        "gate-academy",
        "gate-commander",
        "gate-amulet",
        "gate-normal",
        "gate-god",
        "gate-lumiore",
        "form-dealer",
        "form-dance",
    ] {
        game.load_internal_card(number, &empty).unwrap();
    }
    game
}

fn position(card: &str) -> Value {
    let mut value = draw_position();
    value["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":card}]);
    value["players"]["P1"]["zones"]["deck"] = json!([{"id":"drawn","card":"gate-normal"}]);
    value["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"gate-normal"}]);
    value
}

fn material(value: &mut Value, zone: &str, card: &str, count: usize) {
    let cards = (0..count).map(|i| json!({"id":format!("material-{i}"),"card":card}));
    if let Some(existing) = value["players"]["P1"]["zones"][zone].as_array_mut() {
        existing.extend(cards);
    } else {
        value["players"]["P1"]["zones"][zone] = json!(cards.collect::<Vec<_>>());
    }
}

fn finish(game: &mut Game, mut play: CardPlay) {
    while let CardPlay::Awaiting(question) = play {
        let InputDomain::Targets(domain) = &question.domain else {
            panic!("formal target")
        };
        play = game
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Targets(vec![domain.members[0].handle]),
            )
            .unwrap();
    }
    resolved(play);
}

fn activate(game: &mut Game, card: &str, ordinal: usize, allowed: bool) {
    let id = batch_selection().current[card]
        .program
        .card()
        .abilities
        .iter()
        .filter(|entry| entry.node.kind == AbilityKind::Activated)
        .nth(ordinal)
        .unwrap()
        .node
        .id;
    let before = format!("{game:?}");
    let result = game.activate_internal_card("source", id, PlayerId { seat: 1 });
    if allowed {
        finish(game, result.unwrap());
    } else {
        assert!(
            matches!(result, Err(EngineFailure::IllegalPlay(_))),
            "the printed condition prevents use"
        );
        assert!(
            format!("{game:?}") == before,
            "refusal creates no choices and pays no costs"
        );
    }
}

fn source_paid(state: &Value) {
    assert!(
        state["P1"]["cemetery"]
            .as_array()
            .unwrap()
            .iter()
            .any(|object| object == "source"),
        "the source pays its printed cemetery cost"
    );
}

#[test]
fn bp07_101_requires_two_machine_followers_and_creates_repair_in_ex() {
    for card in ["BP07-101", "BP07-P30"] {
        for count in [1_usize, 2] {
            let mut value = position(card);
            material(&mut value, "field", "gate-machine", count);
            let mut game = batch_game(&value);
            activate(&mut game, card, 0, count == 2);
            if count == 2 {
                let state = game.projection(View::Referee).unwrap();
                source_paid(&state);
                assert_eq!(state["P1"]["ex"].as_array().unwrap().len(), 1);
                let token = state["P1"]["ex"][0].as_str().unwrap();
                assert_eq!(state["objects"][token]["name"], "リペアモード");
            }
        }
    }
}

#[test]
fn bp15_105_requires_three_cemetery_amulets_before_paying_itself() {
    for count in [2_usize, 3] {
        let mut value = position("BP15-105");
        material(&mut value, "cemetery", "gate-amulet", count);
        let mut game = batch_game(&value);
        activate(&mut game, "BP15-105", 0, count == 3);
        if count == 3 {
            let state = game.projection(View::Referee).unwrap();
            source_paid(&state);
            assert_eq!(state["objects"]["enemy"]["damage"], 2_u8);
            assert_eq!(state["P1"]["cemetery"].as_array().unwrap().len(), 4);
        }
    }
}

#[test]
fn bp16_064_sea_follower_unlocks_the_second_ability_and_its_one_pp_payment() {
    for present in [false, true] {
        let mut value = position("BP16-064");
        if present {
            material(&mut value, "field", "gate-sea", 1);
        }
        let mut game = batch_game(&value);
        activate(&mut game, "BP16-064", 1, present);
        if present {
            let state = game.projection(View::Referee).unwrap();
            source_paid(&state);
            assert_eq!(state["P1"]["pp"]["current"], 9_u8);
            assert_eq!(state["objects"]["enemy"]["damage"], 4_u8);
        }
    }
}

fn evolve(game: &mut Game, card: &str, discounted: bool, cost: u8, allowed: bool) {
    let id = batch_selection().current[card]
        .program
        .card()
        .abilities
        .iter()
        .filter(|entry| {
            matches!(
                entry.node.source.origin,
                AbilityOrigin::Keyword {
                    keyword: KeywordId::Evolution,
                    ..
                }
            )
        })
        .nth(usize::from(discounted))
        .unwrap()
        .node
        .id;
    let before = game.projection(View::Referee).unwrap();
    let result = game.internal_rule_action(&json!({"do":"evolve","source":"source","evolve_card":"form","definition":id.0,"pay":{"pp":cost}})).unwrap();
    if allowed {
        assert_eq!(
            result.outcome, "resolved",
            "the chosen printed evolution option is usable"
        );
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
            10_u8.checked_sub(cost).unwrap(),
            "the chosen option pays exactly its printed PP cost"
        );
    } else {
        assert_eq!(
            result.outcome, "cannot-evolve",
            "the discounted option is unavailable below its gate"
        );
        assert_eq!(
            result.events,
            Vec::<Value>::new(),
            "a refusal publishes no events"
        );
        assert_eq!(
            game.projection(View::Referee).unwrap(),
            before,
            "the rejected discount spends no PP or evolution card"
        );
    }
}

#[test]
fn bp17_027_eight_hand_cards_unlock_one_pp_while_the_normal_three_pp_option_remains() {
    for count in [7_usize, 8] {
        let mut value = position("BP17-027");
        material(&mut value, "hand", "gate-normal", count);
        value["players"]["P1"]["zones"]["evolve_deck"] =
            json!([{"id":"form","card":"form-dealer"}]);
        evolve(&mut batch_game(&value), "BP17-027", true, 1, count == 8);
        evolve(&mut batch_game(&value), "BP17-027", false, 3, true);
    }
}

#[test]
fn bp17_109_counts_itself_among_three_amulets_and_pays_one_pp_to_heal() {
    for card in ["BP17-109", "BP17-P26"] {
        for others in [1_usize, 2] {
            let mut value = position(card);
            value["players"]["P1"]["leader"]["life"] = json!(10_u8);
            material(&mut value, "field", "gate-amulet", others);
            let mut game = batch_game(&value);
            activate(&mut game, card, 0, others == 2);
            if others == 2 {
                let state = game.projection(View::Referee).unwrap();
                source_paid(&state);
                assert_eq!(state["P1"]["pp"]["current"], 9_u8);
                assert_eq!(state["P1"]["leader"]["life"], 11_u8);
            }
        }
    }
}

#[test]
fn bp17_109_and_alias_fanfare_select_before_replacing_two_damage_with_three() {
    for card in ["BP17-109", "BP17-P26"] {
        for others in [1_usize, 2] {
            let mut value = position(card);
            value["players"]["P1"]["zones"]["field"] = json!([]);
            value["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
            material(&mut value, "field", "gate-amulet", others);
            let mut game = batch_game(&value);
            assert!(matches!(
                game.play_internal_card("source", PlayerId { seat: 1 })
                    .unwrap(),
                CardPlay::Entered(_)
            ));
            let question = game.internal_pending_choice().unwrap().unwrap();
            let InputDomain::Pending(options) = &question.domain else {
                panic!("printed Fanfare")
            };
            assert_eq!(options.len(), 1);
            let play = game
                .answer_internal_card(
                    question.actor,
                    question.token,
                    Answer::Pending(options[0].handle),
                )
                .unwrap();
            assert!(
                matches!(play, CardPlay::Awaiting(_)),
                "the formal target is required at both two and three allied amulets"
            );
            finish(&mut game, play);
            assert_eq!(
                game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
                if others == 2 { 3_u8 } else { 2_u8 }
            );
        }
    }
}

#[test]
fn bp17_p35_cannot_use_its_own_cemetery_payment_to_reach_ten() {
    for card in ["BP17-P35", "BP17-T08"] {
        for count in [9_usize, 10] {
            let mut value = position(card);
            material(&mut value, "cemetery", "gate-normal", count);
            let mut game = batch_game(&value);
            activate(&mut game, card, 0, count == 10);
            if count == 10 {
                let state = game.projection(View::Referee).unwrap();
                assert_eq!(state["P1"]["hand"][0], "drawn");
                assert!(
                    state["P1"]["field"].as_array().unwrap().is_empty(),
                    "the token leaves the field as a cost"
                );
            }
        }
    }
}

#[test]
fn bp21_098_needs_another_academy_follower_and_only_pays_act() {
    for card in ["BP21-098", "BP21-P51"] {
        for present in [false, true] {
            let mut value = position(card);
            value["players"]["P1"]["leader"]["life"] = json!(10_u8);
            if present {
                material(&mut value, "field", "gate-academy", 1);
            }
            let mut game = batch_game(&value);
            activate(&mut game, card, 0, present);
            if present {
                let state = game.projection(View::Referee).unwrap();
                assert_eq!(state["P1"]["leader"]["life"], 11_u8);
                assert_eq!(state["objects"]["source"]["acted"], true);
                assert_eq!(state["P1"]["pp"]["current"], 10_u8);
            }
        }
    }
}

#[test]
fn bp21_114_requires_two_academy_followers_excluding_its_amulet_self() {
    for card in ["BP21-114", "PR-582"] {
        for count in [1_usize, 2] {
            let mut value = position(card);
            value["players"]["P1"]["leader"]["life"] = json!(10_u8);
            material(&mut value, "field", "gate-academy", count);
            let mut game = batch_game(&value);
            activate(&mut game, card, 0, count == 2);
            if count == 2 {
                let state = game.projection(View::Referee).unwrap();
                source_paid(&state);
                assert_eq!(state["P1"]["leader"]["life"], 11_u8);
            }
        }
    }
}

#[test]
fn bp22_029_commander_amulet_satisfies_the_card_gate_and_unlocks_one_damage() {
    for card in ["BP22-029", "BP22-P14"] {
        for present in [false, true] {
            let mut value = position(card);
            if present {
                material(&mut value, "field", "gate-commander", 1);
            }
            let mut game = batch_game(&value);
            activate(&mut game, card, 0, present);
            if present {
                let state = game.projection(View::Referee).unwrap();
                assert_eq!(state["objects"]["enemy"]["damage"], 1_u8);
                assert_eq!(state["objects"]["source"]["acted"], true);
            }
        }
    }
}

#[test]
fn bp22_060_named_lumiore_unlocks_one_pp_without_removing_the_normal_four_pp_option() {
    for card in ["BP22-060", "PR-602"] {
        for present in [false, true] {
            let mut value = position(card);
            if present {
                material(&mut value, "field", "gate-lumiore", 1);
            }
            value["players"]["P1"]["zones"]["evolve_deck"] =
                json!([{"id":"form","card":"form-dance"}]);
            evolve(&mut batch_game(&value), card, true, 1, present);
            evolve(&mut batch_game(&value), card, false, 4, true);
        }
    }
}

#[test]
fn bp22_098_named_god_unlocks_the_debuff_on_all_opposing_followers() {
    for card in ["BP22-098", "BP22-P53"] {
        for present in [false, true] {
            let mut value = position(card);
            value["players"]["P2"]["zones"]["field"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":"enemy-two","card":"gate-normal"}));
            if present {
                material(&mut value, "field", "gate-god", 1);
            }
            let mut game = batch_game(&value);
            activate(&mut game, card, 0, present);
            if present {
                let state = game.projection(View::Referee).unwrap();
                source_paid(&state);
                for id in ["enemy", "enemy-two"] {
                    assert_eq!(state["objects"][id]["power"], 3_u8);
                    assert_eq!(state["objects"][id]["hp"], 8_u8);
                }
                assert_eq!(
                    state["objects"]["material-0"]["power"], 5_u8,
                    "the allied namesake is untouched"
                );
            }
        }
    }
}
