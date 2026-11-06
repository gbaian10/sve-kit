#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate these printed card scenarios."
)]
use super::*;
use sve_engine::game::internal::SearchAnswer;

const PLAYER: PlayerId = PlayerId { seat: 1 };

fn selected() -> &'static SelectedEffects {
    static READY: OnceLock<SelectedEffects> = OnceLock::new();
    READY.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        // Companions isolate printed filters from unrelated card abilities.
        for (number, name, kind, class, cost, traits) in [
            ("wire-wild", "companion wild", "フォロワー", "ナイトメア", "2", vec!["荒野"]),
            ("wire-arcana", "companion arcana", "スペル", "ウィッチ", "1", vec!["アルカナ"]),
            ("wire-armed", "companion armed", "フォロワー", "ドラゴン", "2", vec!["武装"]),
            ("wire-angel", "companion angel", "フォロワー", "ニュートラル", "3", vec!["天使"]),
            ("wire-thief", "companion thief", "スペル", "ロイヤル", "1", vec!["盗賊"]),
            ("wire-princess", "companion princess", "フォロワー", "エルフ", "3", vec!["プリコネ"]),
            ("wire-dragon", "companion dragon", "フォロワー", "ドラゴン", "7", vec![]),
            ("wire-plain", "companion plain", "フォロワー", "ニュートラル", "1", vec![]),
        ] {
            snapshot.push_str(&json!({"number":number,"faces":[{
                "name":name,"card_class":class,"card_type":kind,"cost":cost,
                "power":if kind == "スペル" {"-"} else {"3"},
                "hp":if kind == "スペル" {"-"} else {"8"},
                "traits":traits,"text":null,"sections":[]
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

fn position(card: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["construction"] = json!("title");
    initial["players"]["P1"]["title_code"] = json!("princess_connect_re_dive");
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"draw1","card":"wire-plain"},{"id":"draw2","card":"wire-plain"},
        {"id":"draw3","card":"wire-plain"},{"id":"draw4","card":"wire-plain"}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"wire-plain"}]);
    initial
}

fn game(initial: &Value) -> Game {
    let mut game = Game::from_selected_effects(
        selected().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "wire-hot01",
    )
    .unwrap();
    let empty = WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![],
    })
    .unwrap();
    for card in [
        "wire-wild",
        "wire-arcana",
        "wire-armed",
        "wire-angel",
        "wire-thief",
        "wire-princess",
        "wire-dragon",
        "wire-plain",
    ] {
        game.load_internal_card(card, &empty).unwrap();
    }
    game
}

fn finish(game: &mut Game, mut play: CardPlay) {
    while let CardPlay::Awaiting(q) = play {
        let answer = match &q.domain {
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            InputDomain::OptionalCost => Answer::OptionalCost(true),
            InputDomain::Targets(d) => Answer::Targets(
                d.members
                    .iter()
                    .take(
                        d.cardinality
                            .maximum
                            .map_or(d.members.len(), |n| usize::try_from(n.0).unwrap()),
                    )
                    .map(|m| m.handle)
                    .collect(),
            ),
            InputDomain::CostMaterials(d) => Answer::CostMaterials(
                d.members
                    .iter()
                    .take(usize::try_from(d.cardinality.minimum.0).unwrap())
                    .map(|m| m.handle)
                    .collect(),
            ),
            InputDomain::Search(d) => Answer::Search(SearchAnswer::Ungrouped(
                d.cards
                    .iter()
                    .take(
                        d.cardinality
                            .maximum
                            .map_or(d.cards.len(), |n| usize::try_from(n.0).unwrap()),
                    )
                    .map(|m| m.handle)
                    .collect(),
            )),
            InputDomain::Order(cards) => Answer::Order(cards.iter().map(|m| m.handle).collect()),
            other => panic!("unexpected decision: {other:?}"),
        };
        play = game.answer_internal_card(q.actor, q.token, answer).unwrap();
    }
}

fn settle(game: &mut Game) {
    loop {
        if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
        let Some(q) = game.internal_pending_choice().unwrap() else {
            break;
        };
        let InputDomain::Pending(options) = &q.domain else {
            panic!("pending")
        };
        let play = game
            .answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
            .unwrap();
        finish(game, play);
    }
}

