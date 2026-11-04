#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate printed card scenarios."
)]
use super::*;
use sve_engine::game::internal::SearchAnswer;

fn fixture() -> &'static SelectedEffects {
    static SELECTED: OnceLock<SelectedEffects> = OnceLock::new();
    SELECTED.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = producer_inputs();
        // Named companions isolate conditions from the other cards' unadmitted abilities.
        for (number, name) in [("ready-daiwa", "ダイワスカーレット"), ("ready-teio", "トウカイテイオー")] {
            snapshot.push_str(&json!({"number":number,"faces":[{
                "name":name,"card_class":"ニュートラル","card_type":"フォロワー",
                "cost":"0","power":"2","hp":"2","traits":["ウマ娘"],"text":null,"sections":[]
            }]}).to_string());
            snapshot.push('\n');
            input["documents"].as_array_mut().unwrap().push(json!([
                format!("{number}.yaml"),json!({"format":1_u8,"kind":"effect_set","cards":{number:{"status":"complete","abilities":[]}}}).to_string()
            ]));
        }
        repository::selection_with_rules_from(&snapshot, &keywords, &input)
    })
}
fn position(card: &str, zone: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"][zone] = json!([{"id":"source","card":card}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"carrot","card":"CSD01-032"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"first","card":"CP01-007"},{"id":"second","card":"BP01-171"},
        {"id":"third","card":"CP01-073"},{"id":"tail","card":"BP01-171"}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    initial
}
fn game(initial: &Value) -> Game {
    let mut game = Game::from_selected_effects(
        fixture().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "ready-cp01",
    )
    .unwrap();
    load_companions(&mut game);
    game
}
fn load_companions(game: &mut Game) {
    let empty = WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![],
    })
    .unwrap();
    for number in ["ready-daiwa", "ready-teio"] {
        game.load_internal_card(number, &empty).unwrap();
    }
}
fn finish(game: &mut Game, mut step: CardPlay, mode: u64) {
    while let CardPlay::Awaiting(question) = step {
        let answer = match &question.domain {
            InputDomain::Modes { .. } => Answer::Modes(vec![ModeId(mode)]),
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            InputDomain::OptionalCost => Answer::OptionalCost(true),
            InputDomain::Targets(domain) => Answer::Targets(
                domain
                    .members
                    .first()
                    .map(|m| vec![m.handle])
                    .unwrap_or_default(),
            ),
            InputDomain::Search(domain) => Answer::Search(SearchAnswer::Ungrouped(
                domain
                    .cards
                    .first()
                    .map(|m| vec![m.handle])
                    .unwrap_or_default(),
            )),
            InputDomain::Order(cards) => {
                Answer::Order(cards.iter().rev().map(|m| m.handle).collect())
            }
            InputDomain::Abilities(_)
            | InputDomain::ResourceEntryPayment { .. }
            | InputDomain::Quick { .. }
            | InputDomain::CostMaterials(_)
            | InputDomain::BeforeDraw
            | InputDomain::Pending(_)
            | InputDomain::TriggerQuota(_)
            | InputDomain::Integer(_)
            | InputDomain::DamageReplacements(_)
            | InputDomain::DriveTrigger { .. }
            | InputDomain::SearchPartition(_)
            | InputDomain::Distribution(_)
            | InputDomain::SearchReplacement
            | InputDomain::TokenCapacity { .. } => {
                panic!("unexpected printed question: {:?}", question.domain)
            }
        };
        step = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
}
fn settle(game: &mut Game) {
    loop {
        if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
        let Some(question) = game.internal_pending_choice().unwrap() else {
            break;
        };
        let InputDomain::Pending(options) = &question.domain else {
            panic!("pending ability");
        };
        let step = game
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap();
        finish(game, step, 1);
    }
}
fn play(game: &mut Game, id: &str, mode: u64) {
    let step = game.play_internal_card(id, PlayerId { seat: 1 }).unwrap();
    finish(game, step, mode);
    settle(game);
}
fn feed(game: &mut Game, card: &str) {
    meal_race::feed(game, card, 0, false);
    settle(game);
}
fn has_keyword(state: &Value, object: &str, keyword: &str) -> bool {
    state["objects"][object]["keywords"]
        .as_array()
        .unwrap()
        .contains(&json!(keyword))
}

#[test]
fn cp01_005_searches_storm_or_buffs_a_storm_follower() {
    for card in ["CP01-005", "CP01-P02"] {
        for mode in [1_u64, 2] {
            let mut initial = position(card, "hand");
            initial["players"]["P1"]["zones"]["field"] = json!([{"id":"ally","card":"CP01-001"}]);
            initial["players"]["P1"]["zones"]["deck"] = json!([
                {"id":"ineligible","card":"BP01-171"},{"id":"storm","card":"CP01-001"},
                {"id":"tail","card":"BP01-171"}
            ]);
            let mut game = game(&initial);
            play(&mut game, "source", mode);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["P1"]["pp"]["current"], 8_u8);
            assert_eq!(
                state["objects"]["ally"]["power"],
                if mode == 1 { 4_u8 } else { 7_u8 }
            );
            assert_eq!(
                state["P1"]["hand"],
                if mode == 1 {
                    json!(["storm"])
                } else {
                    json!([])
                }
            );
        }
    }
}

