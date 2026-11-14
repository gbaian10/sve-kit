#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate the real-card scenarios."
)]
use super::*;
use sve_engine::game::player::{
    Command, DecisionShape, Input, QuestionPurpose, QuestionShape, SubmissionKind, Viewer,
};

const PLAYER: PlayerId = PlayerId { seat: 1 };

fn loaded(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "combos-transform-allocation",
    )
    .unwrap()
}

fn position(puppets: usize) -> Value {
    let mut initial = draw_position();
    initial["turn"]["elapsed_turns"] = json!({"P1":7_u8,"P2":6_u8});
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP08-011"}]);
    initial["players"]["P1"]["zones"]["ex"] = Value::Array(
        (0..puppets)
            .map(|i| json!({"id":format!("puppet{i}"),"card":"BP08-T09"}))
            .collect(),
    );
    initial
}

fn ex_names(game: &Game) -> Vec<String> {
    names(game, "P1")
}

/// Selects `count` puppets, then answers a template question with `template` when one is asked.
fn play(game: &mut Game, count: usize, template: Option<&str>) -> bool {
    let mut step = game.play_internal_card("source", PLAYER).unwrap();
    let mut asked = false;
    while let CardPlay::Awaiting(q) = step {
        let answer = match &q.domain {
            InputDomain::Targets(d) => {
                Answer::Targets(d.members.iter().take(count).map(|m| m.handle).collect())
            }
            InputDomain::TokenCapacity { choices } => {
                assert_eq!(q.actor, PLAYER, "the ability's controller decides");
                asked = true;
                let wanted = template.unwrap_or_else(|| panic!("no template question was expected"));
                let index = choices
                    .iter()
                    .position(|choice| choice == &[wanted.to_owned()])
                    .unwrap();
                Answer::TokenCapacity(u64::try_from(index).unwrap())
            }
            _ => panic!("unexpected decision: {:?}", q.domain),
        };
        step = game.answer_internal_card(q.actor, q.token, answer).unwrap();
    }
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
    asked
}

#[test]
fn bp08_011_turns_two_puppets_into_lloyd_and_victoria_without_a_question() {
    let mut game = loaded(&position(3));
    assert!(!play(&mut game, 2, None));
    assert_eq!(ex_names(&game), ["ロイド", "ヴィクトリア", "操り人形"]);
}

#[test]
fn bp08_011_needs_two_puppets_to_be_played() {
    let mut game = loaded(&position(1));
    assert!(
        game.play_internal_card("source", PLAYER).is_err(),
        "Two puppets must be chosen when the spell is played (Q1060)"
    );
}

/// Author input with BP08-011's target edited, keeping every other author complete.
fn edited_input(target: &Value) -> Value {
    let (_, _, mut input) = inputs().clone();
    let authors = input["authors"].as_array_mut().unwrap();
    let author = authors
        .iter_mut()
        .find(|author| author["input"]["card_no"] == "BP08-011")
        .unwrap();
    let faces = author["input"]["tree"]["cards"]["BP08-011"]["faces"]
        .as_object_mut()
        .unwrap();
    let face = faces.values_mut().next().unwrap();
    let printed = &mut face["abilities"][0]["spell"]["targets"][0];
    for (key, value) in target.as_object().unwrap() {
        if key == "who" {
            printed["from"]["who"] = value.clone();
        } else {
            printed[key] = value.clone();
        }
    }
    input
}

fn edited_selection(lock: &'static OnceLock<SelectedEffects>, target: &Value) -> SelectedEffects {
    lock.get_or_init(|| {
        let (snapshot, keywords, _) = inputs().clone();
        repository::selection_with_rules_from(&snapshot, &keywords, &edited_input(target))
    })
    .clone()
}

fn edited_game(lock: &'static OnceLock<SelectedEffects>, target: &Value, initial: &Value) -> Game {
    Game::from_selected_effects(
        edited_selection(lock, target),
        initial,
        &Value::Null,
        &Value::Null,
        "combos-transform-allocation",
    )
    .unwrap()
}

static ONE_TARGET: OnceLock<SelectedEffects> = OnceLock::new();
static BOTH_SIDES: OnceLock<SelectedEffects> = OnceLock::new();
static OPPONENT: OnceLock<SelectedEffects> = OnceLock::new();

