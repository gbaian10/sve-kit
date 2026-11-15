use super::event_cemetery::finish_checks;
use super::union_burst_batch1::{finish, pending, question};
use super::*;
use sve_engine::game::internal::DecisionView;

fn initial() -> Value {
    let mut position = draw_position();
    position["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP08-103"}]);
    position["players"]["P1"]["zones"]["field"] = json!([{"id":"ally","card":"BP01-171"}]);
    position["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"one-a","card":"BP01-171"},{"id":"two","card":"BP01-018"},
        {"id":"one-b","card":"BP01-171"},{"id":"four","card":"BP01-169"},
        {"id":"one-c","card":"BP01-171"},{"id":"untouched","card":"BP01-171"}
    ]);
    position["players"]["P2"]["zones"]["field"] =
        json!([{"id":"victim","card":"BP01-107"},{"id":"amulet","card":"BP02-091"}]);
    position
}
fn build(position: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        position,
        &Value::Null,
        &Value::Null,
        "number-declaration",
    )
    .unwrap()
}
fn number_question(game: &mut Game) -> DecisionView {
    let play = game
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap();
    finish(game, play);
    finish_checks(game);
    question(pending(game, "BP08-103"))
}
#[test]
fn otherworld_ruler_declares_publicly_then_counts_only_matching_successful_mill_members() {
    for (number, damage) in [(0_u64, 0_u8), (1, 3), (2, 1), (4, 1), (u64::MAX, 0)] {
        let mut game = build(&initial());
        let question = number_question(&mut game);
        let InputDomain::Integer(bounds) = question.domain else {
            panic!("free nonnegative declaration")
        };
        assert_eq!(
            bounds,
            Cardinality {
                minimum: Nat(0),
                maximum: None
            }
        );
        let before = game.projection(View::Referee).unwrap();
        game.answer_internal_card(
            PlayerId { seat: 2 },
            question.token,
            Answer::Integer(Nat(number)),
        )
        .unwrap_err();
        game.answer_internal_card(question.actor, question.token, Answer::Targets(vec![]))
            .unwrap_err();
        assert_eq!(game.projection(View::Referee).unwrap(), before);
        assert_eq!(before["P1"]["deck"].as_array().unwrap().len(), 6);
        let play = game
            .answer_internal_card(question.actor, question.token, Answer::Integer(Nat(number)))
            .unwrap();
        let result = resolved(finish(&mut game, play));
        assert_eq!(result.progress.facts.iter().filter(|fact|matches!(fact, Fact::Declared { player, value } if player.seat == 1 && value.0 == number)).count(), 1);
        assert_eq!(result.progress.facts.iter().filter(|fact|matches!(fact, Fact::Movement(m) if m.from.kind == ZoneKind::Deck && m.movement.actual_to.is_some_and(|to|to.kind == ZoneKind::Cemetery))).count(), 5);
        finish_checks(&mut game);
        game.answer_internal_card(question.actor, question.token, Answer::Integer(Nat(number)))
            .unwrap_err();
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["victim"]["damage"], damage);
        assert_eq!(state["objects"]["ally"]["damage"], 0_u8);
        assert_eq!(state["P1"]["deck"], json!(["untouched"]));
        assert_eq!(state["P2"]["field"], json!(["victim", "amulet"]));
    }
}
#[test]
fn otherworld_ruler_counts_a_short_deck_without_drawing_or_losing() {
    let mut position = initial();
    position["players"]["P1"]["zones"]["deck"]
        .as_array_mut()
        .unwrap()
        .truncate(2);
    let mut game = build(&position);
    let question = number_question(&mut game);
    let play = game
        .answer_internal_card(question.actor, question.token, Answer::Integer(Nat(1)))
        .unwrap();
    resolved(finish(&mut game, play));
    finish_checks(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["victim"]["damage"], 1_u8);
    assert_eq!(state["P1"]["deck"], json!([]));
    assert!(state["game_over"].is_null());
}

#[test]
fn bounded_declaring_instruction_rejects_invalid_answers_without_publishing_a_fact() {
    let bound = Binding::new(90, ScopeId(1));
    let program = producer_steps(vec![
        Action::Declare {
            actor: PlayerRef::Controller {},
            declaration: Declaration::BoundedInteger {
                minimum: Int(1),
                maximum: Int(6),
                binding: bound,
            },
        },
        Action::Life {
            player: PlayerRef::Controller {},
            change: NumericChange::Add {
                amount: Expr::Bound(bound),
            },
        },
    ]);
    let mut position = draw_position();
    position["players"]["P1"]["zones"]["field"] = json!([]);
    position["players"]["P1"]["leader"]["life"] = json!(10_u8);
    let mut game = event_subject_capture::prepared_program(&position, &program);
    let declaring = game
        .activate_internal_card("producer", AbilityId(1), PlayerId { seat: 1 })
        .unwrap();
    let question = question(declaring);
    let before = game.projection(View::Referee).unwrap();
    for value in [0, 7] {
        game.answer_internal_card(question.actor, question.token, Answer::Integer(Nat(value)))
            .unwrap_err();
        assert_eq!(game.projection(View::Referee).unwrap(), before);
    }
    let play = game
        .answer_internal_card(question.actor, question.token, Answer::Integer(Nat(6)))
        .unwrap();
    let result = resolved(finish(&mut game, play));
    assert_eq!(
        result
            .progress
            .facts
            .iter()
            .filter(|fact| matches!(fact, Fact::Declared { value: Nat(6), .. }))
            .count(),
        1
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        16_u8
    );
}

#[test]
fn declared_names_cannot_leak_from_optional_bodies_or_shadow_a_value() {
    let mut tree = expanded::tree().clone();
    let mut control = tree["cards"]["BP01-041"].clone();
    control["abilities"][0]["activate"]["do"] = json!([
        {"declare":{"bind":"declared","kind":"number","min":1_u8,"max":6_u8}},
        {"if":{"cond":{"cmp":{"left":"declared","op":"ge","right":4_u8}},"then":[{"draw":{"n":"declared"}}]}}
    ]);
    tree["cards"] = json!({"number-control":control});
    lowerer().lower(&tree).unwrap();
    tree["cards"]["number-control"]["abilities"][0]["activate"]["do"] = json!([
        {"optional":{"do":[{"declare":{"bind":"hidden","kind":"number"}}]}},
        {"draw":{"n":"hidden"}}
    ]);
    assert_eq!(
        lowerer().lower(&tree).unwrap_err().code,
        "unsupported_construct"
    );
    tree["cards"]["number-control"]["abilities"][0]["activate"]["do"] = json!([
        {"declare":{"bind":"twice","kind":"number"}},
        {"declare":{"bind":"twice","kind":"number"}}
    ]);
    assert_eq!(
        lowerer().lower(&tree).unwrap_err().code,
        "unsupported_construct"
    );
}
