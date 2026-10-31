#![expect(
    clippy::panic,
    reason = "Unexpected choices invalidate the printed UB scenarios."
)]
use super::*;
use std::{fs::read_to_string, path::PathBuf};
use sve_engine::catalog::EngineIdentityInput;
use sve_engine::game::internal::DecisionView;

struct Fixture {
    selection: SelectedEffects,
    programs: Vec<(String, WellFormedCardProgram)>,
}

fn fixture() -> &'static Fixture {
    static FIXTURE: OnceLock<Fixture> = OnceLock::new();
    FIXTURE.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        let rows: Vec<Value> = snapshot.lines().map(|line| serde_json::from_str(line).unwrap()).collect();
        let template = expanded::tree()["cards"]["synthetic-evolution"].clone();
        let mut tree = expanded::tree().clone();
        let mut loaded = Vec::new();
        for number in [
            "CP04-006",
            "CP04-010",
            "CP04-014",
            "CP04-020",
            "CP04-028",
            "CP04-050",
            "CP04-068",
            "CP04-082",
            "CP04-110",
            "CP04-117",
            "PCS01-005",
        ] {
            let form = rows.iter().find(|row| row["number"] == number).unwrap();
            let name = format!("ub-base-{number}");
            let face = json!({"name":form["faces"][0]["name"],"card_type":"フォロワー","card_class":"ニュートラル","cost":"1","power":"1","hp":"1","traits":[],"text":null,"sections":[]});
            add_card(&mut snapshot, &mut input, &name, &face);
            tree["cards"] = json!({name.clone():template.clone()});
            loaded.push((name.clone(), lowerer().lower(&tree).unwrap().remove(&name).unwrap()));
        }
        for (name, kind, power, hp) in [("ub-enemy", "フォロワー", "10", "20"), ("ub-amulet", "アミュレット", "-", "-")] {
            add_card(&mut snapshot, &mut input, name, &json!({"name":name,"card_type":kind,"card_class":"ニュートラル","cost":"1","power":power,"hp":hp,"traits":[],"text":null,"sections":[]}));
            loaded.push((name.into(), WellFormedCardProgram::check(CardProgram {abilities:vec![],card_play_costs:vec![]}).unwrap()));
        }
        let mut rules: Value = serde_saphyr::from_str(&read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../authored/rules/engine/index.yaml")).unwrap()).unwrap();
        rules["input"]["snapshot_sha256"] = json!(rule_fixtures::hash(snapshot.as_bytes()));
        let selected = Catalog::from_current_selection_with_rules(&snapshot, &keywords, &input, &rules.to_string(), &EngineIdentityInput::LegacyJp).unwrap();
        Fixture {selection: selected, programs: loaded}
    })
}
fn add_card(snapshot: &mut String, input: &mut Value, name: &str, face: &Value) {
    snapshot.push_str(&json!({"number":name,"faces":[face]}).to_string());
    snapshot.push('\n');
    input["documents"].as_array_mut().unwrap().push(json!([format!("{name}.yaml"),json!({"format":1_u8,"kind":"effect_set","cards":{name:{"status":"complete","abilities":[]}}}).to_string()]));
}
pub(super) fn position(card: &str, zone: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["construction"] = json!("title");
    initial["players"]["P1"]["title_code"] = json!("princess_connect_re_dive");
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    initial["players"]["P1"]["zones"][zone] = json!([{"id":"source","card":card}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"draw1","card":"BP01-037"},{"id":"draw2","card":"BP01-037"},{"id":"draw3","card":"BP01-037"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"ub-enemy"}]);
    initial
}
pub(super) fn build(initial: &Value) -> Game {
    let prepared = fixture();
    let mut game = Game::from_selected_effects(
        prepared.selection.clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "ub-batch",
    )
    .unwrap();
    for (card, program) in &prepared.programs {
        game.load_internal_card(card, program).unwrap();
    }
    game
}
pub(super) fn question(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(question) = play else {
        panic!("expected question")
    };
    question
}
pub(super) fn finish(game: &mut Game, mut play: CardPlay) -> CardPlay {
    while let CardPlay::Awaiting(question) = play {
        let answer = match &question.domain {
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            InputDomain::OptionalCost => Answer::OptionalCost(true),
            InputDomain::Targets(domain) | InputDomain::CostMaterials(domain) => {
                let count = domain
                    .cardinality
                    .maximum
                    .unwrap_or(domain.cardinality.minimum)
                    .0
                    .min(u64::try_from(domain.members.len()).unwrap());
                let handles = domain
                    .members
                    .iter()
                    .take(usize::try_from(count).unwrap())
                    .map(|member| member.handle)
                    .collect();
                if matches!(question.domain, InputDomain::Targets(_)) {
                    Answer::Targets(handles)
                } else {
                    Answer::CostMaterials(handles)
                }
            }
            InputDomain::TokenCapacity { .. }
            | InputDomain::Quick { .. }
            | InputDomain::BeforeDraw
            | InputDomain::Pending(_)
            | InputDomain::TriggerQuota(_)
            | InputDomain::Integer(_)
            | InputDomain::Order(_)
            | InputDomain::Search(_)
            | InputDomain::SearchPartition(_)
            | InputDomain::Distribution(_)
            | InputDomain::SearchReplacement
            | InputDomain::Modes { .. } => panic!("unexpected printed question"),
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    play
}
pub(super) fn pending(game: &mut Game, card: &str) -> CardPlay {
    if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("pending")
    };
    let option = options.iter().find(|option| option.card == card).unwrap();
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::Pending(option.handle),
    )
    .unwrap()
}
pub(super) fn enter(game: &mut Game) {
    let play = game
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap();
    assert!(
        matches!(finish(game, play), CardPlay::Entered(_)),
        "the real follower enters before its pending fanfare"
    );
}
pub(super) fn evolve(card: &str, mut initial: Value) -> Game {
    initial["players"]["P1"]["zones"]["field"][0]["card"] =
        json!(format!("ub-base-{}", evolution_representative(card)));
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":card}]);
    let mut game = build(&initial);
    game.internal_rule_action(
        &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}}),
    )
    .unwrap();
    game
}
fn evolution_representative(card: &str) -> &str {
    let identity = &selection_with_rules().current[card].card_id;
    for representative in [
        "CP04-006",
        "CP04-010",
        "CP04-014",
        "CP04-020",
        "CP04-028",
        "CP04-050",
        "CP04-068",
        "CP04-082",
        "CP04-110",
        "CP04-117",
        "PCS01-005",
    ] {
        if &selection_with_rules().current[representative].card_id == identity {
            return representative;
        }
    }
    panic!("unregistered evolution companion")
}
pub(super) fn activate(game: &mut Game, card: &str) -> CardPlay {
    let definition = fixture().selection.current[card]
        .program
        .card()
        .abilities
        .iter()
        .find(|a| a.node.kind == AbilityKind::Activated)
        .unwrap()
        .node
        .id;
    game.activate_internal_card("source", definition, PlayerId { seat: 1 })
        .unwrap()
}
pub(super) fn ub(result: &Activation) {
    assert_eq!(
        result
            .progress
            .facts
            .iter()
            .filter(|fact| matches!(
                fact,
                Fact::AbilityPlayed {
                    qualification: AbilityQualification::UnionBurst,
                    ..
                }
            ))
            .count(),
        1,
        "one proper ability play records exactly one UB qualification"
    );
}

