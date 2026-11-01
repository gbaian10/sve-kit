#![expect(
    clippy::panic,
    reason = "Unexpected questions invalidate these printed scenarios."
)]
use super::event_cemetery::{finish_checks, move_named};
use super::*;
use alloc::collections::BTreeMap;

const CONTROLS: [(&str, &str, &str, &str, u8); 9] = [
    (
        "eventx-mercenary",
        "eventx mercenary",
        "傭兵",
        "ニュートラル",
        2,
    ),
    (
        "eventx-aqua",
        "eventx aqua",
        "アクアフォース",
        "ニュートラル",
        1,
    ),
    ("eventx-other", "eventx other", "反例", "ニュートラル", 1),
    ("eventx-dragon", "eventx dragon", "武闘竜人", "ドラゴン", 4),
    (
        "eventx-priconne",
        "eventx priconne",
        "プリコネ",
        "ニュートラル",
        1,
    ),
    ("eventx-fairy", "eventx fairy", "妖精", "エルフ", 1),
    (
        "eventx-shikigami-base",
        "鬼呼びの導師",
        "式神",
        "ウィッチ",
        2,
    ),
    (
        "eventx-dragon-base",
        "彩尾の風紀委員",
        "武闘竜人",
        "ドラゴン",
        4,
    ),
    ("eventx-fairy-base", "妖精剣の担い手", "妖精", "エルフ", 3),
];

fn selected() -> &'static SelectedEffects {
    static READY: OnceLock<SelectedEffects> = OnceLock::new();
    READY.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = producer_inputs();
        for (number, name, tribe, class, power) in CONTROLS {
            snapshot.push_str(&json!({"number":number,"faces":[{
                "name":name,"card_class":class,"card_type":"フォロワー","traits":[tribe],
                "cost":"1","power":power.to_string(),"hp":"6","text":null,"sections":[]
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

fn bases() -> &'static BTreeMap<String, WellFormedCardProgram> {
    static BASES: OnceLock<BTreeMap<String, WellFormedCardProgram>> = OnceLock::new();
    BASES.get_or_init(|| {
        let mut tree = expanded::tree().clone();
        let mut base = tree["cards"]["BP01-041"].clone();
        let mut ability = base["abilities"][0].clone();
        ability.as_object_mut().unwrap().remove("activate");
        ability["evolve"] = json!({"cost":{"pp":1_u8}});
        base["abilities"] = json!([ability]);
        tree["cards"] = json!({"eventx-shikigami-base":base,"eventx-dragon-base":base,"eventx-fairy-base":base});
        lowerer().lower(&tree).unwrap()
    })
}

fn position(number: &str, zone: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    initial["players"]["P1"]["zones"][zone] = json!([{"id":"source","card":number}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"d1","card":"eventx-other"},{"id":"d2","card":"eventx-other"},
        {"id":"d3","card":"eventx-other"},{"id":"d4","card":"eventx-other"}
    ]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"eventx-other","state":{"acted":true}}]);
    initial
}

fn fixture(initial: &Value) -> Game {
    let mut game = Game::from_selected_effects(
        selected().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "eventx-migrations",
    )
    .unwrap();
    let empty = WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![],
    })
    .unwrap();
    for (number, ..) in CONTROLS {
        game.load_internal_card(number, bases().get(number).unwrap_or(&empty))
            .unwrap();
    }
    game
}

