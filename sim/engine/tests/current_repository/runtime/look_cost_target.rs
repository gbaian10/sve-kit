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
