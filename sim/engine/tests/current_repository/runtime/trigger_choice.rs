#![expect(clippy::panic, reason = "A different prompt invalidates the scenario.")]

use super::*;
use sve_engine::game::internal::DecisionView;

fn awaiting(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(question) = play else {
        panic!("expected a declaration")
    };
    question
}

#[test]
fn bp08_064_awakened_fanfare_declares_a_mode_before_its_selected_targets() {
    let mut initial = draw_position();
    initial["players"]["P1"]["pp"] = json!({"current":7_u8,"max":7_u8});
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"dragoons","card":"BP08-064"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"victim","card":"BP01-037"}, {"id":"untouched","card":"BP01-037"}
    ]);
    let mut game = game(&initial);
    assert!(matches!(
        game.play_internal_card("dragoons", PlayerId { seat: 1 })
            .unwrap(),
        CardPlay::Entered(_)
    ));
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("Fanfare")
    };
    assert_eq!(options.len(), 1);
    assert_eq!(options[0].card, "BP08-064");
    let modes = awaiting(
        game.answer_internal_card(
            pending.actor,
            pending.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap(),
    );
    assert!(matches!(modes.domain, InputDomain::Modes { .. }));
    let before = game.projection(View::Referee).unwrap();
    let authority = format!("{game:?}");
    game.answer_internal_card(
        PlayerId { seat: 2 },
        modes.token,
        Answer::Modes(vec![ModeId(1)]),
    )
    .unwrap_err();
    assert_eq!(format!("{game:?}"), authority);
    game.answer_internal_card(modes.actor, modes.token, Answer::Modes(vec![ModeId(99)]))
        .unwrap_err();
    assert_eq!(format!("{game:?}"), authority);
    let targets = awaiting(
        game.answer_internal_card(modes.actor, modes.token, Answer::Modes(vec![ModeId(1)]))
            .unwrap(),
    );
    let InputDomain::Targets(domain) = &targets.domain else {
        panic!("selected mode's target")
    };
    assert_eq!(domain.members.len(), 2);
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["victim"]["damage"],
        0_u8
    );
    let result = resolved(
        game.answer_internal_card(targets.actor, targets.token, Answer::Targets(vec![0]))
            .unwrap(),
    );
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["objects"]["victim"]["damage"], 4_u8);
    assert_eq!(after["objects"]["untouched"]["damage"], 0_u8);
    assert_eq!(after["P1"]["pp"], before["P1"]["pp"]);
    assert_eq!(
        result
            .progress
            .facts
            .iter()
            .filter(|fact| matches!(fact, Fact::AbilityPlayed { .. }))
            .count(),
        1
    );
}
