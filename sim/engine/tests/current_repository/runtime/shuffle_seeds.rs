#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected questions invalidate the printed shuffle scenarios."
)]
use super::*;
use crate::rule_fixtures;
use std::{fs::read_to_string, path::PathBuf};
use sve_engine::catalog::EngineIdentityInput;

fn selected() -> &'static SelectedEffects {
    static READY: OnceLock<SelectedEffects> = OnceLock::new();
    READY.get_or_init(|| {
        let (mut snapshot,keywords,mut input)=producer_inputs();
        // This added probe is not CP01-073 and grants no admission to that named companion.
        snapshot.push_str(&json!({"number":"shuffle-luck","faces":[{"name":"運の行方","card_class":"ニュートラル","card_type":"アミュレット","cost":"1","power":"-","hp":"-","traits":[],"text":null,"sections":[]}]}).to_string());
        snapshot.push('\n');
        input["documents"].as_array_mut().unwrap().push(json!(["shuffle-controls.yaml",json!({"format":1_u8,"kind":"effect_set","cards":{"shuffle-luck":{"status":"complete","abilities":[]}}}).to_string()]));
        let path=PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../authored/rules/engine/index.yaml");
        let mut rules:Value=serde_saphyr::from_str(&read_to_string(path).unwrap()).unwrap();
        rules["input"]=rule_fixtures::rules(&snapshot,json!([]),json!([]))["input"].clone();
        repository::select_with_rules(&snapshot,&keywords,&input,&rules.to_string(),&EngineIdentityInput::LegacyJp).unwrap()
    })
}

fn game(initial: &Value, random: &Value) -> Game {
    let mut game = Game::from_selected_effects(
        selected().clone(),
        initial,
        &Value::Null,
        random,
        "printed-shuffle",
    )
    .unwrap();
    let empty = WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![],
    })
    .unwrap();
    game.load_internal_card("shuffle-luck", &empty).unwrap();
    game.load_internal_card(
        "producer",
        &producer(Action::Destroy {
            objects: subjects(),
        }),
    )
    .unwrap();
    game
}

fn position(number: &str) -> Value {
    let mut state = draw_position();
    state["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":number}]);
    state["players"]["P1"]["zones"]["field"] = json!([{"id":"producer","card":"producer"}]);
    state["players"]["P1"]["zones"]["deck"] = json!(
        (0_u8..4)
            .map(|i| json!({"id":format!("d{i}"),"card":"BP01-171"}))
            .collect::<Vec<_>>()
    );
    state
}

fn checks(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}

fn pending(game: &mut Game, number: &str, line: u64) -> CardPlay {
    checks(game);
    let q = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &q.domain else {
        panic!("printed pending");
    };
    let option = options
        .iter()
        .find(|option| option.card == number && option.line == line)
        .unwrap();
    game.answer_internal_card(q.actor, q.token, Answer::Pending(option.handle))
        .unwrap()
}

fn finish(
    game: &mut Game,
    mut play: CardPlay,
    mode: u64,
    take: usize,
    printing: Option<&str>,
    accept: bool,
) -> CardPlay {
    while let CardPlay::Awaiting(q) = play {
        let answer = match &q.domain {
            InputDomain::Targets(domain) => Answer::Targets(
                domain
                    .members
                    .iter()
                    .filter(|m| printing.is_none_or(|printing| m.card.as_deref() == Some(printing)))
                    .take(
                        take.min(
                            domain
                                .cardinality
                                .maximum
                                .map_or(usize::MAX, |n| usize::try_from(n.0).unwrap()),
                        ),
                    )
                    .map(|m| m.handle)
                    .collect(),
            ),
            InputDomain::Modes { .. } => Answer::Modes(vec![ModeId(mode)]),
            InputDomain::Integer(_) => Answer::Integer(Nat(u64::from(accept))),
            InputDomain::OptionalCost => Answer::OptionalCost(accept),
            InputDomain::Order(cards) => {
                Answer::Order(cards.iter().rev().map(|card| card.handle).collect())
            }
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            InputDomain::ResourceEntryPayment { materials, .. } => Answer::ResourceEntryPayment {
                materials: materials
                    .members
                    .iter()
                    .take(usize::try_from(materials.cardinality.minimum.0).unwrap())
                    .map(|m| m.handle)
                    .collect(),
                use_ep: false,
            },
            other => panic!("unexpected shuffle decision {other:?}"),
        };
        play = game.answer_internal_card(q.actor, q.token, answer).unwrap();
    }
    checks(game);
    play
}

