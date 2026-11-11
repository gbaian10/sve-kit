#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate the hand departure scenarios."
)]
use super::*;
use alloc::collections::BTreeMap;
use sve_engine::game::internal::{DecisionView, InputDomain, SearchAnswer};

fn header(steps: Value) -> Value {
    let mut tree = json!({"auto":{"on":{"any_of":[
        {"discarded":{"ref":"this","from":"hand","who":"self"}},
        {"banished":{"ref":"this","from":"hand","who":"self"}}
    ]},"do":[]}});
    tree["auto"]["do"] = steps;
    tree
}

fn input(observer: &Value, steps: Value) -> Value {
    let mut tree = equipment_played::tree(observer, "unit-follower");
    let mut producer = equipment_played::tree(
        &json!({"activate":{"cost":{"pp":0_u8},"do":[]}}),
        "unit-amulet",
    );
    producer["cards"]["unit-amulet"]["abilities"][0]["activate"]["do"] = steps;
    tree["cards"]["unit-amulet"] = producer["cards"]["unit-amulet"].clone();
    tree
}

fn initial(seat: u32, count: u64) -> Value {
    let actor = format!("P{seat}");
    let mut state = setup(seat, json!([]));
    state["turn"]["active"] = json!(actor);
    for player in ["P1", "P2"] {
        state["players"][player]["zones"]["field"] = json!([]);
        state["players"][player]["zones"]["hand"] = json!([]);
    }
    state["players"][&actor]["zones"]["field"] = json!([{"id":"p","card":"unit-amulet"}]);
    state["players"][&actor]["zones"]["hand"] = json!(
        (0..count)
            .map(|i| json!({"id":format!("h{i}"),"card":"unit-follower"}))
            .collect::<Vec<_>>()
    );
    state
}

fn fixture(tree: &Value, state: &Value, random: &Value) -> Game {
    let programs = lowerer().lower(tree).unwrap();
    prepared(&programs, state, random)
}

fn prepared(
    programs: &BTreeMap<String, WellFormedCardProgram>,
    state: &Value,
    random: &Value,
) -> Game {
    let mut game = Game::new(catalog(), state, &Value::Null, random, "hand-banish").unwrap();
    let empty = card(vec![]);
    for number in ["unit-follower", "unit-spell", "unit-amulet"] {
        game.load_internal_card(number, programs.get(number).unwrap_or(&empty))
            .unwrap();
    }
    game
}

fn question(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(question) = play else {
        panic!("printed question")
    };
    question
}

fn checks(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}

fn pending(game: &mut Game, count: usize, event: &str, seat: u32) -> CardPlay {
    checks(game);
    let q = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &q.domain else {
        panic!("pending")
    };
    assert_eq!(
        q.actor,
        PlayerId { seat },
        "the hand source controls its occurrence"
    );
    assert_eq!(options.len(), count, "each source creates one occurrence");
    assert!(
        options
            .iter()
            .all(|option| option.event == event && option.card == "unit-follower"),
        "the pending source retains its hand event and printing"
    );
    game.answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
        .unwrap()
}

fn banish_steps(count: u64) -> Value {
    json!([
        {"choose":{"bind":"fuel","from":{"who":"self","zone":"hand"},"n":count}},
        {"banish":{"ref":"fuel"}}
    ])
}

fn pay(game: &mut Game, seat: u32, count: u64) -> CardPlay {
    let play = game
        .activate_internal_card("p", AbilityId(1), PlayerId { seat })
        .unwrap();
    if let CardPlay::Awaiting(q) = play {
        assert!(
            matches!(q.domain, InputDomain::Targets(_)),
            "the producer asks for its hand material"
        );
        assert!(
            game.internal_pending_choice().is_err(),
            "an instruction cannot expose an intermediate check timing"
        );
        game.answer_internal_card(q.actor, q.token, Answer::Targets((0..count).collect()))
            .unwrap()
    } else {
        play
    }
}

#[test]
fn union_keeps_one_printed_source_and_rejects_quota_splitting_and_other_event_alternatives() {
    let head = header(json!([{"life":{"who":"self","delta":1_u8}}]));
    let tree = input(&head, banish_steps(1));
    let programs = lowerer().lower(&tree).unwrap();
    let abilities = &programs["unit-follower"].card().abilities;
    assert_eq!(abilities.len(), 2);
    assert_eq!(abilities[0].source, abilities[1].source);
    assert_eq!(abilities[0].node.source, abilities[1].node.source);
    assert_ne!(abilities[0].node.id, abilities[1].node.id);
    assert_ne!(abilities[0].node.scope, abilities[1].node.scope);
    for auto in [
        json!({"on":{"any_of":[{"discarded":{"ref":"this","from":"hand"}}]},"do":[]}),
        json!({"on":{"any_of":[{"discarded":{"ref":"this","from":"hand"}},{"phase":{"whose":"self","which":"end"}}]},"do":[]}),
        json!({"on":{"any_of":[{"discarded":{"ref":"this","from":"hand"}},{"discarded":{"ref":"this","from":"hand"}}]},"do":[]}),
        json!({"on":{"banished":{"ref":"this","from":"hand"}},"do":[]}),
    ] {
        lowerer()
            .lower(&input(&json!({"auto":auto}), banish_steps(1)))
            .unwrap_err();
    }
    for field in ["per_turn", "aggregation", "zone"] {
        let mut bad = tree.clone();
        bad["cards"]["unit-follower"]["abilities"][0]["auto"][field] = match field {
            "per_turn" => json!(1_u8),
            "aggregation" => json!("batch"),
            _ => json!("field"),
        };
        lowerer().lower(&bad).unwrap_err();
    }
    let nested = equipment_played::tree(
        &json!({"static":{"effects":[{"grant":{
        "on":{"who":"self","zone":"field","kind":"follower"},"abilities":[head]}}]}}),
        "unit-follower",
    );
    lowerer().lower(&nested).unwrap_err();
}

