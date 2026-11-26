use super::*;
use alloc::collections::BTreeMap;
use sve_engine::game::internal::SearchAnswer;
const PLAYER: PlayerId = PlayerId { seat: 1 };
type Companion = (
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static [&'static str],
);
const CONTROLS: [Companion; 3] = [
    (
        "wire-plain",
        "plain",
        "ニュートラル",
        "フォロワー",
        "1",
        &[],
    ),
    (
        "wire-academy",
        "academy",
        "エルフ",
        "フォロワー",
        "2",
        &["学院"],
    ),
    (
        "wire-machina",
        "デウスエクスマキナ",
        "ニュートラル",
        "フォロワー",
        "6",
        &[],
    ),
];
const FORMS: [&str; 3] = ["CP02-010", "BP21-032", "BP21-086"];
fn selected() -> &'static SelectedEffects {
    static READY: OnceLock<SelectedEffects> = OnceLock::new();
    READY.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = producer_inputs();
        // Companions isolate filters while the tested current programs remain complete.
        for (number, name, class, kind, cost, traits) in CONTROLS {
            let follower = kind == "フォロワー";
            append(
                &mut snapshot,
                &mut input,
                number,
                &json!({
                    "name": name,
                    "card_class": class,
                    "card_type": kind,
                    "cost": cost,
                    "power": if follower { "3" } else { "-" },
                    "hp": if follower { "20" } else { "-" },
                    "traits": traits,
                    "text": null,
                    "sections": []
                }),
            );
        }
        for form in FORMS {
            let mut face = snapshot
                .lines()
                .map(|line| serde_json::from_str::<Value>(line).unwrap())
                .find(|record| record["number"] == form)
                .unwrap()["faces"][0]
                .clone();
            face["card_type"] = json!("フォロワー");
            face["cost"] = json!("1");
            face["text"] = Value::Null;
            face["sections"] = json!([]);
            append(
                &mut snapshot,
                &mut input,
                &format!("wire-base-{form}"),
                &face,
            );
        }
        repository::selection_with_rules_from(&snapshot, &keywords, &input)
    })
}

fn bases() -> &'static BTreeMap<String, WellFormedCardProgram> {
    static READY: OnceLock<BTreeMap<String, WellFormedCardProgram>> = OnceLock::new();
    READY.get_or_init(|| {
        let mut tree = expanded::tree().clone();
        let template = tree["cards"]["synthetic-evolution"].clone();
        tree["cards"] = json!({});
        for form in FORMS {
            tree["cards"][format!("wire-base-{form}")] = template.clone();
        }
        lowerer().lower(&tree).unwrap()
    })
}

fn load_controls(game: &mut Game) {
    let empty = WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![],
    })
    .unwrap();
    for (number, ..) in CONTROLS {
        game.load_internal_card(number, &empty).unwrap();
    }
    for (number, program) in bases() {
        game.load_internal_card(number, program).unwrap();
    }
}
fn position(card: &str) -> Value {
    let mut initial = draw_position();
    if card.starts_with("CP04") || card.starts_with("PCS01") {
        initial["players"]["P1"]["construction"] = json!("title");
        initial["players"]["P1"]["title_code"] = json!("princess_connect_re_dive");
    }
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    for seat in ["P1", "P2"] {
        initial["players"][seat]["zones"]["deck"] = json!(
            (0_u8..6)
                .map(|i| json!({"id":format!("{seat}-draw{i}"),"card":"wire-plain"}))
                .collect::<Vec<_>>()
        );
    }
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"wire-plain"}]);
    initial
}
fn game(initial: &Value) -> Game {
    let mut game = Game::from_selected_effects(
        selected().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "wire-hot03",
    )
    .unwrap();
    load_controls(&mut game);
    game
}
fn take(c: &Cardinality, n: usize) -> usize {
    c.maximum
        .map_or(n, |x| usize::try_from(x.0).unwrap())
        .min(n)
}
fn finish(game: &mut Game, mut step: CardPlay) {
    while let CardPlay::Awaiting(q) = step {
        let answer = match &q.domain {
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            InputDomain::OptionalCost => Answer::OptionalCost(true),
            InputDomain::Modes { .. } => Answer::Modes(vec![ModeId(1)]),
            InputDomain::Targets(d) => Answer::Targets(
                d.members
                    .iter()
                    .take(take(&d.cardinality, d.members.len()))
                    .map(|m| m.handle)
                    .collect(),
            ),
            InputDomain::CostMaterials(d) => Answer::CostMaterials(
                d.members
                    .iter()
                    .take(usize::try_from(d.cardinality.minimum.0).unwrap())
                    .map(|m| m.handle)
                    .collect(),
            ),
            InputDomain::Search(d) => Answer::Search(d.groups.as_ref().map_or_else(
                || {
                    SearchAnswer::Ungrouped(
                        d.cards
                            .iter()
                            .take(take(&d.cardinality, d.cards.len()))
                            .map(|m| m.handle)
                            .collect(),
                    )
                },
                |groups| {
                    SearchAnswer::Grouped(
                        groups
                            .iter()
                            .map(|g| {
                                g.handles
                                    .iter()
                                    .copied()
                                    .take(take(&g.cardinality, g.handles.len()))
                                    .collect()
                            })
                            .collect(),
                    )
                },
            )),
            InputDomain::Order(d) => Answer::Order(d.iter().map(|m| m.handle).collect()),
            other => panic!("unexpected choice: {other:?}"),
        };
        step = game.answer_internal_card(q.actor, q.token, answer).unwrap();
    }
}
fn settle(game: &mut Game) {
    loop {
        if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
        let Some(q) = game.internal_pending_choice().unwrap() else {
            break;
        };
        let InputDomain::Pending(d) = &q.domain else {
            panic!("pending")
        };
        let step = game
            .answer_internal_card(q.actor, q.token, Answer::Pending(d[0].handle))
            .unwrap();
        finish(game, step);
    }
}
fn play(game: &mut Game, id: &str) {
    let step = game.play_internal_card(id, PLAYER).unwrap();
    finish(game, step);
    settle(game);
}

#[test]
fn bp10_044_fanfare_pays_four_and_one_stack_to_banish_a_cheap_follower() {
    for card in ["BP10-044", "BP10-P10"] {
        let mut initial = position(card);
        initial["players"]["P1"]["zones"]["field"] =
            json!([{"id":"soil","card":"BP03-T02","state":{"counters":{"stack_counter":2_u8}}}]);
        let mut game = game(&initial);
        play(&mut game, "source");
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["pp"]["current"], 5_u8);
        assert_eq!(
            state["objects"]["soil"]["counters"]["スタックカウンター"],
            1_u8
        );
        assert_eq!(state["P2"]["banish"], json!(["enemy"]));
    }
}
#[test]
fn cp04_013_can_be_played_on_its_owners_fifth_turn() {
    for card in ["CP04-013", "CP04-P09"] {
        let mut initial = position(card);
        initial["turn"]["elapsed_turns"]["P1"] = json!(4_u8);
        let mut early = game(&initial);
        early.play_internal_card("source", PLAYER).unwrap_err();
        assert_eq!(
            early.projection(View::Referee).unwrap()["P1"]["hand"],
            json!(["source"])
        );
        initial["turn"]["elapsed_turns"]["P1"] = json!(5_u8);
        let mut game = game(&initial);
        play(&mut game, "source");
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["field"], json!(["source"]));
        assert!(
            state["objects"]["source"]["keywords"]
                .as_array()
                .unwrap()
                .contains(&json!("守護"))
        );
    }
}
