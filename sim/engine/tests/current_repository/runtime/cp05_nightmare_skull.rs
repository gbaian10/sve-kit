#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate these printed card scenarios."
)]
use super::event_cemetery::finish_checks;
use super::*;

const P1: PlayerId = PlayerId { seat: 1 };

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

const EARTH_FORM: &str = "nightmare-earth-evolution";

fn game_with_earth_forms(initial: &Value) -> Game {
    static SELECTED: OnceLock<SelectedEffects> = OnceLock::new();
    let selected = SELECTED.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        let (mut printing, author) = current_fixture::author(
            EARTH_FORM,
            "a671bda5000000000000000000000003",
            "フォロワー・エボルヴ",
            vec![json!({"evolve_time":{"do":[{"draw":{"n":1_u8}}]}})],
        );
        printing["faces"][0]["traits"] = json!(["土属性"]);
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
        "cp05-nightmare-earth-evolution",
    )
    .unwrap()
}

#[test]
fn skull_may_flip_its_fourth_earth_follower_for_storm() {
    for flip in [false, true] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"CP05-P66"}]);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([
            {"id":"first","card":EARTH_FORM,"face_up":true},
            {"id":"second","card":EARTH_FORM,"face_up":true},
            {"id":"third","card":EARTH_FORM,"face_up":true},
            {"id":"fourth","card":EARTH_FORM,"face_up":false},
            {"id":"wrong-up","card":"BP03-021","face_up":true},
            {"id":"wrong","card":"BP03-021","face_up":false}
        ]);
        initial["players"]["P2"]["zones"]["evolve_deck"] =
            json!([{"id":"enemy","card":EARTH_FORM,"face_up":true}]);
        let mut game = game_with_earth_forms(&initial);
        enter_from_hand(&mut game);
        let question = waiting(fanfare(&mut game, "CP05-P66", 2));
        let InputDomain::Targets(domain) = &question.domain else {
            panic!("expected the printed Fanfare's private EVOLVE-deck choice");
        };
        assert!(game.internal_decision(View::P2).is_none());
        assert_eq!(domain.members.len(), 1);
        assert_eq!(domain.members[0].card.as_deref(), Some(EARTH_FORM));
        assert_eq!(domain.cardinality.minimum, Nat(0));
        assert_eq!(domain.cardinality.maximum, Some(Nat(1)));
        let selected = if flip {
            vec![domain.members[0].handle]
        } else {
            vec![]
        };
        resolved(choose(&mut game, &question, Answer::Targets(selected)));
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["fourth"]["face_up"], flip);
        assert_eq!(state["objects"]["wrong"]["face_up"], false);
        assert_eq!(
            state["objects"]["source"]["keywords"]
                .as_array()
                .unwrap()
                .contains(&json!("疾走")),
            flip
        );
        let opponent = game.projection(View::P2).unwrap();
        assert_eq!(opponent["objects"].get("fourth").is_some(), flip);
        assert!(opponent["objects"].get("wrong").is_none());
    }
}
