#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected choices invalidate printed card scenarios."
)]
use super::*;
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
        "medusa-print",
        "毒牙の姫・メドゥーサ",
        "ナイトメア",
        "フォロワー",
        "3",
        &[],
    ),
    (
        "beater",
        "デモンビーター",
        "ナイトメア",
        "フォロワー",
        "1",
        &[],
    ),
    (
        "neutral-one",
        "neutral one",
        "ニュートラル",
        "フォロワー",
        "1",
        &["ゴブリン"],
    ),
    (
        "neutral-two",
        "neutral two",
        "ニュートラル",
        "フォロワー",
        "2",
        &[],
    ),
    ("royal-one", "royal one", "ロイヤル", "フォロワー", "1", &[]),
    ("witch-two", "witch two", "ウィッチ", "フォロワー", "2", &[]),
    ("witch-spell", "witch spell", "ウィッチ", "スペル", "1", &[]),
    (
        "nightmare-one",
        "nightmare one",
        "ナイトメア",
        "フォロワー",
        "1",
        &[],
    ),
    (
        "nightmare-three",
        "nightmare three",
        "ナイトメア",
        "フォロワー",
        "3",
        &[],
    ),
    (
        "amulet-one",
        "amulet one",
        "ニュートラル",
        "アミュレット",
        "1",
        &[],
    ),
    ("steno", "ステンノ", "ナイトメア", "フォロワー", "1", &[]),
    (
        "euryale",
        "エウリュアレー",
        "ナイトメア",
        "フォロワー",
        "1",
        &[],
    ),
    ("old-spell", "old spell", "ニュートラル", "スペル", "1", &[]),
];
// Synthetic supports isolate filters without admitting unrelated card effects.
fn prepared() -> &'static SelectedEffects {
    static READY: OnceLock<SelectedEffects> = OnceLock::new();
    READY.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        for (number, name, class, kind, cost, traits) in CONTROLS {
            let follower=kind=="フォロワー";
            snapshot.push_str(&json!({"number":number,"faces":[{"name":name,"card_class":class,"card_type":kind,"cost":cost,"power":if follower {"2"} else {"-"},"hp":if follower {"3"} else {"-"},"traits":traits,"text":null,"sections":[]}]}).to_string());
            snapshot.push('\n');
            input["documents"].as_array_mut().unwrap().push(json!([format!("{number}.yaml"),json!({"format":1_u8,"kind":"effect_set","cards":{number:{"status":"complete","abilities":[]}}}).to_string()]));
        }
        repository::selection_with_rules_from(&snapshot,&keywords,&input)
    })
}
fn game(initial: &Value) -> Game {
    let mut game = Game::from_selected_effects(
        prepared().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "ready-bp04",
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
            if number == "neutral-two" {
                destroyer_program()
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
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    initial["players"]["P1"]["zones"][zone] = json!([{"id":"source","card":number}]);
    initial["players"]["P1"]["zones"]["deck"] = json!(
        (0..6_u8)
            .map(|i| json!({"id":format!("deck{i}"),"card":"neutral-one"}))
            .collect::<Vec<_>>()
    );
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-037","state":{"hp":12_u8,"max_hp":12_u8,"acted":true}}]);
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
            InputDomain::Targets(d) => Answer::Targets(
                d.members
                    .iter()
                    .take(usize::try_from(d.cardinality.maximum.unwrap().0).unwrap())
                    .map(|m| m.handle)
                    .collect(),
            ),
            InputDomain::CostMaterials(d) => Answer::CostMaterials(
                d.members
                    .iter()
                    .take(usize::try_from(d.cardinality.maximum.unwrap().0).unwrap())
                    .map(|m| m.handle)
                    .collect(),
            ),
            InputDomain::Search(d) => {
                assert_eq!(
                    d.cards.len(),
                    1,
                    "only the intended card matches the printed search"
                );
                Answer::Search(SearchAnswer::Ungrouped(vec![d.cards[0].handle]))
            }
            InputDomain::Order(d) => Answer::Order(d.iter().rev().map(|m| m.handle).collect()),
            InputDomain::OptionalCost => Answer::OptionalCost(true),
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            _ => panic!("unexpected printed question: {:?}", q.domain),
        };
        step = game.answer_internal_card(q.actor, q.token, answer).unwrap();
    }
    assert!(
        !matches!(step, CardPlay::Cancelled),
        "the printed action has its resources"
    );
    checks(game);
}
fn activate_card(game: &mut Game, number: &str, line: u32) {
    let id = selection().current[number]
        .program
        .card()
        .abilities
        .iter()
        .find(|a| a.source.line == line)
        .unwrap()
        .node
        .id;
    let step = game.activate_internal_card("source", id, PLAYER).unwrap();
    finish(game, step);
}
fn assert_disclosed_cards(game: &Game, shown: &[&str], private: &[&str]) {
    let view = game.projection(View::P2).unwrap();
    let known = view["known_cards"].as_array().unwrap();
    for id in shown {
        assert!(
            known.iter().any(|card| card["id"] == *id),
            "the chosen card is disclosed to the opponent"
        );
    }
    for id in private {
        assert!(
            !known.iter().any(|card| card["id"] == *id),
            "the untaken deck cards remain private"
        );
    }
}
// Simultaneous field entries have no printed ordering requirement here.
fn destroyer_program() -> &'static WellFormedCardProgram {
    static PROGRAM: OnceLock<WellFormedCardProgram> = OnceLock::new();
    PROGRAM.get_or_init(|| {
        let mut tree = expanded::tree().clone();
        let base = &tree["cards"]["synthetic-evolution"];
        let mut ability = base["abilities"][0].clone();
        ability.as_object_mut().unwrap().remove("evolve");
        ability["activate"] = json!({"cost":{"pp":0_u8},"do":[
            {"destroy":{"ref":{"who":"self","zone":"field","kind":"follower"}}}
        ]});
        tree["cards"] = json!({"neutral-two":{"meta":base["meta"],"abilities":[ability]}});
        lowerer()
            .lower(&tree)
            .unwrap()
            .remove("neutral-two")
            .unwrap()
    })
}

