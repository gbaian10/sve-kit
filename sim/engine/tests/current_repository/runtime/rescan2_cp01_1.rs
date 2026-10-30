#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate these printed card scenarios."
)]
use super::*;
use crate::rule_fixtures;
use alloc::collections::BTreeMap;
use std::{fs::read_to_string, path::PathBuf};
use sve_engine::catalog::EngineIdentityInput;
use sve_engine::game::internal::SearchAnswer;

fn fixture() -> &'static SelectedEffects {
    static FIXTURE: OnceLock<SelectedEffects> = OnceLock::new();
    FIXTURE.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = producer_inputs();
        let mut cards = BTreeMap::new();
        for (number, form, traits) in [
            ("cp01-maru-base", Some("CP01-054"), vec![]),
            ("cp01-mac-base", Some("CP01-067"), vec![]),
            ("cp01-uma", None, vec!["ウマ娘"]),
            ("cp01-bnw", None, vec!["ウマ娘", "BNW"]),
        ] {
            let name = form.map_or_else(
                || json!(number),
                |form| {
                    snapshot
                        .lines()
                        .map(|line| serde_json::from_str::<Value>(line).unwrap())
                        .find(|record| record["number"] == form)
                        .unwrap()["faces"][0]["name"]
                        .clone()
                },
            );
            snapshot.push_str(
                &json!({"number":number,"faces":[{
                    "name":name,"card_class":"ニュートラル","card_type":"フォロワー",
                    "cost":"0","power":"3","hp":"3","traits":traits,"text":null,"sections":[]
                }]})
                .to_string(),
            );
            snapshot.push('\n');
            cards.insert(number, json!({"status":"complete","abilities":[]}));
        }
        input["documents"].as_array_mut().unwrap().push(json!([
            "cp01-fixtures.yaml",
            json!({"format":1_u8,"kind":"effect_set","cards":cards}).to_string()
        ]));
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut rules: Value = serde_saphyr::from_str(
            &read_to_string(root.join("authored/rules/engine/index.yaml")).unwrap(),
        )
        .unwrap();
        rules["input"] = rule_fixtures::rules(&snapshot, json!([]), json!([]))["input"].clone();
        Catalog::from_current_selection_with_rules(
            &snapshot,
            &keywords,
            &input,
            &rules.to_string(),
            &EngineIdentityInput::LegacyJp,
        )
        .unwrap()
    })
}

fn load_companions(game: &mut Game) {
    let empty = WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![],
    })
    .unwrap();
    for card in ["cp01-uma", "cp01-bnw"] {
        game.load_internal_card(card, &empty).unwrap();
    }
}

fn game(initial: &Value) -> Game {
    let mut game = Game::from_selected_effects(
        fixture().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "cp01-rescan",
    )
    .unwrap();
    load_companions(&mut game);
    game
}

const fn actor() -> PlayerId {
    PlayerId { seat: 1 }
}

fn position(card: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"first","card":"cp01-uma"}, {"id":"second","card":"BP01-171"},
        {"id":"tail","card":"BP01-171"}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    initial
}

fn finish(game: &mut Game, mut step: CardPlay, mode: u64) {
    while let CardPlay::Awaiting(question) = step {
        let answer = if matches!(&question.domain, InputDomain::WardEntry { .. }) {
            Answer::WardEntry(false)
        } else if matches!(&question.domain, InputDomain::Modes { .. }) {
            Answer::Modes(vec![ModeId(mode)])
        } else if let InputDomain::Targets(domain) = &question.domain {
            assert_eq!(
                domain.members.len(),
                1,
                "only the printed target is eligible"
            );
            Answer::Targets(vec![domain.members[0].handle])
        } else if let InputDomain::Search(domain) = &question.domain {
            Answer::Search(SearchAnswer::Ungrouped(vec![domain.cards[0].handle]))
        } else if let InputDomain::Order(cards) = &question.domain {
            Answer::Order(cards.iter().rev().map(|card| card.handle).collect())
        } else {
            panic!("unexpected printed decision")
        };
        step = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
}

fn play(game: &mut Game, mode: u64) {
    let step = game.play_internal_card("source", actor()).unwrap();
    finish(game, step, mode);
}

fn trigger(game: &mut Game) {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("printed trigger")
    };
    assert_eq!(
        options.len(),
        1,
        "only the intended printed trigger is pending"
    );
    let step = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap();
    finish(game, step, 1);
    game.resume_internal_card().unwrap();
}

