use super::*;

fn position(number: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":number}]);
    initial
}

fn has_keyword(initial: &Value, id: &str, keyword: &str) -> bool {
    game(initial).projection(View::Referee).unwrap()["objects"][id]["keywords"]
        .as_array()
        .unwrap()
        .contains(&json!(keyword))
}

fn companion(number: &str, ally: &str, keyword: &str) {
    let mut initial = position(number);
    assert!(
        !has_keyword(&initial, "source", keyword),
        "the source alone does not satisfy the companion condition"
    );
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":ally}]);
    assert!(
        !has_keyword(&initial, "source", keyword),
        "an opposing companion does not satisfy the friendly condition"
    );
    initial["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"unrelated","card":"BP01-037"}));
    assert!(
        !has_keyword(&initial, "source", keyword),
        "a friendly card with the wrong trait or kind does not qualify"
    );
    initial["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"ally","card":ally}));
    assert!(
        has_keyword(&initial, "source", keyword),
        "a qualifying friendly companion enables the keyword"
    );
}

fn cemetery(number: &str, grave_card: &str, count: usize, keyword: &str) {
    let mut initial = position(number);
    initial["players"]["P2"]["zones"]["cemetery"] = json!(
        (0..count)
            .map(|n| json!({"id":format!("enemy-grave{n}"),"card":grave_card}))
            .collect::<Vec<_>>()
    );
    assert!(
        !has_keyword(&initial, "source", keyword),
        "opposing marine graves do not satisfy the friendly cemetery condition"
    );
    initial["players"]["P1"]["zones"]["cemetery"] = json!(
        (0..count)
            .map(|n| json!({"id":format!("unrelated-grave{n}"),"card":"BP01-037"}))
            .collect::<Vec<_>>()
    );
    assert!(
        !has_keyword(&initial, "source", keyword),
        "friendly non-marine graves do not satisfy the marine condition"
    );
    let cards = (0..count)
        .map(|n| json!({"id":format!("grave{n}"),"card":grave_card}))
        .collect::<Vec<_>>();
    initial["players"]["P1"]["zones"]["cemetery"] = json!(cards);
    assert!(
        has_keyword(&initial, "source", keyword),
        "five qualifying friendly graves reach the threshold"
    );
    initial["players"]["P1"]["zones"]["cemetery"]
        .as_array_mut()
        .unwrap()
        .pop();
    assert!(
        !has_keyword(&initial, "source", keyword),
        "four qualifying graves remain below the five-card threshold"
    );
}

fn token_aura(number: &str, token: &str, keyword: &str) {
    let mut initial = position(number);
    initial["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"token","card":token}));
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":token}]);
    initial["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .extend([
            json!({"id":"unrelated","card":"BP01-037"}),
            json!({"id":"foreign-token","card":"BP02-T05"}),
        ]);
    assert!(
        !has_keyword(&initial, "unrelated", keyword),
        "an unrelated friendly follower receives no aura"
    );
    assert!(
        !has_keyword(&initial, "foreign-token", keyword),
        "a token with the wrong name or class receives no aura"
    );
    assert!(
        has_keyword(&initial, "token", keyword),
        "the qualifying friendly token receives the aura"
    );
    assert!(
        !has_keyword(&initial, "enemy", keyword),
        "the same token controlled by the opponent receives no aura"
    );
    initial["players"]["P1"]["zones"]["field"][0]["state"] = json!({"silenced":true});
    assert!(
        !has_keyword(&initial, "token", keyword),
        "silencing the provider removes its aura"
    );
}

#[test]
fn bp07_080_requires_another_friendly_machine_follower_for_rush() {
    companion("BP07-080", "BP07-080", "突進");
    let initial = position("BP07-080");
    assert!(has_keyword(&initial, "source", "指定攻撃"));
    assert!(has_keyword(&initial, "source", "必殺"));
}
#[test]
fn bp09_072_grants_rush_and_assail_only_to_friendly_bats() {
    token_aura("BP09-072", "BP01-T15", "突進");
    token_aura("BP09-072", "BP01-T15", "指定攻撃");
}
#[test]
fn bp09_073_grants_assail_to_friendly_bats_until_silenced() {
    token_aura("BP09-073", "BP01-T15", "指定攻撃");
    token_aura("BP09-073", "BP01-T15", "突進");
}
#[test]
fn bp10_023_grants_rush_to_friendly_royal_tokens() {
    token_aura("BP10-023", "BP17-T12", "突進");
}
#[test]
fn bp10_024_keeps_its_royal_token_aura_on_the_evolved_face() {
    token_aura("BP10-024", "BP17-T12", "突進");
}
#[test]
fn bp11_063_requires_five_marine_cards_in_its_own_cemetery() {
    cemetery("BP11-063", "BP11-063", 5, "疾走");
}
#[test]
fn bp11_064_requires_five_marine_cards_on_its_evolved_face() {
    cemetery("BP11-064", "BP11-063", 5, "疾走");
}
#[test]
fn bp12_025_requires_a_friendly_sky_saber_lecia_for_assail() {
    lecia_condition("BP12-025");
}
#[test]
fn bp12_026_requires_a_friendly_sky_saber_lecia_for_assail() {
    lecia_condition("BP12-026");
}
#[test]
fn bp13_059_requires_a_friendly_amulet_for_rush() {
    companion("BP13-059", "BP01-016", "突進");
}
#[test]
fn bp15_076_crimson_gains_storm_after_life_is_lost() {
    let initial = position("BP15-076");
    let mut game = with_producer(
        &initial,
        Action::Life {
            player: PlayerRef::Controller {},
            change: NumericChange::Add {
                amount: Expr::Literal(Int(-1)),
            },
        },
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["source"]["keywords"],
        json!([])
    );
    resolved(
        game.activate_internal_card("producer", AbilityId(1), PlayerId { seat: 1 })
            .unwrap(),
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["source"]["keywords"],
        json!(["疾走"])
    );
}
#[test]
fn bp15_097_requires_three_friendly_field_cards_for_storm_and_keeps_ward() {
    let mut initial = position("BP15-097");
    initial["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"one","card":"BP01-037"}));
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    assert!(!has_keyword(&initial, "source", "疾走"));
    assert!(has_keyword(&initial, "source", "守護"));
    initial["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"two","card":"BP01-037"}));
    assert!(has_keyword(&initial, "source", "疾走"));
}
#[test]
fn bp18_071_requires_another_friendly_martial_dragonewt_for_assail() {
    companion("BP18-071", "BP18-071", "指定攻撃");
}
#[test]
fn bp18_072_requires_another_friendly_martial_dragonewt_for_assail() {
    companion("BP18-072", "BP18-072", "指定攻撃");
}
#[test]
fn cp02_071_gains_storm_and_bane_at_ten_life_and_loses_them_after_healing() {
    let mut initial = position("CP02-071");
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    let mut game = with_producer(
        &initial,
        Action::Life {
            player: PlayerRef::Controller {},
            change: NumericChange::Add {
                amount: Expr::Literal(Int(1)),
            },
        },
    );
    let before = game.projection(View::Referee).unwrap();
    assert_eq!(
        before["objects"]["source"]["keywords"],
        json!(["必殺", "疾走"])
    );
    resolved(
        game.activate_internal_card("producer", AbilityId(1), PlayerId { seat: 1 })
            .unwrap(),
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["source"]["keywords"],
        json!([])
    );
}

// The named support is synthetic; the real card and accepted source stay unchanged.
fn lecia_selection() -> &'static SelectedEffects {
    static SELECTED: OnceLock<SelectedEffects> = OnceLock::new();
    SELECTED.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        let mut face = snapshot.lines().map(|line| serde_json::from_str::<Value>(line).unwrap())
            .find(|card| card["number"] == "BP01-037").unwrap()["faces"][0].clone();
        face["name"] = json!("スカイセイバー・リーシャ");
        face["text"] = Value::Null;
        face["sections"] = json!([]);
        snapshot.push_str(&json!({"number":"named-companion","faces":[face]}).to_string());
        snapshot.push('\n');
        input["documents"].as_array_mut().unwrap().push(json!([
            "named-companion.yaml", "format: 1\nkind: effect_set\ncards:\n  named-companion:\n    status: complete\n    abilities: []\n"
        ]));
        Catalog::from_current_selection(&snapshot, &keywords, &input).unwrap()
    })
}

fn lecia_condition(number: &str) {
    for owner in [None, Some("P1"), Some("P2")] {
        let mut initial = position(number);
        if let Some(owner) = owner {
            if initial["players"][owner]["zones"]["field"].is_null() {
                initial["players"][owner]["zones"]["field"] = json!([]);
            }
            initial["players"][owner]["zones"]["field"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":"support","card":"named-companion"}));
        }
        let mut game = Game::from_selected_effects(
            lecia_selection().clone(),
            &initial,
            &Value::Null,
            &Value::Null,
            "named-static",
        )
        .unwrap();
        game.load_internal_card(
            "named-companion",
            &WellFormedCardProgram::check(CardProgram {
                abilities: vec![],
                card_play_costs: vec![],
            })
            .unwrap(),
        )
        .unwrap();
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(
            view["objects"]["source"]["keywords"]
                .as_array()
                .unwrap()
                .contains(&json!("指定攻撃")),
            owner == Some("P1"),
            "only a friendly named Lecia enables assail"
        );
        assert!(
            view["objects"]["source"]["keywords"]
                .as_array()
                .unwrap()
                .contains(&json!("必殺")),
            "printed bane survives every companion condition"
        );
    }
}