#[test]
fn both_hand_actions_capture_each_source_once_and_wait_for_the_producers_complete_body() {
    for seat in [1_u32, 2] {
        for banish in [false, true] {
            let mut steps = if banish {
                banish_steps(2)
            } else {
                json!([{"discard":{"all":true}}])
            };
            steps
                .as_array_mut()
                .unwrap()
                .push(json!({"life":{"who":"self","delta":3_u8}}));
            let tree = input(
                &header(json!([{"life":{"who":"self","delta":1_u8}}])),
                steps,
            );
            let mut game = fixture(&tree, &initial(seat, 2), &Value::Null);
            resolved(pay(&mut game, seat, 2));
            let actor = format!("P{seat}");
            assert_eq!(
                game.projection(View::Referee).unwrap()[&actor]["leader"]["life"],
                23_u8
            );
            for remaining in [2_usize, 1] {
                resolved(pending(
                    &mut game,
                    remaining,
                    if banish { "Banish" } else { "Discard" },
                    seat,
                ));
            }
            checks(&mut game);
            assert!(game.internal_pending_choice().unwrap().is_none());
            assert_eq!(
                game.projection(View::Referee).unwrap()[&actor]["leader"]["life"],
                25_u8
            );
        }
    }
}

#[test]
fn banish_self_to_ex_is_optional_and_respects_capacity() {
    let tree = input(
        &header(json!([{"optional":{"do":[{"move":{"ref":"this","to":"ex"}}]}}])),
        banish_steps(1),
    );
    for (accept, full) in [(true, false), (false, false), (true, true)] {
        let mut state = initial(1, 1);
        if full {
            state["players"]["P1"]["zones"]["ex"] = json!([{"filler":5_u8}]);
        }
        let mut game = fixture(&tree, &state, &Value::Null);
        resolved(pay(&mut game, 1, 1));
        let choice = question(pending(&mut game, 1, "Banish", 1));
        assert!(matches!(choice.domain, InputDomain::Integer(_)));
        resolved(
            game.answer_internal_card(
                choice.actor,
                choice.token,
                Answer::Integer(Nat(u64::from(accept))),
            )
            .unwrap(),
        );
        checks(&mut game);
        let after = game.projection(View::Referee).unwrap();
        assert_eq!(
            after["P1"]["banish"],
            if accept && !full {
                json!([])
            } else {
                json!(["h0"])
            }
        );
        if accept && !full {
            assert_eq!(after["P1"]["ex"], json!(["h0"]));
        }
        assert!(game.internal_pending_choice().unwrap().is_none());
    }
}

#[test]
fn field_or_deck_banish_does_not_create_a_hand_occurrence() {
    for (zone, action) in [
        (
            "field",
            json!({"banish":{"ref":{"who":"self","zone":"field","kind":"follower"}}}),
        ),
        ("deck", json!({"banish":{"top":{"who":"self","n":1_u8}}})),
    ] {
        let tree = input(
            &header(json!([{"life":{"who":"self","delta":1_u8}}])),
            json!([action]),
        );
        let mut state = initial(1, 0);
        if zone == "field" {
            state["players"]["P1"]["zones"][zone] = json!([
            {"id":"p","card":"unit-amulet"},{"id":"h0","card":"unit-follower"}]);
        } else {
            state["players"]["P1"]["zones"][zone] = json!([{"id":"h0","card":"unit-follower"}]);
        }
        let mut game = fixture(&tree, &state, &Value::Null);
        resolved(pay(&mut game, 1, 0));
        checks(&mut game);
        assert!(game.internal_pending_choice().unwrap().is_none());
        let after = game.projection(View::Referee).unwrap();
        assert_eq!(after["P1"]["banish"], json!(["h0"]));
        assert_eq!(after["P1"]["leader"]["life"], 20_u8);
    }
}

