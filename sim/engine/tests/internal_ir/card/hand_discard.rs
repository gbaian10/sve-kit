use super::*;
use sve_engine::game::internal::{DecisionToken, DecisionView, InputDomain};
use sve_engine::ir::lowering::ExpandedLowerer;

fn tree(discard: &Value) -> Value {
    let mut tree = fanfare::expanded::tree().clone();
    let mut ability = tree["cards"]["BP08-038"]["abilities"][0].clone();
    ability.as_object_mut().unwrap().remove("fanfare");
    ability["activate"] = json!({"cost":{"pp":1},"do":[{"discard":discard}]});
    let meta = tree["cards"]["BP08-038"]["meta"].clone();
    tree["cards"] = json!({"unit-follower":{"meta":meta,"abilities":[ability]}});
    tree
}

#[test]
fn hand_discard_uses_private_handles_and_discards_as_many_as_available() {
    let programs = ExpandedLowerer::new()
        .unwrap()
        .lower(&tree(&json!({"who":"self","n":2})))
        .unwrap();
    for seat in [1_u32, 2] {
        let actor = if seat == 1 { "P1" } else { "P2" };
        for count in [0_u64, 1, 3] {
            let mut initial = setup(seat, json!([]));
            initial["turn"]["active"] = json!(actor);
            initial["players"][actor]["pp"] = json!({"current":5_u8,"max":5_u8});
            initial["players"][actor]["zones"]["field"] =
                json!([{"id":"s","card":"unit-follower"}]);
            initial["players"][actor]["zones"]["hand"] = json!(
                (0..count)
                    .map(|n| json!({"id":format!("h{n}"),"card":if n % 2 == 0 {"unit-spell"} else {"unit-follower"}}))
                    .collect::<Vec<_>>()
            );
            let mut game = Game::new(
                catalog(),
                &initial,
                &Value::Null,
                &Value::Null,
                "own-discard",
            )
            .unwrap();
            game.load_internal_card("unit-follower", &programs["unit-follower"])
                .unwrap();
            game.load_internal_card("unit-spell", &card(vec![]))
                .unwrap();
            let CardPlay::Awaiting(question) = game
                .activate_internal_card("s", AbilityId(1), PlayerId { seat })
                .unwrap()
            else {
                panic!("discard choice even when empty");
            };
            let InputDomain::Targets(domain) = &question.domain else {
                panic!("opaque objects");
            };
            assert_eq!(domain.members.len(), usize::try_from(count).unwrap());
            assert_eq!(domain.cardinality.minimum, Nat(count.min(2)));
            assert!(
                game.internal_decision(if seat == 1 { View::P2 } else { View::P1 })
                    .is_none()
            );
            reject_answers(&mut game, &question, seat, count);
            let result = resolved(
                game.answer_internal_card(
                    question.actor,
                    question.token,
                    Answer::Targets((0..count.min(2)).collect()),
                )
                .unwrap(),
            );
            check_completed(&game, &result, seat, count);
            if count == 3 {
                reject_old_token(&mut game, seat, question.token);
            }
        }
    }
}

#[test]
fn hand_discard_does_not_open_general_hand_effect_selectors() {
    let hand = Selector::Zone {
        players: PlayerSet {
            relation: PlayerRelation::SelfPlayer,
            relative_to: PlayerRef::Controller {},
            cardinality: Cardinality {
                minimum: Nat(1),
                maximum: Some(Nat(1)),
            },
            text_quantifier: TextQuantifier::Implicit,
        },
        zone: ZoneKind::Hand,
        view: SelectionView::Current {},
        binding: Binding::new(99, ScopeId(1)),
        predicate: Box::new(Expr::Literal(Bool(true))),
    };
    for step in [
        instruction(
            1,
            Action::Destroy {
                objects: hand.clone(),
            },
        ),
        movement::put(1, hand.clone(), OwnerZone::Cemetery),
        damage(1, RecipientSet::Objects { objects: hand }, 1),
    ] {
        let mut game = Game::new(
            catalog(),
            &setup(1, json!([])),
            &Value::Null,
            &Value::Null,
            "private-hand-boundary",
        )
        .unwrap();
        let before = format!("{game:?}");
        game.load_internal_card("unit-follower", &card(vec![ability(1, vec![step])]))
            .unwrap_err();
        assert!(
            format!("{game:?}") == before,
            "private read rejection is atomic"
        );
    }
}

#[test]
fn discard_author_lowering_rejects_opponent_random_filtered_and_zero() {
    for discard in [
        json!({"who":"opp","n":1}),
        json!({"who":"self","n":1,"random":true}),
        json!({"who":"self","n":0}),
        json!({"who":"self","n":1,"sel":{"who":"self","zone":"hand","kind":"spell"}}),
    ] {
        ExpandedLowerer::new()
            .unwrap()
            .lower(&tree(&discard))
            .unwrap_err();
    }
}