#[test]
fn cp01_031_and_alias_deal_three_or_four_with_an_umamusume_on_field() {
    for card in ["CP01-031", "CP01-P22"] {
        for present in [false, true] {
            let mut initial = position(card);
            if present {
                initial["players"]["P1"]["zones"]["field"] =
                    json!([{"id":"ally","card":"cp01-uma"}]);
            }
            let mut game = game(&initial);
            play(&mut game, 1);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["objects"]["enemy"]["damage"],
                if present { 4_u8 } else { 3_u8 }
            );
            assert_eq!(state["P1"]["pp"]["current"], 8_u8);
            assert_eq!(state["P1"]["cemetery"], json!(["source"]));
        }
    }
}

#[test]
fn cp01_044_and_alias_fanfare_require_another_umamusume() {
    for card in ["CP01-044", "CP01-P32"] {
        for present in [false, true] {
            let mut initial = position(card);
            if present {
                initial["players"]["P1"]["zones"]["field"] =
                    json!([{"id":"ally","card":"cp01-uma"}]);
            }
            let mut game = game(&initial);
            play(&mut game, 1);
            trigger(&mut game);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["objects"]["enemy"]["damage"],
                if present { 4_u8 } else { 0_u8 }
            );
            assert_eq!(state["P1"]["pp"]["current"], 3_u8);
        }
    }
}

#[test]
fn cp01_008_and_alias_fanfare_draw_one() {
    for card in ["CP01-008", "CP01-P05"] {
        let mut game = game(&position(card));
        play(&mut game, 1);
        trigger(&mut game);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["hand"], json!(["first"]));
        assert_eq!(state["P1"]["deck"], json!(["second", "tail"]));
        assert_eq!(state["P1"]["pp"]["current"], 8_u8);
    }
}

#[test]
fn cp01_084_and_alias_buff_only_an_allied_umamusume_follower() {
    for card in ["CP01-084", "CP01-P64"] {
        let mut initial = position(card);
        initial["players"]["P1"]["zones"]["field"] = json!([
            {"id":"ally","card":"cp01-uma"}, {"id":"other","card":"BP01-171"}
        ]);
        let mut game = game(&initial);
        play(&mut game, 1);
        trigger(&mut game);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["ally"]["power"], 4_u8);
        assert_eq!(state["objects"]["ally"]["hp"], 5_u8);
        assert_eq!(state["objects"]["source"]["power"], 3_u8);
    }
}

fn evolution(initial: &Value, base: &str) -> Game {
    let mut game = game(initial);
    // Synthetic bases supply evolution without admitting unrelated meal abilities.
    let mut tree = expanded::tree().clone();
    tree["cards"] = json!({base:tree["cards"]["synthetic-evolution"]});
    let program = lowerer().lower(&tree).unwrap().remove(base).unwrap();
    game.load_internal_card(base, &program).unwrap();
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
        )
        .unwrap()
        .outcome,
        "resolved",
        "the synthetic base produces a real evolution event"
    );
    game
}

#[test]
fn cp01_054_and_aliases_evolve_and_reduce_an_enemy_by_two_two() {
    for card in ["CP01-054", "CP01-SL14", "CP01-SP14"] {
        let mut initial = position("cp01-maru-base");
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] =
            json!([{"id":"source","card":"cp01-maru-base"}]);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":card}]);
        let mut game = evolution(&initial, "cp01-maru-base");
        trigger(&mut game);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["enemy"]["power"], 3_u8);
        assert_eq!(state["objects"]["enemy"]["hp"], 3_u8);
        assert_eq!(state["objects"]["source"]["evolved_with"], "form");
    }
}

