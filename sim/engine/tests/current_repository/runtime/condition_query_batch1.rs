#![expect(
    clippy::panic,
    reason = "Unexpected questions invalidate these printed card scenarios."
)]
use super::*;
use std::{fs::read_to_string, path::PathBuf};
use sve_engine::catalog::EngineIdentityInput;

const CONTROLS: [(&str, &str, &[&str], u8); 12] = [
    ("plain", "普通の仲間", &[], 2),
    ("evolution-BP03-003", "コスモスファング", &[], 1),
    ("wilderness", "荒野の仲間", &["荒野"], 2),
    ("garyu", "覇道の龍人・ガリュウ", &[], 2),
    ("pirate", "八獄の盗賊", &["盗賊", "八獄"], 2),
    ("seiun", "セイウンスカイ", &["ウマ娘"], 2),
    ("queen", "テストのクイーンヴァンパイア", &[], 2),
    ("garmieux", "テストのガルミーユ", &[], 2),
    ("uzuki", "テストの島村卯月", &[], 2),
    ("rin", "テストの渋谷凛", &[], 2),
    ("vampire", "吸血鬼の仲間", &["吸血鬼"], 2),
    ("large", "五費の仲間", &[], 5),
];
fn prepared() -> &'static SelectedEffects {
    static READY: OnceLock<SelectedEffects> = OnceLock::new();
    READY.get_or_init(|| {
        let(mut snapshot,keywords,mut input)=inputs().clone();
        for(number,name,traits,cost) in CONTROLS {
            snapshot.push_str(&json!({"number":number,"faces":[{"name":name,"card_class":"ニュートラル","card_type":"フォロワー","traits":traits,"cost":cost.to_string(),"power":"3","hp":"6","text":null,"sections":[]}]}).to_string());snapshot.push('\n');
            input["documents"].as_array_mut().unwrap().push(json!([format!("{number}.yaml"),json!({"format":1_u8,"kind":"effect_set","cards":{number:{"status":"complete","abilities":[]}}}).to_string()]));
        }
        snapshot.push_str(&json!({"number":"external-form-BP15-084","faces":[{"name":"愛絶の崇拝者","card_class":"ニュートラル","card_type":"フォロワー・エボルヴ","traits":[],"cost":"-","power":"3","hp":"6","text":null,"sections":[]}]}).to_string());
        snapshot.push('\n');
        input["documents"].as_array_mut().unwrap().push(json!(["external-form.yaml",json!({"format":1_u8,"kind":"effect_set","cards":{"external-form-BP15-084":{"status":"complete","abilities":[]}}}).to_string()]));
        let path=PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../authored/rules/engine/index.yaml");
        let mut rules:Value=serde_saphyr::from_str(&read_to_string(path).unwrap()).unwrap();
        rules["input"]=rule_fixtures::rules(&snapshot,json!([]),json!([]))["input"].clone();
        Catalog::from_current_selection_with_rules(&snapshot,&keywords,&input,&rules.to_string(),&EngineIdentityInput::LegacyJp).unwrap()
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
        "query-batch1",
    )
    .unwrap();
    let empty = WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![],
    })
    .unwrap();
    for (number, ..) in CONTROLS {
        // The external evolution producer receives its intrinsic program below.
        if number != "evolution-BP03-003" {
            game.load_internal_card(number, &empty).unwrap();
        }
    }
    game.load_internal_card("external-form-BP15-084", &empty)
        .unwrap();
    let mut tree = expanded::tree().clone();
    tree["cards"] = json!({"evolution-BP03-003":tree["cards"]["synthetic-evolution"]});
    let program = lowerer()
        .lower(&tree)
        .unwrap()
        .remove("evolution-BP03-003")
        .unwrap();
    game.load_internal_card("evolution-BP03-003", &program)
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
            InputDomain::ResourceEntryPayment { .. }
            | InputDomain::Quick { .. }
            | InputDomain::BeforeDraw
            | InputDomain::Pending(_)
            | InputDomain::TriggerQuota(_)
            | InputDomain::Integer(_)
            | InputDomain::Order(_)
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

