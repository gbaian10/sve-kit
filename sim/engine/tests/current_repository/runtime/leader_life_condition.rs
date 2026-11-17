#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate printed card scenarios."
)]
use super::*;
use alloc::collections::BTreeMap;
use sve_engine::game::internal::SearchAnswer;

const PLAYER: PlayerId = PlayerId { seat: 1 };
type SupportCard = (
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static [&'static str],
);
const CONTROLS: [SupportCard; 13] = [
    (
        "lalamia-base",
        "音速の機構・ララミア",
        "ロイヤル",
        "フォロワー",
        "1",
        &[],
    ),
    ("one", "ordinary", "ニュートラル", "フォロワー", "1", &[]),
    ("two", "hunter", "エルフ", "フォロワー", "2", &["狩人"]),
    (
        "three",
        "expensive hunter",
        "エルフ",
        "フォロワー",
        "3",
        &["狩人"],
    ),
    (
        "monkey",
        "ハンマーモンキー",
        "エルフ",
        "フォロワー",
        "2",
        &[],
    ),
    (
        "fairy",
        "フェアリー",
        "エルフ",
        "フォロワー",
        "1",
        &["妖精"],
    ),
    ("dragon", "dragon", "ドラゴン", "フォロワー", "2", &[]),
    (
        "golem",
        "golem",
        "ウィッチ",
        "フォロワー",
        "2",
        &["ゴーレム"],
    ),
    ("spell", "spell", "ウィッチ", "スペル", "1", &[]),
    ("reviver", "reviver", "ニュートラル", "フォロワー", "1", &[]),
    (
        "destroyer",
        "destroyer",
        "ニュートラル",
        "フォロワー",
        "1",
        &[],
    ),
    ("hurter", "hurter", "ニュートラル", "フォロワー", "1", &[]),
    ("amulet", "amulet", "ニュートラル", "アミュレット", "1", &[]),
];
// Synthetic supports isolate filters and evolution forms from unrelated admission gaps.
fn prepared() -> &'static (SelectedEffects, BTreeMap<String, WellFormedCardProgram>) {
    static READY: OnceLock<(SelectedEffects, BTreeMap<String, WellFormedCardProgram>)> =
        OnceLock::new();
    READY.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        for (number, name, class, kind, cost, traits) in CONTROLS {
            let follower = kind == "フォロワー";
            snapshot.push_str(&json!({"number":number,"faces":[{"name":name,"card_class":class,"card_type":kind,"cost":cost,"power":if follower {"2"} else {"-"},"hp":if follower {"8"} else {"-"},"traits":traits,"text":null,"sections":[]}]}).to_string());
            snapshot.push('\n');
            input["documents"].as_array_mut().unwrap().push(json!([format!("{number}.yaml"),json!({"format":1_u8,"kind":"effect_set","cards":{number:{"status":"complete","abilities":[]}}}).to_string()]));
        }
        let selected = repository::selection_with_rules_from(&snapshot, &keywords, &input);
        let mut tree = expanded::tree().clone();
        let ability = tree["cards"]["synthetic-evolution"].clone();
        tree["cards"] = json!({});
        for (number, ..) in CONTROLS {
            if number.ends_with("-base") { tree["cards"][number] = ability.clone(); }
        }
        let template = expanded::tree()["cards"]["synthetic-evolution"].clone();
        for (number, steps, targets) in [
            ("reviver", json!([{"move":{"ref":"subject","to":"field","result":"returned"}}]),json!([{"bind":"subject","from":{"who":"self","zone":"cemetery","kind":"follower"},"n":1_u8}])),
            ("destroyer",json!([{"destroy":{"ref":{"who":"self","zone":"field","kind":"follower","name":"フェアリードラゴン"}}}]),json!([])),
            ("hurter",json!([{"life":{"who":"self","delta":-1_i8}}]),json!([])),
        ] {
            let mut action = template["abilities"][0].clone();
            action.as_object_mut().unwrap().remove("evolve");
            action["activate"] = json!({"cost":{"pp":0_u8},"do":steps});
            if !targets.as_array().unwrap().is_empty() {
                action["activate"]["targets"] = targets;
            }
            tree["cards"][number] = json!({"meta":template["meta"],"abilities":[action]});
        }
        (selected, lowerer().lower(&tree).unwrap())
    })
}
fn game(initial: &Value) -> Game {
    let (selected, bases) = prepared();
    let mut game = Game::from_selected_effects(
        selected.clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "ready-bp06",
    )
    .unwrap();
    let empty = WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![],
    })
    .unwrap();
    for (number, ..) in CONTROLS {
        game.load_internal_card(number, bases.get(number).unwrap_or(&empty))
            .unwrap();
    }
    game
}
fn position(number: &str, zone: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    initial["players"]["P1"]["zones"][zone] = json!([{"id":"source","card":number}]);
    initial["players"]["P1"]["zones"]["deck"] = json!(
        (0..10_u8)
            .map(|i| json!({"id":format!("deck{i}"),"card":"one"}))
            .collect::<Vec<_>>()
    );
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"one","state":{"acted":true}}]);
    initial
}
fn checks(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}
fn finish(game: &mut Game, mut step: CardPlay) {
    while let CardPlay::Awaiting(question) = step {
        let answer = match &question.domain {
            InputDomain::Targets(d) => Answer::Targets(
                d.members
                    .iter()
                    .take(usize::try_from(d.cardinality.maximum.unwrap().0).unwrap())
                    .map(|m| m.handle)
                    .collect(),
            ),
            InputDomain::CostMaterials(d) => Answer::CostMaterials(vec![d.members[0].handle]),
            InputDomain::Search(d) => {
                assert_eq!(d.cards.len(), 1, "only the printed filter matches");
                Answer::Search(SearchAnswer::Ungrouped(vec![d.cards[0].handle]))
            }
            InputDomain::Order(d) => Answer::Order(d.iter().rev().map(|m| m.handle).collect()),
            InputDomain::OptionalCost => Answer::OptionalCost(true),
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            InputDomain::TokenCapacity { choices } => {
                assert_eq!(
                    choices.len(),
                    2,
                    "one remaining EX slot offers each printed token"
                );
                Answer::TokenCapacity(0)
            }
            _ => panic!("unexpected decision: {:?}", question.domain),
        };
        step = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    assert!(
        !matches!(step, CardPlay::Cancelled),
        "printed action has its resources"
    );
    checks(game);
}
fn pending(game: &mut Game) -> CardPlay {
    checks(game);
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("printed trigger")
    };
    assert_eq!(options.len(), 1, "only the tested ability is pending");
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::Pending(options[0].handle),
    )
    .unwrap()
}
fn trigger(game: &mut Game) {
    let step = pending(game);
    finish(game, step);
}
fn activate_card(game: &mut Game, number: &str, line: u32) {
    let ability = selection().current[number]
        .program
        .card()
        .abilities
        .iter()
        .find(|a| a.source.line == line)
        .unwrap()
        .node
        .id;
    let step = game
        .activate_internal_card("source", ability, PLAYER)
        .unwrap();
    finish(game, step);
}
fn support(game: &mut Game, number: &str, id: &str) {
    let ability = prepared().1[number].card().abilities[0].node.id;
    let step = game.activate_internal_card(id, ability, PLAYER).unwrap();
    finish(game, step);
}
#[test]
fn frenzied_fur_triggers_only_once_per_turn_and_can_retire_to_heal_at_ten_life() {
    let mut initial = position("BP06-089", "field");
    initial["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .extend([
            json!({"id":"ally","card":"one"}),
            json!({"id":"hurter","card":"hurter"}),
        ]);
    let mut game = game(&initial);
    support(&mut game, "hurter", "hurter");
    trigger(&mut game);
    let first = game.projection(View::Referee).unwrap();
    assert_eq!(
        first["objects"]["ally"]["power"], 3_u8,
        "first life decrease buffs a chosen follower"
    );
    support(&mut game, "hurter", "hurter");
    assert!(
        game.internal_pending_choice().unwrap().is_none(),
        "second decrease does not create another trigger this turn"
    );
    activate_card(&mut game, "BP06-089", 2);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        state["P1"]["leader"]["life"], 10_u8,
        "retirement heals two after two life payments"
    );
    assert_eq!(
        state["P1"]["cemetery"],
        json!(["source"]),
        "the amulet is paid as a cost"
    );
}