#[test]
fn cp01_067_and_aliases_evolve_and_return_only_an_amulet() {
    for card in ["CP01-067", "CP01-SL17", "CP01-SP17"] {
        let mut initial = position("cp01-mac-base");
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] =
            json!([{"id":"source","card":"cp01-mac-base"}]);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":card}]);
        initial["players"]["P1"]["zones"]["cemetery"] = json!([
            {"id":"amulet","card":"CP01-008"},{"id":"follower","card":"CP01-084"}
        ]);
        let mut game = evolution(&initial, "cp01-mac-base");
        trigger(&mut game);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["hand"], json!(["amulet"]));
        assert_eq!(state["P1"]["cemetery"], json!(["follower"]));
    }
}

#[test]
fn cp01_021_and_alias_choose_between_topdeck_and_cemetery_cards() {
    for card in ["CP01-021", "CP01-P15"] {
        for mode in [1_u64, 2] {
            let mut initial = position(card);
            initial["players"]["P1"]["zones"]["cemetery"] = json!([
                {"id":"bnw","card":"cp01-bnw"}, {"id":"other","card":"cp01-uma"}
            ]);
            let mut game = game(&initial);
            play(&mut game, mode);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["P1"]["hand"],
                if mode == 1 {
                    json!(["first"])
                } else {
                    json!(["bnw"])
                }
            );
            assert_eq!(
                state["P1"]["deck"],
                if mode == 1 {
                    json!(["tail", "second"])
                } else {
                    json!(["first", "second", "tail"])
                }
            );
        }
    }
}

#[test]
fn cp01_008_departure_buffs_after_the_amulet_has_left() {
    let mut initial = position("CP01-008");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"ally","card":"cp01-uma"}]);
    let binding = Binding::new(50, ScopeId(1));
    let objects = Selector::Zone {
        players: PlayerSet {
            relation: PlayerRelation::SelfPlayer,
            relative_to: PlayerRef::Controller {},
            text_quantifier: TextQuantifier::All,
            cardinality: Cardinality {
                minimum: Nat(1),
                maximum: Some(Nat(1)),
            },
        },
        zone: ZoneKind::Field,
        view: SelectionView::Current {},
        binding,
        predicate: Box::new(Expr::Compute(Box::new(Predicate::KindEquals {
            left: Expr::Live(Box::new(KindRead::Kind {
                object: ObjectView::Current {
                    object: Box::new(ObjectOperand::Bound { binding }),
                },
                view: CharacteristicView::Effective,
            })),
            right: Expr::Literal(CardKind::Amulet),
        }))),
    };
    let mut game = with_selected_producer(&initial, Action::Destroy { objects }, fixture());
    load_companions(&mut game);
    play(&mut game, 1);
    trigger(&mut game);
    activate(&mut game, "P1");
    game.resume_internal_card().unwrap();
    trigger(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["cemetery"], json!(["source"]));
    assert_eq!(state["objects"]["ally"]["power"], 4_u8);
    assert_eq!(state["objects"]["ally"]["hp"], 4_u8);
}

#[test]
fn cp01_044_own_main_phase_can_damage_the_opposing_leader() {
    let mut initial = position("CP01-044");
    initial["turn"]["active"] = json!("P2");
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"CP01-044"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([]);
    let mut game = game(&initial);
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    let question = game.internal_quick_choice().unwrap().unwrap();
    game.answer_internal_card(question.actor, question.token, Answer::Quick(None))
        .unwrap();
    trigger(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["turn"]["active"], "P1");
    assert_eq!(state["P2"]["leader"]["life"], 16_u8);
}
