#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected questions invalidate these real Lesson scenarios."
)]
use super::*;

fn finish(game: &mut Game, mut play: CardPlay) -> CardPlay {
    while let CardPlay::Awaiting(question) = play {
        let answer = match &question.domain {
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            InputDomain::OptionalCost => Answer::OptionalCost(true),
            InputDomain::CostMaterials(domain) => Answer::CostMaterials(
                domain
                    .members
                    .iter()
                    .take(usize::try_from(domain.cardinality.minimum.0).unwrap())
                    .map(|member| member.handle)
                    .collect(),
            ),
            InputDomain::Targets(domain) => Answer::Targets(
                domain
                    .members
                    .iter()
                    .take(usize::try_from(domain.cardinality.maximum.unwrap_or(Nat(1)).0).unwrap())
                    .map(|member| member.handle)
                    .collect(),
            ),
            _ => panic!("unexpected Lesson question"),
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    play
}

fn run(number: &str, activated: bool, hand: Value) -> (Game, Value) {
    let mut initial = lesson::position();
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    initial["players"]["P1"]["zones"]["ex"] = json!([
        {"id":"item0","card":"PR-155"},{"id":"item1","card":"PR-155"},
        {"id":"item2","card":"PR-155"},{"id":"item3","card":"PR-155"},
        {"id":"item4","card":"PR-155"}
    ]);
    initial["players"]["P1"]["zones"]["deck"] =
        json!([{"id":"drawn","card":"BP01-037"},{"id":"tail","card":"BP01-037"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"enemy","card":"BP01-037","state":{"power":3_u8,"hp":10_u8,"max_hp":10_u8}},
        {"id":"other","card":"BP01-037","state":{"power":3_u8,"hp":10_u8,"max_hp":10_u8}}
    ]);
    initial["players"]["P1"]["zones"]["hand"] = hand;
    if activated {
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":number}]);
    } else {
        initial["players"]["P1"]["zones"]["hand"]
            .as_array_mut()
            .unwrap()
            .insert(0, json!({"id":"source","card":number}));
    }
    let mut game = lesson::game(&initial);
    let before;
    let play = if activated {
        before = game.projection(View::Referee).unwrap();
        let id = selection().current[number]
            .program
            .card()
            .abilities
            .iter()
            .find(|ability| {
                ability.node.kind == AbilityKind::Activated
                    && matches!(ability.node.source.origin, AbilityOrigin::Printed { .. })
            })
            .unwrap()
            .node
            .id;
        game.activate_internal_card("source", id, PlayerId { seat: 1 })
            .unwrap()
    } else {
        let play = game
            .play_internal_card("source", PlayerId { seat: 1 })
            .unwrap();
        assert!(
            matches!(finish(&mut game, play), CardPlay::Entered(_)),
            "the real source enters before its Fanfare"
        );
        before = game.projection(View::Referee).unwrap();
        let pending = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = &pending.domain else {
            panic!("real Fanfare")
        };
        let option = options.iter().find(|option| option.card == number).unwrap();
        game.answer_internal_card(pending.actor, pending.token, Answer::Pending(option.handle))
            .unwrap()
    };
    resolved(finish(&mut game, play));
    (game, before)
}

fn paid(view: &Value, before: &Value, n: usize, pp: i64) {
    assert_eq!(
        view["P1"]["ex"].as_array().unwrap().len(),
        5_usize.checked_sub(n).unwrap(),
        "the exact Lesson amount is consumed"
    );
    assert_eq!(
        view["P1"]["pp"]["current"].as_i64().unwrap(),
        before["P1"]["pp"]["current"]
            .as_i64()
            .unwrap()
            .checked_sub(pp)
            .unwrap(),
        "only the additional printed PP is paid for Lesson"
    );
}

#[test]
fn ecp02_053_lesson_behavior_and_all_printings() {
    for number in ["ECP02-053", "ECP02-P26"] {
        let (game, before) = run(number, false, json!([]));
        let view = game.projection(View::Referee).unwrap();
        paid(&view, &before, 1, 0);
        assert_eq!(view["objects"]["enemy"]["damage"], 1_u8);
        assert_eq!(view["P1"]["leader"]["life"], 11_u8);
    }
}

#[test]
fn ecp02_060_lesson_behavior_and_all_printings() {
    for number in ["ECP02-060", "ECP02-SL28"] {
        let (game, before) = run(number, false, json!([]));
        let view = game.projection(View::Referee).unwrap();
        paid(&view, &before, 1, 0);
        assert_eq!(view["objects"]["enemy"]["power"], 2_u8);
        assert_eq!(view["objects"]["enemy"]["hp"], 9_u8);
    }
}

#[test]
fn ecp02_067_lesson_behavior_and_all_printings() {
    for number in ["ECP02-067", "ECP02-P35"] {
        let (game, before) = run(number, false, json!([]));
        let view = game.projection(View::Referee).unwrap();
        paid(&view, &before, 2, 0);
        assert_eq!(view["P2"]["field"], json!(["other"]));
        assert_eq!(view["P2"]["banish"], json!(["enemy"]));
    }
}

#[test]
fn ecp02_sp03a_lesson_behavior_and_all_printings() {
    for number in ["ECP02-SP03a", "ECP02-SP03b", "ECP02-U03a", "ECP02-U03b"] {
        let (game, before) = run(number, false, json!([]));
        let view = game.projection(View::Referee).unwrap();
        paid(&view, &before, 2, 0);
        assert_eq!(
            view["objects"]["enemy"]["damage"], 1_u8,
            "the source is the one Cool follower"
        );
    }
}

#[test]
fn ecp02_sp09_lesson_behavior_and_all_printings() {
    for number in ["ECP02-SP09", "ECP02-U09"] {
        let (game, before) = run(number, true, json!([]));
        let view = game.projection(View::Referee).unwrap();
        paid(&view, &before, 1, 0);
        assert_eq!(view["P1"]["hand"], json!(["drawn"]));
    }
}

#[test]
fn pr_436_lesson_behavior_and_all_printings() {
    let number = "PR-436";
    let (game, before) = run(number, true, json!([]));
    let view = game.projection(View::Referee).unwrap();
    paid(&view, &before, 1, 1);
    assert!(
        view["objects"]["source"]["keywords"]
            .as_array()
            .unwrap()
            .contains(&json!("疾走"))
    );
}