#[test]
fn cp01_013_racing_buffs_haruurara_and_prevents_damage() {
    for card in ["CP01-013", "CP01-P10", "CSD01-015"] {
        let mut initial = position(card, "field");
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"blast","card":"BP01-089"}]);
        let mut game = game(&initial);
        feed(&mut game, card);
        let fed = game.projection(View::Referee).unwrap();
        assert_eq!(fed["objects"]["source"]["power"], 3_u8);
        assert_eq!(fed["objects"]["source"]["hp"], 3_u8);
        assert_eq!(fed["P1"]["pp"]["current"], 9_u8);
        assert_eq!(fed["P1"]["race"], json!(["carrot"]));
        play(&mut game, "blast", 1);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["source"]["damage"], 0_u8);
        assert!(
            state["P1"]["field"]
                .as_array()
                .unwrap()
                .contains(&json!("source"))
        );
    }
}

#[test]
fn cp01_019_racing_stands_another_follower_but_forbids_both_attack_targets() {
    for card in ["CP01-019", "CP01-P13"] {
        let mut initial = position(card, "field");
        initial["players"]["P2"]["zones"]["field"][0]["state"] = json!({"acted":true});
        initial["players"]["P1"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"ally","card":"BP01-171","state":{"acted":true}}));
        let mut game = game(&initial);
        feed(&mut game, card);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["source"]["power"], 3_u8);
        assert_eq!(state["objects"]["ally"]["acted"], false);
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"attack","attacker":"ally","target":"P2.leader"})
            )
            .unwrap()
            .outcome,
            "cannot-attack"
        );
        assert_eq!(
            game.internal_rule_action(&json!({"do":"attack","attacker":"ally","target":"enemy"}))
                .unwrap()
                .outcome,
            "cannot-attack"
        );
    }
}

#[test]
fn cp01_020_searches_a_three_cost_umamusume_and_resolves_its_fanfare() {
    for card in ["CP01-020", "CP01-P14"] {
        let mut initial = position(card, "hand");
        initial["players"]["P1"]["zones"]["deck"] = json!([
            {"id":"wrong_cost","card":"CP01-073"},{"id":"chosen","card":"CP01-053"},
            {"id":"tail","card":"BP01-171"}
        ]);
        initial["players"]["P1"]["zones"]["cemetery"] = json!(
            (0_u8..10)
                .map(|i| json!({"id":format!("dead{i}"),"card":"BP01-171"}))
                .collect::<Vec<_>>()
        );
        let mut game = game(&initial);
        play(&mut game, "source", 1);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["pp"]["current"], 4_u8);
        assert_eq!(state["P1"]["field"], json!(["source", "chosen"]));
        assert!(has_keyword(&state, "source", "突進"));
        assert_eq!(state["P1"]["deck"].as_array().unwrap().len(), 2);
        assert_eq!(state["objects"]["enemy"]["power"], 1_u8);
        assert_eq!(state["objects"]["enemy"]["hp"], 1_u8);
    }
}

#[test]
fn cp01_024_lastword_searches_tokai_teio() {
    for card in ["CP01-024", "CP01-P18", "CSD01-017"] {
        let mut initial = position(card, "field");
        initial["players"]["P1"]["zones"]["deck"] = json!([
            {"id":"other","card":"BP01-171"},{"id":"teio","card":"ready-teio"},
            {"id":"tail","card":"BP01-171"}
        ]);
        let mut game = with_selected_producer(
            &initial,
            Action::Destroy {
                objects: subjects(),
            },
            fixture(),
        );
        load_companions(&mut game);
        activate(&mut game, "P1");
        settle(&mut game);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["hand"], json!(["teio"]));
        assert!(
            state["P1"]["cemetery"]
                .as_array()
                .unwrap()
                .contains(&json!("source"))
        );
    }
}

