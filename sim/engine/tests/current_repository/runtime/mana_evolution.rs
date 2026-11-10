#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected choices invalidate the printed evolution sequence."
)]

use super::event_cemetery::finish_checks;
use super::*;
use sve_engine::game::internal::DecisionView;

fn position(base: &str, form: &str) -> Value {
    let mut initial = draw_position();
    initial["turn"]["elapsed_turns"]["P1"] = json!(7_u8);
    initial["players"]["P1"]["leader"]["class"] = json!("ウィッチ");
    initial["players"]["P1"]["sep"] = json!(1_u8);
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"source","card":base},
        {"id":"material","card":"BP01-043"}
    ]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"drawn","card":"BP01-043"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-002"}]);
    initial
}

fn evolve(initial: &Value, super_evolve: bool) -> Game {
    let mut board = Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "mana-evolution",
    )
    .unwrap();
    let action = json!({
        "do":"evolve","source":"source","evolve_card":"form",
        "pay":{"pp":1_u8,"sep":u8::from(super_evolve)}
    });
    assert_eq!(
        board.internal_rule_action(&action).unwrap().outcome,
        "resolved",
        "the printed base can evolve into the selected printing"
    );
    finish_checks(&mut board);
    board
}

fn pending_line(board: &mut Game, form: &str, line: u64, count: usize) -> CardPlay {
    let pending = board.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = pending.domain else {
        panic!("expected pending evolution abilities")
    };
    assert_eq!(options.len(), count, "remaining evolution triggers");
    let option = options
        .iter()
        .find(|option| option.card == form && option.line == line)
        .unwrap();
    board
        .answer_internal_card(pending.actor, pending.token, Answer::Pending(option.handle))
        .unwrap()
}

fn awaiting(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(question) = play else {
        panic!("expected a declaration")
    };
    question
}

fn pay_and_resolve(board: &mut Game, play: CardPlay) {
    let cost = awaiting(play);
    assert!(
        matches!(cost.domain, InputDomain::OptionalCost),
        "the controller may decline to play the paid evolution ability"
    );
    let target = awaiting(
        board
            .answer_internal_card(cost.actor, cost.token, Answer::OptionalCost(true))
            .unwrap(),
    );
    let InputDomain::Targets(targets) = &target.domain else {
        panic!("the formal target precedes payment")
    };
    assert_eq!(
        targets.members.len(),
        1,
        "the opponent has one legal target"
    );
    let material = awaiting(
        board
            .answer_internal_card(
                target.actor,
                target.token,
                Answer::Targets(vec![targets.members[0].handle]),
            )
            .unwrap(),
    );
    let InputDomain::CostMaterials(materials) = &material.domain else {
        panic!("expected the field material")
    };
    assert_eq!(
        materials.members.len(),
        1,
        "only the other allied follower can pay"
    );
    assert_eq!(
        materials.members[0].card.as_deref(),
        Some("BP01-043"),
        "the allied companion is the material"
    );
    let result = resolved(
        board
            .answer_internal_card(
                material.actor,
                material.token,
                Answer::CostMaterials(vec![materials.members[0].handle]),
            )
            .unwrap(),
    );
    assert!(
        result.progress.facts.iter().any(|fact| matches!(
            fact, Fact::Movement(movement) if movement.method == MoveMethod::Banish
                && movement.from.kind == ZoneKind::Field
                && movement.movement.actual_to.as_ref().is_some_and(|zone| zone.kind == ZoneKind::Banish)
        )),
        "payment produces a field banishment"
    );
    finish_checks(board);
    let state = board.projection(View::Referee).unwrap();
    assert_eq!(
        state["P1"]["field"],
        json!(["source"]),
        "Mana stays in play"
    );
    assert_eq!(
        state["P1"]["banish"],
        json!(["material"]),
        "the cost is paid"
    );
    assert_eq!(
        state["P2"]["cemetery"],
        json!(["enemy"]),
        "the target is destroyed"
    );
    assert_eq!(
        state["P1"]["hand"],
        json!(["drawn"]),
        "exactly one card is drawn"
    );
}

#[test]
fn mana_evolution_all_jp_printings_pay_banish_and_super_evolve_into_jewel_dress_up() {
    for (base, form, lines) in [
        ("BP18-039", "BP18-040", &[1_u64][..]),
        ("BP18-SL09", "BP18-SL10", &[2_u64, 1][..]),
        ("BP18-039", "BP18-U03", &[1_u64, 2][..]),
    ] {
        let super_evolve = lines.len() == 2;
        let mut board = evolve(&position(base, form), super_evolve);
        for (line, remaining) in lines.iter().copied().zip((1..=lines.len()).rev()) {
            let play = pending_line(&mut board, form, line, remaining);
            if line == 1 {
                pay_and_resolve(&mut board, play);
            } else {
                resolved(play);
                finish_checks(&mut board);
            }
        }
        assert!(board.internal_pending_choice().unwrap().is_none());
        let state = board.projection(View::Referee).unwrap();
        let ex = state["P1"]["ex"].as_array().unwrap();
        assert_eq!(ex.len(), usize::from(super_evolve));
        if let [token] = ex.as_slice() {
            let actor = PlayerId { seat: 1 };
            board.play_internal_card("material", actor).unwrap_err();
            resolved(
                board
                    .play_internal_card(token.as_str().unwrap(), actor)
                    .unwrap(),
            );
            finish_checks(&mut board);
            assert!(matches!(
                board.play_internal_card("material", actor).unwrap(),
                CardPlay::Entered(_)
            ));
            assert_eq!(
                board.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
                6_u8
            );
        }
    }
}

#[test]
fn mana_evolution_requires_a_legal_target_and_a_banishable_allied_follower() {
    for (material_card, enemy_present) in [
        (None, true),
        (Some("BP01-043"), false),
        (Some("BP06-022"), true),
    ] {
        let mut initial = position("BP18-039", "BP18-040");
        let field = initial["players"]["P1"]["zones"]["field"]
            .as_array_mut()
            .unwrap();
        if let Some(card) = material_card {
            field[1]["card"] = json!(card);
        } else {
            field.pop();
        }
        if !enemy_present {
            initial["players"]["P2"]["zones"]["field"] = json!([]);
        }
        let mut board = evolve(&initial, false);
        let before = board.projection(View::Referee).unwrap();
        let mut play = pending_line(&mut board, "BP18-040", 1, 1);
        while let CardPlay::Awaiting(question) = play {
            let answer = match &question.domain {
                InputDomain::OptionalCost => Answer::OptionalCost(true),
                InputDomain::Targets(targets) => {
                    assert_eq!(targets.members.len(), 1);
                    Answer::Targets(vec![targets.members[0].handle])
                }
                InputDomain::CostMaterials(materials) => {
                    assert_eq!(materials.members.len(), 1);
                    assert_eq!(materials.members[0].card.as_deref(), Some("BP06-022"));
                    Answer::CostMaterials(vec![materials.members[0].handle])
                }
                _ => panic!("expected an evolution declaration"),
            };
            play = board
                .answer_internal_card(question.actor, question.token, answer)
                .unwrap();
        }
        assert_eq!(play, CardPlay::Cancelled);
        finish_checks(&mut board);
        let after = board.projection(View::Referee).unwrap();
        assert_eq!(after["P1"]["banish"], before["P1"]["banish"]);
        assert_eq!(after["P1"]["field"], before["P1"]["field"]);
        assert_eq!(after["P1"]["hand"], before["P1"]["hand"]);
        assert_eq!(after["P2"]["field"], before["P2"]["field"]);
    }
}
