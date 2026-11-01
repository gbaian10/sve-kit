use super::*;
use sve_engine::game::internal::SearchAnswer;

// Inert companions isolate the tested cards without claiming admission of their materials.
fn selected() -> &'static SelectedEffects {
    static SELECTED: OnceLock<SelectedEffects> = OnceLock::new();
    SELECTED.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        for (number, name, class, kind, traits, cost) in [
            ("zone-dragon", "Dragon material", "ドラゴン", "フォロワー", vec![], "7"),
            ("zone-machine", "Machine companion", "ニュートラル", "フォロワー", vec!["機械"], "1"),
            ("zone-inmate", "Inmate material", "ニュートラル", "フォロワー", vec!["八獄"], "1"),
            ("zone-cauldron", "魔女の大釜", "ウィッチ", "アミュレット", vec![], "1"),
        ] {
            let follower = kind == "フォロワー";
            snapshot.push_str(&json!({"number":number,"faces":[{
                "name":name,"card_class":class,"card_type":kind,"traits":traits,"cost":cost,
                "power":if follower {"5"} else {"-"},"hp":if follower {"5"} else {"-"},"text":null,"sections":[]
            }]}).to_string());
            snapshot.push('\n');
            input["documents"].as_array_mut().unwrap().push(json!([
                format!("{number}.yaml"),
                json!({"format":1_u8,"kind":"effect_set","cards":{number:{"status":"complete","abilities":[]}}}).to_string()
            ]));
        }
        repository::selection_with_rules_from(&snapshot, &keywords, &input)
    })
}

fn position(initial: &Value) -> Game {
    let mut game = Game::from_selected_effects(
        selected().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "zone-migrations",
    )
    .unwrap();
    let empty = WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![],
    })
    .unwrap();
    for number in [
        "zone-dragon",
        "zone-machine",
        "zone-inmate",
        "zone-cauldron",
    ] {
        game.load_internal_card(number, &empty).unwrap();
    }
    game
}

fn initial(number: &str, zone: &str) -> Value {
    let mut value = draw_position();
    value["players"]["P1"]["zones"][zone] = json!([{"id":"source","card":number}]);
    value
}

fn definition(number: &str, zone: ZoneKind) -> AbilityId {
    selected().current[number]
        .program
        .card()
        .abilities
        .iter()
        .find(|a| a.node.kind == AbilityKind::Activated && a.node.active_zones == [zone])
        .unwrap()
        .node
        .id
}

#[expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate the printed scenarios."
)]
fn finish(game: &mut Game, mut play: CardPlay) {
    while let CardPlay::Awaiting(question) = play {
        let answer = match &question.domain {
            InputDomain::Targets(domain) => Answer::Targets(vec![domain.members[0].handle]),
            InputDomain::CostMaterials(_) => Answer::CostMaterials(vec![0]),
            InputDomain::Search(domain) => {
                Answer::Search(SearchAnswer::Ungrouped(vec![domain.cards[0].handle]))
            }
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            InputDomain::MealPayment { .. }
            | InputDomain::Quick { .. }
            | InputDomain::OptionalCost
            | InputDomain::BeforeDraw
            | InputDomain::Pending(_)
            | InputDomain::DamageReplacements(_)
            | InputDomain::TriggerQuota(_)
            | InputDomain::Integer(_)
            | InputDomain::Order(_)
            | InputDomain::DriveTrigger { .. }
            | InputDomain::SearchPartition(_)
            | InputDomain::Distribution(_)
            | InputDomain::SearchReplacement
            | InputDomain::TokenCapacity { .. }
            | InputDomain::Modes { .. } => panic!("unexpected migration question"),
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    resolved(play);
}

fn activate(game: &mut Game, number: &str, zone: ZoneKind) {
    let play = game
        .activate_internal_card("source", definition(number, zone), PlayerId { seat: 1 })
        .unwrap();
    finish(game, play);
}

#[test]
fn bp11_066_discards_source_and_a_separate_large_dragon_to_heal_five() {
    for number in ["BP11-066", "PR-306", "SD08-018"] {
        let mut value = initial(number, "hand");
        value["players"]["P1"]["leader"]["life"] = json!(10_u8);
        value["players"]["P1"]["zones"]["hand"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"material","card":"zone-dragon"}));
        let mut game = position(&value);
        activate(&mut game, number, ZoneKind::Hand);
        let view = game.projection(View::Referee).unwrap();
        let grave = view["P1"]["cemetery"].as_array().unwrap();
        assert_eq!(grave.len(), 2);
        assert!(grave.contains(&json!("source")) && grave.contains(&json!("material")));
        assert_eq!(view["P1"]["leader"]["life"], 15_u8);
        assert_eq!(view["P1"]["pp"]["current"], 8_u8);
    }
}

