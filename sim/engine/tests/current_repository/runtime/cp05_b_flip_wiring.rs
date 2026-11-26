
use super::*;

const FORMS: [(&str, &str, &str); 4] = [
    ("earth-form", "earth-form", "土属性"),
    ("flip-form", "flip-form", "土属性"),
    ("fiorito-form", "花開く筋肉・フィオリト", "土属性"),
    ("water-form", "water-form", "水属性"),
];

fn selection_with_forms() -> &'static SelectedEffects {
    static SELECTED: OnceLock<SelectedEffects> = OnceLock::new();
    SELECTED.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        snapshot.truncate(snapshot.trim_end().len());
        let mut form_effects = serde_json::Map::new();
        for (number, name, attribute) in FORMS {
            let record = json!({"number":number,"faces":[{
                "name":name,"card_class":"ニュートラル","card_type":"フォロワー・エボルヴ",
                "cost":"-","power":"4","hp":"6","traits":[attribute],"text":null,"sections":[]
            }]});
            snapshot.push('\n');
            snapshot.push_str(&record.to_string());
            form_effects.insert(number.into(), json!({"status":"complete"}));
        }
        let document = json!({"format":1_u8,"kind":"fixture_cards","cards":form_effects});
        input["documents"]
            .as_array_mut()
            .unwrap()
            .push(json!(["flip-fixtures.yaml", document.to_string()]));
        repository::selection_with_rules_from(&snapshot, &keywords, &input)
    })
}

fn board(card: &str) -> Game {
    let mut game = Game::from_selected_effects(
        selection_with_forms().clone(),
        &position(card),
        &Value::Null,
        &Value::Null,
        "cp05-b-flip",
    )
    .unwrap();
    let blank = WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![],
    })
    .unwrap();
    for (number, _, _) in FORMS {
        game.load_internal_card(number, &blank).unwrap();
    }
    game
}

fn position(card: &str) -> Value {
    let mut initial = position_with_source(card, "hand");
    initial["turn"]["elapsed_turns"] = json!({"P1":5_u8,"P2":4_u8});
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([
        {"id":"up-a","card":"earth-form","face_up":true},
        {"id":"up-b","card":"earth-form","face_up":true},
        {"id":"up-c","card":"earth-form","face_up":true},
        {"id":"optional-earth","card":"flip-form","face_up":false},
        {"id":"form","card":"fiorito-form","face_up":false},
        {"id":"water","card":"water-form","face_up":false},
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"enemy","card":"BP01-037","state":{"acted":true,"hp":10_u8,"max_hp":10_u8}},
    ]);
    initial
}

fn run_entry(game: &mut Game, card: &str, flip: bool) {
    let question = await_card_choice(enter_with_fanfare(game, card));
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("expected the private evolve-deck choice");
    };
    assert_eq!(domain.cardinality.minimum, Nat(0), "the flip is optional");
    assert_eq!(
        domain.cardinality.maximum,
        Some(Nat(1)),
        "only one flip is allowed"
    );
    assert_eq!(
        domain.members.len(),
        2,
        "only the two face-down earth evolutions qualify"
    );
    assert!(
        game.internal_decision(View::P2).is_none(),
        "the flip choice is private"
    );
    let selected = domain
        .members
        .iter()
        .filter(|member| flip && member.card.as_deref() == Some("flip-form"))
        .map(|member| member.handle)
        .collect();
    resolved(
        game.answer_internal_card(question.actor, question.token, Answer::Targets(selected))
            .unwrap(),
    );
    finish_checks(game);
    let board = state(game);
    let ex = board["P1"]["ex"].as_array().unwrap();
    assert_eq!(ex.len(), 1, "the fanfare creates exactly one element");
    assert_eq!(
        board["objects"][ex[0].as_str().unwrap()]["card"],
        "CP05-T11",
        "the element is earth"
    );
    assert_eq!(
        board["objects"]["optional-earth"]["face_up"], flip,
        "only the chosen card flips"
    );
    assert_eq!(
        board["objects"]["form"]["face_up"], false,
        "the matching evolution stays face down"
    );
    assert_eq!(
        board["objects"]["water"]["face_up"], false,
        "water is outside the printed filter"
    );
}

#[test]
fn welder_can_turn_on_rush_and_its_attack_trigger_with_the_fanfare_flip() {
    for (card, flip) in [("CP05-015", true), ("CP05-P11", false)] {
        let mut game = board(card);
        run_entry(&mut game, card, flip);
        assert_eq!(has_granted_keyword(&state(&game), "source", "突進"), flip);
        if flip {
            assert_eq!(
                game.internal_rule_action(
                    &json!({"do":"attack","attacker":"source","target":"enemy"})
                )
                .unwrap()
                .outcome,
                "resolved",
            );
            finish_checks(&mut game);
            resolved(resolve_pending_card(&mut game, card));
            assert_eq!(state(&game)["P1"]["ex"].as_array().unwrap().len(), 2);
        }
    }
}

#[test]
fn fiorito_has_ward_and_only_evolves_for_zero_pp_after_the_fourth_earth_flip() {
    for (card, flip) in [("CP05-031", true), ("CP05-P23", false)] {
        let mut game = board(card);
        run_entry(&mut game, card, flip);
        assert!(has_granted_keyword(&state(&game), "source", "守護"));
        let before = pp(&game);
        let evolve = game
            .internal_rule_action(&json!({
                "do":"evolve","source":"source","evolve_card":"form","pay":{"pp":0_u8}
            }))
            .unwrap();
        assert_eq!(
            evolve.outcome,
            if flip { "resolved" } else { "cannot-evolve" }
        );
        let board = state(&game);
        if flip {
            assert_eq!(board["objects"]["source"]["evolved_with"], "form");
        } else {
            assert!(board["objects"]["source"]["evolved_with"].is_null());
        }
        assert_eq!(pp(&game), before);
    }
}
