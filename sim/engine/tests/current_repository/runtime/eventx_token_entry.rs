use super::event_cemetery::finish_checks;
use super::*;

const CONTROLS: [(&str, &str, &str, &str, u8); 1] =
    [("eventx-other", "eventx other", "反例", "ニュートラル", 1)];

fn selected() -> &'static SelectedEffects {
    static READY: OnceLock<SelectedEffects> = OnceLock::new();
    READY.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = producer_inputs();
        for (number, name, tribe, class, power) in CONTROLS {
            snapshot.push_str(&json!({"number":number,"faces":[{
                "name":name,"card_class":class,"card_type":"フォロワー","traits":[tribe],
                "cost":"1","power":power.to_string(),"hp":"6","text":null,"sections":[]
            }]}).to_string());
            snapshot.push('\n');
            input["documents"].as_array_mut().unwrap().push(json!([
                format!("{number}.yaml"),
                json!({"format":1_u8,"kind":"effect_set","cards":{number:{"status":"complete","abilities":[]}}}).to_string()
            ]));
        }
        repository::selection_with_rules_from(&snapshot, &keywords, &input)
    })
}

fn position(number: &str, zone: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    initial["players"]["P1"]["zones"][zone] = json!([{"id":"source","card":number}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"d1","card":"eventx-other"},{"id":"d2","card":"eventx-other"},
        {"id":"d3","card":"eventx-other"},{"id":"d4","card":"eventx-other"}
    ]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"eventx-other","state":{"acted":true}}]);
    initial
}

fn fixture(initial: &Value) -> Game {
    let mut game = Game::from_selected_effects(
        selected().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "eventx-token-entry",
    )
    .unwrap();
    let empty = WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![],
    })
    .unwrap();
    for (number, ..) in CONTROLS {
        game.load_internal_card(number, &empty).unwrap();
    }
    game
}

fn finish(game: &mut Game, mut play: CardPlay, take: bool) {
    while let CardPlay::Awaiting(q) = play {
        let answer = match &q.domain {
            InputDomain::DeclareName(_) => panic!("unexpected card name declaration"),
            InputDomain::DiceReroll { .. } => panic!("unexpected fixture dice reroll"),
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            InputDomain::Targets(d) => {
                assert!(
                    d.members.len() <= 1,
                    "each printed choice has at most one legal target"
                );
                Answer::Targets(if take || d.cardinality.minimum.0 > 0 {
                    d.members.iter().map(|m| m.handle).collect()
                } else {
                    vec![]
                })
            }
            InputDomain::Order(cards) => Answer::Order(cards.iter().map(|m| m.handle).collect()),
            InputDomain::Abilities(_)
            | InputDomain::ResourceEntryPayment { .. }
            | InputDomain::Quick { .. }
            | InputDomain::OptionalCost
            | InputDomain::CostMaterials(_)
            | InputDomain::BeforeDraw
            | InputDomain::Pending(_)
            | InputDomain::TriggerQuota(_)
            | InputDomain::Integer(_)
            | InputDomain::DamageReplacements(_)
            | InputDomain::DriveTrigger { .. }
            | InputDomain::Search(_)
            | InputDomain::SearchPartition(_)
            | InputDomain::Distribution(_)
            | InputDomain::SearchReplacement
            | InputDomain::TokenCapacity { .. }
            | InputDomain::Modes { .. } => {
                panic!("only printed target, Ward or bottom-order choices are expected")
            }
        };
        play = game.answer_internal_card(q.actor, q.token, answer).unwrap();
    }
    assert!(
        !matches!(play, CardPlay::Cancelled),
        "the printed instruction resolves"
    );
    finish_checks(game);
}

fn resolve(game: &mut Game, number: &str, take: bool) {
    let q = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &q.domain else {
        panic!("printed pending ability");
    };
    let matches: Vec<_> = options.iter().filter(|o| o.card == number).collect();
    assert_eq!(
        matches.len(),
        1,
        "the expected printed ability is pending once"
    );
    let play = game
        .answer_internal_card(q.actor, q.token, Answer::Pending(matches[0].handle))
        .unwrap();
    finish(game, play, take);
}

fn play(game: &mut Game, id: &str) {
    let result = game.play_internal_card(id, PlayerId { seat: 1 }).unwrap();
    finish(game, result, true);
}

fn only_created(state: &Value, zone: &str, number: &str) -> String {
    let matching: Vec<_> = state["P1"][zone]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|id| {
            let id = id.as_str().unwrap();
            (state["objects"][id]["card"] == number).then(|| id.to_owned())
        })
        .collect();
    assert_eq!(
        matching.len(),
        1,
        "exactly one printed token was created in its destination"
    );
    matching[0].clone()
}

#[test]
fn historian_creates_an_artifact_then_observes_its_token_entry_for_two_damage() {
    let mut game = fixture(&position("BP08-016", "hand"));
    play(&mut game, "source");
    resolve(&mut game, "BP08-016", true);
    resolve(&mut game, "BP08-016", true);
    let state = game.projection(View::Referee).unwrap();
    only_created(&state, "field", "BP05-T04");
    assert_eq!(state["objects"]["enemy"]["hp"], 4_u8);
    assert!(
        game.internal_pending_choice().unwrap().is_none(),
        "the ordinary historian entry is not a token event"
    );
}
