#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate printed shape scenarios."
)]
use super::*;
use std::{fs::read_to_string, path::PathBuf};
use sve_engine::catalog::EngineIdentityInput;
use sve_engine::game::internal::{DecisionView, SearchAnswer, SearchDomain};

type ShapeFace = (
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    Option<&'static str>,
);
const CONTROLS: [ShapeFace; 14] = [
    (
        "royal-albert",
        "アルベールの仲間",
        "ロイヤル",
        "フォロワー",
        "1",
        None,
    ),
    (
        "elf-albert",
        "アルベールの仲間",
        "エルフ",
        "フォロワー",
        "1",
        None,
    ),
    (
        "royal-plain",
        "別の仲間",
        "ロイヤル",
        "フォロワー",
        "1",
        None,
    ),
    (
        "royal-spell",
        "アルベールの魔法",
        "ロイヤル",
        "スペル",
        "1",
        None,
    ),
    (
        "material",
        "レヴィオンの材料",
        "ロイヤル",
        "スペル",
        "1",
        Some("レヴィオン"),
    ),
    (
        "same-golem",
        "氷塊のゴーレム",
        "ウィッチ",
        "フォロワー",
        "1",
        Some("ゴーレム"),
    ),
    (
        "other-golem",
        "別のゴーレム",
        "ウィッチ",
        "フォロワー",
        "1",
        Some("ゴーレム"),
    ),
    (
        "not-golem",
        "普通の仲間",
        "ウィッチ",
        "フォロワー",
        "1",
        None,
    ),
    (
        "tokyo-a",
        "透京A",
        "ドラゴン",
        "フォロワー",
        "3",
        Some("透京"),
    ),
    (
        "tokyo-a-copy",
        "透京A",
        "ドラゴン",
        "フォロワー",
        "3",
        Some("透京"),
    ),
    (
        "tokyo-b",
        "透京B",
        "ドラゴン",
        "フォロワー",
        "3",
        Some("透京"),
    ),
    (
        "tokyo-c",
        "透京C",
        "ドラゴン",
        "フォロワー",
        "3",
        Some("透京"),
    ),
    (
        "tokyo-costly",
        "透京D",
        "ドラゴン",
        "フォロワー",
        "4",
        Some("透京"),
    ),
    (
        "filler",
        "場の仲間",
        "ニュートラル",
        "フォロワー",
        "0",
        None,
    ),
];
fn prepared() -> &'static SelectedEffects {
    static READY: OnceLock<SelectedEffects> = OnceLock::new();
    READY.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        for (number, name, class, kind, cost, trait_id) in CONTROLS {
            snapshot.push_str(
                &json!({"number":number,"faces":[{
                    "name":name,"card_class":class,"card_type":kind,
                    "traits":trait_id.into_iter().collect::<Vec<_>>(),"cost":cost,
                    "power":if kind == "フォロワー" {"1"} else {"-"},"hp":if kind == "フォロワー" {"3"} else {"-"},"text":null,"sections":[]
                }]})
                .to_string(),
            );
            snapshot.push('\n');
            let mut document = json!({"format":1_u8,"kind":"effect_set","cards":{}});
            document["cards"][number] = json!({"status":"complete","abilities":[]});
            input["documents"]
                .as_array_mut()
                .unwrap()
                .push(json!([format!("{number}.yaml"), document.to_string()]));
        }
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../authored/rules/engine/index.yaml");
        let mut rules: Value = serde_saphyr::from_str(&read_to_string(path).unwrap()).unwrap();
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
fn fixture(initial: &Value) -> Game {
    let mut game = Game::from_selected_effects(
        prepared().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "search-shape",
    )
    .unwrap();
    let empty = WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![],
    })
    .unwrap();
    for (number, ..) in CONTROLS {
        {
            game.load_internal_card(number, &empty).unwrap();
        }
    }
    game
}
fn settle(game: &mut Game, mut play: CardPlay) -> CardPlay {
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
fn trigger(game: &mut Game, number: &str, line: u64) -> CardPlay {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("pending")
    };
    let handle = options
        .iter()
        .find(|option| option.card == number && option.line == line)
        .unwrap()
        .handle;
    let play = game
        .answer_internal_card(question.actor, question.token, Answer::Pending(handle))
        .unwrap();
    settle(game, play)
}
fn enter(game: &mut Game, number: &str, line: u64) -> CardPlay {
    let play = game
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap();
    assert!(
        matches!(settle(game, play), CardPlay::Entered(_)),
        "the real source enters"
    );
    trigger(game, number, line)
}

