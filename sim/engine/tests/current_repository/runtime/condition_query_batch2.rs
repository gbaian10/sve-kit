#![expect(
    clippy::panic,
    reason = "Unexpected questions invalidate these printed card scenarios."
)]
use super::*;
use core::iter::once;
use std::{fs::read_to_string, path::PathBuf};
use sve_engine::catalog::EngineIdentityInput;

const CONTROLS: [(&str, &str, &[&str], u8); 4] = [
    ("plain", "普通の仲間", &[], 2),
    ("evolution-BP11-077", "異形", &[], 1),
    ("wilderness", "荒野の仲間", &["荒野"], 2),
    ("horse", "ウマ娘の仲間", &["ウマ娘"], 2),
];
fn prepared() -> &'static SelectedEffects {
    static READY: OnceLock<SelectedEffects> = OnceLock::new();
    READY.get_or_init(|| {
        let(mut snapshot,keywords,mut input)=inputs().clone();
        for(number,name,traits,cost) in CONTROLS {
            snapshot.push_str(&json!({"number":number,"faces":[{"name":name,"card_class":"ニュートラル","card_type":"フォロワー","traits":traits,"cost":cost.to_string(),"power":"3","hp":"6","text":null,"sections":[]}]}).to_string());snapshot.push('\n');
            input["documents"].as_array_mut().unwrap().push(json!([format!("{number}.yaml"),json!({"format":1_u8,"kind":"effect_set","cards":{number:{"status":"complete","abilities":[]}}}).to_string()]));
        }
        let path=PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../authored/rules/engine/index.yaml");
        let mut rules:Value=serde_saphyr::from_str(&read_to_string(path).unwrap()).unwrap();
        rules["input"]=rule_fixtures::rules(&snapshot,json!([]),json!([]))["input"].clone();
        repository::select_with_rules(&snapshot,&keywords,&input,&rules.to_string(),&EngineIdentityInput::LegacyJp).unwrap()
    })
}
fn position(number: &str, zone: &str) -> Value {
    let mut state = draw_position();
    state["players"]["P1"]["zones"][zone] = json!([{"id":"source","card":number}]);
    state["players"]["P1"]["zones"]["deck"] =
        json!([{"id":"draw","card":"plain"},{"id":"tail","card":"plain"}]);
    state["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"plain"}]);
    state
}
fn fixture(state: &Value) -> Game {
    let mut game = Game::from_selected_effects(
        prepared().clone(),
        state,
        &Value::Null,
        &Value::Null,
        "condition-batch2",
    )
    .unwrap();
    let empty = WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![],
    })
    .unwrap();
    for (number, ..) in CONTROLS {
        // The external evolution producer receives its intrinsic program below.
        if number != "evolution-BP11-077" {
            game.load_internal_card(number, &empty).unwrap();
        }
    }
    let mut tree = expanded::tree().clone();
    tree["cards"] = json!({"evolution-BP11-077":tree["cards"]["synthetic-evolution"]});
    let program = lowerer()
        .lower(&tree)
        .unwrap()
        .remove("evolution-BP11-077")
        .unwrap();
    game.load_internal_card("evolution-BP11-077", &program)
        .unwrap();
    game
}
fn finish(game: &mut Game, mut play: CardPlay, targets: &[&str]) -> CardPlay {
    let mut next = targets.iter();
    while let CardPlay::Awaiting(q) = &play {
        let answer = match &q.domain {
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            InputDomain::OptionalCost => Answer::OptionalCost(true),
            InputDomain::CostMaterials(d) => Answer::CostMaterials(
                d.members
                    .iter()
                    .take(usize::try_from(d.cardinality.minimum.0).unwrap())
                    .map(|m| m.handle)
                    .collect(),
            ),
            InputDomain::Targets(d) if d.members.is_empty() && d.cardinality.minimum == Nat(0) => {
                Answer::Targets(vec![])
            }
            InputDomain::Targets(d) => {
                let Some(wanted) = next.next() else {
                    panic!("each target has an independent printed expectation")
                };
                let handle = d
                    .members
                    .iter()
                    .find(|m| m.card.as_deref() == Some(*wanted))
                    .unwrap()
                    .handle;
                Answer::Targets(vec![handle])
            }
            InputDomain::Order(cards) => {
                Answer::Order(cards.iter().rev().map(|c| c.handle).collect())
            }
            InputDomain::ResourceEntryPayment { .. }
            | InputDomain::Quick { .. }
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
            | InputDomain::Modes { .. } => panic!("unexpected printed question"),
        };
        play = game.answer_internal_card(q.actor, q.token, answer).unwrap();
    }
    assert!(
        next.next().is_none(),
        "every expected target question was asked"
    );
    play
}
fn trigger(game: &mut Game, number: &str, targets: &[&str]) -> CardPlay {
    let q = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &q.domain else {
        panic!("printed trigger")
    };
    let handle = options.iter().find(|o| o.card == number).unwrap().handle;
    let play = game
        .answer_internal_card(q.actor, q.token, Answer::Pending(handle))
        .unwrap();
    finish(game, play, targets)
}
fn enter(game: &mut Game, number: &str, targets: &[&str]) -> CardPlay {
    let play = game
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap();
    assert!(
        matches!(finish(game, play, &[]), CardPlay::Entered(_)),
        "real source enters before fanfare"
    );
    trigger(game, number, targets)
}

