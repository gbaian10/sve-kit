#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate the scenario."
)]
use super::*;
use sve_engine::game::internal::DecisionView;

fn awaiting(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(question) = play else {
        panic!("expected formal declaration")
    };
    question
}

fn trigger(game: &mut Game) -> CardPlay {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("expected triggered ability")
    };
    assert_eq!(options.len(), 1, "only the intended ability is pending");
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::Pending(options[0].handle),
    )
    .unwrap()
}

#[test]
fn bp01_109_cemetery_formal_target_precedes_self_payment() {
    for seat in ["P1", "P2"] {
        let opponent = if seat == "P1" { "P2" } else { "P1" };
        let actor = PlayerId {
            seat: seat_number(seat),
        };
        let mut initial = draw_position();
        initial["turn"]["active"] = json!(seat);
        initial["players"][seat]["zones"]["hand"] = json!([]);
        initial["players"][seat]["zones"]["field"] = json!([{"id":"source","card":"BP01-109"}]);
        initial["players"][seat]["zones"]["cemetery"] = json!([
            {"id":"returned","card":"BP01-037"}, {"id":"cheap","card":"BP01-046"},
            {"id":"spell","card":"BP01-089"}
        ]);
        initial["players"][opponent]["zones"]["cemetery"] =
            json!([{"id":"foreign","card":"BP01-037"}]);
        let mut game = game(&initial);
        let ability = selection().current["BP01-109"].program.card().abilities[0]
            .node
            .id;
        let question = awaiting(
            game.activate_internal_card("source", ability, actor)
                .unwrap(),
        );
        let InputDomain::Targets(domain) = &question.domain else {
            panic!("targets")
        };
        assert_eq!(domain.members.len(), 1);
        assert_eq!(domain.members[0].card.as_deref(), Some("BP01-037"));
        assert_eq!(
            game.projection(View::Referee).unwrap()[seat]["field"],
            json!(["source"])
        );
        resolved(
            game.answer_internal_card(
                question.actor,
                question.token,
                Answer::Targets(vec![domain.members[0].handle]),
            )
            .unwrap(),
        );
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state[seat]["hand"], json!(["returned"]));
        assert_eq!(state[seat]["field"], json!([]));
        assert_eq!(state[seat]["cemetery"], json!(["cheap", "spell", "source"]));
        assert_eq!(state[opponent]["cemetery"], json!(["foreign"]));
    }
}

#[test]
fn bp02_066_cemetery_formal_target_excludes_discard_cost_and_cancels_without_target() {
    for printing in ["BP02-066", "ETD01-014", "PR-023"] {
        for available in [false, true] {
            let mut initial = draw_position();
            initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP02-065"}]);
            initial["players"]["P1"]["zones"]["evolve_deck"] =
                json!([{"id":"form","card":printing}]);
            initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"payer","card":"BP01-096"}]);
            initial["players"]["P1"]["zones"]["cemetery"] = if available {
                json!([{"id":"returned","card":"BP01-096"},{"id":"spell","card":"BP01-089"}])
            } else {
                json!([{"id":"spell","card":"BP01-089"}])
            };
            initial["players"]["P2"]["zones"]["cemetery"] =
                json!([{"id":"foreign","card":"BP01-096"}]);
            let mut game = game(&initial);
            assert_eq!(
                game.internal_rule_action(
                    &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
                )
                .unwrap()
                .outcome,
                "resolved"
            );
            let before = game.projection(View::Referee).unwrap();
            let declaration = awaiting(trigger(&mut game));
            assert!(
                matches!(declaration.domain, InputDomain::OptionalCost),
                "accepting the trigger precedes its targets and payment"
            );
            let play = game
                .answer_internal_card(
                    declaration.actor,
                    declaration.token,
                    Answer::OptionalCost(true),
                )
                .unwrap();
            if !available {
                assert_eq!(play, CardPlay::Cancelled);
                assert_eq!(
                    game.projection(View::Referee).unwrap()["P1"]["hand"],
                    json!(["payer"])
                );
                assert_eq!(
                    game.projection(View::Referee).unwrap()["P1"]["cemetery"],
                    before["P1"]["cemetery"]
                );
                continue;
            }
            let question = awaiting(play);
            let InputDomain::Targets(domain) = &question.domain else {
                panic!("targets before costs")
            };
            assert_eq!(domain.members.len(), 1);
            assert_eq!(domain.members[0].card.as_deref(), Some("BP01-096"));
            game.answer_internal_card(question.actor, question.token, Answer::Targets(vec![1]))
                .unwrap_err();
            assert_eq!(
                game.projection(View::Referee).unwrap()["P1"]["hand"],
                json!(["payer"])
            );
            let cost = awaiting(
                game.answer_internal_card(question.actor, question.token, Answer::Targets(vec![0]))
                    .unwrap(),
            );
            let InputDomain::CostMaterials(materials) = &cost.domain else {
                panic!("discard after targets")
            };
            assert_eq!(materials.members.len(), 1);
            resolved(
                game.answer_internal_card(
                    cost.actor,
                    cost.token,
                    Answer::CostMaterials(vec![materials.members[0].handle]),
                )
                .unwrap(),
            );
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["P1"]["hand"], json!(["returned"]));
            assert_eq!(state["P1"]["cemetery"], json!(["spell", "payer"]));
            assert_eq!(state["P2"]["cemetery"], json!(["foreign"]));
        }
    }
}

#[test]
fn bp20_091_cemetery_formal_attack_target_filters_class_and_keeps_combat_pending() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP20-091"}]);
    initial["players"]["P1"]["zones"]["cemetery"] = json!([
        {"id":"returned","card":"BP01-109"}, {"id":"foreign_class","card":"BP01-096"},
        {"id":"spell","card":"CP02-068"}
    ]);
    initial["players"]["P2"]["zones"]["cemetery"] = json!([{"id":"foreign","card":"BP01-109"}]);
    let mut game = game(&initial);
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"P2.leader"}))
            .unwrap()
            .outcome,
        "resolved"
    );
    let question = awaiting(trigger(&mut game));
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("attack target")
    };
    assert_eq!(domain.members.len(), 1);
    assert_eq!(domain.members[0].card.as_deref(), Some("BP01-109"));
    resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Targets(vec![domain.members[0].handle]),
        )
        .unwrap(),
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["returned"]));
    assert_eq!(state["P1"]["cemetery"], json!(["foreign_class", "spell"]));
    assert_eq!(state["P2"]["leader"]["life"], 20_u8);
    assert_eq!(state["P2"]["cemetery"], json!(["foreign"]));
}
