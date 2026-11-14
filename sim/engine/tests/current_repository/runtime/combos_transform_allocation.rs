#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate the real-card scenarios."
)]
use super::*;

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
    let state = game.projection(View::Referee).unwrap();
    let mut names: Vec<_> = state["P1"]["ex"]
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

/// The printed allocation with one target stands in for a puppet lost before resolution.
fn one_target_selection() -> &'static SelectedEffects {
    static SELECTED: OnceLock<SelectedEffects> = OnceLock::new();
    SELECTED.get_or_init(|| {
        let (snapshot, keywords, mut input) = inputs().clone();
        let authors = input["authors"].as_array_mut().unwrap();
        let author = authors
            .iter_mut()
            .find(|author| author["input"]["card_no"] == "BP08-011")
            .unwrap();
        let faces = author["input"]["tree"]["cards"]["BP08-011"]["faces"]
            .as_object_mut()
            .unwrap();
        let face = faces.values_mut().next().unwrap();
        face["abilities"][0]["spell"]["targets"][0]["n"] = json!(1_u8);
        repository::selection_with_rules_from(&snapshot, &keywords, &input)
    })
}

#[test]
fn bp08_011_lets_a_single_banished_puppet_become_either_template() {
    for template in ["ロイド", "ヴィクトリア"] {
        let mut game = Game::from_selected_effects(
            one_target_selection().clone(),
            &position(2),
            &Value::Null,
            &Value::Null,
            "combos-transform-allocation",
        )
        .unwrap();
        assert!(play(&mut game, 1, Some(template)));
        let mut expected = vec![template.to_owned(), "操り人形".to_owned()];
        expected.sort();
        assert_eq!(ex_names(&game), expected);
    }
}
