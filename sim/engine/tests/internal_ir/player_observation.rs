use super::ordering::{look, looked, order};
use super::*;
use sve_engine::game::internal::{DecisionView, InputDomain, ResumeStage};
use sve_engine::game::player::events::EventKind;
use sve_engine::game::player::observation::{
    CardMemory, CardObservation, Observation, ObservedZone, ZoneObservation,
};
use sve_engine::game::player::{ObjectHandle, Viewer};

fn board(initial: &Value) -> Game {
    Game::new(
        catalog(),
        initial,
        &Value::Null,
        &Value::Null,
        "player-observation",
    )
    .unwrap()
}

fn observed_zone(
    observation: &Observation,
    player: Viewer,
    zone: ObservedZone,
) -> &ZoneObservation {
    observation
        .players
        .iter()
        .find(|entry| entry.player == player)
        .unwrap()
        .zones
        .iter()
        .find(|entry| entry.kind == zone)
        .unwrap()
}

fn visible(observation: &Observation, player: Viewer, zone: ObservedZone) -> Vec<&CardObservation> {
    observed_zone(observation, player, zone)
        .cards
        .iter()
        .map(|entry| &entry.card)
        .collect()
}

fn reveal_and_wait() -> WellFormedProgram {
    reveal_with_tail(vec![])
}

fn reveal_with_tail(tail: Vec<Located<Node>>) -> WellFormedProgram {
    let mut steps = vec![
        instruction(
            1,
            Action::Reveal {
                objects: Selector::Zone {
                    players: PlayerSet {
                        cardinality: Cardinality {
                            minimum: Nat(1),
                            maximum: Some(Nat(1)),
                        },
                        ..players(PlayerRelation::SelfPlayer)
                    },
                    zone: ZoneKind::Hand,
                    view: SelectionView::Current {},
                    binding: Binding::new(90, ScopeId(1)),
                    predicate: Box::new(Expr::Literal(Bool(true))),
                },
            },
        ),
        look(2, 1),
        order(3, looked(2), Position::Top {}),
    ];
    steps.extend(tail);
    checked(steps)
}

fn start(game: &mut Game, program: &WellFormedProgram) -> DecisionView {
    let result = game
        .execute_internal(program, &source("synthetic"), context(1))
        .unwrap();
    assert!(!result.complete, "fixture must stop at its decision");
    game.internal_decision(View::P1).unwrap()
}

fn finish_order(game: &mut Game, question: &DecisionView) {
    assert!(
        matches!(question.domain, InputDomain::Order(_)),
        "fixture must ask for order"
    );
    assert!(
        game.answer_internal(question.actor, question.token, Answer::Order(vec![0]))
            .unwrap()
            .complete,
        "the single-card order must finish the fixture"
    );
}

#[test]
fn hidden_configuration_and_physical_identity_do_not_change_player_observation() {
    let mut first = setup(1, json!([{"id":"private-deck-a","card":"unit-spell"}]));
    first["players"]["P2"]["zones"]["hand"] = json!([
        {"id":"private-a","card":"unit-spell"}, {"id":"private-b","card":"unit-amulet"}
    ]);
    first["players"]["P2"]["zones"]["field"] = json!([
        {"id":"z-physical","card":"unit-follower","state":{"power":4}},
        {"id":"a-physical","card":"unit-follower","state":{"power":9}}
    ]);
    let mut second = first.clone();
    second["players"]["P1"]["zones"]["deck"] =
        json!([{"id":"other-private-deck","card":"unit-amulet"}]);
    second["players"]["P2"]["zones"]["hand"] = json!([
        {"id":"private-other-b","card":"unit-amulet"}, {"id":"private-other-a","card":"unit-spell"}
    ]);
    second["players"]["P2"]["zones"]["field"][0]["id"] = json!("a-physical");
    second["players"]["P2"]["zones"]["field"][1]["id"] = json!("z-physical");
    let left = board(&first).observe(Viewer::P1).unwrap();
    let right = board(&second).observe(Viewer::P1).unwrap();
    assert_eq!(left, right);
    let followers = visible(&left, Viewer::P2, ObservedZone::Field);
    assert_ne!(followers[0].handle, followers[1].handle);
    assert_eq!(
        followers[0]
            .properties
            .state
            .as_ref()
            .unwrap()
            .stats
            .as_ref()
            .unwrap()
            .attack,
        4
    );
    assert_eq!(
        followers[1]
            .properties
            .state
            .as_ref()
            .unwrap()
            .stats
            .as_ref()
            .unwrap()
            .attack,
        9
    );
    assert_eq!(visible(&left, Viewer::P1, ObservedZone::Hand).len(), 1);
    assert_eq!(
        observed_zone(&left, Viewer::P2, ObservedZone::Hand).count,
        2
    );
    assert_eq!(visible(&left, Viewer::P2, ObservedZone::Hand).len(), 0);
    assert_eq!(visible(&left, Viewer::P1, ObservedZone::Deck).len(), 0);
    let debug = format!("{left:?}");
    for secret in [
        "private-deck-a",
        "private-a",
        "z-physical",
        "a-physical",
        "generation",
        "execution",
        "binding",
    ] {
        assert!(!debug.contains(secret));
    }
}

