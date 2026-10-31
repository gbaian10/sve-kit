use super::*;

fn position(number: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":number}]);
    initial
}

fn keywords(initial: &Value, holder: &str) -> Value {
    game(initial).projection(View::Referee).unwrap()["objects"][holder]["keywords"].clone()
}

fn grave_threshold(number: &str, owner: &str, count: usize, granted: &str) {
    let mut initial = position(number);
    assert!(
        !keywords(&initial, "source")
            .as_array()
            .unwrap()
            .contains(&json!(granted)),
        "no qualifying graves means no conditional keyword"
    );
    initial["players"]["P2"]["zones"]["cemetery"] = json!(
        (0..count)
            .map(|n| json!({"id":format!("opposing{n}"),"card":number}))
            .collect::<Vec<_>>()
    );
    initial["players"]["P1"]["zones"]["cemetery"] = json!(
        (0..count)
            .map(|n| json!({"id":format!("unrelated{n}"),"card":"BP01-037"}))
            .collect::<Vec<_>>()
    );
    assert!(
        !keywords(&initial, "source")
            .as_array()
            .unwrap()
            .contains(&json!(granted)),
        "neither opposing graves nor the wrong friendly trait qualifies"
    );
    initial["players"][owner]["zones"]["cemetery"] = json!(
        (0..count)
            .map(|n| json!({"id":format!("grave{n}"),"card":number}))
            .collect::<Vec<_>>()
    );
    assert!(
        keywords(&initial, "source")
            .as_array()
            .unwrap()
            .contains(&json!(granted)),
        "the printed cemetery threshold grants the keyword"
    );
    initial["players"][owner]["zones"]["cemetery"]
        .as_array_mut()
        .unwrap()
        .pop();
    assert!(
        !keywords(&initial, "source")
            .as_array()
            .unwrap()
            .contains(&json!(granted)),
        "one fewer qualifying grave falls below the threshold"
    );
}

fn martial_companion(number: &str, granted: &[&str]) {
    let mut initial = position(number);
    initial["players"]["P2"]["zones"]["field"] = json!([]);
    for owner in ["P2", "P1"] {
        initial["players"][owner]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":format!("{owner}-ally"),"card":"DSD01b-012"}));
        for keyword in granted {
            assert_eq!(
                keywords(&initial, "source")
                    .as_array()
                    .unwrap()
                    .contains(&json!(keyword)),
                owner == "P1",
                "only another friendly martial dragonewt qualifies"
            );
        }
    }
}

