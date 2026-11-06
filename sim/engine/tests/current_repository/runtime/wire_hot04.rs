#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate the printed scenarios."
)]
use super::*;
use alloc::collections::BTreeMap;
use sve_engine::game::internal::SearchAnswer;
const PLAYER: PlayerId = PlayerId { seat: 1 };
type Companion = (
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static [&'static str],
);
const CONTROLS: [Companion; 7] = [
    (
        "wire-plain",
        "plain",
        "ニュートラル",
        "フォロワー",
        "1",
        &[],
    ),
    (
        "wire-idol-one",
        "idol one",
        "ニュートラル",
        "フォロワー",
        "1",
        &["デレマス"],
    ),
    (
        "wire-idol-three",
        "idol three",
        "ニュートラル",
        "フォロワー",
        "3",
        &["デレマス"],
    ),
    (
        "wire-passion",
        "passion",
        "ニュートラル",
        "フォロワー",
        "1",
        &["パッション"],
    ),
    (
        "wire-angel",
        "angel",
        "ニュートラル",
        "フォロワー",
        "1",
        &["天使"],
    ),
    (
        "wire-fallen",
        "fallen",
        "ニュートラル",
        "フォロワー",
        "1",
        &["堕天使"],
    ),
    (
        "wire-history-spell",
        "history spell",
        "ニュートラル",
        "スペル",
        "1",
        &["プリコネ"],
    ),
];
const FORMS: [&str; 4] = ["BP04-085", "PR-624", "PCS01-019", "BP21-095"];
fn append(snapshot: &mut String, input: &mut Value, number: &str, face: &Value) {
    snapshot.push_str(&json!({"number":number,"faces":[face]}).to_string());
    snapshot.push('\n');
    input["documents"].as_array_mut().unwrap().push(json!([format!("{number}.yaml"),json!({"format":1_u8,"kind":"effect_set","cards":{number:{"status":"complete","abilities":[]}}}).to_string()]));
}
fn selected() -> &'static SelectedEffects {
    static READY: OnceLock<SelectedEffects> = OnceLock::new();
    READY.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = producer_inputs();
        // Companions isolate filters while the tested current programs remain complete.
        for (number, name, class, kind, cost, traits) in CONTROLS {
            let follower = kind == "フォロワー";
            append(
                &mut snapshot,
                &mut input,
                number,
                &json!({
                    "name": name,
                    "card_class": class,
                    "card_type": kind,
                    "cost": cost,
                    "power": if follower { "3" } else { "-" },
                    "hp": if follower { "20" } else { "-" },
                    "traits": traits,
                    "text": null,
                    "sections": []
                }),
            );
        }
        for form in FORMS {
            let mut face = snapshot
                .lines()
                .map(|line| serde_json::from_str::<Value>(line).unwrap())
                .find(|record| record["number"] == form)
                .unwrap()["faces"][0]
                .clone();
            face["card_type"] = json!("フォロワー");
            face["cost"] = json!("1");
            face["text"] = Value::Null;
            face["sections"] = json!([]);
            append(
                &mut snapshot,
                &mut input,
                &format!("wire-base-{form}"),
                &face,
            );
        }
        repository::selection_with_rules_from(&snapshot, &keywords, &input)
    })
}

fn bases() -> &'static BTreeMap<String, WellFormedCardProgram> {
    static READY: OnceLock<BTreeMap<String, WellFormedCardProgram>> = OnceLock::new();
    READY.get_or_init(|| {
        let mut tree = expanded::tree().clone();
        let template = tree["cards"]["synthetic-evolution"].clone();
        tree["cards"] = json!({});
        for form in FORMS {
            tree["cards"][format!("wire-base-{form}")] = template.clone();
        }
        let mut spell = template;
        let mut entry = spell["abilities"][0].clone();
        entry.as_object_mut().unwrap().remove("evolve");
        entry["spell"] = json!({"do":[{"draw":{"n":0_u8}}]});
        spell["abilities"] = json!([entry]);
        tree["cards"]["wire-history-spell"] = spell;
        lowerer()
            .lower_with_printed_costs(
                &tree,
                &BTreeMap::from([("wire-history-spell".to_owned(), Nat(1))]),
            )
            .unwrap()
    })
}

