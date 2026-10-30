#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate a printed ability scenario."
)]
use super::*;
use alloc::collections::BTreeMap;
use std::{fs::read_to_string, path::PathBuf};
use sve_engine::catalog::EngineIdentityInput;
use sve_engine::game::internal::SearchAnswer;

#[expect(
    clippy::too_many_lines,
    reason = "One cached fixture lists the external class, trait and cost counterexamples together."
)]
fn prepared() -> &'static (SelectedEffects, BTreeMap<String, WellFormedCardProgram>) {
    static READY: OnceLock<(SelectedEffects, BTreeMap<String, WellFormedCardProgram>)> =
        OnceLock::new();
    READY.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        let mut programs = BTreeMap::new();
        let mut profiles = vec![
            (
                "set-machine".to_owned(),
                "外部機械".to_owned(),
                "ニュートラル".to_owned(),
                "フォロワー".to_owned(),
                vec!["機械".to_owned()],
                "2",
                "2",
                "6",
            ),
            (
                "set-needle".into(),
                "撃針の看守".into(),
                "ロイヤル".into(),
                "フォロワー".into(),
                vec!["機械".into()],
                "2",
                "2",
                "5",
            ),
            (
                "set-royal".into(),
                "外部兵士".into(),
                "ロイヤル".into(),
                "フォロワー".into(),
                vec!["兵士".into()],
                "0",
                "1",
                "5",
            ),
            (
                "set-elf".into(),
                "外部妖精".into(),
                "エルフ".into(),
                "フォロワー".into(),
                vec!["妖精".into()],
                "0",
                "1",
                "5",
            ),
            (
                "set-uma".into(),
                "外部ウマ娘".into(),
                "ドラゴン".into(),
                "フォロワー".into(),
                vec!["ウマ娘".into()],
                "0",
                "1",
                "5",
            ),
            (
                "set-idol".into(),
                "外部デレマス".into(),
                "エルフ".into(),
                "フォロワー".into(),
                vec!["デレマス".into()],
                "0",
                "1",
                "5",
            ),
            (
                "set-prison".into(),
                "外部八獄".into(),
                "エルフ".into(),
                "フォロワー".into(),
                vec!["八獄".into()],
                "0",
                "1",
                "5",
            ),
            (
                "set-omen".into(),
                "外部ナイトメア絶傑".into(),
                "ナイトメア".into(),
                "フォロワー".into(),
                vec!["絶傑".into()],
                "0",
                "1",
                "5",
            ),
            (
                "set-neutral-omen".into(),
                "外部ニュートラル絶傑".into(),
                "ニュートラル".into(),
                "フォロワー".into(),
                vec!["絶傑".into()],
                "0",
                "1",
                "5",
            ),
            (
                "set-first".into(),
                "ファースト・ワン".into(),
                "ナイトメア".into(),
                "スペル".into(),
                vec![],
                "0",
                "-",
                "-",
            ),
            (
                "set-galmieux".into(),
                "侮蔑の絶傑・ガルミーユ".into(),
                "ドラゴン".into(),
                "フォロワー".into(),
                vec![],
                "0",
                "1",
                "5",
            ),
            (
                "set-cheap".into(),
                "外部１コスト".into(),
                "ニュートラル".into(),
                "フォロワー".into(),
                vec![],
                "1",
                "1",
                "5",
            ),
            (
                "set-expensive".into(),
                "外部２コスト".into(),
                "ニュートラル".into(),
                "フォロワー".into(),
                vec![],
                "2",
                "1",
                "5",
            ),
            (
                "set-amulet".into(),
                "外部護符".into(),
                "ニュートラル".into(),
                "アミュレット".into(),
                vec![],
                "1",
                "-",
                "-",
            ),
            (
                "producer".into(),
                "外部入場producer".into(),
                "ニュートラル".into(),
                "フォロワー".into(),
                vec![],
                "0",
                "1",
                "1",
            ),
        ];
        for form in ["BP07-070", "BP19-002", "BP20-075", "CP01-041", "ECP02-002"] {
            let row = snapshot
                .lines()
                .map(|line| serde_json::from_str::<Value>(line).unwrap())
                .find(|row| row["number"] == form)
                .unwrap();
            let face = &row["faces"][0];
            let number = format!("set-base-{form}");
            let traits = face["traits"]
                .as_array()
                .unwrap()
                .iter()
                .map(|value| value.as_str().unwrap().to_owned())
                .collect();
            profiles.push((
                number.clone(),
                face["name"].as_str().unwrap().into(),
                face["card_class"].as_str().unwrap().into(),
                "フォロワー".into(),
                traits,
                "0",
                "1",
                "1",
            ));
            let mut tree = expanded::tree().clone();
            let evolution = tree["cards"]["synthetic-evolution"].clone();
            tree["cards"] = json!({});
            tree["cards"][&number] = evolution;
            programs.insert(
                number.clone(),
                lowerer().lower(&tree).unwrap().remove(&number).unwrap(),
            );
        }
        for (number, name, class, kind, traits, cost, power, hp) in profiles {
            let row = json!({"number":number,"faces":[{
                "name":name, "card_class":class, "card_type":kind, "traits":traits,
                "cost":cost, "power":power, "hp":hp, "text":null, "sections":[]
            }]});
            snapshot.push_str(&row.to_string());
            snapshot.push('\n');
            let mut document = json!({"format":1_u8,"kind":"effect_set","cards":{}});
            document["cards"][&number] = json!({"status":"complete","abilities":[]});
            input["documents"]
                .as_array_mut()
                .unwrap()
                .push(json!([format!("{number}.yaml"), document.to_string()]));
            programs.entry(number).or_insert_with(|| {
                WellFormedCardProgram::check(CardProgram {
                    abilities: vec![],
                    card_play_costs: vec![],
                })
                .unwrap()
            });
        }
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../authored/rules/engine/index.yaml");
        let mut rules: Value = serde_saphyr::from_str(&read_to_string(path).unwrap()).unwrap();
        rules["input"] = rule_fixtures::rules(&snapshot, json!([]), json!([]))["input"].clone();
        let selected = Catalog::from_current_selection_with_rules(
            &snapshot,
            &keywords,
            &input,
            &rules.to_string(),
            &EngineIdentityInput::LegacyJp,
        )
        .unwrap();
        (selected, programs)
    })
}
fn fixture(initial: &Value) -> Game {
    let (selected, programs) = prepared();
    let mut game = Game::from_selected_effects(
        selected.clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "set-batch",
    )
    .unwrap();
    for (number, program) in programs {
        if number != "producer" {
            game.load_internal_card(number, program).unwrap();
        }
    }
    game
}
fn activated(number: &str) -> AbilityId {
    selection().current[number]
        .program
        .card()
        .abilities
        .iter()
        .find(|ability| ability.node.kind == AbilityKind::Activated)
        .unwrap()
        .node
        .id
}
fn accept(game: &mut Game, mut play: CardPlay) -> CardPlay {
    while let CardPlay::Awaiting(question) = &play {
        let answer = if matches!(question.domain, InputDomain::WardEntry { .. }) {
            Answer::WardEntry(false)
        } else if matches!(question.domain, InputDomain::OptionalCost) {
            Answer::OptionalCost(true)
        } else {
            break;
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    play
}
fn trigger(game: &mut Game, card: &str, line: u64) -> CardPlay {
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = pending.domain else {
        panic!("trigger")
    };
    let option = options
        .iter()
        .find(|option| option.card == card && option.line == line)
        .unwrap();
    let play = game
        .answer_internal_card(pending.actor, pending.token, Answer::Pending(option.handle))
        .unwrap();
    accept(game, play)
}
fn choose(game: &mut Game, play: CardPlay, card: &str) -> CardPlay {
    let CardPlay::Awaiting(question) = play else {
        panic!("objects")
    };
    let (InputDomain::Targets(domain) | InputDomain::CostMaterials(domain)) = &question.domain
    else {
        panic!("objects domain")
    };
    let handle = domain
        .members
        .iter()
        .find(|member| member.card.as_deref() == Some(card))
        .unwrap()
        .handle;
    let answer = if matches!(question.domain, InputDomain::CostMaterials(_)) {
        Answer::CostMaterials(vec![handle])
    } else {
        Answer::Targets(vec![handle])
    };
    game.answer_internal_card(question.actor, question.token, answer)
        .unwrap()
}
fn mode(game: &mut Game, play: CardPlay, index: u64) -> CardPlay {
    let CardPlay::Awaiting(question) = play else {
        panic!("modes")
    };
    assert!(
        matches!(question.domain, InputDomain::Modes { .. }),
        "the printed choice fixes a mode before its target"
    );
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::Modes(vec![ModeId(index)]),
    )
    .unwrap()
}
fn enter(game: &mut Game, card: &str) -> CardPlay {
    enter_line(game, card, 1)
}
fn enter_line(game: &mut Game, card: &str, line: u64) -> CardPlay {
    let play = game
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap();
    assert!(
        matches!(accept(game, play), CardPlay::Entered(_)),
        "the printed source enters before Fanfare"
    );
    trigger(game, card, line)
}
fn evolve(game: &mut Game, super_evolution: bool) {
    let pay = if super_evolution {
        json!({"pp":1_u8,"sep":1_u8})
    } else {
        json!({"pp":1_u8})
    };
    game.internal_rule_action(
        &json!({"do":"evolve","source":"source","evolve_card":"form","pay":pay}),
    )
    .unwrap();
}
fn evolution_position(form: &str) -> Value {
    let mut initial = draw_position();
    initial["turn"]["elapsed_turns"] = json!({"P1":7_u8,"P2":6_u8});
    initial["players"]["P1"]["sep"] = json!(1_u8);
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"source","card":format!("set-base-{form}")}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
    initial
}
fn keyword(state: &Value, id: &str, name: &str) -> bool {
    state["objects"][id]["keywords"]
        .as_array()
        .unwrap()
        .contains(&json!(name))
}
#[test]
fn bp05_068_damages_both_sides_and_checks_galmieux_before_its_second_sweep() {
    for present in [false, true] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP05-068"}]);
        initial["players"]["P1"]["zones"]["field"] =
            json!([{"id":"ally","card":if present{"set-galmieux"}else{"set-royal"}}]);
        initial["players"]["P2"]["zones"]["field"] =
            json!([{"id":"enemy","card":"BP04-117"},{"id":"amulet","card":"set-amulet"}]);
        let mut game = fixture(&initial);
        let play = game
            .play_internal_card("source", PlayerId { seat: 1 })
            .unwrap();
        resolved(choose(
            &mut game,
            play,
            if present { "set-galmieux" } else { "set-royal" },
        ));
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["ally"]["damage"], 2_u8);
        assert_eq!(
            state["objects"]["enemy"]["damage"],
            if present { 4_u8 } else { 2_u8 }
        );
        assert_eq!(state["P2"]["field"], json!(["enemy", "amulet"]));
    }
}
#[test]
fn bp07_070_banishes_first_one_once_per_turn_and_buffs_only_machines() {
    let mut initial = evolution_position("BP07-070");
    initial["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .extend([
            json!({"id":"machine","card":"set-machine"}),
            json!({"id":"royal","card":"set-royal"}),
        ]);
    initial["players"]["P1"]["zones"]["cemetery"] =
        json!([{"id":"cost1","card":"set-first"},{"id":"cost2","card":"set-first"}]);
    let mut game = fixture(&initial);
    evolve(&mut game, false);
    let play = game
        .activate_internal_card("source", activated("BP07-070"), PlayerId { seat: 1 })
        .unwrap();
    resolved(choose(&mut game, play, "set-first"));
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["machine"]["power"], 4_u8);
    assert_eq!(state["objects"]["machine"]["hp"], 8_u8);
    assert_eq!(state["objects"]["royal"]["power"], 1_u8);
    assert!(keyword(&state, "machine", "突進"));
    assert!(keyword(&state, "source", "疾走"));
    assert_eq!(state["P1"]["pp"]["current"], 7_u8);
    assert_eq!(state["P1"]["banish"].as_array().unwrap().len(), 1);
    game.resume_internal_card().unwrap();
    let before_retry = game.projection(View::Referee).unwrap();
    game.activate_internal_card("source", activated("BP07-070"), PlayerId { seat: 1 })
        .unwrap_err();
    assert_eq!(game.projection(View::Referee).unwrap(), before_retry);
}
#[test]
fn bp12_091_always_creates_repair_mode_but_machine_buff_requires_ability_entry() {
    for by_ability in [false, true] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"][if by_ability { "cemetery" } else { "hand" }] =
            json!([{"id":"source","card":"BP12-091"}]);
        initial["players"]["P1"]["zones"]["field"] =
            json!([{"id":"producer","card":"producer"},{"id":"machine","card":"set-machine"}]);
        let mut game = fixture(&initial);
        let action = Action::Move {
            objects: Selector::Zone {
                players: PlayerSet {
                    relation: PlayerRelation::SelfPlayer,
                    relative_to: PlayerRef::Controller {},
                    cardinality: Cardinality {
                        minimum: Nat(1),
                        maximum: Some(Nat(1)),
                    },
                    text_quantifier: TextQuantifier::Implicit,
                },
                zone: ZoneKind::Cemetery,
                view: SelectionView::Current {},
                binding: Binding::new(9, ScopeId(1)),
                predicate: Box::new(Expr::Literal(Bool(true))),
            },
            movement: MoveSpec {
                to: MoveDestination::Shared {
                    zone: ZoneRef {
                        player: PlayerRef::Controller {},
                        kind: ZoneKind::Field,
                    },
                },
                initial_posture: None,
                entry_effects: vec![],
                position: None,
                order: None,
            },
            reason: MoveReason::Put,
        };
        // Only the external entry producer changes; the printed Fanfare remains selected.
        game.load_internal_card("producer", &producer(action))
            .unwrap();
        let play = if by_ability {
            resolved(
                game.activate_internal_card("producer", AbilityId(1), PlayerId { seat: 1 })
                    .unwrap(),
            );
            game.resume_internal_card().unwrap();
            trigger(&mut game, "BP12-091", 1)
        } else {
            enter(&mut game, "BP12-091")
        };
        resolved(play);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["objects"]["machine"]["power"],
            2_u8 + u8::from(by_ability)
        );
        assert_eq!(
            state["objects"]["source"]["power"],
            2_u8 + u8::from(by_ability)
        );
        let ex = state["P1"]["ex"].as_array().unwrap();
        assert_eq!(ex.len(), 1);
        assert_eq!(
            state["objects"][ex[0].as_str().unwrap()]["card"],
            "BP07-T02"
        );
    }
}
#[test]
fn bp16_016_buffs_all_other_elf_followers_and_excludes_royal_and_itself() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP16-016"}]);
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"elf","card":"set-elf"},{"id":"royal","card":"set-royal"}]);
    let mut game = fixture(&initial);
    let play = enter(&mut game, "BP16-016");
    resolved(play);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["elf"]["power"], 2_u8);
    assert_eq!(state["objects"]["royal"]["power"], 1_u8);
    assert_eq!(state["objects"]["source"]["power"], 2_u8);
}
#[test]
fn bp19_002_evolution_declares_combo_damage_and_super_evolution_grants_only_other_eight_prisons_storm()
 {
    for super_evolution in [false, true] {
        let mut initial = evolution_position("BP19-002");
        initial["players"]["P1"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"prison","card":"set-prison"}));
        initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP04-117"}]);
        let prior = if super_evolution { 3_usize } else { 0 };
        initial["players"]["P1"]["zones"]["hand"] = json!(
            (0..prior)
                .map(|n| json!({"id":format!("prior{n}"),"card":"set-royal"}))
                .collect::<Vec<_>>()
        );
        let mut game = fixture(&initial);
        for n in 0..prior {
            assert!(
                matches!(
                    game.play_internal_card(&format!("prior{n}"), PlayerId { seat: 1 })
                        .unwrap(),
                    CardPlay::Entered(_)
                ),
                "external plays establish actual Combo history"
            );
            game.resume_internal_card().unwrap();
        }
        evolve(&mut game, super_evolution);
        let play = trigger(&mut game, "BP19-002", 2);
        resolved(choose(&mut game, play, "BP04-117"));
        game.resume_internal_card().unwrap();
        if super_evolution {
            let super_play = trigger(&mut game, "BP19-002", 3);
            resolved(super_play);
            game.resume_internal_card().unwrap();
        }
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["objects"]["enemy"]["damage"],
            if super_evolution { 4_u8 } else { 0_u8 }
        );
        assert_eq!(keyword(&state, "prison", "疾走"), super_evolution);
        assert!(keyword(&state, "source", "疾走"));
    }
}
#[test]
fn bp20_075_modes_create_two_real_lulunai_or_buff_only_nightmare_omens_and_self() {
    for selection in [1_u64, 2] {
        let mut initial = evolution_position("BP20-075");
        initial["players"]["P1"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .extend([
                json!({"id":"omen","card":"set-omen"}),
                json!({"id":"neutral","card":"set-neutral-omen"}),
            ]);
        let mut game = fixture(&initial);
        evolve(&mut game, false);
        let play = trigger(&mut game, "BP20-075", 1);
        let play = mode(&mut game, play, selection);
        resolved(play);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["objects"]["omen"]["power"],
            if selection == 2 { 2_u8 } else { 1_u8 }
        );
        assert_eq!(state["objects"]["neutral"]["power"], 1_u8);
        assert_eq!(
            state["objects"]["source"]["power"],
            if selection == 2 { 4_u8 } else { 3_u8 }
        );
        assert_eq!(keyword(&state, "source", "疾走"), selection == 2);
        let field = state["P1"]["field"].as_array().unwrap();
        assert_eq!(field.len(), if selection == 1 { 5 } else { 3 });
        for id in field {
            if [json!("source"), json!("omen"), json!("neutral")].contains(id) {
                continue;
            }
            let object = &state["objects"][id.as_str().unwrap()];
            assert_eq!(object["card"], "BP15-P32");
            assert_eq!(object["power"], 2_u8);
            assert!(keyword(&state, id.as_str().unwrap(), "突進"));
            assert!(keyword(&state, id.as_str().unwrap(), "指定攻撃"));
        }
    }
}
#[test]
fn bp21_012_grants_only_fairy_tokens_and_draws_only_at_combo_three() {
    for prior in [0_usize, 2] {
        let mut initial = draw_position();
        let mut hand = vec![json!({"id":"source","card":"BP21-012"})];
        for n in 0..prior {
            hand.push(json!({"id":format!("prior{n}"),"card":"set-royal"}));
        }
        initial["players"]["P1"]["zones"]["hand"] = json!(hand);
        initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"drawn","card":"set-royal"}]);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"elf","card":"set-elf"}]);
        initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-T03"}]);
        let mut game = fixture(&initial);
        for n in 0..prior {
            assert!(
                matches!(
                    game.play_internal_card(&format!("prior{n}"), PlayerId { seat: 1 })
                        .unwrap(),
                    CardPlay::Entered(_)
                ),
                "external cards establish Combo history"
            );
            game.resume_internal_card().unwrap();
        }
        resolved(
            game.play_internal_card("source", PlayerId { seat: 1 })
                .unwrap(),
        );
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P1"]["hand"],
            if prior == 2 {
                json!(["drawn"])
            } else {
                json!([])
            }
        );
        assert!(!keyword(&state, "elf", "突進"));
        assert!(!keyword(&state, "enemy", "指定攻撃"));
        let fairy = state["P1"]["field"]
            .as_array()
            .unwrap()
            .iter()
            .find(|id| state["objects"][id.as_str().unwrap()]["card"] == "BP01-T03")
            .unwrap()
            .as_str()
            .unwrap();
        assert!(keyword(&state, fairy, "突進"));
        assert!(keyword(&state, fairy, "指定攻撃"));
    }
}
#[test]
fn cp01_041_evolution_buffs_only_other_horse_girls_when_awakened() {
    for maximum in [6_u8, 7] {
        let mut initial = evolution_position("CP01-041");
        initial["players"]["P1"]["pp"] = json!({"current":maximum,"max":maximum});
        initial["players"]["P1"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .extend([
                json!({"id":"uma","card":"set-uma"}),
                json!({"id":"royal","card":"set-royal"}),
            ]);
        let mut game = fixture(&initial);
        evolve(&mut game, false);
        let play = trigger(&mut game, "CP01-041", 1);
        resolved(play);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["objects"]["uma"]["power"],
            if maximum == 7 { 2_u8 } else { 1_u8 }
        );
        assert_eq!(state["objects"]["royal"]["power"], 1_u8);
        assert_eq!(state["objects"]["source"]["power"], 3_u8);
    }
}
#[test]
fn ecp02_002_evolution_buffs_chosen_idol_and_super_evolution_buffs_every_other_idol() {
    let mut initial = evolution_position("ECP02-002");
    initial["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .extend([
            json!({"id":"idol1","card":"set-idol"}),
            json!({"id":"idol2","card":"set-idol"}),
            json!({"id":"royal","card":"set-royal"}),
        ]);
    let mut game = fixture(&initial);
    evolve(&mut game, true);
    let play = trigger(&mut game, "ECP02-002", 1);
    resolved(choose(&mut game, play, "set-idol"));
    game.resume_internal_card().unwrap();
    let super_play = trigger(&mut game, "ECP02-002", 2);
    resolved(super_play);
    let state = game.projection(View::Referee).unwrap();
    let mut powers = [
        state["objects"]["idol1"]["power"].as_u64().unwrap(),
        state["objects"]["idol2"]["power"].as_u64().unwrap(),
    ];
    powers.sort_unstable();
    assert_eq!(powers, [2, 3]);
    assert_eq!(state["objects"]["royal"]["power"], 1_u8);
    assert_eq!(state["objects"]["source"]["power"], 3_u8);
}
fn cheap_buff(number: &str) {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":number}]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"cheap","card":"set-cheap","state":{"cost":9_u8}},{"id":"expensive","card":"set-expensive","state":{"cost":1_u8}},{"id":"amulet","card":"set-amulet"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP04-117"}]);
    let mut game = fixture(&initial);
    let play = game
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap();
    resolved(choose(&mut game, play, "BP04-117"));
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        state["objects"]["enemy"]["damage"], 3_u8,
        "the declared enemy takes three damage"
    );
    assert_eq!(
        state["objects"]["cheap"]["power"], 2_u8,
        "printed one cost qualifies despite a changed effective cost"
    );
    assert_eq!(
        state["objects"]["expensive"]["power"], 1_u8,
        "printed two cost does not qualify despite a changed effective cost"
    );
    assert_eq!(
        state["P1"]["field"],
        json!(["cheap", "expensive", "amulet"]),
        "the amulet is not a follower recipient"
    );
}
#[test]
fn cp02_085_damages_its_target_and_buffs_only_original_cost_one_or_less_allies() {
    cheap_buff("CP02-085");
}
#[test]
fn pr_149_damages_its_target_and_buffs_only_original_cost_one_or_less_allies() {
    cheap_buff("PR-149");
}