pub(super) fn princess(number: &str) {
    for take in [0_usize, 3] {
        let mut initial = position(number);
        initial["players"]["P1"]["pp"] = json!({"current":0_u8,"max":1_u8});
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"princess","card":"BP03-018"}]);
        initial["players"]["P1"]["zones"]["cemetery"] = json!([
            {"id":"g0","card":"BP03-018"},{"id":"g1","card":"BP03-019"},{"id":"g2","card":"BP03-027"},{"id":"wrong-class","card":"BP01-171"}
        ]);
        let mut order = (0..take).map(|i| format!("g{i}")).collect::<Vec<_>>();
        order.extend(["d3".into(), "d2".into(), "d1".into(), "d0".into()]);
        let mut game = game(
            &initial,
            &json!({"shuffles":[{"player":"P1","result":order}]}),
        );
        let play = game
            .play_internal_card("source", PlayerId { seat: 1 })
            .unwrap();
        let activation = resolved(finish(&mut game, play, 2, take, None, true));
        assert!(
            activation
                .progress
                .facts
                .iter()
                .any(|fact| matches!(fact, Fact::Shuffled { .. })),
            "Q467 still shuffles when zero cards are selected"
        );
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(
            view["P1"]["hand"],
            json!([order[0]]),
            "draw uses the shuffled top card"
        );
        assert_eq!(
            view["P1"]["deck"],
            json!(order[1..]),
            "the remaining shuffle order survives drawing"
        );
        assert_eq!(
            view["P1"]["pp"]["current"], 0_u8,
            "an existing Princess makes the spell free"
        );
        assert!(
            view["P1"]["cemetery"]
                .as_array()
                .unwrap()
                .contains(&json!("wrong-class")),
            "only Royal followers return"
        );
    }
    for accept in [false, true] {
        let mut initial = position(number);
        initial["players"]["P1"]["pp"] = json!({"current":0_u8,"max":1_u8});
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"ally","card":"BP03-018"}]);
        let mut game = game(&initial, &Value::Null);
        let before = game.projection(View::Referee).unwrap()["objects"]["ally"]["power"]
            .as_i64()
            .unwrap();
        let play = game
            .play_internal_card("source", PlayerId { seat: 1 })
            .unwrap();
        resolved(finish(&mut game, play, 1, 1, None, accept));
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(
            view["objects"]["ally"]["power"],
            before.checked_add(1).unwrap(),
            "mode one buffs the formal ally"
        );
        assert_eq!(
            view["objects"]["ally"]["counters"]["童話カウンター"]
                .as_i64()
                .unwrap_or(0),
            i64::from(accept),
            "the Fairy Tale counter is optional"
        );
    }
}

#[test]
fn princess_kiss_chooses_either_full_mode_and_shuffles_even_with_zero_returns() {
    princess("BP03-030");
}