#[test]
fn repeated_observations_are_read_only_and_do_not_consume_authority_randomness() {
    let game = board(&setup(1, json!([{"id":"d","card":"unit-follower"}])));
    let before = serde_json::to_value(&game).unwrap();
    let first = game.observe(Viewer::P1).unwrap();
    for _ in 0..3 {
        assert_eq!(game.observe(Viewer::P1).unwrap(), first);
        game.observe(Viewer::P2).unwrap();
    }
    assert_eq!(serde_json::to_value(&game).unwrap(), before);
}

#[test]
fn simultaneous_revelation_ignores_opponent_hand_slots_and_expiry_retires_handles() {
    let mut first = setup(1, json!([{"id":"d","card":"unit-follower"}]));
    first["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"s","card":"unit-spell"}, {"id":"hidden-follower","card":"unit-follower"}
    ]);
    let mut second = first.clone();
    second["players"]["P1"]["zones"]["hand"]
        .as_array_mut()
        .unwrap()
        .reverse();
    let mut left = board(&first);
    let mut right = board(&second);
    let program = reveal_and_wait();
    let first_question = start(&mut left, &program);
    let second_question = start(&mut right, &program);
    let revealed = left.observe(Viewer::P2).unwrap();
    assert_eq!(revealed, right.observe(Viewer::P2).unwrap());
    let old: Vec<ObjectHandle> = visible(&revealed, Viewer::P1, ObservedZone::Hand)
        .into_iter()
        .map(|card| card.handle.clone())
        .collect();
    assert_eq!(old.len(), 2);
    assert!(
        visible(&revealed, Viewer::P1, ObservedZone::Hand)
            .iter()
            .all(|card| card.properties.state.is_none() && !card.properties.names.is_empty())
    );
    assert!(
        observed_zone(&revealed, Viewer::P1, ObservedZone::Hand)
            .cards
            .iter()
            .all(|entry| entry.position.is_none())
    );
    finish_order(&mut left, &first_question);
    finish_order(&mut right, &second_question);
    assert!(
        visible(&revealed, Viewer::P1, ObservedZone::Hand)
            .iter()
            .all(|card| card.public)
    );
    let owner_after = left.observe(Viewer::P1).unwrap();
    assert!(
        visible(&owner_after, Viewer::P1, ObservedZone::Hand)
            .iter()
            .all(|card| !card.public)
    );
    let expired = left.observe(Viewer::P2).unwrap();
    assert_eq!(visible(&expired, Viewer::P1, ObservedZone::Hand).len(), 0);
    assert_eq!(revealed.remembered, expired.remembered);
    assert_eq!(
        visible(
            &left.observe(Viewer::P1).unwrap(),
            Viewer::P1,
            ObservedZone::Hand
        )
        .len(),
        2
    );
    start(&mut left, &program);
    let again = left.observe(Viewer::P2).unwrap();
    for card in visible(&again, Viewer::P1, ObservedZone::Hand) {
        assert!(!old.contains(&card.handle));
    }
}