fn reject_answers(game: &mut Game, question: &DecisionView, seat: u32, count: u64) {
    let before = format!("{game:?}");
    game.answer_internal_card(
        PlayerId {
            seat: 3_u32.checked_sub(seat).unwrap(),
        },
        question.token,
        Answer::Targets(vec![]),
    )
    .unwrap_err();
    assert!(
        format!("{game:?}") == before,
        "another actor cannot change the choice"
    );
    let mut foreign = vec![0; usize::try_from(count.clamp(1, 2)).unwrap()];
    foreign[0] = 99;
    game.answer_internal_card(question.actor, question.token, Answer::Targets(foreign))
        .unwrap_err();
    assert!(
        format!("{game:?}") == before,
        "foreign handles preserve payment and question"
    );
    if count > 1 {
        game.answer_internal_card(question.actor, question.token, Answer::Targets(vec![0, 0]))
            .unwrap_err();
        assert!(
            format!("{game:?}") == before,
            "duplicate handles are not distinct cards"
        );
    }
    if count > 0 {
        game.answer_internal_card(question.actor, question.token, Answer::Targets(vec![]))
            .unwrap_err();
        assert!(
            format!("{game:?}") == before,
            "available required cards cannot be omitted"
        );
    }
}

fn check_completed(game: &Game, result: &Activation, seat: u32, count: u64) {
    let actor = if seat == 1 { "P1" } else { "P2" };
    let opponent = if seat == 1 { "P2" } else { "P1" };
    let outcomes = result
        .progress
        .receipts
        .values()
        .filter_map(|outcome| {
            if let InstructionOutcome::Discard(discard) = outcome {
                Some(discard)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(
        outcomes.len(),
        1,
        "one instruction produces one discard outcome"
    );
    assert_eq!(
        outcomes[0].successful.len(),
        usize::try_from(count.min(2)).unwrap(),
        "all required available cards are discarded"
    );
    assert_eq!(
        outcomes[0].moves.len(),
        usize::try_from(count.min(2)).unwrap(),
        "each discard has a movement receipt"
    );
    assert_eq!(
        result
            .progress
            .facts
            .iter()
            .filter(|fact| matches!(fact, Fact::Discard(_)))
            .count(),
        usize::try_from(count.min(2)).unwrap(),
        "each actual discard has a typed fact"
    );
    assert!(
        result
            .progress
            .facts
            .iter()
            .filter_map(|fact| if let Fact::Movement(m) = fact {
                Some(m)
            } else {
                None
            })
            .all(|m| m.method == MoveMethod::Discard && m.from.kind == ZoneKind::Hand),
        "discard movements originate in hand"
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        state[actor]["pp"]["current"], 4_u8,
        "activation PP remains consumed"
    );
    assert_eq!(
        state[actor]["cemetery"],
        json!(
            (0..count.min(2))
                .map(|n| format!("h{n}"))
                .collect::<Vec<_>>()
        ),
        "only selected cards enter cemetery"
    );
    assert_eq!(
        state[actor]["hand"],
        json!((2..count).map(|n| format!("h{n}")).collect::<Vec<_>>()),
        "unselected cards stay in hand"
    );
    assert_eq!(
        state["semantic_state"]["counters_this_turn"][actor]["discarded"]
            .as_u64()
            .unwrap_or(0),
        count.min(2),
        "discard history matches the actual count"
    );
    let public = game
        .projection(if seat == 1 { View::P2 } else { View::P1 })
        .unwrap();
    assert!(
        public["objects"].get("h2").is_none(),
        "unselected identity stays private"
    );
    assert_eq!(
        state[opponent]["pp"]["current"], 2_u8,
        "opponent PP is unchanged"
    );
}

#[expect(
    clippy::panic,
    reason = "A missing question fails this scenario helper immediately."
)]
fn reject_old_token(game: &mut Game, seat: u32, old_token: DecisionToken) {
    let actor = if seat == 1 { "P1" } else { "P2" };
    game.resume_internal_card().unwrap();
    let CardPlay::Awaiting(next) = game
        .activate_internal_card("s", AbilityId(1), PlayerId { seat })
        .unwrap()
    else {
        panic!("next discard occurrence");
    };
    let pending_state = format!("{game:?}");
    game.answer_internal_card(next.actor, old_token, Answer::Targets(vec![0]))
        .unwrap_err();
    assert!(
        format!("{game:?}") == pending_state,
        "an old token cannot select a new hand member with the same handle"
    );
    resolved(
        game.answer_internal_card(next.actor, next.token, Answer::Targets(vec![0]))
            .unwrap(),
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["semantic_state"]["counters_this_turn"][actor]["discarded"],
        3_u8,
        "the second discard increments history independently"
    );
}
