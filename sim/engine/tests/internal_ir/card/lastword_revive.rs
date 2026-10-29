#![expect(
    clippy::panic,
    reason = "A missing Lastword authority question invalidates these synthetic scenarios."
)]
use super::evolution_linked_movement::{loaded, tree};
use super::*;
use sve_engine::game::internal::{Fact, InputDomain};

#[test]
fn lastword_returns_the_exact_departed_generation_as_a_fresh_field_object() {
    let programs = lowerer()
        .lower(&tree(&[
            (
                "activate",
                json!({"cost":{"pp":0},"do":[{"destroy":"this"}]}),
            ),
            (
                "lastword",
                json!({"do":[{"move":{"ref":"this","to":"field"}}]}),
            ),
        ]))
        .unwrap();
    let program = &programs["unit-follower"];
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{
        "id":"s","card":"unit-follower","state":{"damage":1_u8,"acted":true,"counters":{"old":2_u8}}
    }]);
    let mut game = loaded(&initial, program);
    resolved(
        game.activate_internal_card(
            "s",
            program.card().abilities[0].node.id,
            PlayerId { seat: 1 },
        )
        .unwrap(),
    );
    game.resume_internal_card().unwrap();
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = question.domain else {
        panic!("Lastword");
    };
    assert_eq!(options.len(), 1);
    let revived = resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap(),
    );
    game.resume_internal_card().unwrap();
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["field"], json!(["s"]));
    assert_eq!(state["P1"]["cemetery"], json!([]));
    assert_eq!(state["objects"]["s"]["generation"], 2_u8);
    assert_eq!(state["objects"]["s"]["damage"], 0_u8);
    assert_eq!(state["objects"]["s"]["acted"], false);
    assert_eq!(state["objects"]["s"]["counters"], json!({}));
    assert!(revived.progress.facts.iter().any(|fact| matches!(fact,
        Fact::Movement(movement) if movement.from.kind == ZoneKind::Cemetery
            && movement.movement.actual_to == Some(RuntimeZoneRef {player:PlayerId {seat:1},kind:ZoneKind::Field})
    )));
    assert!(game.internal_pending_choice().unwrap().is_none());
}

fn board(pp: u8) -> Value {
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["pp"]["current"] = json!(pp);
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
    initial
}

fn depart(game: &mut Game, ability: AbilityId) {
    resolved(
        game.activate_internal_card("s", ability, PlayerId { seat: 1 })
            .unwrap(),
    );
    game.resume_internal_card().unwrap();
}

fn fire(game: &mut Game, line: u64) -> CardPlay {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = question.domain else {
        panic!("Lastword");
    };
    let option = options.iter().find(|option| option.line == line).unwrap();
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::Pending(option.handle),
    )
    .unwrap()
}

#[test]
fn a_later_move_does_not_let_an_old_lastword_retrieve_the_new_generation() {
    let programs = lowerer()
        .lower(&tree(&[
            (
                "activate",
                json!({"cost":{"pp":0},"do":[{"destroy":"this"}]}),
            ),
            (
                "lastword",
                json!({"do":[{"move":{"ref":"this","to":"ex"}}]}),
            ),
            (
                "lastword",
                json!({"do":[{"move":{"ref":"this","to":"field"}}]}),
            ),
        ]))
        .unwrap();
    let program = &programs["unit-follower"];
    let mut game = loaded(&board(3), program);
    depart(&mut game, program.card().abilities[0].node.id);
    resolved(fire(&mut game, 2));
    game.resume_internal_card().unwrap();
    let before = game.projection(View::Referee).unwrap();
    let stale = resolved(fire(&mut game, 3));
    game.resume_internal_card().unwrap();
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["P1"]["ex"], json!(["s"]));
    assert_eq!(after["P1"]["field"], json!([]));
    assert_eq!(after["objects"]["s"], before["objects"]["s"]);
    assert!(
        stale
            .progress
            .facts
            .iter()
            .all(|fact| !matches!(fact, Fact::Movement(_)))
    );
}

#[test]
fn resurrection_head_payment_can_be_declined_or_unaffordable_without_reviving() {
    let programs = lowerer()
        .lower(&tree(&[
            (
                "activate",
                json!({"cost":{"pp":0},"do":[{"destroy":"this"}]}),
            ),
            (
                "lastword",
                json!({"cost":{"pp":2},"do":[{"move":{"ref":"this","to":"field"}}]}),
            ),
        ]))
        .unwrap();
    let program = &programs["unit-follower"];
    for (pp, accept) in [(1_u8, true), (2, false), (3, true)] {
        let mut game = loaded(&board(pp), program);
        depart(&mut game, program.card().abilities[0].node.id);
        let CardPlay::Awaiting(question) = fire(&mut game, 2) else {
            panic!("head payment");
        };
        assert!(matches!(question.domain, InputDomain::OptionalCost));
        let play = game
            .answer_internal_card(question.actor, question.token, Answer::OptionalCost(accept))
            .unwrap();
        let succeeds = accept && pp >= 2;
        if succeeds {
            resolved(play);
        } else {
            assert_eq!(play, CardPlay::Cancelled);
        }
        game.resume_internal_card().unwrap();
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P1"]["field"],
            if succeeds { json!(["s"]) } else { json!([]) }
        );
        assert_eq!(
            state["P1"]["cemetery"],
            if succeeds { json!([]) } else { json!(["s"]) }
        );
        assert_eq!(
            state["P1"]["pp"]["current"],
            pp - if succeeds { 2 } else { 0 }
        );
        assert!(game.internal_pending_choice().unwrap().is_none());
    }
}