#[test]
fn known_deck_offsets_survive_reading_but_private_reorder_retires_their_handles() {
    let initial = setup(
        1,
        json!([
            {"id":"d0","card":"unit-follower"}, {"id":"d1","card":"unit-amulet"}
        ]),
    );
    let mut game = board(&initial);
    let program = checked(vec![look(1, 1), order(2, looked(1), Position::Bottom {})]);
    let question = start(&mut game, &program);
    let looked_view = game.observe(Viewer::P1).unwrap();
    let top = observed_zone(&looked_view, Viewer::P1, ObservedZone::Deck);
    assert_eq!(top.cards.len(), 1);
    assert_eq!(top.cards[0].position, Some(0));
    assert!(!top.cards[0].card.public);
    assert!(!top.cards[0].card.face_up);
    assert_eq!(
        visible(
            &game.observe(Viewer::P2).unwrap(),
            Viewer::P1,
            ObservedZone::Deck
        )
        .len(),
        0
    );
    let old = top.cards[0].card.handle.clone();
    finish_order(&mut game, &question);
    let again = start(
        &mut game,
        &checked(vec![look(1, 2), order(2, looked(1), Position::Top {})]),
    );
    let reordered = game.observe(Viewer::P1).unwrap();
    let cards = visible(&reordered, Viewer::P1, ObservedZone::Deck);
    assert_eq!(cards.len(), 2);
    assert!(cards.iter().all(|card| card.handle != old));
    assert_eq!(game.internal_decision(View::P1).unwrap().token, again.token);
}

#[test]
fn an_atomic_failure_preserves_all_observations_and_authority_state() {
    let initial = setup(1, json!([{"id":"d0","card":"unit-follower"}]));
    let mut saved = serde_json::to_value(board(&initial)).unwrap();
    saved["state"]["objects"]["d0"]["generation"] = json!(u64::MAX);
    let mut game: Game = serde_json::from_value(saved).unwrap();
    let before = game.observe(Viewer::P1).unwrap();
    let other = game.observe(Viewer::P2).unwrap();
    let authority = exact_authority(&game);
    let program = checked(vec![draw(1, 1)]);
    game.execute_internal(&program, &source("synthetic"), context(1))
        .unwrap_err();
    assert_eq!(game.observe(Viewer::P1).unwrap(), before);
    assert_eq!(game.observe(Viewer::P2).unwrap(), other);
    assert_eq!(exact_authority(&game), authority);
}

#[test]
fn public_movement_preserves_identification_across_authority_generations() {
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
    let mut game = board(&initial);
    game.advance_player_input();
    let before = [Viewer::P1, Viewer::P2].map(|viewer| game.observe(viewer).unwrap());
    let generation = game.projection(View::Referee).unwrap()["objects"]["s"]["generation"].clone();
    game.execute_internal(
        &checked(vec![movement::put(
            1,
            Selector::Objects {
                objects: vec![ObjectOperand::SourceHolder {}],
            },
            OwnerZone::Cemetery,
        )]),
        &source("synthetic"),
        context(1),
    )
    .unwrap();
    game.advance_player_input();
    assert_ne!(
        game.projection(View::Referee).unwrap()["objects"]["s"]["generation"],
        generation
    );
    for observation in before {
        let old = &visible(&observation, Viewer::P1, ObservedZone::Field)[0].handle;
        let after = game.observe(observation.viewer).unwrap();
        let events = game
            .events(observation.viewer, None)
            .unwrap()
            .collect::<Vec<_>>();
        assert_eq!(events.len(), 1);
        assert!(
            matches!(&events[0].kind, EventKind::Movement { before, after, .. }
            if before.card.as_ref().and_then(|card| card.handle.as_ref()) == Some(old)
            && after.card.as_ref().and_then(|card| card.handle.as_ref()) == Some(old))
        );

        assert_eq!(
            &visible(&after, Viewer::P1, ObservedZone::Cemetery)[0].handle,
            old
        );
    }
}

fn exhausted_deck(initial: &Value) -> Game {
    let mut saved = serde_json::to_value(board(initial)).unwrap();
    saved["state"]["objects"]["d0"]["generation"] = json!(u64::MAX);
    serde_json::from_value(saved).unwrap()
}

