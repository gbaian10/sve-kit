#![expect(
    clippy::panic,
    reason = "Unexpected choices invalidate the printed optional evolution scenarios."
)]
use super::event_cemetery::finish_checks;
use super::*;

const PRINTINGS: [&str; 4] = ["BP10-054", "BP10-SL12", "BP10-U04", "SD08-003"];

fn initial(number: &str, top: &str, form: bool) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":number}]);
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    initial["players"]["P1"]["zones"]["ex"] = json!([]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"top","card":top},{"id":"next","card":"BP01-034"}
    ]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = if form {
        json!([{"id":"form","card":"BP10-055"}])
    } else {
        json!([])
    };
    initial
}

fn enter(board: &mut Game) {
    let CardPlay::Awaiting(ward) = board
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("the printed Ward offers entry posture")
    };
    assert!(
        matches!(ward.domain, InputDomain::WardEntry { .. }),
        "the complete card retains Ward"
    );
    assert!(
        matches!(
            board
                .answer_internal_card(ward.actor, ward.token, Answer::WardEntry(false))
                .unwrap(),
            CardPlay::Entered(_)
        ),
        "the actual base enters before Fanfare"
    );
    finish_checks(board);
}

fn trigger(board: &mut Game, number: &str, line: u64) -> CardPlay {
    finish_checks(board);
    let question = board.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("printed pending ability")
    };
    let option = options
        .iter()
        .find(|option| option.card == number && option.line == line)
        .unwrap();
    board
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(option.handle),
        )
        .unwrap()
}

fn evolution(board: &mut Game, play: CardPlay, available: bool, accept: bool) {
    let CardPlay::Awaiting(question) = play else {
        panic!("optional effect evolution")
    };
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("private evolution forms")
    };
    assert_eq!(
        domain.members.len(),
        usize::from(available),
        "only an available matching form is offered"
    );
    assert!(
        board.internal_decision(View::P2).is_none(),
        "the opponent cannot inspect evolution forms"
    );
    let handles = if accept {
        domain.members.iter().map(|member| member.handle).collect()
    } else {
        vec![]
    };
    resolved(
        board
            .answer_internal_card(question.actor, question.token, Answer::Targets(handles))
            .unwrap(),
    );
    finish_checks(board);
}

#[test]
fn r7a_optional_evolution_all_printings_return_and_heal_when_absent_or_declined() {
    for number in PRINTINGS {
        for available in [false, true] {
            let mut board = game(&initial(number, "BP01-037", available));
            enter(&mut board);
            let play = trigger(&mut board, number, 3);
            evolution(&mut board, play, available, false);
            let after = board.projection(View::Referee).unwrap();
            assert_eq!(
                after["P1"]["hand"],
                json!(["source"]),
                "Q1303 requires return after no actual evolution"
            );
            assert_eq!(
                after["P1"]["ex"],
                json!(["top"]),
                "the high-cost card actually reached EX"
            );
            resolved(trigger(&mut board, number, 2));
            finish_checks(&mut board);
            assert_eq!(
                board.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
                18_u8,
                "leaving the field heals eight"
            );
            assert!(
                board.internal_pending_choice().unwrap().is_none(),
                "return creates only the departure ability"
            );
        }
    }
}

#[test]
fn r7a_optional_evolution_all_printings_keep_low_cost_and_blocked_ex_without_evolving() {
    for number in PRINTINGS {
        for full in [false, true] {
            let mut position = initial(number, if full { "BP01-037" } else { "BP01-034" }, true);
            if full {
                position["players"]["P1"]["zones"]["ex"] = json!([
                    {"id":"e1","card":"BP01-034"},{"id":"e2","card":"BP01-034"},
                    {"id":"e3","card":"BP01-034"},{"id":"e4","card":"BP01-034"},{"id":"e5","card":"BP01-034"}
                ]);
            }
            let mut board = game(&position);
            enter(&mut board);
            resolved(trigger(&mut board, number, 3));
            finish_checks(&mut board);
            let after = board.projection(View::Referee).unwrap();
            assert_eq!(
                after["P1"]["field"],
                json!(["source"]),
                "no qualifying actual arrival means no return or evolution"
            );
            assert_eq!(
                after["P1"]["leader"]["life"], 10_u8,
                "the follower never left the field"
            );
            assert_eq!(
                after["P1"]["deck"],
                if full {
                    json!(["top", "next"])
                } else {
                    json!(["next"])
                },
                "EX capacity determines whether the top card moves"
            );
            assert!(
                board.internal_pending_choice().unwrap().is_none(),
                "low cost and failed movement do not create an evolution choice"
            );
        }
    }
}

#[test]
fn r7a_optional_evolution_success_keeps_the_source_and_preserves_ordinary_evolution() {
    for number in PRINTINGS {
        let mut position = initial(number, "BP01-037", true);
        position["players"]["P1"]["zones"]["field"] = json!([{"id":"ordinary","card":"BP14-008"}]);
        position["players"]["P1"]["zones"]["evolve_deck"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"ordinary-form","card":"BP14-009"}));
        let mut board = game(&position);
        enter(&mut board);
        let play = trigger(&mut board, number, 3);
        evolution(&mut board, play, true, true);
        let after = board.projection(View::Referee).unwrap();
        assert_eq!(
            after["P1"]["hand"],
            json!([]),
            "successful evolution does not return the source"
        );
        assert_eq!(
            after["objects"]["source"]["power"], 11_u8,
            "the admitted matching form replaces the base"
        );
        assert_eq!(
            after["P1"]["pp"]["current"], 2_u8,
            "effect evolution has no ordinary evolution payment"
        );
        assert!(
            board.internal_pending_choice().unwrap().is_none(),
            "evolution does not count as leaving the field"
        );
        let ordinary = json!({
            "do":"evolve","source":"ordinary","evolve_card":"ordinary-form","pay":{"pp":1_u8}
        });
        assert_eq!(
            board.internal_rule_action(&ordinary).unwrap().outcome,
            "resolved",
            "Q1306 preserves the once-per-turn ordinary evolution action"
        );
    }
}