#[test]
fn a_full_field_keeps_the_departed_card_in_cemetery_without_an_entry_fact() {
    let programs = lowerer()
        .lower(&tree(&[
            (
                "activate",
                json!({"cost":{"pp":0},"do":[{"destroy":"this"}]}),
            ),
            (
                "lastword",
                json!({"do":[{"move":{"ref":"this","to":"field"}}]}),
            ),
        ]))
        .unwrap();
    let mut raw = programs["unit-follower"].card().clone();
    let origin = raw.abilities[0].node.source.clone();
    let mut arrive = entry::enter(900, entry::own_zone(ZoneKind::Ex, 90));
    arrive.source = raw.abilities[0].node.body.as_ref().unwrap().steps[0]
        .source
        .clone();
    let Node::Instruction { instruction } = &mut arrive.node else {
        panic!("entry instruction");
    };
    instruction.source = origin;
    raw.abilities[0]
        .node
        .body
        .as_mut()
        .unwrap()
        .steps
        .push(arrive);
    let program = WellFormedCardProgram::check(raw).unwrap();
    let mut initial = board(3);
    initial["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .extend((0..4).map(|n| json!({"id":format!("f{n}"),"card":"unit-follower"})));
    initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"fifth","card":"unit-follower"}]);
    let mut game = loaded(&initial, &program);
    depart(&mut game, program.card().abilities[0].node.id);
    let blocked = resolved(fire(&mut game, 2));
    game.resume_internal_card().unwrap();
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["field"].as_array().unwrap().len(), 5);
    assert_eq!(state["P1"]["cemetery"], json!(["s"]));
    assert_eq!(state["objects"]["s"]["generation"], 1_u8);
    assert!(
        blocked
            .progress
            .facts
            .iter()
            .all(|fact| !matches!(fact, Fact::Movement(_)))
    );
    assert!(game.internal_pending_choice().unwrap().is_none());
}

#[test]
fn field_return_lowering_does_not_admit_arbitrary_targets_or_other_ability_heads() {
    lowerer()
        .lower(&tree(&[(
            "activate",
            json!({"cost":{"pp":0},"do":[{"move":{"ref":"this","to":"field"}}]}),
        )]))
        .err()
        .unwrap();
    lowerer()
        .lower(&tree(&[(
            "lastword",
            json!({"do":[{"move":{"ref":{"who":"self","zone":"field"},"to":"field"}}]}),
        )]))
        .err()
        .unwrap();
}

#[test]
fn lastword_declares_targets_after_departure_and_rejects_foreign_answers() {
    let programs = lowerer()
        .lower(&tree(&[
            ("activate", json!({"cost":{"pp":0},"do":[{"destroy":"this"}]})),
            ("lastword", json!({
                "targets":[{"bind":"enemy","from":{"who":"opp","zone":"field","kind":"follower"},"n":1}],
                "do":[{"damage":{"to":"enemy","n":2}},{"life":{"who":"self","delta":1}}]
            })),
        ]))
        .unwrap();
    let program = &programs["unit-follower"];
    for available in [false, true] {
        let mut initial = board(3);
        initial["players"]["P2"]["zones"]["field"] = if available {
            json!([{"id":"enemy","card":"unit-follower"}])
        } else {
            json!([])
        };
        let mut game = loaded(&initial, program);
        depart(&mut game, program.card().abilities[0].node.id);
        let before = game.projection(View::Referee).unwrap();
        assert_eq!(before["P1"]["field"], json!([]));
        assert_eq!(before["P1"]["cemetery"], json!(["s"]));
        let play = fire(&mut game, 2);
        if !available {
            assert_eq!(play, CardPlay::Cancelled);
            assert_eq!(game.projection(View::Referee).unwrap(), before);
            game.resume_internal_card().unwrap();
            assert!(game.internal_pending_choice().unwrap().is_none());
            continue;
        }
        let CardPlay::Awaiting(question) = play else {
            panic!("Lastword target declaration");
        };
        let InputDomain::Targets(domain) = &question.domain else {
            panic!("formal targets");
        };
        assert_eq!(question.actor, PlayerId { seat: 1 });
        assert_eq!(domain.members.len(), 1);
        assert_eq!(domain.cardinality.minimum, Nat(1));
        let declaring = game.projection(View::Referee).unwrap();
        for part in ["P1", "P2", "objects"] {
            assert_eq!(declaring[part], before[part]);
        }
        let pending = format!("{game:?}");
        for (actor, handles) in [
            (PlayerId { seat: 2 }, vec![domain.members[0].handle]),
            (question.actor, vec![]),
            (question.actor, vec![99]),
        ] {
            game.answer_internal_card(actor, question.token, Answer::Targets(handles))
                .unwrap_err();
            assert!(
                format!("{game:?}") == pending,
                "invalid answers preserve authority"
            );
        }
        let played = resolved(
            game.answer_internal_card(
                question.actor,
                question.token,
                Answer::Targets(vec![domain.members[0].handle]),
            )
            .unwrap(),
        );
        assert_eq!(
            played
                .progress
                .facts
                .iter()
                .filter(|fact| matches!(fact, Fact::AbilityPlayed { .. }))
                .count(),
            1
        );
        game.resume_internal_card().unwrap();
        let after = game.projection(View::Referee).unwrap();
        assert_eq!(after["objects"]["enemy"]["damage"], 2);
        assert_eq!(after["P1"]["leader"]["life"], 21);
        assert_eq!(after["P1"]["cemetery"], json!(["s"]));
        assert!(game.internal_pending_choice().unwrap().is_none());
    }
}