fn names(game: &Game, seat: &str) -> Vec<String> {
    let state = game.projection(View::Referee).unwrap();
    let mut names: Vec<_> = state[seat]["ex"]
        .as_array()
        .unwrap()
        .iter()
        .map(|id| {
            state["objects"][id.as_str().unwrap()]["name"]
                .as_str()
                .unwrap()
                .to_owned()
        })
        .collect();
    names.sort();
    names
}

/// Answers the targets internally, then the template question through the player pipeline.
fn play_as_player(game: &mut Game, destination: Viewer, template: &str) {
    let played = game.play_internal_card("source", PLAYER).unwrap();
    let CardPlay::Awaiting(q) = played else {
        panic!("targets were expected")
    };
    let InputDomain::Targets(d) = &q.domain else {
        panic!("targets were expected")
    };
    let handles = d.members.iter().map(|m| m.handle).collect();
    let answered = game
        .answer_internal_card(q.actor, q.token, Answer::Targets(handles))
        .unwrap();
    assert!(matches!(answered, CardPlay::Awaiting(_)), "a template question");
    game.advance_player_input();
    let Input::Question(question) = game.input(Viewer::P1) else {
        panic!("the ability's controller decides the templates")
    };
    assert_eq!(
        question.purpose,
        QuestionPurpose::ResolutionChoice,
        "allocating templates is a resolution choice, not a capacity rule"
    );
    let QuestionShape::Decision(DecisionShape::TokenCapacity(choice)) = &question.shape else {
        panic!("template combinations")
    };
    assert_eq!(
        choice.destination, destination,
        "the tokens go to the banished object's controller"
    );
    assert_eq!(choice.available, Nat(1), "one successful banishment");
    assert!(
        matches!(game.input(Viewer::P2), Input::Waiting(_)),
        "the other seat only waits"
    );
    let handle = choice
        .choices
        .iter()
        .position(|names| names == &[template.to_owned()])
        .unwrap();
    let answer = Answer::TokenCapacity(u64::try_from(handle).unwrap());
    assert_eq!(
        game.submit_player(
            Viewer::P1,
            Command::Answer {
                token: question.token,
                answer
            }
        )
        .kind,
        SubmissionKind::Applied,
        "the decider's answer is accepted"
    );
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}

#[test]
fn bp08_011_lets_a_single_banished_puppet_become_either_template() {
    for template in ["ロイド", "ヴィクトリア"] {
        let mut game = edited_game(&ONE_TARGET, &json!({"n": 1_u8}), &position(2));
        assert!(play(&mut game, 1, Some(template)));
        let mut expected = vec![template.to_owned(), "操り人形".to_owned()];
        expected.sort();
        assert_eq!(ex_names(&game), expected);
    }
}

#[test]
fn bp08_011_assigns_templates_across_both_controllers_ex_areas() {
    let mut initial = position(1);
    initial["players"]["P2"]["zones"]["ex"] = json!([{"id":"rival","card":"BP08-T09"}]);
    let mut game = edited_game(&BOTH_SIDES, &json!({"who": "all"}), &initial);
    play_as_player(&mut game, Viewer::P1, "ヴィクトリア");
    assert_eq!(names(&game, "P1"), ["ヴィクトリア"]);
    assert_eq!(names(&game, "P2"), ["ロイド"]);
}

#[test]
fn bp08_011_controller_chooses_the_template_for_an_opponent_ex() {
    let mut initial = position(0);
    initial["players"]["P2"]["zones"]["ex"] = json!([{"id":"rival","card":"BP08-T09"}]);
    let mut game = edited_game(&OPPONENT, &json!({"who": "opp", "n": 1_u8}), &initial);
    play_as_player(&mut game, Viewer::P2, "ロイド");
    assert_eq!(names(&game, "P1"), Vec::<String>::new());
    assert_eq!(names(&game, "P2"), ["ロイド"]);
}

#[test]
fn bp08_011_rejects_more_targets_than_templates_at_load() {
    let (snapshot, keywords, _) = inputs().clone();
    let input = edited_input(&json!({"n": 3_u8}));
    assert!(
        repository::select(&snapshot, &keywords, &input).is_err(),
        "three objects cannot each become a different one of two templates"
    );
}
