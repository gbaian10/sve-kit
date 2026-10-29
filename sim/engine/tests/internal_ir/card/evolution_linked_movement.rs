use super::*;
use sve_engine::game::internal::{Fact, InputDomain};

pub(super) fn tree(heads: &[(&str, Value)]) -> Value {
    let mut tree = fanfare::expanded::tree().clone();
    let template = tree["cards"]["BP08-038"]["abilities"][0].clone();
    let abilities = heads
        .iter()
        .enumerate()
        .map(|(index, (head, body))| {
            let mut entry = template.clone();
            entry.as_object_mut().unwrap().remove("fanfare");
            entry["line"] = json!(index.saturating_add(1));
            entry[*head] = body.clone();
            entry
        })
        .collect::<Vec<_>>();
    let meta = tree["cards"]["BP08-038"]["meta"].clone();
    tree["cards"] = json!({"unit-follower":{"meta":meta,"abilities":abilities}});
    tree
}

pub(super) fn loaded(initial: &Value, program: &WellFormedCardProgram) -> Game {
    let mut game = Game::new(
        catalog(),
        initial,
        &Value::Null,
        &Value::Null,
        "linked-movement",
    )
    .unwrap();
    game.load_internal_card("unit-follower", program).unwrap();
    game
}

#[test]
fn public_ex_uses_owner_and_keeps_other_link_families_closed() {
    let programs = lowerer().lower(&tree(&[("activate", json!({
        "cost":{"pp":0},
        "targets":[{"bind":"victim","from":{"who":"opp","zone":"field","kind":"follower"},"n":1}],
        "do":[{"move":{"ref":"victim","to":"ex"}}]
    }))])).unwrap();
    let program = &programs["unit-follower"];
    let ability = program.card().abilities[0].node.id;
    for owner in [1_u32, 2] {
        for other_link in [false, true] {
            let mut initial = setup(1, json!([]));
            initial["players"]["P1"]["zones"]["hand"] = json!([]);
            initial["players"]["P1"]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
            initial["players"]["P2"]["zones"]["field"] = json!([{
                "id":"victim","card":"unit-follower","owner":format!("P{owner}"),
                "state":{"links":if other_link {json!({"race":[]})} else {Value::Null}}
            }]);
            let mut game = loaded(&initial, program);
            let before = game.projection(View::Referee).unwrap();
            let CardPlay::Awaiting(question) = game
                .activate_internal_card("s", ability, PlayerId { seat: 1 })
                .unwrap()
            else {
                panic!("target question");
            };
            let answer =
                game.answer_internal_card(question.actor, question.token, Answer::Targets(vec![0]));
            if other_link {
                answer.unwrap_err();
                assert_eq!(game.projection(View::Referee).unwrap(), before);
            } else {
                let moved = resolved(answer.unwrap());
                assert!(moved.progress.facts.iter().any(|fact| matches!(fact, Fact::Movement(member)
                if member.movement.actual_to == Some(RuntimeZoneRef { player: PlayerId {seat:owner}, kind:ZoneKind::Ex }) && member.movement.requested_to == RuntimeZoneRef {player:PlayerId {seat:owner}, kind:ZoneKind::Ex})));
                let state = game.projection(View::Referee).unwrap();
                assert_eq!(state[format!("P{owner}")]["ex"], json!(["victim"]));
                assert_eq!(state[if owner == 1 { "P2" } else { "P1" }]["ex"], json!([]));
                assert_eq!(state["P2"]["field"], json!([]));
            }
        }
    }
}