#[test]
fn cp01_027_only_a_spell_play_weakens_the_enemy() {
    for card in ["CP01-027", "CP01-SL07", "CP01-SP07"] {
        let mut initial = position(card, "field");
        initial["players"]["P1"]["zones"]["hand"] =
            json!([{"id":"spell","card":"BP01-065"},{"id":"follower","card":"BP01-171"}]);
        let mut game = game(&initial);
        play(&mut game, "follower", 1);
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["enemy"]["power"],
            5_u8
        );
        play(&mut game, "spell", 1);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["enemy"]["power"], 4_u8);
        assert_eq!(state["objects"]["enemy"]["hp"], 4_u8);
    }
}

#[test]
fn cp01_030_spell_play_buffs_umamusume_only_once_per_turn() {
    for card in ["CP01-030", "CP01-P21", "PR-362"] {
        let mut initial = position(card, "field");
        initial["players"]["P1"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"other","card":"BP01-171"}));
        initial["players"]["P1"]["zones"]["hand"] =
            json!([{"id":"spell1","card":"BP01-065"},{"id":"spell2","card":"BP01-065"}]);
        let mut game = game(&initial);
        play(&mut game, "spell1", 1);
        play(&mut game, "spell2", 1);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["source"]["power"], 4_u8);
        assert_eq!(state["objects"]["source"]["hp"], 4_u8);
        assert!(has_keyword(&state, "source", "突進"));
        assert_eq!(state["objects"]["other"]["power"], 2_u8);
    }
}

#[test]
fn cp01_030_named_companions_reduce_play_cost_once() {
    for card in ["CP01-030", "CP01-P21", "PR-362"] {
        for count in [0_u8, 1, 2] {
            let mut initial = position(card, "hand");
            initial["players"]["P1"]["zones"]["field"] = json!(
                (0..count)
                    .map(|i| json!({"id":format!("daiwa{i}"),"card":"ready-daiwa"}))
                    .collect::<Vec<_>>()
            );
            let mut game = game(&initial);
            play(&mut game, "source", 1);
            assert_eq!(
                game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
                if count == 0 { 7_u8 } else { 9_u8 }
            );
        }
    }
}

#[test]
fn cp01_062_fanfare_grants_storm_only_at_ten_umamusume_cards() {
    for card in ["CP01-062", "CP01-P47", "CSD01-025"] {
        for count in [9_u8, 10] {
            let mut initial = position(card, "hand");
            initial["players"]["P1"]["zones"]["cemetery"] = json!(
                (0..count)
                    .map(|i| json!({"id":format!("dead{i}"),"card":"CP01-007"}))
                    .collect::<Vec<_>>()
            );
            let mut game = game(&initial);
            play(&mut game, "source", 1);
            let state = game.projection(View::Referee).unwrap();
            assert!(has_keyword(&state, "source", "指定攻撃"));
            assert_eq!(has_keyword(&state, "source", "疾走"), count == 10);
        }
    }
}

#[test]
fn cp01_071_racing_searches_a_one_cost_amulet_and_resolves_its_fanfare() {
    for card in ["CP01-071", "CP01-P53"] {
        let mut initial = position(card, "field");
        initial["players"]["P1"]["zones"]["deck"] = json!([
            {"id":"follower","card":"CP01-007"},{"id":"amulet","card":"BP01-149"},
            {"id":"tail","card":"BP01-171"}
        ]);
        let mut game = game(&initial);
        feed(&mut game, card);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["source"]["power"], 4_u8);
        assert_eq!(state["objects"]["source"]["hp"], 4_u8);
        assert_eq!(state["P1"]["field"], json!(["source", "amulet"]));
        assert_eq!(state["P1"]["pp"]["current"], 9_u8);
        assert!(has_keyword(&state, "source", "守護"));
    }
}

#[test]
fn cp01_072_only_mejiro_followers_or_any_amulet_departures_grant_storm() {
    for card in ["CP01-072", "CP01-P54"] {
        for (companion, name, expected) in [
            ("BP01-025", "森の聖域", true),
            ("CP01-066", "メジロマックイーン", true),
            ("BP01-171", "ゴブリン", false),
        ] {
            let mut initial = position(card, "field");
            initial["players"]["P1"]["zones"]["field"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":"offering","card":companion}));
            let action = event_cemetery::move_named(ZoneKind::Field, OwnerZone::Cemetery, name);
            let mut game = with_selected_producer(&initial, action, fixture());
            load_companions(&mut game);
            assert!(!has_keyword(
                &game.projection(View::Referee).unwrap(),
                "source",
                "疾走"
            ));
            activate(&mut game, "P1");
            settle(&mut game);
            assert_eq!(
                has_keyword(&game.projection(View::Referee).unwrap(), "source", "疾走"),
                expected
            );
        }
    }
}