#[test]
fn bp20_056_gains_storm_exactly_at_seven_maximum_pp() {
    let mut initial = position("BP20-056");
    initial["players"]["P1"]["pp"] = json!({"current":6_u8,"max":6_u8});
    assert_eq!(keywords(&initial, "source"), json!([]));
    initial["players"]["P1"]["pp"] = json!({"current":7_u8,"max":7_u8});
    assert_eq!(keywords(&initial, "source"), json!(["疾走"]));
}
#[test]
fn bp20_072_grants_ward_to_friendly_orcas_until_silenced() {
    let mut initial = position("BP20-072");
    initial["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"orca","card":"BP02-T05"}));
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP02-T05"}]);
    assert_eq!(keywords(&initial, "orca"), json!(["守護"]));
    assert_eq!(keywords(&initial, "enemy"), json!([]));
    initial["players"]["P1"]["zones"]["field"][0]["state"] = json!({"silenced":true});
    assert_eq!(keywords(&initial, "orca"), json!([]));
}
#[test]
fn cp03_089_requires_ten_friendly_shadow_paladin_cards_for_bane_and_rush() {
    grave_threshold("CP03-089", "P1", 10, "必殺");
    grave_threshold("CP03-089", "P1", 10, "突進");
}
#[test]
fn cp03_097_requires_ten_friendly_shadow_paladin_cards_for_storm() {
    grave_threshold("CP03-097", "P1", 10, "疾走");
}
#[test]
fn dsd01b_012_excludes_itself_and_opposing_martial_dragonewts() {
    martial_companion("DSD01b-012", &["疾走"]);
    assert_eq!(keywords(&position("DSD01b-012"), "source"), json!(["必殺"]));
}
#[test]
fn dsd01b_016_excludes_itself_and_opposing_martial_dragonewts() {
    martial_companion("DSD01b-016", &["突進", "指定攻撃"]);
}
#[test]
fn ecp02_070_requires_five_passion_cards_and_keeps_printed_rush() {
    grave_threshold("ECP02-070", "P1", 5, "指定攻撃");
    grave_threshold("ECP02-070", "P1", 5, "必殺");
    assert_eq!(keywords(&position("ECP02-070"), "source"), json!(["突進"]));
}

#[test]
fn bp04_089_grants_aura_to_both_named_friendly_followers_only() {
    static SELECTED: OnceLock<SelectedEffects> = OnceLock::new();
    let selected = SELECTED.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        let face = snapshot.lines().map(|line| serde_json::from_str::<Value>(line).unwrap())
            .find(|card| card["number"] == "BP01-037").unwrap()["faces"][0].clone();
        for (number, name) in [("named-medusa", "毒牙の姫・メドゥーサ"), ("named-stheno", "ステンノ")] {
            let mut synthetic = face.clone();
            synthetic["name"] = json!(name);
            synthetic["text"] = Value::Null;
            synthetic["sections"] = json!([]);
            snapshot.push_str(&json!({"number":number,"faces":[synthetic]}).to_string());
            snapshot.push('\n');
            input["documents"].as_array_mut().unwrap().push(json!([
                format!("{number}.yaml"), format!("format: 1\nkind: effect_set\ncards:\n  {number}:\n    status: complete\n    abilities: []\n")
            ]));
        }
        Catalog::from_current_selection(&snapshot, &keywords, &input).unwrap()
    });
    let mut initial = position("BP04-089");
    initial["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .extend([
            json!({"id":"medusa","card":"named-medusa"}),
            json!({"id":"stheno","card":"named-stheno"}),
            json!({"id":"unrelated","card":"BP01-037"}),
        ]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"named-medusa"}]);
    let mut game = Game::from_selected_effects(
        selected.clone(),
        &initial,
        &Value::Null,
        &Value::Null,
        "named-gorgons",
    )
    .unwrap();
    for number in ["named-medusa", "named-stheno"] {
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
    let view = game.projection(View::Referee).unwrap();
    for holder in ["medusa", "stheno"] {
        assert_eq!(view["objects"][holder]["keywords"], json!(["オーラ"]));
    }
    for holder in ["enemy", "unrelated", "source"] {
        assert_eq!(view["objects"][holder]["keywords"], json!([]));
    }
}

#[test]
fn bp20_t05_crest_damages_a_friendly_omen_once_per_turn_from_ex() {
    let mut initial = position("BP20-056");
    // Silence isolates the crest from the recipient's damage trigger.
    initial["players"]["P1"]["zones"]["field"][0]["state"] = json!({"silenced":true});
    initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"crest","card":"BP20-T05"}]);
    initial["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"unrelated","card":"BP01-037"}));
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy-omen","card":"BP20-056"}]);
    let mut board = game(&initial);
    let CardPlay::Awaiting(question) = board
        .activate_internal_card("crest", AbilityId(1), PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("formal Omen target");
    };
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("target domain");
    };
    assert_eq!(domain.members.len(), 1);
    resolved(
        board
            .answer_internal_card(question.actor, question.token, Answer::Targets(vec![0]))
            .unwrap(),
    );
    let view = board.projection(View::Referee).unwrap();
    assert_eq!(view["objects"]["source"]["damage"], 1_u8);
    assert_eq!(view["objects"]["enemy-omen"]["damage"], 0_u8);
    let before = format!("{board:?}");
    board
        .activate_internal_card("crest", AbilityId(1), PlayerId { seat: 1 })
        .unwrap_err();
    assert_eq!(format!("{board:?}"), before);
}