#[test]
fn published_rollback_does_not_reissue_a_disclosed_handle_and_keeps_continuous_handles() {
    let initial = setup(1, json!([{"id":"d0","card":"unit-follower"}]));
    let mut game = exhausted_deck(&initial);
    let continuous = visible(
        &game.observe(Viewer::P1).unwrap(),
        Viewer::P2,
        ObservedZone::Field,
    )[0]
    .handle
    .clone();
    let program = checked(vec![
        look(1, 1),
        order(2, looked(1), Position::Top {}),
        draw(3, 1),
    ]);
    let question = start(&mut game, &program);
    let disclosed = visible(
        &game.observe(Viewer::P1).unwrap(),
        Viewer::P1,
        ObservedZone::Deck,
    )[0]
    .handle
    .clone();
    game.answer_internal(question.actor, question.token, Answer::Order(vec![0]))
        .unwrap_err();
    assert!(
        game.observe(Viewer::P1)
            .unwrap()
            .remembered
            .iter()
            .any(|memory| {
                memory.card == "unit-follower" && memory.face == 0 && memory.owner == Viewer::P1
            })
    );
    assert_eq!(
        visible(
            &game.observe(Viewer::P1).unwrap(),
            Viewer::P1,
            ObservedZone::Deck
        )
        .len(),
        0
    );
    assert_eq!(
        visible(
            &game.observe(Viewer::P1).unwrap(),
            Viewer::P2,
            ObservedZone::Field
        )[0]
        .handle,
        continuous
    );
    start(
        &mut game,
        &checked(vec![look(1, 1), order(2, looked(1), Position::Top {})]),
    );
    assert_ne!(
        visible(
            &game.observe(Viewer::P1).unwrap(),
            Viewer::P1,
            ObservedZone::Deck
        )[0]
        .handle,
        disclosed
    );
}

fn attack_and_finish(game: &mut Game) {
    game.internal_rule_action(&json!({"do":"attack","attacker":"s","target":"P2.leader"}))
        .unwrap();
    while let Some(question) = game.internal_quick_choice().unwrap() {
        game.answer_internal_card(question.actor, question.token, Answer::Quick(None))
            .unwrap();
    }
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}

#[test]
fn return_to_hand_then_origin_rollback_cannot_resurrect_the_opponents_old_handle() {
    let mut initial = setup(1, json!([{"id":"d0","card":"unit-follower"}]));
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
    let mut game = exhausted_deck(&initial);
    let empty = WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![],
    })
    .unwrap();
    game.load_internal_card("unit-follower", &empty).unwrap();
    attack_and_finish(&mut game);
    assert!(
        game.observe(Viewer::P2)
            .unwrap()
            .history
            .subjects
            .iter()
            .any(|fact| fact.current.is_some())
    );
    let old = visible(
        &game.observe(Viewer::P2).unwrap(),
        Viewer::P1,
        ObservedZone::Field,
    )[0]
    .handle
    .clone();
    let own = visible(
        &game.observe(Viewer::P1).unwrap(),
        Viewer::P1,
        ObservedZone::Field,
    )[0]
    .handle
    .clone();
    let program = checked(vec![
        movement::put(
            1,
            Selector::Objects {
                objects: vec![ObjectOperand::SourceHolder {}],
            },
            OwnerZone::Hand,
        ),
        look(2, 1),
        order(3, looked(2), Position::Top {}),
        draw(4, 1),
    ]);
    let question = start(&mut game, &program);
    let hidden = game.observe(Viewer::P2).unwrap();
    assert_eq!(visible(&hidden, Viewer::P1, ObservedZone::Field).len(), 0);
    assert_eq!(visible(&hidden, Viewer::P1, ObservedZone::Hand).len(), 0);
    assert_eq!(
        visible(
            &game.observe(Viewer::P1).unwrap(),
            Viewer::P1,
            ObservedZone::Hand
        )[0]
        .handle,
        own
    );
    game.answer_internal(question.actor, question.token, Answer::Order(vec![0]))
        .unwrap_err();
    assert_ne!(
        visible(
            &game.observe(Viewer::P2).unwrap(),
            Viewer::P1,
            ObservedZone::Field
        )[0]
        .handle,
        old
    );
    assert_eq!(
        visible(
            &game.observe(Viewer::P1).unwrap(),
            Viewer::P1,
            ObservedZone::Field
        )[0]
        .handle,
        own
    );
    let opponent_history = game.observe(Viewer::P2).unwrap().history;
    assert!(
        opponent_history
            .subjects
            .iter()
            .all(|fact| fact.current.is_none()),
        "published loss followed by origin rollback must not reconnect historical identities"
    );
    assert!(
        game.observe(Viewer::P1)
            .unwrap()
            .history
            .subjects
            .iter()
            .any(|fact| fact.current.as_ref() == Some(&own)),
        "the controller kept identifying the card throughout"
    );
}

