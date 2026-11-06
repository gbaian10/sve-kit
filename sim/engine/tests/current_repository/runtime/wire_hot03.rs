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
const CONTROLS: [Companion; 3] = [
    (
        "wire-plain",
        "plain",
        "ニュートラル",
        "フォロワー",
        "1",
        &[],
    ),
    (
        "wire-academy",
        "academy",
        "エルフ",
        "フォロワー",
        "2",
        &["学院"],
    ),
    (
        "wire-machina",
        "デウスエクスマキナ",
        "ニュートラル",
        "フォロワー",
        "6",
        &[],
    ),
];
const FORMS: [&str; 3] = ["CP02-010", "BP21-032", "BP21-086"];
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
        lowerer().lower(&tree).unwrap()
    })
}

fn load_controls(game: &mut Game) {
    let empty = WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![],
    })
    .unwrap();
    for (number, ..) in CONTROLS {
        game.load_internal_card(number, &empty).unwrap();
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
        "wire-hot03",
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
fn bp21_108_sacrifices_itself_as_its_amulet_payment_and_drains_one_life() {
    for card in ["BP21-108", "BP21-P60"] {
        let mut initial = position(card);
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":card}]);
        let mut game = game(&initial);
        activate(&mut game, card);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["cemetery"], json!(["source"]));
        assert_eq!(state["P1"]["leader"]["life"], 11_u8);
        assert_eq!(state["P2"]["leader"]["life"], 19_u8);
    }
}
#[test]
fn bp10_044_fanfare_pays_four_and_one_stack_to_banish_a_cheap_follower() {
    for card in ["BP10-044", "BP10-P10"] {
        let mut initial = position(card);
        initial["players"]["P1"]["zones"]["field"] =
            json!([{"id":"soil","card":"BP03-T02","state":{"counters":{"stack_counter":2_u8}}}]);
        let mut game = game(&initial);
        play(&mut game, "source");
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["pp"]["current"], 5_u8);
        assert_eq!(
            state["objects"]["soil"]["counters"]["スタックカウンター"],
            1_u8
        );
        assert_eq!(state["P2"]["banish"], json!(["enemy"]));
    }
}
#[test]
fn bp10_045_lastword_creates_an_earth_sigil_then_adds_a_second_stack() {
    for card in ["BP10-045", "BP10-P11"] {
        let mut initial = position(card);
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":card}]);
        let Action::Move { objects, .. } =
            event_cemetery::move_named(ZoneKind::Field, OwnerZone::Cemetery, "猫の奇術師")
        else {
            panic!("selector")
        };
        let mut game = producer_game(&initial, Action::Destroy { objects });
        super::activate(&mut game, "P1");
        settle(&mut game);
        let state = game.projection(View::Referee).unwrap();
        let soil = state["P1"]["field"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(Value::as_str)
            .find(|id| *id != "producer")
            .unwrap();
        assert_eq!(
            state["objects"][soil]["counters"]["スタックカウンター"],
            2_u8
        );
        assert!(
            state["P1"]["cemetery"]
                .as_array()
                .unwrap()
                .contains(&json!("source"))
        );
    }
}
#[test]
fn bp10_050_first_mode_creates_a_stack_when_no_sigil_exists() {
    for card in ["BP10-050", "PR-240"] {
        let mut game = game(&position(card));
        play(&mut game, "source");
        let state = game.projection(View::Referee).unwrap();
        let field = state["P1"]["field"].as_array().unwrap();
        assert_eq!(field.len(), 2);
        let id = field
            .iter()
            .filter_map(Value::as_str)
            .find(|id| *id != "source")
            .unwrap();
        assert_eq!(state["objects"][id]["counters"]["スタックカウンター"], 1_u8);
    }
}
#[test]
fn cp02_010_evolution_bottoms_an_enemy_with_three_hp() {
    for card in ["CP02-010", "CP02-P07"] {
        let mut initial = evolution(card);
        initial["players"]["P2"]["zones"]["field"][0]["state"] = json!({"hp":3_u8});
        let mut game = game(&initial);
        evolve(&mut game);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P2"]["field"], json!([]));
        assert_eq!(
            state["P2"]["deck"].as_array().unwrap().last(),
            Some(&json!("enemy"))
        );
    }
}
#[test]
fn bp21_032_evolution_enters_a_cheap_follower_and_its_entry_heals_one() {
    for card in ["BP21-032", "BP21-P17"] {
        let mut initial = evolution(card);
        initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"friend","card":"wire-academy"},{"id":"other1","card":"wire-plain"},{"id":"other2","card":"wire-plain"},{"id":"other3","card":"wire-plain"},{"id":"untouched","card":"wire-plain"}]);
        let mut game = game(&initial);
        evolve(&mut game);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["field"], json!(["source", "friend"]));
        assert_eq!(state["P1"]["leader"]["life"], 11_u8);
        assert_eq!(
            state["P1"]["deck"],
            json!(["untouched", "other1", "other2", "other3"])
        );
    }
}
#[test]
fn bp22_099_creates_a_holy_falcon_and_damages_the_enemy_for_its_entry() {
    for card in ["BP22-099", "BP22-P54"] {
        let mut game = game(&position(card));
        play(&mut game, "source");
        activate(&mut game, card);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["enemy"]["hp"], 17_u8);
        let field = state["P1"]["field"].as_array().unwrap();
        assert_eq!(field.len(), 2);
        let id = field
            .iter()
            .filter_map(Value::as_str)
            .find(|id| *id != "source")
            .unwrap();
        assert!(
            state["objects"][id]["keywords"]
                .as_array()
                .unwrap()
                .contains(&json!("疾走"))
        );
    }
}
#[test]
fn cp04_070_attack_ub_moves_the_attacked_follower_to_its_owners_ex() {
    for card in ["CP04-070", "CP04-P54"] {
        let mut initial = position(card);
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":card}]);
        initial["players"]["P2"]["zones"]["field"][0]["state"] = json!({"acted":true});
        let mut game = game(&initial);
        assert_eq!(
            game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"enemy"}))
                .unwrap()
                .outcome,
            "resolved",
            "the attack declares its printed UB"
        );
        settle(&mut game);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P2"]["field"], json!([]));
        assert_eq!(state["P2"]["ex"], json!(["enemy"]));
    }
}
#[test]
fn bp15_100_searches_machina_and_creates_an_ancient_artifact() {
    let card = "BP15-100";
    let mut initial = position(card);
    initial["players"]["P1"]["zones"]["deck"] =
        json!([{"id":"machina","card":"wire-machina"},{"id":"plain","card":"wire-plain"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    let field = state["P1"]["field"].as_array().unwrap();
    assert_eq!(field.len(), 2);
    assert!(field.contains(&json!("machina")));
    let id = field
        .iter()
        .filter_map(Value::as_str)
        .find(|id| *id != "machina")
        .unwrap();
    assert!(
        state["objects"][id]["keywords"]
            .as_array()
            .unwrap()
            .contains(&json!("突進"))
    );
}
#[test]
fn cp04_013_can_be_played_on_its_owners_fifth_turn() {
    for card in ["CP04-013", "CP04-P09"] {
        let mut initial = position(card);
        initial["turn"]["elapsed_turns"]["P1"] = json!(4_u8);
        let mut early = game(&initial);
        early.play_internal_card("source", PLAYER).unwrap_err();
        assert_eq!(
            early.projection(View::Referee).unwrap()["P1"]["hand"],
            json!(["source"])
        );
        initial["turn"]["elapsed_turns"]["P1"] = json!(5_u8);
        let mut game = game(&initial);
        play(&mut game, "source");
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["field"], json!(["source"]));
        assert!(
            state["objects"]["source"]["keywords"]
                .as_array()
                .unwrap()
                .contains(&json!("守護"))
        );
    }
}
#[test]
fn cp01_006_fanfare_may_put_the_top_card_on_the_bottom() {
    for card in ["CP01-006", "CP01-P03"] {
        let mut game = game(&position(card));
        play(&mut game, "source");
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P1"]["deck"],
            json!([
                "P1-draw1", "P1-draw2", "P1-draw3", "P1-draw4", "P1-draw5", "P1-draw0"
            ])
        );
    }
}
#[test]
fn pr_036_fanfare_grants_storm_when_awakened() {
    for card in ["PR-036", "SD04-014"] {
        let mut game = game(&position(card));
        play(&mut game, "source");
        assert!(
            game.projection(View::Referee).unwrap()["objects"]["source"]["keywords"]
                .as_array()
                .unwrap()
                .contains(&json!("疾走"))
        );
    }
}
#[test]
fn bp21_085_attacking_a_follower_deals_its_power_to_the_enemy_leader() {
    for card in ["BP21-085", "BP21-P46"] {
        let mut initial = position(card);
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":card}]);
        let mut game = game(&initial);
        let power = game.projection(View::Referee).unwrap()["objects"]["source"]["power"]
            .as_u64()
            .unwrap();
        assert_eq!(
            game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"enemy"}))
                .unwrap()
                .outcome,
            "resolved",
            "Assail permits attacking the standing follower"
        );
        settle(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
            20_u64.checked_sub(power).unwrap()
        );
    }
}
#[test]
fn bp21_086_evolution_deals_four_to_the_selected_enemy() {
    for card in ["BP21-086", "BP21-P47"] {
        let mut game = game(&evolution(card));
        evolve(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["enemy"]["hp"],
            16_u8
        );
    }
}