#[test]
fn bp19_041_searches_its_named_follower_then_buffs_only_other_machines_on_attack() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP19-041"}]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"royal","card":"set-royal"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"needle","card":"set-needle"}, {"id":"wrong","card":"set-machine"}
    ]);
    let mut game = fixture(&initial);
    let play = enter_line(&mut game, "BP19-041", 3);
    let CardPlay::Awaiting(question) = play else {
        panic!("search")
    };
    let InputDomain::Search(domain) = &question.domain else {
        panic!("search domain")
    };
    assert_eq!(domain.cards.len(), 1);
    assert_eq!(domain.cards[0].card, "set-needle");
    let entry = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Search(SearchAnswer::Ungrouped(vec![domain.cards[0].handle])),
        )
        .unwrap();
    resolved(entry);
    game.resume_internal_card().unwrap();
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["field"], json!(["royal", "source", "needle"]));
    assert_eq!(state["P1"]["deck"], json!(["wrong"]));
    assert!(keyword(&state, "source", "疾走"));
    game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"P2.leader"}))
        .unwrap();
    let attack = trigger(&mut game, "BP19-041", 2);
    resolved(attack);
    let attacking = game.projection(View::Referee).unwrap();
    assert_eq!(attacking["objects"]["needle"]["power"], 3_u8);
    assert_eq!(attacking["objects"]["royal"]["power"], 1_u8);
}