#[test]
fn bp11_p31_buffs_only_the_selected_wilderness_follower() {
    for (card, power, hp) in [("wilderness", 4_u8, 7_u8), ("plain", 3, 6)] {
        let mut state = position("BP11-P31", "field");
        state["players"]["P1"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"ally","card":card}));
        let mut game = fixture(&state);
        let play = game
            .activate_internal_card("source", AbilityId(1), PlayerId { seat: 1 })
            .unwrap();
        resolved(finish(&mut game, play, &[card]));
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["objects"]["ally"]["power"], power);
        assert_eq!(view["objects"]["ally"]["hp"], hp);
        assert!(
            view["objects"].get("source").is_none(),
            "sacrificed vehicle token disappears"
        );
    }
}
#[test]
fn bp11_105_creates_its_real_vehicle_in_ex() {
    let mut game = fixture(&position("BP11-105", "hand"));
    resolved(enter(&mut game, "BP11-105", &[]));
    let view = game.projection(View::Referee).unwrap();
    let id = view["P1"]["ex"][0].as_str().unwrap();
    assert_eq!(view["objects"][id]["card"], "BP11-P31");
    assert_eq!(view["P1"]["pp"]["current"], 9_u8);
}
#[test]
fn bp11_023_pays_two_vehicle_acts_before_its_damage() {
    let mut state = position("BP11-023", "field");
    for id in ["v1", "v2"] {
        state["players"]["P1"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":id,"card":"BP11-P31"}));
    }
    let mut game = fixture(&state);
    let play = game
        .activate_internal_card("source", AbilityId(4), PlayerId { seat: 1 })
        .unwrap();
    resolved(finish(&mut game, play, &["plain"]));
    let view = game.projection(View::Referee).unwrap();
    for id in ["source", "v1", "v2"] {
        assert_eq!(view["objects"][id]["acted"], true);
    }
    assert_eq!(view["objects"]["enemy"]["damage"], 2_u8);
    let mut entry_game = fixture(&position("BP11-023", "hand"));
    resolved(enter(&mut entry_game, "BP11-023", &[]));
    let entry_view = entry_game.projection(View::Referee).unwrap();
    let horses = entry_view["P1"]["field"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|id| entry_view["objects"][id.as_str().unwrap()]["card"] == "BP11-P31")
        .count();
    assert_eq!(
        horses, 1_usize,
        "the printed fanfare creates its real vehicle on the field"
    );
}
#[test]
fn bp19_028_creates_its_real_pirate_flag_in_ex() {
    let mut game = fixture(&position("BP19-028", "hand"));
    resolved(enter(&mut game, "BP19-028", &[]));
    let view = game.projection(View::Referee).unwrap();
    let id = view["P1"]["ex"][0].as_str().unwrap();
    assert_eq!(view["objects"][id]["card"], "BP19-P33");
    assert_eq!(view["P1"]["pp"]["current"], 8_u8);
}
#[test]
fn bp19_030_creates_its_real_pirate_flag_in_ex() {
    let mut game = fixture(&position("BP19-030", "hand"));
    resolved(enter(&mut game, "BP19-030", &[]));
    let view = game.projection(View::Referee).unwrap();
    let id = view["P1"]["ex"][0].as_str().unwrap();
    assert_eq!(view["objects"][id]["card"], "BP19-P33");
    assert_eq!(view["P1"]["pp"]["current"], 9_u8);
}
#[test]
fn bp19_p33_checks_the_selected_eight_prisons_pirate() {
    let mut state = position("BP19-P33", "ex");
    state["players"]["P1"]["zones"]["field"] = json!([{"id":"ally","card":"pirate"}]);
    let mut game = fixture(&state);
    let play = game
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap();
    resolved(finish(&mut game, play, &["pirate"]));
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["objects"]["ally"]["power"], 4_u8);
    assert_eq!(view["P2"]["leader"]["life"], 19_u8);
}
#[test]
fn cp01_047_draws_and_adds_the_printed_seiun_sky_bonus() {
    let mut state = position("CP01-047", "hand");
    state["players"]["P1"]["zones"]["field"] = json!([{"id":"ally","card":"seiun"}]);
    let mut game = fixture(&state);
    let play = game
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap();
    resolved(finish(&mut game, play, &["seiun"]));
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["objects"]["ally"]["power"], 5_u8);
    assert_eq!(view["objects"]["ally"]["hp"], 7_u8);
    assert_eq!(view["P1"]["hand"], json!(["draw"]));
}
#[test]
fn bp18_089_pays_its_fanfare_then_creates_three_real_bats() {
    let mut state = position("BP18-089", "hand");
    state["players"]["P1"]["zones"]["hand"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"discard","card":"vampire"}));
    state["players"]["P1"]["zones"]["field"] = json!([{"id":"ally","card":"queen"}]);
    let mut game = fixture(&state);
    resolved(enter(&mut game, "BP18-089", &[]));
    let fanfare_view = game.projection(View::Referee).unwrap();
    assert_eq!(fanfare_view["P1"]["hand"], json!(["draw"]));
    game.resume_internal_card().unwrap();
    let play = game
        .activate_internal_card("source", AbilityId(4), PlayerId { seat: 1 })
        .unwrap();
    resolved(finish(&mut game, play, &[]));
    let summoned_view = game.projection(View::Referee).unwrap();
    let bats = summoned_view["P1"]["field"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|id| summoned_view["objects"][id.as_str().unwrap()]["card"] == "BP01-T15")
        .count();
    assert_eq!(bats, 3_usize);
    assert!(
        summoned_view["P1"]["cemetery"]
            .as_array()
            .unwrap()
            .contains(&json!("source"))
    );
}
#[test]
fn bp20_063_increases_max_pp_after_both_damages_and_draw() {
    let mut state = position("BP20-063", "hand");
    state["players"]["P1"]["zones"]["field"] = json!([{"id":"ally","card":"garmieux"}]);
    state["players"]["P1"]["pp"] = json!({"current":8_u8,"max":8_u8});
    let mut game = fixture(&state);
    let play = game
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap();
    resolved(finish(&mut game, play, &["garmieux", "plain"]));
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["objects"]["ally"]["damage"], 1_u8);
    assert_eq!(view["objects"]["enemy"]["damage"], 4_u8);
    assert_eq!(view["P1"]["hand"], json!(["draw"]));
    assert_eq!(view["P1"]["pp"]["max"], 9_u8);
}
#[test]
fn pr_437_uses_original_cost_after_its_draw() {
    for (card, expected) in [("large", 10_u8), ("plain", 9_u8)] {
        let mut state = position("PR-437", "hand");
        state["players"]["P1"]["zones"]["field"] =
            json!([{"id":"ally","card":card,"state":{"cost":1_u8}}]);
        let mut game = fixture(&state);
        let play = game
            .play_internal_card("source", PlayerId { seat: 1 })
            .unwrap();
        resolved(finish(&mut game, play, &[]));
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["P1"]["hand"], json!(["draw"]));
        assert_eq!(view["P1"]["pp"]["current"], expected);
    }
}