#[test]
fn self_return_does_not_follow_a_later_generation_of_the_same_physical_card() {
    let tree = input(
        &header(json!([{"move":{"ref":"this","to":"ex"}}])),
        banish_steps(1),
    );
    let mut programs = lowerer().lower(&tree).unwrap();
    let mut producer = programs["unit-amulet"].card().clone();
    let body = producer.abilities[0].node.body.as_mut().unwrap();
    let previous = body.steps.last().unwrap().clone();
    let Node::Instruction {
        instruction: banished,
    } = previous.node
    else {
        panic!("banish instruction")
    };
    let next = banished.id.0.checked_add(1).unwrap();
    // The IR fixture avoids opening an authored banish-result variant just for this probe.
    body.steps.push(Located {
        source: previous.source,
        node: Node::Instruction {
            instruction: Box::new(Instruction {
                id: InstructionId(next),
                receipt: ReceiptId(next),
                actor: PlayerRef::Controller {},
                source: banished.source.clone(),
                precondition: None,
                context: InstructionContext::Effect,
                action: Action::Move {
                    objects: Selector::MoveAfter {
                        receipt: MovementReceipt::Banish {
                            receipt: ReceiptReference::new(banished.receipt),
                        },
                        permission: FollowPermission::SameEffectExplicitReference {},
                    },
                    movement: MoveSpec {
                        to: MoveDestination::Owners {
                            zone: OwnerZone::Cemetery,
                        },
                        position: None,
                        order: None,
                        initial_posture: None,
                        entry_effects: vec![],
                    },
                    reason: MoveReason::Put,
                },
            }),
        },
    });
    programs.insert(
        "unit-amulet".into(),
        WellFormedCardProgram::check(producer).unwrap(),
    );
    let mut game = prepared(&programs, &initial(1, 1), &Value::Null);
    resolved(pay(&mut game, 1, 1));
    resolved(pending(&mut game, 1, "Banish", 1));
    checks(&mut game);
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["P1"]["cemetery"], json!(["h0"]));
    assert_eq!(after["P1"]["ex"], json!([]));
    assert!(game.internal_pending_choice().unwrap().is_none());
}

#[test]
fn unrelated_duplicate_printed_coordinates_remain_an_atomic_load_error() {
    for union in [false, true] {
        let head = if union {
            header(json!([{"life":{"who":"self","delta":1_u8}}]))
        } else {
            json!({"auto":{"on":{"discarded":{"ref":"this","from":"hand","who":"self"}},"do":[{"life":{"who":"self","delta":1_u8}}]}})
        };
        let mut tree = input(&head, banish_steps(1));
        let entry = tree["cards"]["unit-follower"]["abilities"][0].clone();
        tree["cards"]["unit-follower"]["abilities"] = json!([entry.clone(), entry]);
        let programs = lowerer().lower(&tree).unwrap();
        let mut game = Game::new(
            catalog(),
            &initial(1, 1),
            &Value::Null,
            &Value::Null,
            "duplicate-hand-source",
        )
        .unwrap();
        let before = exact_authority(&game);
        game.load_internal_card("unit-follower", &programs["unit-follower"])
            .unwrap_err();
        assert_eq!(exact_authority(&game), before);
    }
}

#[test]
fn a_failure_after_banish_restores_the_hand_pending_and_random_authority() {
    let mut steps = banish_steps(1);
    steps
        .as_array_mut()
        .unwrap()
        .push(json!({"search":{"sel":{"who":"self","zone":"deck"},"n":1_u8,"to":"hand"}}));
    let tree = input(
        &header(json!([{"life":{"who":"self","delta":1_u8}}])),
        steps,
    );
    let mut state = initial(1, 1);
    state["players"]["P1"]["zones"]["deck"] = json!([{"id":"d","card":"unit-spell"}]);
    let random = json!({"shuffles":[{"player":"P1","result":["not-a-member"]}]});
    let mut game = fixture(&tree, &state, &random);
    let origin = game.clone();
    let before = game.projection(View::Referee).unwrap();
    let search = question(pay(&mut game, 1, 1));
    let InputDomain::Search(domain) = &search.domain else {
        panic!("search after banish")
    };
    let published = game.clone();
    game.answer_internal_card(
        search.actor,
        search.token,
        Answer::Search(SearchAnswer::Ungrouped(vec![domain.cards[0].handle])),
    )
    .unwrap_err();
    transactions::assert_reverted_from(&game, &origin, &published);
    assert_eq!(game.projection(View::Referee).unwrap(), before);
    checks(&mut game);
    assert!(game.internal_pending_choice().unwrap().is_none());
    let retry = question(pay(&mut game, 1, 1));
    let InputDomain::Search(domain) = &retry.domain else {
        panic!("retry search")
    };
    let published = game.clone();
    game.answer_internal_card(
        retry.actor,
        retry.token,
        Answer::Search(SearchAnswer::Ungrouped(vec![domain.cards[0].handle])),
    )
    .unwrap_err();
    transactions::assert_reverted_from(&game, &origin, &published);
    assert_eq!(
        game.projection(View::Referee).unwrap(),
        before,
        "the same invalid shuffle proves its cursor also rolled back"
    );
}