fn keyword(game: &Game, expected: &str) {
    assert!(
        game.projection(View::Referee).unwrap()["objects"]["source"]["keywords"]
            .as_array()
            .unwrap()
            .contains(&json!(expected)),
        "the source retains its printed keyword"
    );
}

#[test]
fn shiori_evolution_damages_the_opposing_leader() {
    for card in ["CP04-006", "CP04-P02"] {
        let mut game = evolve(card, position(card, "field"));
        let play = pending(&mut game, card);
        ub(&resolved(finish(&mut game, play)));
        assert_eq!(
            game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
            18_u8
        );
    }
}
#[test]
fn makoto_declares_targets_before_three_posture_payments_and_can_decline() {
    for card in ["CP04-008", "CP04-P04", "PCS01-003"] {
        for accepted in [false, true] {
            let mut initial = position(card, "hand");
            initial["players"]["P1"]["zones"]["field"] =
                json!([{"id":"fee1","card":"ub-amulet"},{"id":"fee2","card":"ub-amulet"}]);
            let mut game = build(&initial);
            enter(&mut game);
            let declare = question(pending(&mut game, card));
            assert_eq!(declare.domain, InputDomain::OptionalCost);
            let play = game
                .answer_internal_card(declare.actor, declare.token, Answer::OptionalCost(accepted))
                .unwrap();
            if accepted {
                let targets = question(play);
                let InputDomain::Targets(target_domain) = &targets.domain else {
                    panic!("formal targets before material payment")
                };
                assert_eq!(target_domain.members.len(), 1);
                assert_eq!(target_domain.members[0].card.as_deref(), Some("ub-enemy"));
                let materials = question(
                    game.answer_internal_card(
                        targets.actor,
                        targets.token,
                        Answer::Targets(vec![target_domain.members[0].handle]),
                    )
                    .unwrap(),
                );
                let InputDomain::CostMaterials(material_domain) = &materials.domain else {
                    panic!("materials after formal targets")
                };
                assert_eq!(material_domain.members.len(), 3);
                assert_eq!(
                    material_domain
                        .members
                        .iter()
                        .filter(|member| member.card.as_deref() == Some("ub-amulet"))
                        .count(),
                    2
                );
                assert_eq!(
                    material_domain
                        .members
                        .iter()
                        .filter(|member| member.card.as_deref() == Some(card))
                        .count(),
                    1
                );
                for id in ["source", "fee1", "fee2"] {
                    assert_eq!(
                        game.projection(View::Referee).unwrap()["objects"][id]["acted"],
                        false
                    );
                }
                let paid = game
                    .answer_internal_card(
                        materials.actor,
                        materials.token,
                        Answer::CostMaterials(
                            material_domain
                                .members
                                .iter()
                                .map(|member| member.handle)
                                .collect(),
                        ),
                    )
                    .unwrap();
                ub(&resolved(paid));
            } else {
                assert_eq!(play, CardPlay::Cancelled);
            }
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["objects"]["enemy"]["damage"],
                if accepted { 4_u8 } else { 0_u8 }
            );
            assert_eq!(
                state["P2"]["leader"]["life"],
                if accepted { 18_u8 } else { 20_u8 }
            );
            for id in ["fee1", "fee2", "source"] {
                assert_eq!(state["objects"][id]["acted"], accepted);
            }
        }
    }
}
#[test]
fn rino_evolution_damages_all_enemies_and_is_ub_even_with_an_empty_opposing_field() {
    for card in ["CP04-010", "CP04-P06"] {
        for enemies in [false, true] {
            let mut initial = position(card, "field");
            if !enemies {
                initial["players"]["P2"]["zones"]["field"] = json!([]);
            }
            let mut game = evolve(card, initial);
            let play = pending(&mut game, card);
            ub(&resolved(finish(&mut game, play)));
            if enemies {
                assert_eq!(
                    game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
                    1_u8
                );
            }
        }
    }
}
#[test]
fn lima_keeps_ward_and_heals_on_evolution() {
    for card in ["CP04-014", "CP04-P10"] {
        let mut game = evolve(card, position(card, "field"));
        let play = pending(&mut game, card);
        ub(&resolved(finish(&mut game, play)));
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["leader"]["life"], 13_u8);
        assert!(
            state["objects"]["source"]["keywords"]
                .as_array()
                .unwrap()
                .contains(&json!("守護"))
        );
    }
}
#[test]
fn suzuna_discards_for_non_ub_fanfare_then_acts_to_damage() {
    for card in ["CP04-017", "CP04-P13", "PCS01-006"] {
        let mut initial = position(card, "hand");
        initial["players"]["P1"]["zones"]["hand"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"discard","card":"BP01-037"}));
        let mut game = build(&initial);
        enter(&mut game);
        let play = pending(&mut game, card);
        let result = resolved(finish(&mut game, play));
        assert!(!result.progress.facts.iter().any(|f| matches!(
            f,
            Fact::AbilityPlayed {
                qualification: AbilityQualification::UnionBurst,
                ..
            }
        )));
        game.resume_internal_card().unwrap();
        let activation = activate(&mut game, card);
        ub(&resolved(finish(&mut game, activation)));
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["cemetery"], json!(["discard"]));
        assert_eq!(state["objects"]["enemy"]["damage"], 3_u8);
    }
}
#[test]
fn pecorine_keeps_ward_and_declares_four_damage_on_evolution() {
    for card in [
        "CP04-020",
        "CP04-SL06",
        "CP04-SP03",
        "CP04-U03",
        "PCS01-009",
    ] {
        let mut game = evolve(card, position(card, "field"));
        let play = pending(&mut game, card);
        keyword(&game, "守護");
        ub(&resolved(finish(&mut game, play)));
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
            4_u8
        );
    }
}
#[test]
fn tomo_storm_attack_declares_a_separate_damage_target() {
    for card in ["CP04-028", "CP04-P20"] {
        let mut game = evolve(card, position(card, "field"));
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"P2.leader"}))
            .unwrap();
        let play = pending(&mut game, card);
        keyword(&game, "疾走");
        ub(&resolved(finish(&mut game, play)));
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
            2_u8
        );
    }
}
#[test]
fn luka_pays_one_pp_and_acts_before_dealing_two_damage() {
    for card in ["CP04-030", "CP04-P22", "PCS01-012"] {
        let mut game = build(&position(card, "field"));
        let play = activate(&mut game, card);
        ub(&resolved(finish(&mut game, play)));
        keyword(&game, "守護");
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["enemy"]["damage"], 2_u8);
        assert_eq!(state["P1"]["pp"]["current"], 9_u8);
        assert_eq!(state["objects"]["source"]["acted"], true);
    }
}
#[test]
fn yuki_evolution_reduces_both_enemy_stats() {
    for card in ["CP04-050", "CP04-P38"] {
        let mut game = evolve(card, position(card, "field"));
        let play = pending(&mut game, card);
        ub(&resolved(finish(&mut game, play)));
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["enemy"]["power"], 8_u8);
        assert_eq!(state["objects"]["enemy"]["hp"], 18_u8);
    }
}
#[test]
fn hatsune_non_ub_entry_acts_and_ub_activation_damages_all_opposing_recipients() {
    for card in ["CP04-051", "CP04-P39"] {
        let mut entry_game = build(&position(card, "hand"));
        enter(&mut entry_game);
        let entry_play = pending(&mut entry_game, card);
        resolved(finish(&mut entry_game, entry_play));
        assert_eq!(
            entry_game.projection(View::Referee).unwrap()["objects"]["source"]["acted"],
            true
        );
        let mut activation_game = build(&position(card, "field"));
        let activation_play = activate(&mut activation_game, card);
        ub(&resolved(finish(&mut activation_game, activation_play)));
        keyword(&activation_game, "守護");
        let state = activation_game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["enemy"]["damage"], 2_u8);
        assert_eq!(state["P2"]["leader"]["life"], 18_u8);
    }
}