#[test]
fn bp06_065_reads_the_selected_garyu_name_when_attacking() {
    for (card, power, hp) in [("garyu", 5_u8, 8_u8), ("plain", 4, 7)] {
        let mut state = position("BP06-065", "field");
        state["players"]["P1"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"ally","card":card}));
        let mut game = fixture(&state);
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"attack","attacker":"source","target":"P2.leader"})
            )
            .unwrap()
            .outcome,
            "resolved"
        );
        resolved(trigger(&mut game, "BP06-065", &[card]));
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["objects"]["ally"]["power"], power);
        assert_eq!(view["objects"]["ally"]["hp"], hp);
    }
}

#[test]
fn cp02_022_applies_both_independent_attack_name_conditions() {
    let mut state = position("CP02-022", "field");
    for card in ["uzuki", "rin"] {
        state["players"]["P1"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":card,"card":card}));
    }
    let mut game = fixture(&state);
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"P2.leader"}))
            .unwrap()
            .outcome,
        "resolved"
    );
    resolved(trigger(&mut game, "CP02-022", &[]));
    game.resume_internal_card().unwrap();
    resolved(trigger(&mut game, "CP02-022", &[]));
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["objects"]["source"]["power"], 3_u8);
    assert_eq!(view["objects"]["source"]["hp"], 2_u8);
}