#[test]
fn an_unpublished_revocation_before_origin_rollback_does_not_change_public_handles() {
    let mut initial = setup(1, json!([{"id":"d0","card":"unit-follower"}]));
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
    let mut game = exhausted_deck(&initial);
    let question = start(
        &mut game,
        &checked(vec![
            look(1, 1),
            order(2, looked(1), Position::Top {}),
            movement::put(
                3,
                Selector::Objects {
                    objects: vec![ObjectOperand::SourceHolder {}],
                },
                OwnerZone::Hand,
            ),
            draw(4, 1),
        ]),
    );
    let published = game.observe(Viewer::P2).unwrap();
    game.answer_internal(question.actor, question.token, Answer::Order(vec![0]))
        .unwrap_err();
    assert_eq!(game.observe(Viewer::P2).unwrap(), published);
}

#[test]
fn unpublished_conditional_mutations_do_not_change_observed_handles_on_failure() {
    let mut initial = setup(1, json!([{"id":"d0","card":"unit-follower"}]));
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
    let program = checked(vec![
        integer_choice(2, 1),
        node(Node::Control {
            control: Control::If {
                condition: Expr::Compute(Box::new(Predicate::CompareNat {
                    left: Expr::Bound(Binding::new(1, ScopeId(1))),
                    comparison: Comparison::Equal,
                    right: Expr::Literal(Nat(1)),
                })),
                then_body: body(
                    9,
                    vec![movement::put(
                        3,
                        Selector::Objects {
                            objects: vec![ObjectOperand::SourceHolder {}],
                        },
                        OwnerZone::Hand,
                    )],
                ),
                else_body: None,
            },
        }),
        draw(4, 1),
    ]);
    let mut observations = Vec::new();
    for answer in [0, 1] {
        let mut game = exhausted_deck(&initial);
        let question = start(&mut game, &program);
        assert!(game.internal_decision(View::P2).is_none());
        let published = game.observe(Viewer::P2).unwrap();
        game.answer_internal(question.actor, question.token, Answer::Integer(Nat(answer)))
            .unwrap_err();
        let after = game.observe(Viewer::P2).unwrap();
        assert_eq!(after, published);
        observations.push(after);
        let mut control = board(&initial);
        let question = start(&mut control, &program);
        assert!(
            control
                .answer_internal(question.actor, question.token, Answer::Integer(Nat(answer)))
                .unwrap()
                .complete
        );
        assert_eq!(
            observed_zone(
                &control.observe(Viewer::P2).unwrap(),
                Viewer::P1,
                ObservedZone::Field
            )
            .count,
            u64::from(answer == 0)
        );
    }
    assert_eq!(observations[0], observations[1]);
}

#[test]
fn effective_stats_posture_and_counters_use_the_existing_authority_readers() {
    let mut initial = setup(1, json!([]));
    initial["players"]["P2"]["zones"]["field"][0]["state"] =
        json!({"acted":true,"counters":{"sample_counter":2}});
    let mut game = board(&initial);
    game.execute_internal(
        &checked(vec![numeric::buff(1, 3, 2)]),
        &source("synthetic"),
        context(1),
    )
    .unwrap();
    for viewer in [Viewer::P1, Viewer::P2] {
        let observation = game.observe(viewer).unwrap();
        let card = visible(&observation, Viewer::P2, ObservedZone::Field)[0];
        let state = card.properties.state.as_ref().unwrap();
        assert_eq!(state.posture, Posture::Acted);
        assert_eq!(state.evolution, EvolutionState::Base);
        assert_eq!(state.stats.as_ref().unwrap().attack, 5);
        assert_eq!(state.stats.as_ref().unwrap().life, 5);
        assert_eq!(state.counters[0].name, "sample_counter");
        assert_eq!(state.counters[0].amount, 2);
        assert!(!state.boxed);
    }
}

#[test]
fn historical_memory_cannot_link_identical_copies_across_private_reordering() {
    let initial = setup(
        1,
        json!([
            {"id":"d0","card":"unit-follower"},
            {"id":"d1","card":"unit-follower"}
        ]),
    );
    let mut left = board(&initial);
    let mut right = left.clone();
    for (game, position) in [
        (&mut left, Position::Top {}),
        (&mut right, Position::Bottom {}),
    ] {
        let question = start(
            game,
            &checked(vec![look(1, 1), order(2, looked(1), position)]),
        );
        finish_order(game, &question);
        start(
            game,
            &checked(vec![look(1, 1), order(2, looked(1), Position::Top {})]),
        );
    }
    assert_eq!(
        left.observe(Viewer::P1).unwrap(),
        right.observe(Viewer::P1).unwrap()
    );
    assert_eq!(
        left.observe(Viewer::P2).unwrap(),
        right.observe(Viewer::P2).unwrap()
    );
}