#[test]
fn bp11_093_discards_only_its_hand_copy_to_heal_two() {
    let mut value = initial("BP11-093", "hand");
    value["players"]["P1"]["leader"]["life"] = json!(10_u8);
    value["players"]["P1"]["zones"]["hand"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"copy","card":"BP11-093"}));
    let mut game = position(&value);
    activate(&mut game, "BP11-093", ZoneKind::Hand);
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["hand"], json!(["copy"]));
    assert_eq!(view["P1"]["cemetery"], json!(["source"]));
    assert_eq!(view["P1"]["leader"]["life"], 12_u8);
    assert_eq!(view["P1"]["pp"]["current"], 9_u8);
}

#[test]
fn bp13_114_uses_hand_quick_on_opponent_turn_to_destroy_the_chosen_follower() {
    let mut value = draw_position();
    value["players"]["P2"]["zones"]["hand"] = json!([{"id":"source","card":"BP13-114"}]);
    value["players"]["P1"]["zones"]["field"] = json!([{"id":"enemy","card":"zone-dragon"}]);
    let mut game = position(&value);
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    let question = game.internal_quick_choice().unwrap().unwrap();
    let InputDomain::Quick { options, .. } = &question.domain else {
        panic!("Quick");
    };
    let option = options.iter().find(|o| o.card == "BP13-114").unwrap();
    let play = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Quick(Some(option.handle)),
        )
        .unwrap();
    finish(&mut game, play);
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P2"]["cemetery"], json!(["source"]));
    assert_eq!(view["P2"]["pp"]["current"], 6_u8);
    assert_eq!(view["P1"]["cemetery"], json!(["enemy"]));
}

#[test]
fn bp15_019_returns_own_card_and_creates_one_fairy_wisp_in_ex_after_discard() {
    let mut value = initial("BP15-019", "hand");
    value["players"]["P1"]["zones"]["field"] = json!([{"id":"ally","card":"zone-dragon"}]);
    let mut game = position(&value);
    activate(&mut game, "BP15-019", ZoneKind::Hand);
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["hand"], json!(["ally"]));
    assert_eq!(view["P1"]["cemetery"], json!(["source"]));
    assert_eq!(view["P1"]["ex"].as_array().unwrap().len(), 1);
    assert_eq!(view["P1"]["pp"]["current"], 9_u8);
}

#[test]
fn bp15_074_discards_source_and_large_dragon_then_draws_two() {
    for number in ["BP15-074", "PR-617", "SD08-017"] {
        let mut value = initial(number, "hand");
        value["players"]["P1"]["zones"]["hand"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"material","card":"zone-dragon"}));
        value["players"]["P1"]["zones"]["deck"] =
            json!([{"id":"first","card":"BP01-037"},{"id":"second","card":"BP01-037"}]);
        let mut game = position(&value);
        activate(&mut game, number, ZoneKind::Hand);
        let view = game.projection(View::Referee).unwrap();
        let grave = view["P1"]["cemetery"].as_array().unwrap();
        assert_eq!(grave.len(), 2);
        assert!(grave.contains(&json!("source")) && grave.contains(&json!("material")));
        assert_eq!(view["P1"]["hand"], json!(["first", "second"]));
        assert_eq!(view["P1"]["pp"]["current"], 9_u8);
    }
}

#[test]
fn bp18_064_discards_its_source_to_draw_one_for_two_pp() {
    for number in ["BP18-064", "PR-473"] {
        let mut value = initial(number, "hand");
        value["players"]["P1"]["zones"]["deck"] = json!([{"id":"drawn","card":"BP01-037"}]);
        let mut game = position(&value);
        activate(&mut game, number, ZoneKind::Hand);
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["P1"]["hand"], json!(["drawn"]));
        assert_eq!(view["P1"]["cemetery"], json!(["source"]));
        assert_eq!(view["P1"]["pp"]["current"], 8_u8);
    }
}

#[test]
fn bp19_007_discards_source_destroys_own_inmate_damages_opponent_and_draws() {
    for number in ["BP19-007", "BP19-P03"] {
        let mut value = initial(number, "hand");
        value["players"]["P1"]["zones"]["field"] = json!([{"id":"inmate","card":"zone-inmate"}]);
        value["players"]["P1"]["zones"]["deck"] = json!([{"id":"drawn","card":"BP01-037"}]);
        let mut game = position(&value);
        activate(&mut game, number, ZoneKind::Hand);
        let view = game.projection(View::Referee).unwrap();
        let grave = view["P1"]["cemetery"].as_array().unwrap();
        assert_eq!(grave.len(), 2);
        assert!(grave.contains(&json!("source")) && grave.contains(&json!("inmate")));
        assert_eq!(view["P1"]["hand"], json!(["drawn"]));
        assert_eq!(view["P2"]["leader"]["life"], 19_u8);
        assert_eq!(view["P1"]["pp"]["current"], 10_u8);
    }
}