#[test]
fn mifuyu_evolution_destroys_an_opposing_amulet() {
    for card in ["CP04-068", "CP04-P52"] {
        let mut initial = position(card, "field");
        initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"ub-amulet"}]);
        let mut game = evolve(card, initial);
        let play = pending(&mut game, card);
        keyword(&game, "守護");
        ub(&resolved(finish(&mut game, play)));
        assert_eq!(
            game.projection(View::Referee).unwrap()["P2"]["cemetery"],
            json!(["enemy"])
        );
    }
}
#[test]
fn illya_banishes_five_cemetery_cards_and_heals_after_damage() {
    for card in [
        "CP04-074",
        "CP04-SL18",
        "CP04-SP09",
        "CP04-U09",
        "PCS01-030",
    ] {
        let mut initial = position(card, "field");
        initial["players"]["P1"]["zones"]["cemetery"] = json!(
            (0_u8..5)
                .map(|n| json!({"id":format!("fee{n}"),"card":"BP01-037"}))
                .collect::<Vec<_>>()
        );
        let mut game = build(&initial);
        let play = activate(&mut game, card);
        ub(&resolved(finish(&mut game, play)));
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["enemy"]["damage"], 3_u8);
        assert_eq!(state["P1"]["leader"]["life"], 13_u8);
        assert_eq!(state["P1"]["banish"].as_array().unwrap().len(), 5);
    }
}
#[test]
fn yori_evolution_deals_two_damage() {
    for card in ["CP04-082", "CP04-P62"] {
        let mut game = evolve(card, position(card, "field"));
        let play = pending(&mut game, card);
        ub(&resolved(finish(&mut game, play)));
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
            2_u8
        );
    }
}
#[test]
fn misaki_base_retains_non_ub_self_damage_and_ub_attack() {
    for card in ["CP04-085", "CP04-P65"] {
        let mut entry_game = build(&position(card, "hand"));
        enter(&mut entry_game);
        let entry_play = pending(&mut entry_game, card);
        resolved(finish(&mut entry_game, entry_play));
        assert_eq!(
            entry_game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
            8_u8
        );
        let mut attack_game = build(&position(card, "field"));
        attack_game
            .internal_rule_action(&json!({"do":"attack","attacker":"source","target":"P2.leader"}))
            .unwrap();
        let attack_play = pending(&mut attack_game, card);
        ub(&resolved(finish(&mut attack_game, attack_play)));
        assert_eq!(
            attack_game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
            3_u8
        );
    }
}

#[test]
fn akino_heals_and_draws_then_uses_hand_size_for_two_targets() {
    for card in ["CP04-094", "CP04-SL24"] {
        let mut initial = position(card, "hand");
        initial["players"]["P2"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"enemy2","card":"ub-enemy"}));
        let mut game = build(&initial);
        enter(&mut game);
        let play = pending(&mut game, card);
        resolved(finish(&mut game, play));
        game.resume_internal_card().unwrap();
        let activation = activate(&mut game, card);
        ub(&resolved(finish(&mut game, activation)));
        keyword(&game, "守護");
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["leader"]["life"], 13_u8);
        assert_eq!(state["P1"]["hand"].as_array().unwrap().len(), 3);
        for enemy in ["enemy", "enemy2"] {
            assert_eq!(state["objects"][enemy]["damage"], 3_u8);
        }
    }
}