#[test]
fn lastword_follow_does_not_retarget_a_later_generation() {
    let heads = [
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
            json!({"do":[{"move":{"ref":"this","to":"ex"}}]}),
        ),
    ];
    let programs = lowerer().lower(&tree(&heads)).unwrap();
    let mut card = programs["unit-follower"].card().clone();
    let first = &mut card.abilities[1].node.body.as_mut().unwrap().steps[0].node;
    let Node::Instruction { instruction } = first else {
        panic!("move");
    };
    let Action::Move { movement, .. } = &mut instruction.action else {
        panic!("move");
    };
    movement.to = MoveDestination::Owners {
        zone: OwnerZone::Banish,
    };
    let program = WellFormedCardProgram::check(card).unwrap();
    let ability = program.card().abilities[0].node.id;
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
    let mut game = loaded(&initial, &program);
    resolved(
        game.activate_internal_card("s", ability, PlayerId { seat: 1 })
            .unwrap(),
    );
    game.resume_internal_card().unwrap();
    for line in [2_u64, 3] {
        let question = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = question.domain else {
            panic!("lastword");
        };
        let handle = options
            .iter()
            .find(|option| option.line == line)
            .unwrap()
            .handle;
        resolved(
            game.answer_internal_card(question.actor, question.token, Answer::Pending(handle))
                .unwrap(),
        );
        game.resume_internal_card().unwrap();
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["banish"], json!(["s"]));
        assert_eq!(state["P1"]["ex"], json!([]));
    }
}

#[test]
fn lastword_cannot_move_into_a_full_owner_ex() {
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
        ]))
        .unwrap();
    let program = &programs["unit-follower"];
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
    initial["players"]["P1"]["zones"]["ex"] = json!(
        (0..5)
            .map(|n| json!({"id":format!("ex{n}"),"card":"unit-follower"}))
            .collect::<Vec<_>>()
    );
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
    let played = resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap(),
    );
    assert!(
        !played
            .progress
            .facts
            .iter()
            .any(|fact| matches!(fact, Fact::Movement(_)))
    );
    game.resume_internal_card().unwrap();
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["cemetery"], json!(["s"]));
    assert_eq!(state["P1"]["ex"].as_array().unwrap().len(), 5);
    assert!(game.internal_pending_choice().unwrap().is_none());
}

#[test]
fn public_ex_capacity_is_per_owner_and_partial_batches_roll_back() {
    let programs = lowerer().lower(&tree(&[("activate", json!({
        "cost":{"pp":0},
        "targets":[{"bind":"victims","from":{"who":"opp","zone":"field","kind":"follower"},"n":2}],
        "do":[{"move":{"ref":"victims","to":"ex"}}]
    }))])).unwrap();
    let program = &programs["unit-follower"];
    for mixed_owners in [true, false] {
        let mut initial = setup(1, json!([]));
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
        initial["players"]["P2"]["zones"]["field"] = json!([
            {"id":"first","card":"unit-follower","owner":if mixed_owners {"P1"} else {"P2"}},
            {"id":"second","card":"unit-follower","owner":"P2"}
        ]);
        let (full_owner, occupied) = if mixed_owners { ("P1", 5) } else { ("P2", 4) };
        initial["players"][full_owner]["zones"]["ex"] = json!(
            (0..occupied)
                .map(|n| json!({"id":format!("ex{n}"),"card":"unit-follower"}))
                .collect::<Vec<_>>()
        );
        let mut game = loaded(&initial, program);
        let before = game.projection(View::Referee).unwrap();
        let CardPlay::Awaiting(question) = game
            .activate_internal_card(
                "s",
                program.card().abilities[0].node.id,
                PlayerId { seat: 1 },
            )
            .unwrap()
        else {
            panic!("targets");
        };
        let InputDomain::Targets(domain) = &question.domain else {
            panic!("target domain");
        };
        assert_eq!(domain.members.len(), 2);
        let answer =
            game.answer_internal_card(question.actor, question.token, Answer::Targets(vec![0, 1]));
        if mixed_owners {
            let moved = resolved(answer.unwrap());
            let movements = moved
                .progress
                .facts
                .iter()
                .filter_map(|fact| {
                    if let Fact::Movement(member) = fact {
                        Some(member)
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();
            assert_eq!(movements.len(), 1);
            assert_eq!(
                movements[0].movement.actual_to,
                Some(RuntimeZoneRef {
                    player: PlayerId { seat: 2 },
                    kind: ZoneKind::Ex
                })
            );
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["P1"]["ex"], before["P1"]["ex"]);
            assert_eq!(state["P2"]["ex"], json!(["second"]));
            assert_eq!(state["P2"]["field"], json!(["first"]));
        } else {
            let error = answer.unwrap_err();
            assert!(format!("{error:?}").contains("capacity selection"));
            assert_eq!(game.projection(View::Referee).unwrap(), before);
        }
    }
}