fn destroy_own_followers(game: &mut Game) {
    let program = destroyer_program();
    let step = game
        .activate_internal_card("killer", program.card().abilities[0].node.id, PLAYER)
        .unwrap();
    finish(game, step);
}

#[test]
fn stairway_counts_each_fallen_follower_and_requires_six_before_looking_five() {
    let mut initial = position("BP04-122", "field");
    initial["players"]["P1"]["zones"]["field"][0]["state"] =
        json!({"counters":{"魂カウンター":4_u8}});
    initial["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .extend([
            json!({"id":"killer","card":"neutral-two"}),
            json!({"id":"fallen","card":"neutral-one"}),
        ]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"spell","card":"witch-spell"}, {"id":"first","card":"neutral-one"},
        {"id":"second","card":"nightmare-one"}, {"id":"amulet","card":"amulet-one"},
        {"id":"rest","card":"old-spell"}, {"id":"unseen","card":"neutral-one"}
    ]);
    let mut game = game(&initial);
    let ability = selection().current["BP04-122"].program.card().abilities[1]
        .node
        .id;
    let before = game.projection(View::Referee).unwrap();
    game.activate_internal_card("source", ability, PLAYER)
        .unwrap_err();
    assert_eq!(
        game.projection(View::Referee).unwrap(),
        before,
        "insufficient counters preserve the source and resources"
    );
    destroy_own_followers(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["field"],
        json!(["source"]),
        "only the two followers fall; the counter holder survives"
    );
    for _ in 0..2_u8 {
        let question = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = &question.domain else {
            panic!("counter triggers")
        };
        let step = game
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap();
        finish(&mut game, step);
    }
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["source"]["counters"]["魂カウンター"],
        6_u8
    );
    activate_card(&mut game, "BP04-122", 2);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["first", "second"]));
    assert_disclosed_cards(
        &game,
        &["first", "second"],
        &["spell", "amulet", "rest", "unseen"],
    );
    assert_eq!(
        state["P1"]["deck"],
        json!(["unseen", "rest", "amulet", "spell"])
    );
    assert_eq!(state["P1"]["field"], json!([]));
    assert_eq!(state["P2"]["field"], json!(["enemy"]));
}