fn finish(game: &mut Game, mut play: CardPlay, take: bool) {
    while let CardPlay::Awaiting(q) = play {
        let answer = match &q.domain {
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            InputDomain::Targets(d) => {
                assert!(
                    d.members.len() <= 1,
                    "each printed choice has at most one legal target"
                );
                Answer::Targets(if take || d.cardinality.minimum.0 > 0 {
                    d.members.iter().map(|m| m.handle).collect()
                } else {
                    vec![]
                })
            }
            InputDomain::Order(cards) => Answer::Order(cards.iter().map(|m| m.handle).collect()),
            InputDomain::MealPayment { .. }
            | InputDomain::Quick { .. }
            | InputDomain::OptionalCost
            | InputDomain::CostMaterials(_)
            | InputDomain::BeforeDraw
            | InputDomain::Pending(_)
            | InputDomain::TriggerQuota(_)
            | InputDomain::Integer(_)
            | InputDomain::DamageReplacements(_)
            | InputDomain::DriveTrigger { .. }
            | InputDomain::Search(_)
            | InputDomain::SearchPartition(_)
            | InputDomain::Distribution(_)
            | InputDomain::SearchReplacement
            | InputDomain::TokenCapacity { .. }
            | InputDomain::Modes { .. } => {
                panic!("only printed target, Ward or bottom-order choices are expected")
            }
        };
        play = game.answer_internal_card(q.actor, q.token, answer).unwrap();
    }
    assert!(
        !matches!(play, CardPlay::Cancelled),
        "the printed instruction resolves"
    );
    finish_checks(game);
}

fn resolve(game: &mut Game, number: &str, take: bool) {
    let q = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &q.domain else {
        panic!("printed pending ability");
    };
    let matches: Vec<_> = options.iter().filter(|o| o.card == number).collect();
    assert_eq!(
        matches.len(),
        1,
        "the expected printed ability is pending once"
    );
    let play = game
        .answer_internal_card(q.actor, q.token, Answer::Pending(matches[0].handle))
        .unwrap();
    finish(game, play, take);
}

fn evolve(game: &mut Game) {
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
        )
        .unwrap()
        .outcome,
        "resolved",
        "the matching base evolves by its printed cost"
    );
}

fn play(game: &mut Game, id: &str) {
    let result = game.play_internal_card(id, PlayerId { seat: 1 }).unwrap();
    finish(game, result, true);
}

fn only_created(state: &Value, zone: &str, number: &str) -> String {
    let matching: Vec<_> = state["P1"][zone]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|id| {
            let id = id.as_str().unwrap();
            (state["objects"][id]["card"] == number).then(|| id.to_owned())
        })
        .collect();
    assert_eq!(
        matching.len(),
        1,
        "exactly one printed token was created in its destination"
    );
    matching[0].clone()
}

#[test]
fn evolved_shikigami_printings_create_the_real_token_and_strengthen_that_entry() {
    for number in ["BP06-045", "BP06-P12"] {
        let mut initial = position("eventx-shikigami-base", "field");
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":number}]);
        let mut game = fixture(&initial);
        evolve(&mut game);
        resolve(&mut game, number, true);
        resolve(&mut game, number, true);
        let state = game.projection(View::Referee).unwrap();
        let token = only_created(&state, "field", "BP06-T02");
        assert_eq!(state["objects"][&token]["power"], 3_u8);
        assert!(
            state["objects"][&token]["keywords"]
                .as_array()
                .unwrap()
                .contains(&json!("突進")),
            "the actual entering token gains Rush"
        );
        assert_eq!(state["objects"]["source"]["power"], 4_u8);
    }
}

#[test]
fn historian_creates_an_artifact_then_observes_its_token_entry_for_two_damage() {
    let mut game = fixture(&position("BP08-016", "hand"));
    play(&mut game, "source");
    resolve(&mut game, "BP08-016", true);
    resolve(&mut game, "BP08-016", true);
    let state = game.projection(View::Referee).unwrap();
    only_created(&state, "field", "BP05-T04");
    assert_eq!(state["objects"]["enemy"]["hp"], 4_u8);
    assert!(
        game.internal_pending_choice().unwrap().is_none(),
        "the ordinary historian entry is not a token event"
    );
}

#[test]
fn matros_printings_strengthen_only_the_matching_entering_mercenary() {
    for number in ["BP10-005", "PR-483"] {
        for (control, expected) in [("eventx-mercenary", true), ("eventx-other", false)] {
            let mut initial = position(number, "field");
            initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"material","card":control}]);
            let mut game = fixture(&initial);
            play(&mut game, "material");
            assert_eq!(game.internal_pending_choice().unwrap().is_some(), expected);
            if expected {
                resolve(&mut game, number, true);
            }
            let state = game.projection(View::Referee).unwrap();
            let power = if expected { 3_u8 } else { 1 };
            assert_eq!(state["objects"]["material"]["power"], power);
            assert_eq!(
                state["objects"]["material"]["hp"],
                6_u8 + u8::from(expected)
            );
            assert_eq!(
                state["objects"]["material"]["keywords"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("突進")),
                expected
            );
        }
    }
}

