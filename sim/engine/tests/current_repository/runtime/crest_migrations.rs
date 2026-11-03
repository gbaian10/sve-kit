#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected questions invalidate printed Crest scenarios."
)]
use super::*;

fn position(card: &str, count: usize, token: bool) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    initial["players"]["P1"]["zones"]["hand"] = if token {
        json!([])
    } else {
        json!([{"id":"source","card":card}])
    };
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"ally","card":"BP20-101"}]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-037","state":{"power":10_u8,"hp":20_u8,"max_hp":20_u8}}]);
    let mut ex = if token {
        vec![json!({"id":"source","card":card})]
    } else {
        vec![]
    };
    let extras = [
        "BP20-T01", "BP20-T05", "BP20-P70", "BP20-T07", "BP20-T09", "BP20-T10",
    ];
    let canonical = match card {
        "BP20-P66" => "BP20-T07",
        "BP20-P68" => "BP20-T09",
        "BP20-P69" => "BP20-T10",
        _ => card,
    };
    for (i, no) in extras
        .iter()
        .filter(|no| **no != canonical)
        .take(count.saturating_sub(usize::from(token)))
        .enumerate()
    {
        ex.push(json!({"id":format!("crest-{i}"),"card":no}));
    }
    if count < 5 {
        ex.push(json!({"id":"ordinary","card":"BP01-009"}));
    }
    initial["players"]["P1"]["zones"]["ex"] = json!(ex);
    initial["players"]["P2"]["zones"]["ex"] = json!([{"id":"foreign","card":"BP20-T01"}]);
    initial
}
fn game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "crest-migrations",
    )
    .unwrap()
}
fn finish(board: &mut Game, mut play: CardPlay, mode: u64) {
    while let CardPlay::Awaiting(q) = play {
        let answer = match &q.domain {
            InputDomain::Modes { .. } => Answer::Modes(vec![ModeId(mode)]),
            InputDomain::Targets(d) => Answer::Targets(vec![
                d.members.iter().find(|m| m.card.is_some()).unwrap().handle,
            ]),
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            _ => panic!("unexpected Crest question"),
        };
        play = board
            .answer_internal_card(q.actor, q.token, answer)
            .unwrap();
    }
}
fn resolve(board: &mut Game, card: &str, mode: u64) {
    if board.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        board.resume_internal_card().unwrap();
    }
    let q = board.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &q.domain else {
        panic!("pending")
    };
    let chosen = options.iter().find(|a| a.card == card).unwrap();
    let play = board
        .answer_internal_card(q.actor, q.token, Answer::Pending(chosen.handle))
        .unwrap();
    finish(board, play, mode);
}
fn play(board: &mut Game, card: &str, mode: u64) {
    let result = board
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap();
    finish(board, result, mode);
    resolve(board, card, mode);
}
fn assert_created(board: &Game, name: &str) {
    let view = board.projection(View::Referee).unwrap();
    assert!(
        view["P1"]["ex"]
            .as_array()
            .unwrap()
            .iter()
            .any(|id| { view["objects"][id.as_str().unwrap()]["name"] == name }),
        "the named Crest is created in own EX"
    );
}
fn end(board: &mut Game, card: &str) {
    assert_eq!(
        board
            .internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap()
            .outcome,
        "resolved",
        "the real own end phase queues the token ability"
    );
    resolve(board, card, 1);
}

#[test]
fn bp20_093_all_printings_choose_conditional_banish_or_create_marwynn() {
    for card in ["BP20-093", "BP20-SL25"] {
        for count in [2_usize, 3] {
            let mut board = game(&position(card, count, false));
            play(&mut board, card, 1);
            let v = board.projection(View::Referee).unwrap();
            assert_eq!(
                v["P2"]["banish"],
                if count == 3 {
                    json!(["enemy"])
                } else {
                    json!([])
                },
                "banishment requires three own EX Crests"
            );
            assert_eq!(
                v["P1"]["leader"]["life"], 12_u8,
                "healing is outside the threshold condition"
            );
        }
        let mut board = game(&position(card, 0, false));
        play(&mut board, card, 2);
        assert_created(&board, "クレスト 絶望の顕現・マーウィン");
    }
}

#[test]
fn bp20_097_all_printings_create_the_printed_crest() {
    for card in ["BP20-097", "BP20-P47"] {
        let mut board = game(&position(card, 0, false));
        play(&mut board, card, 1);
        assert_created(&board, "クレスト 安息の団結者");
    }
}

#[test]
fn bp20_098_all_printings_execute_the_evolution_effect() {
    for card in ["BP20-098", "BP20-P48"] {
        let mut initial = position("BP20-097", 3, false);
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP20-097"}]);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":card}]);
        let mut board = game(&initial);
        assert_eq!(
            board
                .internal_rule_action(
                    &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
                )
                .unwrap()
                .outcome,
            "resolved",
            "the printed base evolves into each printing"
        );
        resolve(&mut board, card, 1);
        assert_eq!(
            board.projection(View::Referee).unwrap()["objects"]["enemy"]["hp"],
            14_u8,
            "three own Crests deal six damage"
        );
    }
}

#[test]
fn bp20_102_all_printings_execute_the_evolution_effect() {
    for card in ["BP20-102", "BP20-P51"] {
        let mut initial = position("BP20-101", 3, false);
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP20-101"}]);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":card}]);
        let mut board = game(&initial);
        assert_eq!(
            board
                .internal_rule_action(
                    &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
                )
                .unwrap()
                .outcome,
            "resolved",
            "the printed base evolves into each printing"
        );
        resolve(&mut board, card, 1);
        assert_created(&board, "クレスト 安息の祈祷者");
    }
}

#[test]
fn bp20_t07_all_printings_resolve_the_named_token_at_crest_thresholds() {
    for card in ["BP20-T07", "BP20-P66"] {
        for count in [2_usize, 3, 5] {
            let mut board = game(&position(card, count, true));
            end(&mut board, card);
            let v = board.projection(View::Referee).unwrap();
            let damage = if count == 5 {
                4_u8
            } else if count >= 3 {
                2
            } else {
                1
            };
            assert_eq!(
                v["objects"]["enemy"]["hp"],
                20_u8.checked_sub(damage).unwrap(),
                "the named token has exact-five and at-least-three replacements"
            );
        }
    }
}

#[test]
fn bp20_t09_all_printings_resolve_the_named_token_at_crest_thresholds() {
    for card in ["BP20-T09", "BP20-P68"] {
        for count in [2_usize, 3, 5] {
            let mut board = game(&position(card, count, true));
            end(&mut board, card);
            let v = board.projection(View::Referee).unwrap();
            let raised = if count >= 3 { 3_u8 } else { 2 };
            assert_eq!(
                v["objects"]["ally"]["power"], raised,
                "only an own Omen follower receives the threshold buff"
            );
        }
    }
}

#[test]
fn bp20_t10_all_printings_resolve_the_named_token_at_crest_thresholds() {
    for card in ["BP20-T10", "BP20-P69"] {
        for count in [2_usize, 3, 5] {
            let mut board = game(&position(card, count, true));
            end(&mut board, card);
            let v = board.projection(View::Referee).unwrap();
            assert_eq!(
                v["P1"]["leader"]["life"],
                if count >= 3 { 11_u8 } else { 10 },
                "the own leader heals only at three or more Crests"
            );
        }
    }
}
