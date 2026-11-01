#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate the printed card scenarios."
)]
use super::*;
use alloc::collections::BTreeMap;
use sve_engine::game::internal::SearchAnswer;

const PLAYER: PlayerId = PlayerId { seat: 1 };
const CONTROLS: [(&str, &str, &str, &str, &[&str]); 11] = [
    (
        "ready-fencer-form",
        "ヴァリアントフェンサー",
        "フォロワー・エボルヴ",
        "-",
        &["ヒーロー"],
    ),
    ("ready-hero", "other hero", "スペル", "1", &["ヒーロー"]),
    (
        "ready-drag-base",
        "ナックルドラゴン・ドラグニル",
        "フォロワー",
        "3",
        &["武装"],
    ),
    (
        "ready-zombie-base",
        "ミスチーフゾンビ",
        "フォロワー",
        "2",
        &[],
    ),
    ("ready-ghost", "ゴースト", "フォロワー", "1", &[]),
    ("ready-armed", "armed ally", "フォロワー", "2", &["武装"]),
    (
        "ready-tale-three",
        "three cost tale",
        "フォロワー",
        "3",
        &["童話"],
    ),
    (
        "ready-tale-six",
        "six cost tale",
        "フォロワー",
        "6",
        &["童話"],
    ),
    (
        "ready-cosmos-form",
        "コスモスファング",
        "フォロワー・エボルヴ",
        "-",
        &["獣"],
    ),
    (
        "ready-beast-one",
        "one cost beast",
        "フォロワー",
        "1",
        &["獣"],
    ),
    (
        "ready-beast-three",
        "three cost beast",
        "フォロワー",
        "3",
        &["獣"],
    ),
];