#[test]
fn evolved_dragon_printings_pick_only_a_dragon_and_strengthen_the_actual_attacker() {
    for number in ["BP18-067", "BP18-P27"] {
        for take in [false, true] {
            let mut initial = position("eventx-dragon-base", "field");
            initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":number}]);
            initial["players"]["P1"]["zones"]["deck"] = json!([
                {"id":"dragon","card":"eventx-dragon"},{"id":"other","card":"eventx-other"},{"id":"tail","card":"eventx-other"}
            ]);
            initial["players"]["P1"]["zones"]["field"].as_array_mut().unwrap().push(json!({"id":"attacker","card":"eventx-dragon","state":{"entered_this_turn":false}}));
            let mut game = fixture(&initial);
            evolve(&mut game);
            resolve(&mut game, number, take);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["P1"]["hand"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("dragon")),
                take
            );
            assert_eq!(
                game.projection(View::P2).unwrap()["known_cards"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|card| card["id"] == "dragon"),
                take,
                "only the chosen Dragon card is revealed to the opponent"
            );
            assert_eq!(state["P1"]["deck"][0], json!("tail"));
            assert_eq!(
                state["P1"]["deck"].as_array().unwrap().len(),
                if take { 2_usize } else { 3 }
            );
            assert_eq!(
                game.internal_rule_action(
                    &json!({"do":"attack","attacker":"attacker","target":"P2.leader"})
                )
                .unwrap()
                .outcome,
                "resolved"
            );
            resolve(&mut game, number, true);
            assert_eq!(
                game.projection(View::Referee).unwrap()["objects"]["attacker"]["power"],
                5_u8
            );
        }
    }
}

#[test]
fn fairy_sword_printings_create_ex_fairy_and_only_buff_a_token_fairy_entry() {
    for number in ["BP20-013", "BP20-P07"] {
        let mut game = fixture(&position(number, "hand"));
        play(&mut game, "source");
        resolve(&mut game, number, true);
        let state = game.projection(View::Referee).unwrap();
        let fairy = only_created(&state, "ex", "BP01-T03");
        play(&mut game, &fairy);
        resolve(&mut game, number, true);
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"][&fairy]["power"],
            2_u8
        );
        let mut initial = position(number, "field");
        initial["players"]["P1"]["zones"]["hand"] =
            json!([{"id":"ordinary","card":"eventx-fairy"}]);
        let mut non_token = fixture(&initial);
        play(&mut non_token, "ordinary");
        assert!(
            non_token.internal_pending_choice().unwrap().is_none(),
            "a non-token fairy is excluded"
        );
    }
}

#[test]
fn evolved_fairy_sword_printings_deal_two_then_buff_a_later_token_entry() {
    for number in ["BP20-014", "BP20-P08"] {
        let mut initial = position("eventx-fairy-base", "field");
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":number}]);
        initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"fairy","card":"BP01-T03"}]);
        let mut game = fixture(&initial);
        evolve(&mut game);
        resolve(&mut game, number, true);
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["enemy"]["hp"],
            4_u8
        );
        play(&mut game, "fairy");
        resolve(&mut game, number, true);
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["fairy"]["power"],
            2_u8
        );
    }
}

#[test]
fn aurelia_printings_select_the_opposing_leader_and_check_its_current_field_count() {
    for number in ["BP01-028", "BP01-SL06"] {
        for count in [2_u8, 3] {
            let mut initial = position(number, "hand");
            initial["players"]["P2"]["zones"]["field"] = Value::Array(
                (0..count)
                    .map(|i| json!({"id":format!("enemy{i}"),"card":"eventx-other"}))
                    .collect(),
            );
            let mut game = fixture(&initial);
            play(&mut game, "source");
            resolve(&mut game, number, true);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["objects"]["source"]["power"],
                if count == 3 { 6_u8 } else { 4 }
            );
            assert_eq!(
                state["objects"]["source"]["hp"],
                if count == 3 { 8_u8 } else { 6 }
            );
            assert_eq!(
                state["objects"]["source"]["keywords"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("オーラ")),
                count == 3
            );
        }
    }
}