#[test]
fn fukukitaru_all_printings_return_the_whole_cemetery_race_and_return_the_exact_lastword_generation()
 {
    for number in ["ECP01-052", "ECP01-P28"] {
        let mut initial = position(number);
        initial["players"]["P1"]["zones"]["cemetery"] =
            json!([{"id":"g0","card":"BP01-171"},{"id":"g1","card":"BP01-171"}]);
        initial["players"]["P1"]["zones"]["deck"] = json!([
            {"id":"luck0","card":"shuffle-luck"},{"id":"luck1","card":"shuffle-luck"},
            {"id":"d0","card":"BP01-171"},{"id":"d1","card":"BP01-171"},{"id":"d2","card":"BP01-171"},{"id":"d3","card":"BP01-171"},{"id":"d4","card":"BP01-171"},{"id":"tail","card":"BP01-171"}
        ]);
        initial["players"]["P1"]["zones"]["evolve_deck"] =
            json!([{"id":"carrot","card":"CSD01-032"}]);
        let random = json!({"shuffles":[
            {"player":"P1","result":["luck0","luck1","d0","d1","d2","g0","g1","d3","d4","tail"]},
            {"player":"P1","result":["source","d0","d1","d2","d3","d4","g0","g1","tail"]}
        ]});
        let mut game = game(&initial, &random);
        let play = game
            .play_internal_card("source", PlayerId { seat: 1 })
            .unwrap();
        finish(&mut game, play, 1, usize::MAX, None, true);
        let fanfare = pending(&mut game, number, 3);
        resolved(finish(
            &mut game,
            fanfare,
            1,
            usize::MAX,
            Some("shuffle-luck"),
            true,
        ));
        for _ in 0_u8..2 {
            let entry = pending(&mut game, number, 2);
            resolved(finish(&mut game, entry, 1, usize::MAX, None, true));
        }
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(
            view["P1"]["leader"]["life"], 34_u8,
            "Q1852 counts each actual named amulet arrival"
        );
        assert_eq!(view["P1"]["cemetery"], json!([]));
        assert_eq!(
            view["P1"]["deck"],
            json!(["d3", "d4", "tail", "g1", "g0", "d2", "d1", "d0"])
        );
        let meal = selected().current[number]
            .program
            .card()
            .abilities
            .iter()
            .find(|a| {
                matches!(
                    a.node.source.origin,
                    AbilityOrigin::Keyword {
                        keyword: KeywordId::Meal,
                        ..
                    }
                )
            })
            .unwrap()
            .node
            .id;
        let meal_play = game
            .activate_internal_card("source", meal, PlayerId { seat: 1 })
            .unwrap();
        resolved(finish(&mut game, meal_play, 1, usize::MAX, None, true));
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["source"]["racing"],
            true
        );
        activate(&mut game, "P1");
        let lastword = pending(&mut game, number, 4);
        resolved(finish(&mut game, lastword, 1, usize::MAX, None, true));
        let after_lastword = game.projection(View::Referee).unwrap();
        assert_eq!(
            after_lastword["P1"]["deck"],
            random["shuffles"][1]["result"]
        );
        assert!(game.internal_pending_choice().unwrap().is_none());
    }
}

#[test]
fn phoenix_all_printings_keep_ward_optional_payment_awakening_ex_capacity_and_lastword() {
    for number in ["BP11-054", "BP11-SL12", "BP11-U04", "SCS01-005"] {
        for (awakened, accept, full) in [
            (true, true, false),
            (true, false, false),
            (false, true, false),
            (true, true, true),
        ] {
            let mut initial = position(number);
            let pp = if awakened { 7_u8 } else { 6_u8 };
            initial["players"]["P1"]["pp"] = json!({"current":pp,"max":pp});
            if full {
                initial["players"]["P1"]["zones"]["ex"] = json!([{"filler":5_u8}]);
            }
            let random = json!({"shuffles":[{"player":"P1","result":["d3","d2","d1","d0"]}]});
            let mut game = game(&initial, &random);
            let play = game
                .play_internal_card("source", PlayerId { seat: 1 })
                .unwrap();
            finish(&mut game, play, 1, usize::MAX, None, true);
            let fanfare = pending(&mut game, number, 2);
            let completed = finish(&mut game, fanfare, 1, usize::MAX, None, accept);
            let shuffled = if let CardPlay::Resolved(result) = completed {
                result
                    .progress
                    .facts
                    .iter()
                    .any(|fact| matches!(fact, Fact::Shuffled { .. }))
            } else {
                assert!(
                    !accept && matches!(completed, CardPlay::Cancelled),
                    "declining the printed PP cost cancels only this occurrence"
                );
                false
            };
            assert_eq!(shuffled, awakened && accept);
            let view = game.projection(View::Referee).unwrap();
            assert_eq!(
                view["P1"]["pp"]["current"],
                pp - 3_u8 - if accept { 2_u8 } else { 0_u8 }
            );
            assert!(
                view["objects"]["source"]["keywords"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("守護"))
            );
            if awakened && accept && !full {
                assert_eq!(view["P1"]["ex"], json!(["d3"]));
                let before_free_play = view["P1"]["pp"]["current"].clone();
                let free_play = game.play_internal_card("d3", PlayerId { seat: 1 }).unwrap();
                finish(&mut game, free_play, 1, usize::MAX, None, true);
                assert_eq!(
                    game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
                    before_free_play,
                    "the actual EX play uses the installed zero-cost modifier"
                );
            } else {
                assert_eq!(
                    projected_zone_count(&view["P1"]["ex"]),
                    if full { 5 } else { 0 }
                );
            }
            activate(&mut game, "P1");
            let lastword = pending(&mut game, number, 3);
            resolved(finish(&mut game, lastword, 1, usize::MAX, None, true));
            assert_eq!(
                game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
                22_u8
            );
        }
    }
}