fn load_controls(game: &mut Game) {
    let empty = WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![],
    })
    .unwrap();
    for (number, ..) in CONTROLS {
        if !bases().contains_key(number) {
            game.load_internal_card(number, &empty).unwrap();
        }
    }
    for (number, program) in bases() {
        game.load_internal_card(number, program).unwrap();
    }
}
fn position(card: &str) -> Value {
    let mut initial = draw_position();
    if card.starts_with("CP04") || card.starts_with("PCS01") {
        initial["players"]["P1"]["construction"] = json!("title");
        initial["players"]["P1"]["title_code"] = json!("princess_connect_re_dive");
    }
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    for seat in ["P1", "P2"] {
        initial["players"][seat]["zones"]["deck"] = json!(
            (0_u8..6)
                .map(|i| json!({"id":format!("{seat}-draw{i}"),"card":"wire-plain"}))
                .collect::<Vec<_>>()
        );
    }
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"wire-plain"}]);
    initial
}
fn game(initial: &Value) -> Game {
    let mut game = Game::from_selected_effects(
        selected().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "wire-hot04",
    )
    .unwrap();
    load_controls(&mut game);
    game
}
fn producer_game(initial: &Value, action: Action) -> Game {
    let mut game = with_selected_producer(initial, action, selected());
    load_controls(&mut game);
    game
}
fn take(c: &Cardinality, n: usize) -> usize {
    c.maximum
        .map_or(n, |x| usize::try_from(x.0).unwrap())
        .min(n)
}
fn finish(game: &mut Game, mut step: CardPlay) {
    while let CardPlay::Awaiting(q) = step {
        let answer = match &q.domain {
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            InputDomain::OptionalCost => Answer::OptionalCost(true),
            InputDomain::Modes { .. } => Answer::Modes(vec![ModeId(1)]),
            InputDomain::Targets(d) => Answer::Targets(
                d.members
                    .iter()
                    .take(take(&d.cardinality, d.members.len()))
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
            InputDomain::Search(d) => Answer::Search(d.groups.as_ref().map_or_else(
                || {
                    SearchAnswer::Ungrouped(
                        d.cards
                            .iter()
                            .take(take(&d.cardinality, d.cards.len()))
                            .map(|m| m.handle)
                            .collect(),
                    )
                },
                |groups| {
                    SearchAnswer::Grouped(
                        groups
                            .iter()
                            .map(|g| {
                                g.handles
                                    .iter()
                                    .copied()
                                    .take(take(&g.cardinality, g.handles.len()))
                                    .collect()
                            })
                            .collect(),
                    )
                },
            )),
            InputDomain::Order(d) => Answer::Order(d.iter().map(|m| m.handle).collect()),
            other => panic!("unexpected choice: {other:?}"),
        };
        step = game.answer_internal_card(q.actor, q.token, answer).unwrap();
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
        let InputDomain::Pending(d) = &q.domain else {
            panic!("pending")
        };
        let step = game
            .answer_internal_card(q.actor, q.token, Answer::Pending(d[0].handle))
            .unwrap();
        finish(game, step);
    }
}
fn play(game: &mut Game, id: &str) {
    let step = game.play_internal_card(id, PLAYER).unwrap();
    finish(game, step);
    settle(game);
}
fn activate(game: &mut Game, card: &str) {
    let id = selected().current[card]
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
    let step = game.activate_internal_card("source", id, PLAYER).unwrap();
    finish(game, step);
    settle(game);
}
fn evolution(card: &str) -> Value {
    let mut initial = position(card);
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"source","card":format!("wire-base-{}", selected().current[card].card_no)}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":card}]);
    initial
}
fn evolve(game: &mut Game) {
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
        )
        .unwrap()
        .outcome,
        "resolved",
        "the synthetic base supplies only the external evolution"
    );
    settle(game);
}

