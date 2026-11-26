use super::event_cemetery::finish_checks;
use super::*;

fn answer_trigger(game: &mut Game, card: &str) -> CardPlay {
    finish_checks(game);
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("expected the printed ability");
    };
    assert_eq!(options.len(), 1, "one printed occurrence is pending");
    assert_eq!(options[0].card, card, "the selected card triggered");
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::Pending(options[0].handle),
    )
    .unwrap()
}

#[test]
fn bp06_032_cannot_draw_without_a_fanfare_target() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP06-032"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"top","card":"BP01-037"}]);
    let mut game = game(&initial);
    assert!(matches!(
        game.play_internal_card("source", PlayerId { seat: 1 })
            .unwrap(),
        CardPlay::Entered(_)
    ));
    assert_eq!(answer_trigger(&mut game, "BP06-032"), CardPlay::Cancelled);

    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["field"], json!(["source"]));
    assert_eq!(state["P1"]["deck"], json!(["top"]));
    assert_eq!(state["P1"]["hand"], json!([]));
}

#[test]
fn bp08_090_stays_evolved_when_its_trigger_has_no_follower_target() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP08-089"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP08-090"}]);
    let mut game = game(&initial);
    let step = game
        .internal_rule_action(&json!({
            "do":"evolve","source":"source","evolve_card":"form","pay":{"pp":2_u8}
        }))
        .unwrap();
    assert_eq!(step.outcome, "resolved");
    assert_eq!(answer_trigger(&mut game, "BP08-090"), CardPlay::Cancelled);

    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["field"], json!(["source"]));
    assert_eq!(state["P1"]["evolution"], json!(["form"]));
    assert_eq!(state["objects"]["source"]["evolved_with"], "form");
}

fn riko_attacks(initial: &Value) -> (Game, CardPlay) {
    let mut game = game(initial);
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"P2.leader"}))
            .unwrap()
            .outcome,
        "resolved",
        "the attack is declared"
    );
    let play = answer_trigger(&mut game, "CP01-079");
    (game, play)
}

#[test]
fn cp01_079_does_not_declare_an_attack_time_target_with_another_follower() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"source","card":"CP01-079"},{"id":"ally","card":"BP01-037"}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    let (mut game, play) = riko_attacks(&initial);
    resolved(play);
    finish_checks(&mut game);
    if let Some(question) = game.internal_quick_choice().unwrap() {
        game.answer_internal_card(question.actor, question.token, Answer::Quick(None))
            .unwrap();
        finish_checks(&mut game);
    }
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["leader"]["life"], 14_u8);
    assert_eq!(state["P2"]["field"], json!(["enemy"]));
}

#[test]
fn cp01_079_requires_one_target_when_attacking_as_the_only_follower() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"CP01-079"}]);

    let (empty_game, empty_play) = riko_attacks(&initial);
    assert_eq!(empty_play, CardPlay::Cancelled);
    let empty_state = empty_game.projection(View::Referee).unwrap();
    assert_eq!(empty_state["P2"]["leader"]["life"], 20_u8);

    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    let (mut game, play) = riko_attacks(&initial);
    let CardPlay::Awaiting(question) = play else {
        panic!("a lone Riko requires an enemy follower target");
    };
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("expected the formal target question");
    };
    assert_eq!(domain.cardinality.minimum, Nat(1));
    assert_eq!(domain.cardinality.maximum, Some(Nat(1)));
    assert_eq!(domain.members.len(), 1);
    resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Targets(vec![domain.members[0].handle]),
        )
        .unwrap(),
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["leader"]["life"], 17_u8);
    assert_eq!(state["P2"]["cemetery"], json!(["enemy"]));
}