// Supporting cards isolate the tested programs from unrelated admission gaps.
fn prepared() -> &'static (SelectedEffects, BTreeMap<String, WellFormedCardProgram>) {
    static READY: OnceLock<(SelectedEffects, BTreeMap<String, WellFormedCardProgram>)> =
        OnceLock::new();
    READY.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        for (number, name, kind, cost, traits) in CONTROLS {
            snapshot.push_str(&json!({"number":number,"faces":[{
                "name":name,"card_class":"ニュートラル","card_type":kind,"cost":cost,
                "power":if kind == "スペル" {"-"} else {"3"},"hp":if kind == "スペル" {"-"} else {"3"},"traits":traits,"text":null,"sections":[]
            }]}).to_string());
            snapshot.push('\n');
            input["documents"].as_array_mut().unwrap().push(json!([
                format!("{number}.yaml"),
                json!({"format":1_u8,"kind":"effect_set","cards":{number:{"status":"complete","abilities":[]}}}).to_string()
            ]));
        }
        let selected = repository::selection_with_rules_from(&snapshot, &keywords, &input);
        let mut tree = expanded::tree().clone();
        tree["cards"] = json!({"ready-drag-base":tree["cards"]["synthetic-evolution"],"ready-zombie-base":tree["cards"]["synthetic-evolution"]});
        let base = lowerer().lower(&tree).unwrap();
        (selected, base)
    })
}
fn game(initial: &Value) -> Game {
    let (selected, base) = prepared();
    let mut game = Game::from_selected_effects(
        selected.clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "ready-bp03",
    )
    .unwrap();
    let empty = WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![],
    })
    .unwrap();
    for (number, ..) in CONTROLS {
        game.load_internal_card(
            number,
            if ["ready-drag-base", "ready-zombie-base"].contains(&number) {
                &base[number]
            } else {
                &empty
            },
        )
        .unwrap();
    }
    game
}
fn position(number: &str, zone: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"][zone] = json!([{"id":"source","card":number}]);
    for seat in ["P1", "P2"] {
        initial["players"][seat]["zones"]["deck"] = json!([
            {"id":format!("{seat}-draw1"),"card":"BP01-037"},
            {"id":format!("{seat}-draw2"),"card":"BP01-037"}
        ]);
    }
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-037","state":{"acted":true,"hp":12_u8,"max_hp":12_u8}}]);
    initial
}
fn checks(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}
fn finish(game: &mut Game, mut step: CardPlay) {
    while let CardPlay::Awaiting(q) = step {
        let answer = match &q.domain {
            InputDomain::Targets(d) => Answer::Targets(vec![
                d.members.iter().find(|m| m.card.is_some()).unwrap().handle,
            ]),
            InputDomain::CostMaterials(d) => Answer::CostMaterials(vec![d.members[0].handle]),
            InputDomain::Search(d) => {
                assert_eq!(d.cards.len(), 1, "only the other Hero card is searchable");
                Answer::Search(SearchAnswer::Ungrouped(vec![d.cards[0].handle]))
            }
            InputDomain::Order(d) => Answer::Order(d.iter().rev().map(|m| m.handle).collect()),
            InputDomain::OptionalCost => Answer::OptionalCost(true),
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            _ => panic!("unexpected printed decision: {:?}", q.domain),
        };
        step = game.answer_internal_card(q.actor, q.token, answer).unwrap();
    }
    assert!(
        !matches!(step, CardPlay::Cancelled),
        "the printed play has its resources"
    );
    checks(game);
}
fn play(game: &mut Game) {
    let step = game.play_internal_card("source", PLAYER).unwrap();
    finish(game, step);
}
fn trigger(game: &mut Game) {
    checks(game);
    let q = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &q.domain else {
        panic!("printed trigger")
    };
    assert_eq!(
        options.len(),
        1,
        "only the intended printed trigger is pending"
    );
    let step = game
        .answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
        .unwrap();
    finish(game, step);
}
fn evolve(game: &mut Game) {
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
        )
        .unwrap()
        .outcome,
        "resolved",
        "the real form evolves using the matching base"
    );
    checks(game);
}
fn end_phase(game: &mut Game) {
    assert_eq!(
        game.internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap()
            .outcome,
        "resolved",
        "the current main phase ends"
    );
    loop {
        checks(game);
        if game.internal_pending_choice().unwrap().is_some() {
            break;
        }
        let Some(q) = game.internal_quick_choice().unwrap() else {
            break;
        };
        let step = game
            .answer_internal_card(q.actor, q.token, Answer::Quick(None))
            .unwrap();
        finish(game, step);
    }
}
#[test]
fn fencer_search_excludes_its_own_name_and_accepts_a_hero_spell() {
    let mut initial = position("BP03-020", "hand");
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"same-name","card":"BP03-SL05"},{"id":"wrong-trait","card":"BP01-037"},{"id":"hero","card":"ready-hero"}
    ]);
    let mut game = game(&initial);
    play(&mut game);
    trigger(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["hand"],
        json!(["hero"])
    );
}
#[test]
fn fencer_evolves_only_with_at_most_two_cards_in_hand() {
    for count in [2_u8, 3] {
        let mut initial = position("BP03-020", "field");
        initial["players"]["P1"]["zones"]["evolve_deck"] =
            json!([{"id":"form","card":"ready-fencer-form"}]);
        initial["players"]["P1"]["zones"]["hand"] = json!(
            (0..count)
                .map(|i| json!({"id":format!("hand{i}"),"card":"BP01-037"}))
                .collect::<Vec<_>>()
        );
        let mut game = game(&initial);
        let step = game
            .internal_rule_action(
                &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}}),
            )
            .unwrap();
        assert_eq!(
            step.outcome == "resolved",
            count == 2,
            "the evolution prerequisite uses the current hand count"
        );
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["evolve_deck"]
                .as_array()
                .unwrap()
                .is_empty(),
            count == 2
        );
    }
}
#[test]
fn dragnir_evolution_needs_another_armed_follower_for_four_damage() {
    for other in [false, true] {
        let mut initial = position("ready-drag-base", "field");
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP03-061"}]);
        if other {
            initial["players"]["P1"]["zones"]["field"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":"ally","card":"ready-armed"}));
        }
        let mut game = game(&initial);
        evolve(&mut game);
        trigger(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
            if other { 4_u8 } else { 2 }
        );
    }
}
#[test]
fn trombone_base_end_phase_damage_needs_real_crimson() {
    for crimson in [false, true] {
        let mut initial = position("BP03-081", "field");
        if crimson {
            initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP01-118"}]);
            initial["players"]["P1"]["zones"]["field"] =
                json!([{"id":"trombone","card":"BP03-081"}]);
        }
        let mut game = game(&initial);
        if crimson {
            play(&mut game);
            trigger(&mut game);
        }
        end_phase(&mut game);
        trigger(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
            if crimson { 3_u8 } else { 0 }
        );
    }
}
#[test]
fn trombone_evolution_damages_both_leaders_and_enables_its_own_end_trigger() {
    let mut initial = position("BP03-081", "field");
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP03-082"}]);
    let mut game = game(&initial);
    evolve(&mut game);
    trigger(&mut game);
    let state = game.projection(View::Referee).unwrap();
    for seat in ["P1", "P2"] {
        assert_eq!(state[seat]["leader"]["life"], 19_u8);
    }
    end_phase(&mut game);
    trigger(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
        3_u8
    );
}
#[test]
fn pumpkin_creates_the_complete_giant_ghost_in_ex() {
    let mut game = game(&position("BP03-084", "hand"));
    play(&mut game);
    trigger(&mut game);
    let state = game.projection(View::Referee).unwrap();
    let ex = state["P1"]["ex"].as_array().unwrap();
    assert_eq!(ex.len(), 1, "one token enters EX");
    let token = &state["objects"][ex[0].as_str().unwrap()];
    assert_eq!(token["name"], "ジャイアントゴースト");
    assert_eq!(token["power"], 3_u8);
    assert_eq!(token["hp"], 3_u8);
}
#[test]
fn parade_pays_only_another_own_follower_and_revives_cost_five_or_less() {
    let mut initial = position("BP03-085", "hand");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"fee","card":"BP01-171"}]);
    initial["players"]["P1"]["zones"]["cemetery"] =
        json!([{"id":"too-expensive","card":"ready-tale-six"},{"id":"revived","card":"BP01-037"}]);
    let mut game = game(&initial);
    play(&mut game);
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("fanfare")
    };
    let mut step = game
        .answer_internal_card(
            pending.actor,
            pending.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap();
    while let CardPlay::Awaiting(q) = step {
        let answer = match &q.domain {
            InputDomain::Targets(d) => {
                assert_eq!(d.members.len(), 1, "six cost cannot be revived");
                Answer::Targets(vec![d.members[0].handle])
            }
            InputDomain::CostMaterials(d) => {
                assert_eq!(
                    d.members.len(),
                    1,
                    "only the other own follower is a payment candidate"
                );
                let victim = &d.members[0];
                assert_eq!(
                    victim.player.seat, 1,
                    "opposing followers cannot pay this cost"
                );
                Answer::CostMaterials(vec![victim.handle])
            }
            InputDomain::OptionalCost => Answer::OptionalCost(true),
            _ => panic!("parade payment or target"),
        };
        step = game.answer_internal_card(q.actor, q.token, answer).unwrap();
    }
    checks(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["cemetery"], json!([]));
    assert_eq!(state["P2"]["field"], json!(["enemy"]));
    assert_eq!(state["P1"]["cemetery"], json!(["too-expensive", "fee"]));
    assert_eq!(state["P1"]["field"], json!(["source", "revived"]));
}
#[test]
fn mischief_evolution_grants_bane_to_the_named_ghost() {
    let mut initial = position("ready-zombie-base", "field");
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP03-087"}]);
    initial["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"ghost","card":"ready-ghost"}));
    let mut game = game(&initial);
    evolve(&mut game);
    trigger(&mut game);
    assert!(
        game.projection(View::Referee).unwrap()["objects"]["ghost"]["keywords"]
            .as_array()
            .unwrap()
            .contains(&json!("必殺")),
        "the selected Ghost gains Bane"
    );
}
#[test]
fn tea_party_looks_five_enters_one_eligible_tale_and_orders_the_rest() {
    let mut initial = position("BP03-100", "hand");
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"too-expensive","card":"ready-tale-six"},{"id":"wrong-trait","card":"BP01-037"},
        {"id":"chosen","card":"ready-tale-three"},{"id":"rest1","card":"BP01-037"},{"id":"rest2","card":"BP01-037"},{"id":"unseen","card":"BP01-037"}
    ]);
    let mut game = game(&initial);
    play(&mut game);
    trigger(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["field"], json!(["source", "chosen"]));
    assert_eq!(
        state["P1"]["deck"],
        json!(["unseen", "rest2", "rest1", "wrong-trait", "too-expensive"])
    );
}
#[test]
fn tea_party_activation_pays_pp_and_itself_before_reviving_a_small_tale() {
    let mut initial = position("BP03-100", "field");
    initial["players"]["P1"]["zones"]["cemetery"] = json!([{"id":"too-expensive","card":"ready-tale-six"},{"id":"chosen","card":"ready-tale-three"}]);
    let mut game = game(&initial);
    let id = selection().current["BP03-100"]
        .program
        .card()
        .abilities
        .iter()
        .find(|a| a.source.line == 2)
        .unwrap()
        .node
        .id;
    let step = game.activate_internal_card("source", id, PLAYER).unwrap();
    finish(&mut game, step);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["pp"]["current"], 8_u8);
    assert_eq!(state["P1"]["field"], json!(["chosen"]));
    assert_eq!(state["P1"]["cemetery"], json!(["too-expensive", "source"]));
}
#[test]
fn rapunzel_requires_its_fairy_tale_counter_to_attack_either_kind_of_enemy() {
    for count in [0_u8, 1] {
        for target in ["P2.leader", "enemy"] {
            let mut initial = position("BP03-110", "field");
            initial["players"]["P1"]["zones"]["field"][0]["state"] =
                json!({"counters":{"童話カウンター":count}});
            let mut game = game(&initial);
            let outcome = game
                .internal_rule_action(&json!({"do":"attack","attacker":"source","target":target}))
                .unwrap()
                .outcome;
            assert_eq!(
                outcome,
                if count == 1 {
                    "resolved"
                } else {
                    "cannot-attack"
                },
                "Rapunzel's counter controls both attack target kinds"
            );
        }
    }
}
#[test]
fn giant_ghost_banishes_only_at_its_own_next_main_phase() {
    for active in ["P1", "P2"] {
        let mut initial = position("BP03-T06", "field");
        initial["turn"]["active"] = json!(active);
        let mut game = game(&initial);
        end_phase(&mut game);
        if active == "P2" {
            let q = game.internal_pending_choice().unwrap().unwrap();
            let InputDomain::Pending(options) = &q.domain else {
                panic!("the Ghost's main-phase trigger")
            };
            assert_eq!(options.len(), 1, "only the Ghost trigger is pending");
            let step = game
                .answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
                .unwrap();
            let activation = resolved(step);
            assert!(activation.progress.facts.iter().any(|fact| matches!(fact,
                Fact::Movement(movement) if movement.method == MoveMethod::Banish && movement.from.kind == ZoneKind::Field
            )), "the printed trigger banishes the field token");
            checks(&mut game);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["turn"]["active"], "P1");
            assert_eq!(state["P1"]["field"], json!([]));
            assert_eq!(state["P1"]["banish"], json!([]));
            assert!(
                state["objects"].get("source").is_none(),
                "CR 9.1.4.4 removes the token after banishment"
            );
        } else {
            assert_eq!(
                game.projection(View::Referee).unwrap()["P1"]["field"],
                json!(["source"])
            );
        }
    }
}