#[test]
fn pr_406_replaces_three_with_four_damage_for_an_evolved_follower() {
    for evolved in [false, true] {
        let card = if evolved { "BP07-031" } else { "plain" };
        let mut state = position("PR-406", "hand");
        if evolved {
            // A prior health buff keeps the real evolved follower alive for damage inspection.
            state["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":card,"state":{"evolved":true,"evolved_with":"enemy-form","power":3_u8,"hp":6_u8,"max_hp":6_u8}}]);
            state["players"]["P2"]["zones"]["evolution"] =
                json!([{"id":"enemy-form","card":"BP07-032"}]);
        }
        let mut game = fixture(&state);
        let play = game
            .play_internal_card("source", PlayerId { seat: 1 })
            .unwrap();
        resolved(finish(&mut game, play, &[card]));
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(
            view["objects"]["enemy"]["damage"],
            if evolved { 4_u8 } else { 3 }
        );
        assert_eq!(view["P1"]["pp"]["current"], 8_u8);
    }
}

#[test]
fn bp03_003_real_form_returns_an_ordinary_target_but_destroys_an_evolved_target() {
    for evolved in [false, true] {
        let mut state = position("evolution-BP03-003", "field");
        state["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP03-003"}]);
        let enemy = if evolved { "BP07-031" } else { "plain" };
        state["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":enemy,"state":{"evolved":evolved,"evolved_with":if evolved {json!("enemy-form")} else {Value::Null}}}]);
        if evolved {
            state["players"]["P2"]["zones"]["evolution"] =
                json!([{"id":"enemy-form","card":"BP07-032"}]);
        }
        let mut game = fixture(&state);
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
            )
            .unwrap()
            .outcome,
            "resolved"
        );
        resolved(trigger(&mut game, "BP03-003", &[enemy]));
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["P2"]["field"], json!([]));
        assert_eq!(
            view["P2"][if evolved { "cemetery" } else { "hand" }],
            json!(["enemy"]),
            "the selected target's current evolved kind selects destruction or return"
        );
    }
}

#[test]
fn bp15_084_name_condition_evolves_only_with_the_matching_name_and_available_form() {
    for (matching, form_available) in [(false, true), (true, true), (true, false)] {
        let mut state = position("BP15-084", "hand");
        state["players"]["P1"]["zones"]["field"] = if matching {
            json!([{"id":"name","card":"BP15-076"}])
        } else {
            json!([])
        };
        if form_available {
            state["players"]["P1"]["zones"]["evolve_deck"] =
                json!([{"id":"form","card":"external-form-BP15-084"}]);
        }
        let mut game = fixture(&state);
        let entry_play = game
            .play_internal_card("source", PlayerId { seat: 1 })
            .unwrap();
        assert!(matches!(
            finish(&mut game, entry_play, &[]),
            CardPlay::Entered(_)
        ));
        let targets = if matching && form_available {
            vec!["external-form-BP15-084"]
        } else {
            vec![]
        };
        resolved(trigger(&mut game, "BP15-084", &targets));
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(
            view["P1"]["leader"]["life"], 19_u8,
            "self damage precedes either name branch"
        );
        assert_eq!(
            view["objects"]["source"]["evolved"],
            matching && form_available,
            "the unmodified base program uses an explicit external form; BP15-085 is not admitted here"
        );
        assert_eq!(
            view["P1"]["evolution"],
            if matching && form_available {
                json!(["form"])
            } else {
                json!([])
            }
        );
    }
}