#[test]
fn bp21_p70_ex_activation_buffs_a_ward_according_to_the_five_slot_threshold() {
    for card in ["BP21-P70", "BP21-T10"] {
        for count in [3_u8, 5] {
            let mut initial = position(card);
            initial["players"]["P1"]["zones"]["hand"] = json!([]);
            initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"source","card":card}]);
            initial["players"]["P1"]["zones"]["field"] = json!(
                (0_u8..count)
                    .map(|i| json!({"id":format!("ward{i}"),"card":"BP01-146"}))
                    .collect::<Vec<_>>()
            );
            let mut game = game(&initial);
            activate(&mut game, card);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["objects"]["ward0"]["hp"],
                if count == 5 { 5_u8 } else { 4_u8 }
            );
            assert_eq!(state["P1"]["pp"]["current"], 10_u8);
            assert_eq!(state["P1"]["ex"], json!(["source"]));
        }
    }
}
#[test]
fn bp15_121_destroys_the_enemy_then_reveals_an_angel_from_its_look() {
    let card = "BP15-121";
    let mut initial = position(card);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"angel","card":"wire-angel"},{"id":"fallen","card":"wire-fallen"},{"id":"other1","card":"wire-plain"},{"id":"other2","card":"wire-plain"},{"id":"untouched","card":"wire-plain"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["cemetery"], json!(["enemy"]));
    assert_eq!(state["P1"]["hand"], json!(["angel"]));
    assert_eq!(
        state["P1"]["deck"],
        json!(["untouched", "fallen", "other1", "other2"])
    );
}
#[test]
fn bp04_084_fanfare_creates_a_viper_whose_entry_triggers_two_damage() {
    for card in ["BP04-084", "BP04-P21"] {
        let mut game = game(&position(card));
        play(&mut game, "source");
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["field"].as_array().unwrap().len(), 2);
        assert_eq!(state["objects"]["enemy"]["hp"], 18_u8);
    }
}
#[test]
fn bp04_085_evolution_creates_a_viper_whose_entry_triggers_two_damage() {
    for card in ["BP04-085", "BP04-P22"] {
        let mut game = game(&evolution(card));
        evolve(&mut game);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["field"].as_array().unwrap().len(), 2);
        assert_eq!(state["objects"]["enemy"]["hp"], 18_u8);
    }
}
#[test]
fn bp04_121_stands_and_buffs_an_ally_but_forbids_its_leader_attack() {
    for card in ["BP04-121", "SP01-052", "SP01-SL52"] {
        let mut initial = position(card);
        initial["players"]["P1"]["zones"]["field"] =
            json!([{"id":"friend","card":"wire-plain","state":{"acted":true}}]);
        let mut game = game(&initial);
        play(&mut game, "source");
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["friend"]["acted"], false);
        assert_eq!(state["objects"]["friend"]["power"], 5_u8);
        assert_eq!(state["objects"]["friend"]["hp"], 22_u8);
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"attack","attacker":"friend","target":"P2.leader"})
            )
            .unwrap()
            .outcome,
            "cannot-attack",
            "the printed restriction survives the Stand action"
        );
    }
}
#[test]
fn bp22_049_discounts_its_ex_golem_and_observes_its_entry() {
    for card in ["BP22-049", "BP22-P26"] {
        let mut game = game(&position(card));
        play(&mut game, "source");
        let id = game.projection(View::Referee).unwrap()["P1"]["ex"][0]
            .as_str()
            .unwrap()
            .to_owned();
        activate(&mut game, card);
        let before = game.projection(View::Referee).unwrap()["P1"]["pp"]["current"].clone();
        play(&mut game, &id);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["pp"]["current"], before);
        assert_eq!(state["objects"]["enemy"]["hp"], 18_u8);
        assert_eq!(state["P2"]["leader"]["life"], 19_u8);
    }
}
#[test]
fn cp02_074_banishes_one_magic_item_and_pays_one_pp_to_deal_three() {
    for card in ["CP02-074", "CP02-P59"] {
        let mut initial = position(card);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":card}]);
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"fuel","card":"PR-155"}]);
        let mut game = game(&initial);
        activate(&mut game, card);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P1"]["ex"],
            json!([]),
            "Lesson consumes the Magic Item token"
        );
        assert_eq!(state["P1"]["pp"]["current"], 9_u8);
        assert_eq!(state["objects"]["enemy"]["hp"], 17_u8);
    }
}
#[test]
fn cp02_sp08a_searches_both_cost_groups_and_observes_both_idol_entries() {
    for card in ["CP02-SP08a", "CP02-SP08b", "CP02-U08a", "CP02-U08b"] {
        let mut initial = position(card);
        initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"large","card":"wire-idol-three"},{"id":"small","card":"wire-idol-one"},{"id":"other","card":"wire-history-spell"}]);
        let mut game = game(&initial);
        play(&mut game, "source");
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["field"], json!(["source", "large", "small"]));
        assert_eq!(state["P1"]["deck"], json!(["other"]));
        assert_eq!(state["objects"]["enemy"]["hp"], 16_u8);
    }
}
#[test]
fn cp01_012_returning_to_hand_still_resolves_its_damage_trigger() {
    for card in ["CP01-012", "CP01-P09"] {
        let mut initial = position(card);
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":card}]);
        let mut game = producer_game(
            &initial,
            event_cemetery::move_named(ZoneKind::Field, OwnerZone::Hand, "タイキシャトル"),
        );
        super::activate(&mut game, "P1");
        settle(&mut game);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["hand"], json!(["source"]));
        assert_eq!(state["objects"]["enemy"]["hp"], 18_u8);
    }
}
#[test]
fn pr_440_damages_a_follower_and_draws_with_a_passion_ally() {
    let card = "PR-440";
    let mut initial = position(card);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"friend","card":"wire-passion"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["hp"], 15_u8);
    assert_eq!(state["P1"]["hand"], json!(["P1-draw0"]));
}
#[test]
fn pr_624_evolution_deals_six_and_heals_two() {
    let card = "PR-624";
    let mut game = game(&evolution(card));
    evolve(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["hp"], 14_u8);
    assert_eq!(state["P1"]["leader"]["life"], 12_u8);
}
#[test]
fn sd02_001_first_mode_destroys_the_declared_enemy() {
    let card = "SD02-001";
    let mut game = game(&position(card));
    play(&mut game, "source");
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["cemetery"],
        json!(["enemy"])
    );
}
#[test]
fn sd02_005_activation_pays_one_and_grants_bane() {
    let card = "SD02-005";
    let mut initial = position(card);
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":card}]);
    let mut game = game(&initial);
    activate(&mut game, card);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["pp"]["current"], 9_u8);
    assert!(
        state["objects"]["source"]["keywords"]
            .as_array()
            .unwrap()
            .contains(&json!("必殺"))
    );
}
#[test]
fn sd02_006_rush_attack_buffs_the_other_ally_by_two_power() {
    let card = "SD02-006";
    let mut initial = position(card);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"friend","card":"wire-plain"}]);
    initial["players"]["P2"]["zones"]["field"][0]["state"] = json!({"acted":true});
    let mut game = game(&initial);
    play(&mut game, "source");
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"enemy"}))
            .unwrap()
            .outcome,
        "resolved",
        "Rush permits the entry-turn follower attack"
    );
    settle(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["friend"]["power"],
        5_u8
    );
}
#[test]
fn pcs01_019_evolution_ub_uses_a_real_spell_play_to_damage_the_leader() {
    let card = "PCS01-019";
    let mut initial = evolution(card);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"spell","card":"wire-history-spell"}]);
    let mut game = game(&initial);
    play(&mut game, "spell");
    evolve(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["hp"], 18_u8);
    assert_eq!(state["P2"]["leader"]["life"], 18_u8);
}