fn play(game: &mut Game, id: &str) {
    let step = game.play_internal_card(id, PLAYER).unwrap();
    finish(game, step);
    settle(game);
}

#[test]
fn bp11_059_hand_activation_discards_both_dragons_and_deals_four() {
    for card in ["BP11-059", "SD08-012"] {
        let mut initial = position(card);
        initial["players"]["P1"]["zones"]["hand"] = json!([
            {"id":"source","card":card},{"id":"fuel","card":"wire-dragon"}
        ]);
        let mut game = game(&initial);
        let ability = selected().current[card]
            .program
            .card()
            .abilities
            .iter()
            .find(|a| {
                a.node.kind == AbilityKind::Activated
                    && matches!(a.node.source.origin, AbilityOrigin::Printed { .. })
            })
            .unwrap()
            .node
            .id;
        let step = game
            .activate_internal_card("source", ability, PLAYER)
            .unwrap();
        finish(&mut game, step);
        settle(&mut game);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["hand"], json!([]));
        let grave = state["P1"]["cemetery"].as_array().unwrap();
        assert_eq!(grave.len(), 2);
        assert!(grave.contains(&json!("source")) && grave.contains(&json!("fuel")));
        assert_eq!(state["P1"]["pp"]["current"], json!(9_u8));
        assert_eq!(state["objects"]["enemy"]["hp"], json!(4_u8));
    }
}

#[test]
fn bp15_025_evolution_reveals_a_thief_and_bottoms_the_other_cards() {
    for card in ["BP15-025", "PR-531"] {
        let mut initial = position("wire-plain");
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP15-024"}]);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":card}]);
        initial["players"]["P1"]["zones"]["deck"] = json!([
            {"id":"thief","card":"wire-thief"},{"id":"other1","card":"wire-plain"},
            {"id":"other2","card":"wire-plain"},{"id":"other3","card":"wire-plain"},
            {"id":"untouched","card":"wire-plain"}
        ]);
        let mut game = game(&initial);
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
            )
            .unwrap()
            .outcome,
            "resolved"
        );
        settle(&mut game);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["hand"], json!(["thief"]));
        assert_eq!(
            state["P1"]["deck"],
            json!(["untouched", "other1", "other2", "other3"])
        );
    }
}

#[test]
fn cp04_003_fanfare_finds_a_princess_and_discounts_her_next_play() {
    for card in ["CP04-003", "CP04-SL03", "CP04-SP02", "CP04-U02"] {
        let mut initial = position(card);
        initial["players"]["P1"]["zones"]["deck"] = json!([
            {"id":"friend","card":"wire-princess"},{"id":"other1","card":"wire-plain"},
            {"id":"other2","card":"wire-plain"},{"id":"other3","card":"wire-plain"}
        ]);
        let mut game = game(&initial);
        play(&mut game, "source");
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["hand"],
            json!(["friend"])
        );
        play(&mut game, "friend");
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["pp"]["current"], json!(5_u8));
        assert_eq!(state["P1"]["field"], json!(["source", "friend"]));
    }
}

fn evolution(base: &str, form: &str) -> Value {
    let mut initial = position(base);
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":base}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
    initial
}

fn evolve(game: &mut Game, pp: u8) {
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":pp}})
        )
        .unwrap()
        .outcome,
        "resolved",
        "the printed evolution resolves before its triggered ability"
    );
    settle(game);
}

#[test]
fn bp11_059_cannot_pay_both_discard_groups_with_the_source_alone() {
    let mut game = game(&position("BP11-059"));
    let ability = selected().current["BP11-059"]
        .program
        .card()
        .abilities
        .iter()
        .find(|a| {
            a.node.kind == AbilityKind::Activated
                && matches!(a.node.source.origin, AbilityOrigin::Printed { .. })
        })
        .unwrap()
        .node
        .id;
    let CardPlay::Awaiting(q) = game
        .activate_internal_card("source", ability, PLAYER)
        .unwrap()
    else {
        panic!("formal target before private payment");
    };
    let InputDomain::Targets(domain) = &q.domain else {
        panic!("formal target");
    };
    game.answer_internal_card(
        q.actor,
        q.token,
        Answer::Targets(vec![domain.members[0].handle]),
    )
    .unwrap_err();
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["source"]));
    assert_eq!(state["P1"]["pp"]["current"], json!(10_u8));
}

