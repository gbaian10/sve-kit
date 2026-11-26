use super::event_cemetery::finish_checks;
use super::*;

const P1: PlayerId = PlayerId { seat: 1 };
const ELEMENT: &str = "CP05-T14";
const LOOK_MATCH: &str = "nightmare-look-follower";

fn waiting(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(question) = play else {
        panic!("the printed ability must ask its player");
    };
    question
}

fn choose(game: &mut Game, question: &DecisionView, answer: Answer) -> CardPlay {
    game.answer_internal_card(question.actor, question.token, answer)
        .unwrap()
}

fn enter_from_hand(game: &mut Game) {
    assert!(
        matches!(
            game.play_internal_card("source", P1).unwrap(),
            CardPlay::Entered(_)
        ),
        "the printed follower enters play before its Fanfare resolves"
    );
}

fn fanfare(game: &mut Game, printing: &str, line: u64) -> CardPlay {
    finish_checks(game);
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("expected the printed Fanfare");
    };
    let option = options
        .iter()
        .find(|option| option.card == printing && option.line == line)
        .unwrap();
    choose(game, &question, Answer::Pending(option.handle))
}

fn assert_dark_elements(view: &Value, count: usize) {
    let elements = view["P1"]["ex"].as_array().unwrap();
    assert_eq!(elements.len(), count, "the expected elements enter EX");
    for element in elements {
        assert_eq!(
            view["objects"][element.as_str().unwrap()]["card"],
            ELEMENT,
            "the created objects use the dark element printing"
        );
    }
}

#[test]
fn lady_grey_creates_a_dark_element_in_ex_on_entry() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"CP05-P64"}]);
    let mut game = game(&initial);
    enter_from_hand(&mut game);
    resolved(fanfare(&mut game, "CP05-P64", 4));
    assert_dark_elements(&game.projection(View::Referee).unwrap(), 1);
}

#[test]
fn ewig_kamerad_quick_creates_elements_and_draws_only_after_the_dark_payment() {
    for (has_material, accept) in [(false, false), (true, false), (true, true)] {
        let mut initial = draw_position();
        initial["turn"]["active"] = json!("P2");
        let mut hand = vec![
            json!({"id":"spell","card":"CP05-P67"}),
            json!({"id":"other","card":"BP01-037"}),
        ];
        if has_material {
            hand.push(json!({"id":"ferry","card":"CP05-076"}));
        }
        initial["players"]["P1"]["zones"]["hand"] = json!(hand);
        initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"drawn","card":"BP01-171"}]);
        let mut game = game(&initial);
        game.internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap();
        let question = game.internal_quick_choice().unwrap().unwrap();
        let InputDomain::Quick { options, .. } = &question.domain else {
            panic!("play the spell during the opposing turn");
        };
        let option = options
            .iter()
            .find(|option| option.card == "CP05-P67")
            .unwrap();
        let mut play = choose(
            &mut game,
            &question,
            Answer::Quick(Some(option.handle)),
        );
        if has_material {
            let declaration = waiting(play);
            assert!(matches!(declaration.domain, InputDomain::OptionalCost));
            play = choose(&mut game, &declaration, Answer::OptionalCost(accept));
        }
        if accept {
            let materials = waiting(play);
            let InputDomain::CostMaterials(domain) = &materials.domain else {
                panic!("the additional discard uses a private filtered payment");
            };
            assert_eq!(domain.members.len(), 1);
            assert_eq!(domain.members[0].card.as_deref(), Some("CP05-076"));
            assert!(game.internal_decision(View::P2).is_none());
            play = choose(
                &mut game,
                &materials,
                Answer::CostMaterials(vec![domain.members[0].handle]),
            );
        }
        resolved(play);
        finish_checks(&mut game);
        assert!(game.internal_pending_choice().unwrap().is_none());
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["P1"]["pp"]["current"], 9_u8);
        assert_eq!(view["P1"]["field"], json!([]));
        assert_dark_elements(&view, if accept { 2 } else { 0 });
        if accept {
            assert_eq!(view["P1"]["hand"], json!(["other", "drawn"]));
            assert!(
                view["P1"]["cemetery"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("ferry"))
            );
        } else {
            assert_eq!(
                view["P1"]["hand"],
                if has_material {
                    json!(["other", "ferry"])
                } else {
                    json!(["other"])
                }
            );
            assert_eq!(view["P1"]["cemetery"], json!(["spell"]));
        }
    }
}

fn game_with_look_fixture(initial: &Value) -> Game {
    static SELECTED: OnceLock<SelectedEffects> = OnceLock::new();
    let selected = SELECTED.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        let (mut printing, mut author) = current_fixture::author(
            LOOK_MATCH,
            "a671bda5000000000000000000000004",
            "フォロワー",
            vec![
                json!({"activate":{"zone":"field","cost":{"pp":0_u8},"do":[{"draw":{"n":1_u8}}]}}),
            ],
        );
        printing["faces"][0]["cost"] = json!("5");
        printing["faces"][0]["traits"] = json!(["グラブル"]);
        author["input"]["source_payload"]["faces"][0]["cost"] = json!(5_u8);
        snapshot.push_str(&printing.to_string());
        snapshot.push('\n');
        input["authors"].as_array_mut().unwrap().push(author);
        repository::selection_with_rules_from(&snapshot, &keywords, &input)
    });
    Game::from_selected_effects(
        selected.clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "cp05-nightmare-look",
    )
    .unwrap()
}

#[test]
fn nehan_looks_past_its_own_name_and_orders_the_four_remaining_cards() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"CP05-P58"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"same","card":"CP05-GBF51"}, {"id":"match","card":LOOK_MATCH},
        {"id":"low","card":"CP05-074"}, {"id":"different","card":"BP01-037"},
        {"id":"spell","card":"BP11-068"}, {"id":"tail","card":LOOK_MATCH}
    ]);
    let mut game = game_with_look_fixture(&initial);
    enter_from_hand(&mut game);
    let question = waiting(fanfare(&mut game, "CP05-P58", 1));
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("expected a private choice among the observed five cards");
    };
    assert!(game.internal_decision(View::P2).is_none());
    assert_eq!(domain.cardinality.minimum, Nat(0));
    assert_eq!(domain.cardinality.maximum, Some(Nat(1)));
    assert_eq!(domain.members.len(), 1);
    assert_eq!(domain.members[0].card.as_deref(), Some(LOOK_MATCH));
    let order = waiting(choose(
        &mut game,
        &question,
        Answer::Targets(vec![domain.members[0].handle]),
    ));
    let InputDomain::Order(cards) = &order.domain else {
        panic!("expected a private permutation of the remaining four cards");
    };
    assert_eq!(cards.len(), 4);
    assert!(game.internal_decision(View::P2).is_none());
    resolved(choose(
        &mut game,
        &order,
        Answer::Order(cards.iter().rev().map(|card| card.handle).collect()),
    ));
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["field"], json!(["source", "match"]));
    assert_eq!(
        view["P1"]["deck"],
        json!(["tail", "spell", "different", "low", "same"])
    );
}