fn observed_state(game: &Game) -> ([Value; 3], Option<DecisionView>, Option<ResumeStage>) {
    (
        [
            game.projection(View::Referee).unwrap(),
            game.projection(View::P1).unwrap(),
            game.projection(View::P2).unwrap(),
        ],
        game.internal_decision(View::Referee),
        game.internal_card_stage(),
    )
}
fn initial(number: &str, deck: Value) -> Value {
    let mut value = draw_position();
    value["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":number}]);
    value["players"]["P1"]["zones"]["deck"] = deck;
    value["players"]["P1"]["leader"]["life"] = json!(10_u8);
    value
}
fn single(game: &mut Game, play: CardPlay, expected: &str) {
    let CardPlay::Awaiting(question) = play else {
        panic!("search")
    };
    let InputDomain::Search(domain) = &question.domain else {
        panic!("search domain")
    };
    assert_eq!(
        domain
            .cards
            .iter()
            .map(|c| c.card.as_str())
            .collect::<Vec<_>>(),
        vec![expected],
        "the private domain contains exactly the matching card"
    );
    let played = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Search(SearchAnswer::Ungrouped(vec![domain.cards[0].handle])),
        )
        .unwrap();
    resolved(settle(game, played));
}
#[test]
fn bp13_026_discards_a_levion_then_searches_both_royal_and_name_fragment() {
    let mut state = initial(
        "BP13-026",
        json!([
            {"id":"good","card":"royal-albert"},{"id":"class","card":"elf-albert"},
            {"id":"name","card":"royal-plain"},{"id":"kind","card":"royal-spell"}
        ]),
    );
    state["players"]["P1"]["zones"]["hand"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"payment","card":"material"}));
    let mut game = fixture(&state);
    let play = enter(&mut game, "BP13-026", 2);
    let CardPlay::Awaiting(q) = play else {
        panic!("discard")
    };
    let InputDomain::CostMaterials(domain) = &q.domain else {
        panic!("materials")
    };
    assert_eq!(domain.members.len(), 1);
    let paid = game
        .answer_internal_card(
            q.actor,
            q.token,
            Answer::CostMaterials(vec![domain.members[0].handle]),
        )
        .unwrap();
    single(&mut game, paid, "royal-albert");
    let finished = game.projection(View::Referee).unwrap();
    assert_eq!(finished["P1"]["hand"], json!(["good"]));
    assert_eq!(finished["P1"]["cemetery"], json!(["payment"]));
}
#[test]
fn bp22_052_excludes_its_source_name_even_when_a_different_printing_matches_trait() {
    let state = initial(
        "BP22-052",
        json!([
            {"id":"same","card":"same-golem"},{"id":"other","card":"other-golem"},
            {"id":"plain","card":"not-golem"}
        ]),
    );
    let mut game = fixture(&state);
    let play = enter(&mut game, "BP22-052", 1);
    single(&mut game, play, "other-golem");
    let finished = game.projection(View::Referee).unwrap();
    assert_eq!(finished["P1"]["field"], json!(["source", "other"]));
    assert_eq!(finished["P1"]["deck_count"], 2_u8);
}
fn allocate_one(game: &mut Game, allocation: CardPlay, old_handles: &[u64]) -> CardPlay {
    let CardPlay::Awaiting(capacity) = allocation else {
        panic!("capacity")
    };
    let InputDomain::SearchPartition(domain) = &capacity.domain else {
        panic!("capacity domain")
    };
    assert_eq!(
        domain.cards.len(),
        3,
        "all found cards survive into the capacity decision"
    );
    assert_eq!(
        domain.cardinality,
        Cardinality {
            minimum: Nat(1),
            maximum: Some(Nat(1))
        },
        "the field has exactly one free place"
    );
    assert!(
        domain
            .cards
            .iter()
            .all(|card| !old_handles.contains(&card.handle)),
        "capacity uses fresh handles"
    );
    let before_capacity = observed_state(game);
    assert!(
        game.answer_internal_card(
            PlayerId { seat: 2 },
            capacity.token,
            Answer::SearchPartition(vec![domain.cards[0].handle])
        )
        .is_err(),
        "only the searcher allocates the found cards"
    );
    assert!(
        observed_state(game) == before_capacity,
        "an opposing answer preserves projections and decision authority"
    );
    let selected = domain
        .cards
        .iter()
        .find(|card| card.card == "tokyo-b")
        .unwrap()
        .handle;
    game.answer_internal_card(
        capacity.actor,
        capacity.token,
        Answer::SearchPartition(vec![selected]),
    )
    .unwrap()
}
#[test]
fn bp18_065_finds_three_distinct_names_before_selecting_one_for_field_capacity() {
    let mut state = initial(
        "BP18-065",
        json!([
            {"id":"a","card":"tokyo-a"},{"id":"copy","card":"tokyo-a-copy"},
            {"id":"b","card":"tokyo-b"},{"id":"c","card":"tokyo-c"},
            {"id":"cost","card":"tokyo-costly"},{"id":"trait","card":"filler"}
        ]),
    );
    state["players"]["P1"]["zones"]["field"] = json!(
        (0_u8..4)
            .map(|i| json!({"id":format!("f{i}"),"card":"filler"}))
            .collect::<Vec<_>>()
    );
    let mut game = fixture(&state);
    let play = game
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap();
    let CardPlay::Awaiting(q) = play else {
        panic!("search")
    };
    let InputDomain::Search(SearchDomain {
        cards,
        cardinality,
        groups,
    }) = &q.domain
    else {
        panic!("search")
    };
    assert_eq!(
        cardinality.maximum,
        Some(Nat(3)),
        "capacity does not truncate finding"
    );
    assert_eq!(cards.len(), 4, "only the requested cost and trait match");
    assert_eq!(groups.as_ref().unwrap().len(), 1);
    let handle = |name: &str| cards.iter().find(|c| c.card == name).unwrap().handle;
    let bad = vec![handle("tokyo-a"), handle("tokyo-a-copy")];
    let before = observed_state(&game);
    assert!(
        game.answer_internal_card(
            q.actor,
            q.token,
            Answer::Search(SearchAnswer::Grouped(vec![bad]))
        )
        .is_err(),
        "duplicate names cannot satisfy distinct search"
    );
    assert!(
        observed_state(&game) == before,
        "duplicate names preserve projections, stage and decision authority"
    );
    let chosen = vec![handle("tokyo-a"), handle("tokyo-b"), handle("tokyo-c")];
    let allocation = game
        .answer_internal_card(
            q.actor,
            q.token,
            Answer::Search(SearchAnswer::Grouped(vec![chosen.clone()])),
        )
        .unwrap();
    let continuation = allocate_one(&mut game, allocation, &chosen);
    resolved(settle(&mut game, continuation));
    let finished = game.projection(View::Referee).unwrap();
    assert_eq!(
        finished["P1"]["field"],
        json!(["f0", "f1", "f2", "f3", "b"])
    );
    assert_eq!(finished["P1"]["deck_count"], 5_u8);
    assert_eq!(
        finished["P1"]["leader"]["life"], 12_u8,
        "the spell continues after capacity"
    );
}