fn token_in(game: &Game, zone: &str, number: &str) -> String {
    let v = game.projection(View::Referee).unwrap();
    let ids = v["P1"][zone]
        .as_array()
        .unwrap()
        .iter()
        .filter(|id| v["objects"][id.as_str().unwrap()]["card"] == number)
        .collect::<Vec<_>>();
    assert_eq!(ids.len(), 1, "one real vehicle is created");
    ids[0].as_str().unwrap().to_owned()
}
fn vehicle(number: &str, keyword: &str, power: u8, hp: u8) {
    for wilderness in [false, true] {
        let card = if wilderness { "wilderness" } else { "plain" };
        let mut state = position(number, "field");
        state["players"]["P1"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"ally","card":card}));
        let mut game = fixture(&state);
        let p = game
            .activate_internal_card("source", AbilityId(1), PlayerId { seat: 1 })
            .unwrap();
        resolved(finish(&mut game, p, &[card]));
        let v = game.projection(View::Referee).unwrap();
        assert_eq!(
            v["objects"]["ally"]["power"],
            if wilderness { power } else { 3 },
            "only the selected wilderness follower receives the power bonus"
        );
        assert_eq!(
            v["objects"]["ally"]["hp"],
            if wilderness { hp } else { 6 },
            "only the selected wilderness follower receives the health bonus"
        );
        assert!(
            v["objects"]["ally"]["keywords"]
                .as_array()
                .unwrap()
                .contains(&json!(keyword)),
            "the granted keyword applies to either selected follower"
        );
        assert!(
            v["objects"].get("source").is_none(),
            "sacrificed token disappears"
        );
    }
}
#[test]
fn bp11_p32_grants_rush_before_the_selected_wilderness_power_bonus() {
    vehicle("BP11-P32", "突進", 4, 6);
}
#[test]
fn bp11_p33_grants_ward_before_the_selected_wilderness_health_bonus() {
    vehicle("BP11-P33", "守護", 3, 7);
}
#[test]
fn bp11_027_fanfare_creates_the_real_bike() {
    let mut g = fixture(&position("BP11-027", "hand"));
    resolved(enter(&mut g, "BP11-027", &[]));
    token_in(&g, "field", "BP11-P32");
    let mut lastword_position = position("BP11-027", "field");
    lastword_position["turn"]["active"] = json!("P2");
    lastword_position["players"]["P2"]["zones"]["hand"] =
        json!([{"id":"execution","card":"BP01-169"}]);
    let mut lastword_game = fixture(&lastword_position);
    let destruction = lastword_game
        .play_internal_card("execution", PlayerId { seat: 2 })
        .unwrap();
    resolved(finish(&mut lastword_game, destruction, &["BP11-027"]));
    lastword_game.resume_internal_card().unwrap();
    resolved(trigger(&mut lastword_game, "BP11-027", &[]));
    assert_eq!(
        lastword_game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
        18_u8
    );
}
#[test]
fn bp11_065_lastword_creates_the_real_bike() {
    let mut s = position("BP11-065", "field");
    s["turn"]["active"] = json!("P2");
    s["players"]["P2"]["zones"]["hand"] = json!([{"id":"execution","card":"BP01-169"}]);
    let mut g = fixture(&s);
    let p = g
        .play_internal_card("execution", PlayerId { seat: 2 })
        .unwrap();
    resolved(finish(&mut g, p, &["BP11-065"]));
    g.resume_internal_card().unwrap();
    resolved(trigger(&mut g, "BP11-065", &[]));
    token_in(&g, "field", "BP11-P32");
}
#[test]
fn bp11_099_creates_a_bike_and_pays_six_for_its_printed_buff() {
    let mut g = fixture(&position("BP11-099", "hand"));
    resolved(enter(&mut g, "BP11-099", &[]));
    token_in(&g, "ex", "BP11-P32");
    g.resume_internal_card().unwrap();
    let p = g
        .activate_internal_card("source", AbilityId(7), PlayerId { seat: 1 })
        .unwrap();
    resolved(finish(&mut g, p, &[]));
    let v = g.projection(View::Referee).unwrap();
    assert_eq!(v["P1"]["pp"]["current"], 2_u8);
    assert_eq!(v["objects"]["source"]["power"], 6_u8);
    assert_eq!(v["objects"]["source"]["hp"], 5_u8);
}
#[test]
fn cp01_018_grants_storm_to_both_kinds_but_buffs_only_umamusume() {
    for horse in [false, true] {
        let card = if horse { "horse" } else { "plain" };
        let mut s = position("CP01-018", "hand");
        s["players"]["P1"]["zones"]["field"] = json!([{"id":"ally","card":card}]);
        let mut g = fixture(&s);
        let p = g
            .play_internal_card("source", PlayerId { seat: 1 })
            .unwrap();
        resolved(finish(&mut g, p, &[card]));
        let v = g.projection(View::Referee).unwrap();
        assert_eq!(v["objects"]["ally"]["power"], if horse { 4_u8 } else { 3 });
        assert_eq!(v["objects"]["ally"]["hp"], if horse { 7_u8 } else { 6 });
        assert!(
            v["objects"]["ally"]["keywords"]
                .as_array()
                .unwrap()
                .contains(&json!("疾走"))
        );
    }
}
#[test]
fn bp11_077_real_evolved_form_deals_five_to_the_selected_enemy() {
    let mut s = position("evolution-BP11-077", "field");
    s["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP11-077"}]);
    let mut g = fixture(&s);
    assert_eq!(
        g.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
        )
        .unwrap()
        .outcome,
        "resolved"
    );
    resolved(trigger(&mut g, "BP11-077", &["plain"]));
    let v = g.projection(View::Referee).unwrap();
    assert_eq!(v["objects"]["enemy"]["damage"], 5_u8);
    let mut lastword_position = position("evolution-BP11-077", "field");
    lastword_position["players"]["P1"]["zones"]["field"][0]["state"] =
        json!({"evolved":true,"evolved_with":"form"});
    lastword_position["players"]["P1"]["zones"]["evolution"] =
        json!([{"id":"form","card":"BP11-077"}]);
    lastword_position["turn"]["active"] = json!("P2");
    lastword_position["players"]["P2"]["zones"]["hand"] =
        json!([{"id":"execution","card":"BP01-169"}]);
    let mut lastword_game = fixture(&lastword_position);
    let destruction = lastword_game
        .play_internal_card("execution", PlayerId { seat: 2 })
        .unwrap();
    resolved(finish(
        &mut lastword_game,
        destruction,
        &["evolution-BP11-077"],
    ));
    lastword_game.resume_internal_card().unwrap();
    resolved(trigger(&mut lastword_game, "BP11-077", &[]));
    token_in(&lastword_game, "field", "BP11-P32");
    let lastword_view = lastword_game.projection(View::Referee).unwrap();
    assert_eq!(lastword_view["P1"]["deck"], json!([]));
    assert_eq!(
        lastword_view["P1"]["cemetery"],
        json!(["source", "draw", "tail"])
    );
}
fn look_position(number: &str, pool: &[&str]) -> Value {
    let mut initial = position(
        number,
        if number == "BP02-003" {
            "field"
        } else {
            "hand"
        },
    );
    initial["players"]["P1"]["zones"]["deck"] = json!(
        pool.iter()
            .enumerate()
            .map(|(index, card)| json!({"id":format!("deck-{index}"),"card":card}))
            .chain(once(json!({"id":"tail","card":"plain"})))
            .collect::<Vec<_>>()
    );
    if number == "BP06-004" {
        initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"fairy","card":"BP01-T02"}]);
    }
    if number == "BP03-070" {
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP03-069"}]);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":number}]);
    }
    if number == "BP02-003" {
        initial["turn"]["active"] = json!("P2");
        initial["players"]["P2"]["zones"]["hand"] = json!([{"id":"execution","card":"BP01-169"}]);
    }
    initial
}
fn start_look(game: &mut Game, number: &str) -> CardPlay {
    if number == "BP02-003" {
        let destruction = game
            .play_internal_card("execution", PlayerId { seat: 2 })
            .unwrap();
        resolved(finish(game, destruction, &["BP02-003"]));
        game.resume_internal_card().unwrap();
    } else if number == "BP03-070" {
        // Its already admitted real base supplies the printed evolution cost.
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
            )
            .unwrap()
            .outcome,
            "resolved",
            "the real base produces this evolution trigger"
        );
    } else {
        let entry = game
            .play_internal_card("source", PlayerId { seat: 1 })
            .unwrap();
        assert!(
            matches!(finish(game, entry, &[]), CardPlay::Entered(_)),
            "the real source enters before its observation fanfare"
        );
    }
    start_trigger(game, number)
}
fn pay_look_costs(game: &mut Game, mut play: CardPlay) -> CardPlay {
    while let CardPlay::Awaiting(choice) = &play {
        let answer = if matches!(&choice.domain, InputDomain::OptionalCost) {
            Answer::OptionalCost(true)
        } else if let InputDomain::CostMaterials(materials) = &choice.domain {
            assert_eq!(
                materials.members.len(),
                1,
                "the real Fairy token is the sole payment material"
            );
            Answer::CostMaterials(vec![materials.members[0].handle])
        } else {
            break;
        };
        play = game
            .answer_internal_card(choice.actor, choice.token, answer)
            .unwrap();
    }
    play
}
fn assert_look_result(
    view: &Value,
    number: &str,
    pool: &[&str],
    chosen: Option<&str>,
    bottom: bool,
) {
    let chosen_index = chosen.map(|card| {
        pool.iter()
            .position(|candidate| *candidate == card)
            .unwrap()
    });
    assert_eq!(
        view["P1"]["hand"],
        json!(
            chosen_index
                .filter(|_| number != "BP02-003")
                .map(|index| vec![format!("deck-{index}")])
                .unwrap_or_default()
        ),
        "only a selected hand destination produces a hand card"
    );
    if number == "BP02-003" {
        let id = format!("deck-{}", chosen_index.unwrap());
        assert_eq!(
            view["P1"]["field"],
            json!([id]),
            "the selected Elf follower enters the field"
        );
        assert_eq!(
            view["P1"]["cemetery"],
            json!(["source"]),
            "the real Lastword source remains in the cemetery"
        );
    }
    let mut remaining = pool
        .iter()
        .enumerate()
        .filter(|(index, _)| Some(*index) != chosen_index)
        .map(|(index, _)| format!("deck-{index}"))
        .collect::<Vec<_>>();
    let expected = if bottom {
        remaining.reverse();
        let mut expected = vec!["tail".to_owned()];
        expected.extend(remaining);
        expected
    } else {
        remaining.push("tail".to_owned());
        remaining
    };
    assert_eq!(
        view["P1"]["deck"],
        json!(expected),
        "the remaining cards preserve the printed destination and chosen order"
    );
    if number == "BP06-004" {
        assert_eq!(
            view["P1"]["ex"],
            json!([]),
            "the Fairy token leaves EX as payment"
        );
        assert!(
            view["objects"].get("fairy").is_none(),
            "the paid Fairy token disappears"
        );
    }
}
fn look(number: &str, pool: &[&str], eligible: &[&str], chosen: Option<&str>, bottom: bool) {
    let initial = look_position(number, pool);
    let mut game = fixture(&initial);
    let initial_play = start_look(&mut game, number);
    let CardPlay::Awaiting(choice) = pay_look_costs(&mut game, initial_play) else {
        panic!("private Look choice")
    };
    let InputDomain::Targets(domain) = &choice.domain else {
        panic!("observed members")
    };
    assert!(
        game.internal_decision(View::P2).is_none(),
        "the opponent cannot read a private observation choice"
    );
    assert_eq!(
        domain.cardinality.minimum,
        Nat(0),
        "the printed observation permits declining"
    );
    assert_eq!(
        domain.cardinality.maximum,
        Some(Nat(1)),
        "the printed observation takes at most one card"
    );
    assert_eq!(
        domain
            .members
            .iter()
            .map(|member| member.card.as_deref().unwrap())
            .collect::<Vec<_>>(),
        eligible,
        "the complete offered set matches the printed filters"
    );
    let handles = chosen
        .map(|card| {
            vec![
                domain
                    .members
                    .iter()
                    .find(|member| member.card.as_deref() == Some(card))
                    .unwrap()
                    .handle,
            ]
        })
        .unwrap_or_default();
    let selected_play = game
        .answer_internal_card(choice.actor, choice.token, Answer::Targets(handles))
        .unwrap();
    resolved(finish(&mut game, selected_play, &[]));
    let view = game.projection(View::Referee).unwrap();
    assert_look_result(&view, number, pool, chosen, bottom);
}
fn start_trigger(g: &mut Game, number: &str) -> CardPlay {
    let q = g.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &q.domain else {
        panic!("printed trigger")
    };
    let handle = options.iter().find(|o| o.card == number).unwrap().handle;
    g.answer_internal_card(q.actor, q.token, Answer::Pending(handle))
        .unwrap()
}
#[test]
fn bp03_070_real_evolution_looks_for_a_dragon_follower() {
    look(
        "BP03-070",
        &["BP01-086", "BP01-090", "plain"],
        &["BP01-086"],
        Some("BP01-086"),
        true,
    );
}
#[test]
fn bp06_004_pays_a_real_fairy_token_then_can_take_an_elf_spell() {
    look(
        "BP06-004",
        &["BP01-009", "BP01-015", "plain", "BP01-090"],
        &["BP01-009", "BP01-015"],
        Some("BP01-015"),
        true,
    );
}
#[test]
fn bp06_066_awakened_fanfare_takes_an_original_five_cost_dragon_follower() {
    look(
        "BP06-066",
        &["BP01-086", "BP01-009", "BP03-069", "BP01-090", "BP01-015"],
        &["BP01-086"],
        Some("BP01-086"),
        true,
    );
    let mut inactive_position = position("BP06-066", "hand");
    inactive_position["players"]["P1"]["pp"] = json!({"current":6_u8,"max":6_u8});
    let mut inactive_game = fixture(&inactive_position);
    resolved(enter(&mut inactive_game, "BP06-066", &[]));
    let inactive_view = inactive_game.projection(View::Referee).unwrap();
    assert_eq!(inactive_view["P1"]["deck"], json!(["draw", "tail"]));
    assert_eq!(inactive_view["P1"]["hand"], json!([]));
}
#[test]
fn bp01_059_takes_a_spell_but_leaves_a_declined_top_card_in_place() {
    look(
        "BP01-059",
        &["BP01-071"],
        &["BP01-071"],
        Some("BP01-071"),
        false,
    );
    look("BP01-059", &["BP01-071"], &["BP01-071"], None, false);
}
#[test]
fn bp16_101_only_takes_an_amulet() {
    look(
        "BP16-101",
        &["BP01-016", "BP01-009", "BP01-071"],
        &["BP01-016"],
        Some("BP01-016"),
        true,
    );
}
#[test]
fn cp02_089_only_takes_an_amulet_from_five_cards() {
    look(
        "CP02-089",
        &["BP01-016", "BP01-009", "BP01-071", "plain", "wilderness"],
        &["BP01-016"],
        Some("BP01-016"),
        true,
    );
}
#[test]
fn bp22_103_reads_printed_ward_on_private_deck_members() {
    look(
        "BP22-103",
        &["BP01-086", "BP01-009", "BP01-071"],
        &["BP01-086"],
        Some("BP01-086"),
        true,
    );
}

#[test]
fn bp02_003_lastword_puts_the_selected_elf_follower_into_play() {
    look(
        "BP02-003",
        &["BP01-009", "BP01-015", "plain", "BP01-086"],
        &["BP01-009"],
        Some("BP01-009"),
        true,
    );
}