fn attack_three(game: &mut Game) {
    for id in ["a1", "a2", "a3"] {
        assert_eq!(
            game.internal_rule_action(&json!({"do":"attack","attacker":id,"target":"P2.leader"}))
                .unwrap()
                .outcome,
            "resolved",
            "each declaration actually commits"
        );
        let q = game.internal_quick_choice().unwrap().unwrap();
        let play = game
            .answer_internal_card(q.actor, q.token, Answer::Quick(None))
            .unwrap();
        finish(game, play, true);
    }
}

fn attackers(initial: &mut Value, third: &str) {
    for (id, card) in [("a1", "eventx-aqua"), ("a2", "eventx-aqua"), ("a3", third)] {
        initial["players"]["P1"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":id,"card":card,"state":{"entered_this_turn":false}}));
    }
}

fn end(game: &mut Game) {
    assert_eq!(
        game.internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap()
            .outcome,
        "resolved",
        "the own End timing is reached"
    );
}

#[test]
fn sapphire_priest_printings_draw_three_only_after_three_actual_own_attacks() {
    for number in ["BP02-098", "BP02-P29"] {
        for attacked in [false, true] {
            let mut initial = position(number, "field");
            attackers(&mut initial, "eventx-other");
            let mut game = fixture(&initial);
            if attacked {
                attack_three(&mut game);
            }
            end(&mut game);
            resolve(&mut game, number, true);
            assert_eq!(
                game.projection(View::Referee).unwrap()["P1"]["hand"]
                    .as_array()
                    .unwrap()
                    .len(),
                if attacked { 3_usize } else { 0 }
            );
        }
    }
}

#[test]
fn medical_officer_printings_count_only_aqua_force_attacks_for_end_healing() {
    for number in ["CP03-018", "CP03-P12"] {
        for (third, expected) in [("eventx-aqua", 12_u8), ("eventx-other", 10)] {
            let mut initial = position(number, "field");
            initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
            attackers(&mut initial, third);
            let mut game = fixture(&initial);
            attack_three(&mut game);
            end(&mut game);
            resolve(&mut game, number, true);
            assert_eq!(
                game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
                expected
            );
        }
    }
}

#[test]
fn acorn_charge_printings_offer_only_priconne_from_the_top_then_heal_when_acted() {
    for number in ["CP04-108", "CP04-P84", "PCS01-046"] {
        for (card, take) in [
            ("eventx-priconne", true),
            ("eventx-priconne", false),
            ("eventx-other", true),
        ] {
            let mut initial = position(number, "hand");
            initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
            initial["players"]["P1"]["zones"]["deck"] =
                json!([{"id":"top","card":card},{"id":"tail","card":"eventx-other"}]);
            initial["players"]["P1"]["zones"]["field"] =
                json!([{"id":"producer","card":"producer"}]);
            let mut game = fixture(&initial);
            let Action::Move { objects, .. } =
                move_named(ZoneKind::Field, OwnerZone::Hand, "ドングリチャージ")
            else {
                panic!("named posture fixture");
            };
            game.load_internal_card(
                "producer",
                &producer(Action::Posture {
                    objects,
                    posture: Posture::Acted,
                }),
            )
            .unwrap();
            play(&mut game, "source");
            resolve(&mut game, number, take);
            let state = game.projection(View::Referee).unwrap();
            let picked = take && card == "eventx-priconne";
            assert_eq!(
                game.projection(View::P2).unwrap()["known_cards"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|known| known["id"] == "top"),
                picked,
                "the chosen Priconne card becomes public while an unchosen top stays private"
            );
            assert_eq!(
                state["P1"]["hand"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("top")),
                picked
            );
            assert_eq!(
                state["P1"]["deck"][0],
                json!(if picked { "tail" } else { "top" })
            );
            activate(&mut game, "P1");
            finish_checks(&mut game);
            resolve(&mut game, number, true);
            assert_eq!(
                game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
                11_u8
            );
        }
    }
}