#[test]
fn another_viewers_private_acquisition_does_not_shift_future_public_handles() {
    let initial = setup(1, json!([{"id":"d0","card":"unit-follower"}]));
    let mut left = board(&initial);
    let mut right = left.clone();
    let before = left.observe(Viewer::P2).unwrap();
    let question = start(
        &mut left,
        &checked(vec![look(1, 1), order(2, looked(1), Position::Top {})]),
    );
    assert_eq!(left.observe(Viewer::P2).unwrap(), before);
    finish_order(&mut left, &question);
    assert_eq!(left.observe(Viewer::P2).unwrap(), before);
    start(&mut left, &reveal_and_wait());
    start(&mut right, &reveal_and_wait());
    assert_eq!(
        left.observe(Viewer::P2).unwrap(),
        right.observe(Viewer::P2).unwrap()
    );
}

#[test]
fn shuffle_retires_known_handles_without_erasing_card_memory() {
    let mut game = board(&setup(1, json!([{"id":"d0","card":"unit-follower"}])));
    start(
        &mut game,
        &checked(vec![look(1, 1), order(2, looked(1), Position::Top {})]),
    );
    let known = game.observe(Viewer::P1).unwrap();
    let old = visible(&known, Viewer::P1, ObservedZone::Deck)[0]
        .handle
        .clone();
    let question = game.internal_decision(View::P1).unwrap();
    finish_order(&mut game, &question);
    game.execute_internal(
        &checked(vec![instruction(
            1,
            Action::Shuffle {
                deck: PlayerRef::Controller {},
                scope: ShuffleScope::FullZone {},
                randomness: RandomnessPolicy::FreshRequest,
            },
        )]),
        &source("synthetic"),
        context(1),
    )
    .unwrap();
    let hidden = game.observe(Viewer::P1).unwrap();
    assert_eq!(visible(&hidden, Viewer::P1, ObservedZone::Deck).len(), 0);
    assert_eq!(hidden.remembered, known.remembered);
    start(
        &mut game,
        &checked(vec![look(1, 1), order(2, looked(1), Position::Top {})]),
    );
    assert_ne!(
        visible(
            &game.observe(Viewer::P1).unwrap(),
            Viewer::P1,
            ObservedZone::Deck
        )[0]
        .handle,
        old
    );
}

#[test]
fn malformed_authority_returns_a_fixed_error_without_private_details() {
    let initial = setup(1, json!([]));
    let mut saved = serde_json::to_value(board(&initial)).unwrap();
    saved["state"]["objects"]["f"]["owner"] = json!("private-authority-detail");
    let game: Game = serde_json::from_value(saved).unwrap();
    for viewer in [Viewer::P1, Viewer::P2] {
        assert_eq!(
            game.observe(viewer).unwrap_err().to_string(),
            "invalid: player observation unavailable"
        );
    }
}

#[test]
fn published_memory_survives_rollback_for_a_previously_unknown_seat() {
    let mut initial = setup(1, json!([{"id":"d0","card":"unit-follower"}]));
    initial["players"]["P2"]["zones"]["field"] = json!([]);
    let mut game = exhausted_deck(&initial);
    assert_eq!(
        game.observe(Viewer::P2).unwrap().remembered,
        Vec::<CardMemory>::new()
    );
    let question = start(&mut game, &reveal_with_tail(vec![draw(4, 1)]));
    let revealed = game.observe(Viewer::P2).unwrap();
    let old = visible(&revealed, Viewer::P1, ObservedZone::Hand)[0]
        .handle
        .clone();
    assert_eq!(revealed.remembered.len(), 1);
    game.answer_internal(question.actor, question.token, Answer::Order(vec![0]))
        .unwrap_err();
    let rolled_back = game.observe(Viewer::P2).unwrap();
    assert_eq!(rolled_back.remembered, revealed.remembered);
    assert_eq!(
        visible(&rolled_back, Viewer::P1, ObservedZone::Hand),
        Vec::<&CardObservation>::new()
    );
    let next = start(&mut game, &reveal_and_wait());
    let again = game.observe(Viewer::P2).unwrap();
    assert_ne!(
        visible(&again, Viewer::P1, ObservedZone::Hand)[0].handle,
        old
    );
    finish_order(&mut game, &next);
}