#[test]
fn cosmos_evolution_returns_only_another_own_follower_before_installing_the_form() {
    for material in ["fee", "enemy"] {
        let mut initial = position("BP03-002", "field");
        initial["players"]["P1"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"fee","card":"BP01-171"}));
        initial["players"]["P1"]["zones"]["evolve_deck"] =
            json!([{"id":"form","card":"ready-cosmos-form"}]);
        let mut game = game(&initial);
        let before = game.projection(View::Referee).unwrap();
        let outcome = game.internal_rule_action(&json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8},"costs":{"materials":[material]}})).unwrap().outcome;
        assert_eq!(
            outcome,
            if material == "fee" {
                "resolved"
            } else {
                "cannot-evolve"
            },
            "the evolution payment uses only another own follower"
        );
        let state = game.projection(View::Referee).unwrap();
        if material == "fee" {
            assert_eq!(state["P1"]["hand"], json!(["fee"]));
            assert_eq!(state["P1"]["pp"]["current"], 9_u8);
            assert_eq!(state["objects"]["source"]["evolved_with"], "form");
            assert_eq!(state["P2"]["field"], json!(["enemy"]));
        } else {
            assert_eq!(
                state, before,
                "an opposing follower cannot pay the evolution cost"
            );
        }
    }
}
#[test]
fn cosmos_may_replace_a_small_beast_search_with_field_entry_but_keeps_larger_beasts_in_hand() {
    for (number, replace, can_replace) in [
        ("ready-beast-one", false, true),
        ("ready-beast-one", true, true),
        ("ready-beast-three", true, false),
    ] {
        let mut initial = position("BP03-002", "hand");
        initial["players"]["P1"]["zones"]["deck"] =
            json!([{"id":"wrong-trait","card":"BP01-171"},{"id":"found","card":number}]);
        let mut game = game(&initial);
        play(&mut game);
        let pending = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = &pending.domain else {
            panic!("Cosmos Fanfare")
        };
        let mut step = game
            .answer_internal_card(
                pending.actor,
                pending.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap();
        let mut replacement_asked = false;
        while let CardPlay::Awaiting(q) = step {
            let answer = match &q.domain {
                InputDomain::Search(d) => {
                    assert_eq!(d.cards.len(), 1, "only the Beast follower is searchable");
                    Answer::Search(SearchAnswer::Ungrouped(vec![d.cards[0].handle]))
                }
                InputDomain::SearchReplacement => {
                    replacement_asked = true;
                    Answer::SearchReplacement(if replace {
                        SearchReplacementAnswer::Replacement
                    } else {
                        SearchReplacementAnswer::Original
                    })
                }
                _ => panic!("Cosmos search or destination"),
            };
            step = game.answer_internal_card(q.actor, q.token, answer).unwrap();
        }
        checks(&mut game);
        assert_eq!(
            replacement_asked, can_replace,
            "only a found cost-two-or-less follower offers the replacement"
        );
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P1"]["field"],
            if replace && can_replace {
                json!(["source", "found"])
            } else {
                json!(["source"])
            }
        );
        assert_eq!(
            state["P1"]["hand"],
            if replace && can_replace {
                json!([])
            } else {
                json!(["found"])
            }
        );
        assert_eq!(state["P1"]["deck"], json!(["wrong-trait"]));
    }
}