#[test]
fn bp10_037_finds_two_arcana_spells_and_bottoms_the_remainder() {
    for card in ["BP10-037", "BP10-SL09", "BP10-U03"] {
        let mut initial = position(card);
        initial["players"]["P1"]["zones"]["deck"] = json!([
            {"id":"arcana1","card":"wire-arcana"},{"id":"arcana2","card":"wire-arcana"},
            {"id":"plain1","card":"wire-plain"},{"id":"plain2","card":"wire-plain"},
            {"id":"plain3","card":"wire-plain"},{"id":"untouched","card":"wire-plain"}
        ]);
        let mut game = game(&initial);
        play(&mut game, "source");
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["hand"], json!(["arcana1", "arcana2"]));
        assert_eq!(
            state["P1"]["deck"],
            json!(["untouched", "plain1", "plain2", "plain3"])
        );
    }
}

#[test]
fn bp11_060_evolution_deals_six_to_the_chosen_enemy() {
    for card in ["BP11-060", "SD08-013"] {
        let mut game = game(&evolution("BP11-059", card));
        evolve(&mut game, 1);
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["enemy"]["hp"],
            json!(2_u8)
        );
    }
}

#[test]
fn bp11_052_recovers_a_cheap_wild_follower_and_has_awakened_storm() {
    for card in ["BP11-052", "BP11-SL10"] {
        let mut initial = position(card);
        initial["players"]["P1"]["zones"]["cemetery"] = json!([{"id":"wild","card":"wire-wild"}]);
        let mut game = game(&initial);
        play(&mut game, "source");
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["field"], json!(["source", "wild"]));
        assert!(
            state["objects"]["source"]["keywords"]
                .as_array()
                .unwrap()
                .contains(&json!("疾走"))
        );
    }
}

#[test]
fn bp11_053_evolution_buffs_other_wild_followers() {
    for card in ["BP11-053", "BP11-SL11"] {
        let mut initial = evolution("BP11-052", card);
        initial["players"]["P1"]["zones"]["field"] = json!([
            {"id":"source","card":"BP11-052"},{"id":"wild","card":"wire-wild"}
        ]);
        let mut game = game(&initial);
        evolve(&mut game, 1);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["wild"]["power"], json!(4_u8));
        assert_eq!(state["objects"]["source"]["power"], json!(3_u8));
    }
}

#[test]
fn bp11_078_necrocharge_damages_the_enemy_and_recovers_one_wild_follower() {
    let mut initial = position("BP11-078");
    let mut grave = vec![json!({"id":"wild","card":"wire-wild"})];
    grave.extend((0_u8..9).map(|n| json!({"id":format!("grave{n}"),"card":"wire-plain"})));
    initial["players"]["P1"]["zones"]["cemetery"] = json!(grave);
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["hp"], json!(4_u8));
    assert_eq!(state["P1"]["field"], json!(["source", "wild"]));
}

#[test]
fn bp10_070_finds_an_armed_card_and_recovers_one_pp() {
    for card in ["BP10-070", "PR-242"] {
        let mut initial = position(card);
        initial["players"]["P1"]["zones"]["deck"] = json!([
            {"id":"armed","card":"wire-armed"},{"id":"plain","card":"wire-plain"},
            {"id":"untouched","card":"wire-plain"}
        ]);
        let mut game = game(&initial);
        play(&mut game, "source");
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["hand"], json!(["armed"]));
        assert_eq!(state["P1"]["deck"], json!(["untouched", "plain"]));
        assert_eq!(state["P1"]["pp"]["current"], json!(9_u8));
    }
}

