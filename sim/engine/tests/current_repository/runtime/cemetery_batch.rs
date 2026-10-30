#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate the scenario."
)]
use super::*;
use sve_engine::game::internal::DecisionView;

fn awaiting(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(fanfare) = play else {
        panic!("expected declaration")
    };
    fanfare
}

fn trigger(game: &mut Game, card: &str) -> CardPlay {
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("pending ability")
    };
    let option = options.iter().find(|option| option.card == card).unwrap();
    game.answer_internal_card(pending.actor, pending.token, Answer::Pending(option.handle))
        .unwrap()
}

#[test]
fn bp03_111_damages_the_formal_recipient_then_lastword_returns_a_cheap_fallen_angel_card() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP03-111"}]);
    initial["players"]["P1"]["zones"]["cemetery"] = json!([
        {"id":"returned","card":"BP03-118"},{"id":"expensive","card":"BP01-152"},
        {"id":"wrong-trait","card":"BP01-046"}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP04-117"}]);
    initial["players"]["P2"]["zones"]["cemetery"] = json!([{"id":"foreign","card":"BP03-118"}]);
    let mut objects = subjects();
    if let Selector::Zone { players, .. } = &mut objects {
        players.relation = PlayerRelation::SelfPlayer;
        players.cardinality = Cardinality {
            minimum: Nat(1),
            maximum: Some(Nat(1)),
        };
    }
    let mut game = with_producer(&initial, Action::Destroy { objects });
    assert!(matches!(
        game.play_internal_card("source", PlayerId { seat: 1 })
            .unwrap(),
        CardPlay::Entered(_)
    ));
    let fanfare = awaiting(trigger(&mut game, "BP03-111"));
    let InputDomain::Targets(recipients) = &fanfare.domain else {
        panic!("fanfare recipients")
    };
    let victim = recipients
        .members
        .iter()
        .find(|member| member.card.as_deref() == Some("BP04-117"))
        .unwrap();
    resolved(
        game.answer_internal_card(
            fanfare.actor,
            fanfare.token,
            Answer::Targets(vec![victim.handle]),
        )
        .unwrap(),
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
        2_u8
    );
    game.resume_internal_card().unwrap();
    activate(&mut game, "P1");
    game.resume_internal_card().unwrap();
    let lastword = awaiting(trigger(&mut game, "BP03-111"));
    let InputDomain::Targets(candidates) = &lastword.domain else {
        panic!("lastword cemetery target")
    };
    assert_eq!(candidates.members.len(), 1);
    assert_eq!(candidates.members[0].card.as_deref(), Some("BP03-118"));
    resolved(
        game.answer_internal_card(
            lastword.actor,
            lastword.token,
            Answer::Targets(vec![candidates.members[0].handle]),
        )
        .unwrap(),
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["returned"]));
    assert!(
        state["P1"]["cemetery"]
            .as_array()
            .unwrap()
            .contains(&json!("source"))
    );
    assert_eq!(state["P2"]["cemetery"], json!(["foreign"]));
}

#[test]
fn bp09_081_evolution_returns_only_own_nightmare_followers() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP09-080"}]);
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP09-081"}]);
    initial["players"]["P1"]["zones"]["cemetery"] = json!([
        {"id":"returned","card":"BP01-109"},{"id":"wrong-class","card":"BP01-096"},
        {"id":"spell","card":"BP01-116"}
    ]);
    initial["players"]["P2"]["zones"]["cemetery"] = json!([{"id":"foreign","card":"BP01-109"}]);
    let mut game = game(&initial);
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
        )
        .unwrap()
        .outcome,
        "resolved"
    );
    let question = awaiting(trigger(&mut game, "BP09-081"));
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("cemetery target")
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
    assert_eq!(state["P1"]["cemetery"], json!(["wrong-class", "spell"]));
    assert_eq!(state["P2"]["cemetery"], json!(["foreign"]));
}

#[test]
fn bp02_103_evolution_returns_only_own_bishop_followers() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP02-102"}]);
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP02-103"}]);
    initial["players"]["P1"]["zones"]["cemetery"] = json!([
        {"id":"returned","card":"BP01-135"},{"id":"wrong-class","card":"BP01-109"},
        {"id":"spell","card":"BP01-131"}
    ]);
    initial["players"]["P2"]["zones"]["cemetery"] = json!([{"id":"foreign","card":"BP01-135"}]);
    let mut game = game(&initial);
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
        )
        .unwrap()
        .outcome,
        "resolved"
    );
    let question = awaiting(trigger(&mut game, "BP02-103"));
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("cemetery target")
    };
    assert_eq!(domain.members.len(), 1);
    assert_eq!(domain.members[0].card.as_deref(), Some("BP01-135"));
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
    assert_eq!(state["P1"]["cemetery"], json!(["wrong-class", "spell"]));
    assert_eq!(state["P2"]["cemetery"], json!(["foreign"]));
}

#[test]
fn dsd01a_015_mode_two_requires_a_cemetery_academy_spell_before_counting_five_cards() {
    for (eligible, count) in [(false, 5_usize), (true, 4), (true, 5)] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"DSD01a-015"}]);
        let followers = count - usize::from(eligible);
        let mut grave = (0..followers)
            .map(|i| json!({"id":format!("academy-{i}"),"card":"DSD01a-002"}))
            .collect::<Vec<_>>();
        if eligible {
            grave.push(json!({"id":"returned","card":"DSD01a-009"}));
        }
        grave.push(json!({"id":"wrong-trait","card":"CP02-068"}));
        initial["players"]["P1"]["zones"]["cemetery"] = json!(grave);
        initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"outside","card":"DSD01a-009"}]);
        initial["players"]["P2"]["zones"]["cemetery"] =
            json!([{"id":"foreign","card":"DSD01a-009"}]);
        let mut game = game(&initial);
        assert!(matches!(
            game.play_internal_card("source", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ));
        let question = awaiting(trigger(&mut game, "DSD01a-015"));
        let InputDomain::Modes { modes, .. } = &question.domain else {
            panic!("formal mode")
        };
        assert_eq!(
            modes.contains(&ModeId(2)),
            eligible,
            "Q1588 removes the mode when its required target is absent"
        );
        if !eligible {
            let before = game.projection(View::Referee).unwrap();
            game.answer_internal_card(
                question.actor,
                question.token,
                Answer::Modes(vec![ModeId(2)]),
            )
            .unwrap_err();
            assert_eq!(game.projection(View::Referee).unwrap(), before);
            resolved(
                game.answer_internal_card(
                    question.actor,
                    question.token,
                    Answer::Modes(vec![ModeId(1)]),
                )
                .unwrap(),
            );
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["P1"]["ex"].as_array().unwrap().len(), 2);
            assert_eq!(state["P1"]["hand"], json!([]));
            continue;
        }
        let target = awaiting(
            game.answer_internal_card(
                question.actor,
                question.token,
                Answer::Modes(vec![ModeId(2)]),
            )
            .unwrap(),
        );
        let InputDomain::Targets(domain) = &target.domain else {
            panic!("required cemetery target")
        };
        assert_eq!(domain.members.len(), 1);
        assert_eq!(domain.members[0].card.as_deref(), Some("DSD01a-009"));
        resolved(
            game.answer_internal_card(
                target.actor,
                target.token,
                Answer::Targets(vec![domain.members[0].handle]),
            )
            .unwrap(),
        );
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P1"]["hand"],
            if count == 5 {
                json!(["returned"])
            } else {
                json!([])
            }
        );
        assert_eq!(state["P2"]["cemetery"], json!(["foreign"]));
    }
}