fn evolve_mode(game: &mut Game, mode: u64) {
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
        )
        .unwrap()
        .outcome,
        "resolved",
        "the external base evolves into the complete current card"
    );
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("evolution occurrence")
    };
    let step = game
        .answer_internal_card(
            pending.actor,
            pending.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap();
    let CardPlay::Awaiting(choice) = step else {
        panic!("evolution choice")
    };
    assert!(
        matches!(choice.domain, InputDomain::Modes { .. }),
        "the evolution offers its two printed modes"
    );
    let chosen = game
        .answer_internal_card(
            choice.actor,
            choice.token,
            Answer::Modes(vec![ModeId(mode)]),
        )
        .unwrap();
    finish(game, chosen);
    settle(game);
}
#[test]
fn bp21_095_first_evolution_mode_creates_and_strengthens_a_holy_cavalry() {
    for card in ["BP21-095", "BP21-SL25"] {
        let mut game = game(&evolution(card));
        evolve_mode(&mut game, 1);
        let state = game.projection(View::Referee).unwrap();
        let field = state["P1"]["field"].as_array().unwrap();
        assert_eq!(field.len(), 2);
        let id = field
            .iter()
            .filter_map(Value::as_str)
            .find(|id| *id != "source")
            .unwrap();
        let keywords = state["objects"][id]["keywords"].as_array().unwrap();
        assert!(keywords.contains(&json!("指定攻撃")));
        assert!(keywords.contains(&json!("突進")));
        assert_eq!(state["objects"][id]["power"], 2_u8);
    }
}
#[test]
fn bp21_095_second_evolution_mode_creates_the_admitted_crest_in_ex() {
    for card in ["BP21-095", "BP21-SL25"] {
        let mut game = game(&evolution(card));
        evolve_mode(&mut game, 2);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["field"], json!(["source"]));
        assert_eq!(state["P1"]["ex"].as_array().unwrap().len(), 1);
    }
}