#[test]
fn cp04_066_ub_hits_the_enemy_and_its_leader_with_a_large_ally() {
    for card in ["CP04-066", "CP04-P50"] {
        let mut initial = position(card);
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] = json!([
            {"id":"source","card":card},{"id":"large","card":"wire-dragon"}
        ]);
        let mut game = game(&initial);
        let ability = selected().current[card]
            .program
            .card()
            .abilities
            .iter()
            .find(|a| {
                a.node.kind == AbilityKind::Activated
                    && matches!(a.node.source.origin, AbilityOrigin::Printed { .. })
            })
            .unwrap()
            .node
            .id;
        let step = game
            .activate_internal_card("source", ability, PLAYER)
            .unwrap();
        finish(&mut game, step);
        settle(&mut game);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["enemy"]["hp"], json!(5_u8));
        assert_eq!(state["P2"]["leader"]["life"], json!(17_u8));
    }
}

#[test]
fn bp10_115_reveals_an_angel_from_the_top_card() {
    for card in ["BP10-115", "BP10-P25"] {
        let mut initial = position(card);
        initial["players"]["P1"]["zones"]["deck"] = json!([
            {"id":"angel","card":"wire-angel"},{"id":"untouched","card":"wire-plain"}
        ]);
        let mut game = game(&initial);
        play(&mut game, "source");
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["hand"], json!(["angel"]));
        assert_eq!(state["P1"]["deck"], json!(["untouched"]));
    }
}

#[test]
fn bp07_006_evolution_recovers_a_three_cost_or_cheaper_follower() {
    for card in ["BP07-006", "PR-215"] {
        let mut initial = evolution("BP07-005", card);
        initial["players"]["P1"]["zones"]["cemetery"] =
            json!([{"id":"friend","card":"wire-plain"}]);
        let mut game = game(&initial);
        evolve(&mut game, 2);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["field"],
            json!(["source", "friend"])
        );
    }
}

#[test]
fn bp11_079_necrocharge_returns_both_cheap_wild_followers() {
    for card in ["BP11-079", "BP11-P20", "EBD03-015"] {
        let mut initial = position(card);
        let mut grave = vec![
            json!({"id":"first","card":"wire-wild"}),
            json!({"id":"second","card":"wire-wild"}),
        ];
        grave.extend((0_u8..8).map(|n| json!({"id":format!("grave{n}"),"card":"wire-plain"})));
        initial["players"]["P1"]["zones"]["cemetery"] = json!(grave);
        let mut game = game(&initial);
        play(&mut game, "source");
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["field"],
            json!(["first", "second"])
        );
    }
}

#[test]
fn cp04_003_fourth_ub_grants_storm_after_three_previous_activations() {
    let mut initial = position("CP04-003");
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"source1","card":"CP04-003"},{"id":"source2","card":"CP04-003"},
        {"id":"source3","card":"CP04-003"},{"id":"source4","card":"CP04-003"},
        {"id":"friend","card":"wire-princess"}
    ]);
    let mut game = game(&initial);
    let ability = selected().current["CP04-003"]
        .program
        .card()
        .abilities
        .iter()
        .find(|a| {
            a.node.kind == AbilityKind::Activated
                && matches!(a.node.source.origin, AbilityOrigin::Printed { .. })
        })
        .unwrap()
        .node
        .id;
    for (i, id) in ["source1", "source2", "source3", "source4"]
        .iter()
        .enumerate()
    {
        let CardPlay::Awaiting(q) = game.activate_internal_card(id, ability, PLAYER).unwrap()
        else {
            panic!("target")
        };
        let InputDomain::Targets(d) = &q.domain else {
            panic!("target")
        };
        let chosen = d
            .members
            .iter()
            .find(|m| m.card.as_deref() == Some("wire-princess"))
            .unwrap()
            .handle;
        let step = game
            .answer_internal_card(q.actor, q.token, Answer::Targets(vec![chosen]))
            .unwrap();
        finish(&mut game, step);
        settle(&mut game);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["objects"]["friend"]["keywords"]
                .as_array()
                .unwrap()
                .contains(&json!("疾走")),
            i == 3
        );
    }
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["friend"]["power"],
        json!(7_u8)
    );
}