#[test]
fn bp19_053_discards_source_then_searches_named_cauldron_onto_field() {
    let mut value = initial("BP19-053", "hand");
    value["players"]["P1"]["zones"]["deck"] = json!([{"id":"cauldron","card":"zone-cauldron"}]);
    let mut game = position(&value);
    activate(&mut game, "BP19-053", ZoneKind::Hand);
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["field"], json!(["cauldron"]));
    assert_eq!(view["P1"]["cemetery"], json!(["source"]));
    assert_eq!(view["P1"]["pp"]["current"], 9_u8);
}

#[test]
fn bp14_088_banishes_its_cemetery_source_creates_a_fox_and_buffs_all_own_foxes() {
    for number in ["BP14-088", "BP14-SL18"] {
        let mut value = initial(number, "cemetery");
        value["players"]["P1"]["zones"]["field"] = json!([{"id":"fox","card":"BP14-T06"}]);
        let mut game = position(&value);
        activate(&mut game, number, ZoneKind::Cemetery);
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["P1"]["banish"], json!(["source"]));
        assert_eq!(view["P1"]["field"].as_array().unwrap().len(), 2);
        assert_eq!(view["P1"]["pp"]["current"], 8_u8);
        for id in view["P1"]["field"].as_array().unwrap() {
            let fox = &view["objects"][id.as_str().unwrap()];
            assert_eq!(fox["power"], 2_u8);
            assert_eq!(fox["hp"], 2_u8);
        }
    }
}

#[test]
fn bp17_076_requires_exactly_five_own_machine_followers_for_its_free_once_per_turn_buff() {
    for number in ["BP17-076", "BP17-SL20"] {
        for count in [4_usize, 5] {
            let mut value = initial(number, "field");
            let field = value["players"]["P1"]["zones"]["field"]
                .as_array_mut()
                .unwrap();
            field.extend(
                (1..count).map(|i| json!({"id":format!("machine-{i}"),"card":"zone-machine"})),
            );
            if count == 4 {
                field.push(json!({"id":"other","card":"zone-dragon"}));
            }
            value["players"]["P2"]["zones"]["field"] =
                json!([{"id":"opponent-machine","card":"zone-machine"}]);
            let mut game = position(&value);
            let before = format!("{game:?}");
            let play = game.activate_internal_card(
                "source",
                definition(number, ZoneKind::Field),
                PlayerId { seat: 1 },
            );
            if count == 4 {
                assert!(matches!(play, Err(EngineFailure::IllegalPlay(_))));
                assert!(
                    format!("{game:?}") == before,
                    "wrong population leaves quota and PP intact"
                );
                continue;
            }
            finish(&mut game, play.unwrap());
            let view = game.projection(View::Referee).unwrap();
            assert_eq!(view["objects"]["source"]["power"], 3_u8);
            assert_eq!(view["objects"]["source"]["hp"], 3_u8);
            assert_eq!(view["P1"]["pp"]["current"], 10_u8);
            game.resume_internal_card().unwrap();
            let committed = format!("{game:?}");
            assert!(matches!(
                game.activate_internal_card(
                    "source",
                    definition(number, ZoneKind::Field),
                    PlayerId { seat: 1 }
                ),
                Err(EngineFailure::IllegalPlay(_))
            ));
            assert!(
                format!("{game:?}") == committed,
                "PP0 consumes the same instance quota"
            );
        }
    }
}

#[test]
fn bp17_076_attack_damage_counts_only_other_own_machine_followers() {
    for number in ["BP17-076", "BP17-SL20"] {
        let mut value = initial(number, "field");
        value["players"]["P1"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .extend([
                json!({"id":"first-machine","card":"zone-machine"}),
                json!({"id":"second-machine","card":"zone-machine"}),
                json!({"id":"non-machine","card":"zone-dragon"}),
            ]);
        value["players"]["P2"]["zones"]["field"] = json!([
            {"id":"combat","card":"zone-dragon","state":{"acted":true,"power":0_u8,"hp":30_u8,"max_hp":30_u8}},
            {"id":"victim","card":"zone-machine"}
        ]);
        let mut game = position(&value);
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"combat"}))
            .unwrap();
        let pending = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = &pending.domain else {
            panic!("attack trigger");
        };
        assert_eq!(options.len(), 1);
        let CardPlay::Awaiting(question) = game
            .answer_internal_card(
                pending.actor,
                pending.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap()
        else {
            panic!("damage target");
        };
        let InputDomain::Targets(domain) = &question.domain else {
            panic!("damage target");
        };
        let target = domain
            .members
            .iter()
            .find(|member| member.card.as_deref() == Some("zone-machine"))
            .unwrap()
            .handle;
        let play = game
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Targets(vec![target]),
            )
            .unwrap();
        finish(&mut game, play);
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["victim"]["damage"],
            2_u8
        );
    }
}
