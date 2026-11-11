#![expect(
    clippy::panic,
    reason = "Capacity fixtures require their explicit question sequence."
)]
use super::*;
use sve_engine::game::internal::{DecisionToken, DecisionView, InputDomain};

fn question(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(question) = play else {
        panic!("capacity fixture question")
    };
    question
}

fn field_tree() -> Value {
    evolution_linked_movement::tree(&[(
        "activate",
        json!({
            "cost":{"pp":1},
            "targets":[{"bind":"returned","from":{"who":"self","zone":"cemetery","kind":"follower"},"n":2}],
            "do":[{"move":{"ref":"returned","to":"field","result":"entered"}},
                {"buff":{"on":{"follow":"entered"},"power":1,"hp":1}},
                {"buff":{"on":"this","power":1,"hp":1}}]
        }),
    )])
}

fn field_position(free: usize) -> Value {
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["pp"] = json!({"current":2,"max":2});
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    let mut field = vec![json!({"id":"source","card":"unit-follower"})];
    field.extend(
        (1..5_usize.saturating_sub(free))
            .map(|i| json!({"id":format!("old-{i}"),"card":"unit-follower"})),
    );
    initial["players"]["P1"]["zones"]["field"] = json!(field);
    initial["players"]["P1"]["zones"]["cemetery"] = json!([
        {"id":"grave-a","card":"unit-follower"},
        {"id":"grave-b","card":"unit-follower"}
    ]);
    initial
}

fn choose_field_capacity(
    game: &mut Game,
    targets_token: DecisionToken,
    choice: &DecisionView,
) -> CardPlay {
    let InputDomain::SearchPartition(domain) = &choice.domain else {
        panic!("capacity partition")
    };
    assert_eq!(
        domain.cardinality,
        Cardinality {
            minimum: Nat(1),
            maximum: Some(Nat(1))
        },
        "the single free field space requires exactly one arrival"
    );
    assert_eq!(
        domain.cards.len(),
        2,
        "both declared cemetery cards compete"
    );
    assert_eq!(
        domain
            .cards
            .iter()
            .filter_map(|card| card.object.as_deref())
            .collect::<Vec<_>>(),
        vec!["grave-a", "grave-b"],
        "public coordinates distinguish the two copies"
    );
    let before = game.projection(View::Referee).unwrap();
    assert_eq!(
        before["awaiting"]["domain"]["cards"][0]["object"], "grave-a",
        "the wire question links an already public candidate to its state"
    );
    assert_eq!(
        before["P1"]["pp"]["current"], 1,
        "the head fee remains paid while capacity is chosen"
    );
    assert_eq!(
        before["P1"]["cemetery"],
        json!(["grave-a", "grave-b"]),
        "neither candidate moves before the allocation"
    );
    for answer in [vec![], vec![0, 0], vec![0, 1], vec![99]] {
        game.answer_internal_card(choice.actor, choice.token, Answer::SearchPartition(answer))
            .unwrap_err();
        assert_eq!(
            game.projection(View::Referee).unwrap(),
            before,
            "invalid handles and cardinalities preserve the pending allocation"
        );
    }
    game.answer_internal_card(
        PlayerId { seat: 2 },
        choice.token,
        Answer::SearchPartition(vec![1]),
    )
    .unwrap_err();
    game.answer_internal_card(
        choice.actor,
        targets_token,
        Answer::SearchPartition(vec![1]),
    )
    .unwrap_err();
    assert_eq!(
        game.projection(View::Referee).unwrap(),
        before,
        "wrong actors and old tokens preserve the pending allocation"
    );
    game.answer_internal_card(choice.actor, choice.token, Answer::SearchPartition(vec![1]))
        .unwrap()
}

#[test]
fn field_capacity_is_a_resolution_choice_and_follow_buffs_only_successful_arrivals() {
    let program = lowerer()
        .lower(&field_tree())
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    for free in [0_usize, 1, 3] {
        let mut game = evolution_linked_movement::loaded(&field_position(free), &program);
        let targets = question(
            game.activate_internal_card(
                "source",
                program.card().abilities[0].node.id,
                PlayerId { seat: 1 },
            )
            .unwrap(),
        );
        let play = game
            .answer_internal_card(targets.actor, targets.token, Answer::Targets(vec![0, 1]))
            .unwrap();
        let play = if free == 1 {
            choose_field_capacity(&mut game, targets.token, &question(play))
        } else {
            play
        };
        resolved(play);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P1"]["field"].as_array().unwrap().len(),
            5_usize.saturating_sub(free) + free.min(2)
        );
        assert_eq!(
            state["objects"]["source"]["power"], 3,
            "later independent instructions still resolve"
        );
        assert_eq!(
            state["P1"]["pp"]["current"], 1,
            "full capacity never refunds the fee"
        );
        for (id, entered) in [("grave-a", free > 1), ("grave-b", free > 0)] {
            assert_eq!(
                state["P1"]["field"]
                    .as_array()
                    .unwrap()
                    .contains(&json!(id)),
                entered
            );
            assert_eq!(
                state["objects"][id]["power"],
                if entered { 3 } else { 2 },
                "only successful moves are followed"
            );
            assert_eq!(state["objects"][id]["generation"], u8::from(entered));
        }
    }
}

#[test]
fn player_pipeline_capacity_preserves_distinct_choices_without_physical_ids() {
    use crate::player_pipeline::{answer, question as player_question};
    use sve_engine::game::player::{QuestionShape, SubmissionKind, Viewer};

    let program = lowerer()
        .lower(&field_tree())
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    let mut game = evolution_linked_movement::loaded(&field_position(1), &program);
    game.activate_internal_card(
        "source",
        program.card().abilities[0].node.id,
        PlayerId { seat: 1 },
    )
    .unwrap();
    assert_eq!(
        answer(&mut game, Viewer::P1, Answer::Targets(vec![0, 1])).kind,
        SubmissionKind::Applied
    );
    let capacity = player_question(&game, Viewer::P1);
    let QuestionShape::Decision(InputDomain::SearchPartition(domain)) = &capacity.shape else {
        panic!("capacity question");
    };
    assert_eq!(domain.cards.len(), 2);
    assert_ne!(domain.cards[0].handle, domain.cards[1].handle);
    assert!(domain.cards.iter().all(|card| card.object.is_none()));
    assert!(!format!("{capacity:?}").contains("grave-"));
    assert_eq!(
        answer(&mut game, Viewer::P1, Answer::SearchPartition(vec![1])).kind,
        SubmissionKind::Applied
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["grave-b"]["power"], 3);
    assert_eq!(state["objects"]["grave-a"]["generation"], 0);
}

fn private_look_game() -> (Game, AbilityId) {
    let tree = evolution_linked_movement::tree(&[(
        "activate",
        json!({"cost":{"pp":0},"do":[
            {"look":{"bind":"seen","top":{"who":"self","n":2}}},
            {"choose":{"bind":"picked","from":{"in":"seen"},"upto":2}},
            {"move":{"ref":"picked","to":"ex","result":"moved"}},
            {"move":{"ref":{"in":"seen","except":{"in":{"objects_of":"moved","view":"before"}}},"to":"deck","position":"bottom","order":"chooser"}},
            {"apply":{"on":{"follow":"moved"},"effect":{"cost_mod":{"when_playing":{"in":"affected"},"set":0}},"until":"turn"}}
        ]}),
    )]);
    let program = lowerer()
        .lower(&tree)
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    let mut initial = field_position(3);
    initial["players"]["P1"]["pp"] = json!({"current":0,"max":0});
    initial["players"]["P1"]["zones"]["cemetery"] = json!([]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"private-a","card":"unit-follower"},{"id":"private-b","card":"unit-follower"}]);
    initial["players"]["P1"]["zones"]["ex"] = json!(
        (0..4)
            .map(|i| json!({"id":format!("old-ex-{i}"),"card":"unit-follower"}))
            .collect::<Vec<_>>()
    );
    let game = evolution_linked_movement::loaded(&initial, &program);
    (game, program.card().abilities[0].node.id)
}

#[test]
fn private_look_ex_capacity_leaves_the_unchosen_card_for_the_printed_remainder() {
    let (mut game, ability) = private_look_game();
    let chosen = question(
        game.activate_internal_card("source", ability, PlayerId { seat: 1 })
            .unwrap(),
    );
    let capacity = question(
        game.answer_internal_card(chosen.actor, chosen.token, Answer::Targets(vec![0, 1]))
            .unwrap(),
    );
    let InputDomain::SearchPartition(domain) = &capacity.domain else {
        panic!("private capacity partition")
    };
    assert_eq!(
        domain.cardinality,
        Cardinality {
            minimum: Nat(1),
            maximum: Some(Nat(1))
        }
    );
    assert!(
        domain.cards.iter().all(|card| card.object.is_none()),
        "private deck identities are never given to a player"
    );
    assert!(
        game.projection(View::P1).unwrap()["awaiting"]["domain"]["cards"]
            .as_array()
            .unwrap()
            .iter()
            .all(|card| !card.as_object().unwrap().contains_key("object")),
        "private allocation packets omit the optional public object coordinate"
    );
    let opponent = game.projection(View::P2).unwrap().to_string();
    assert!(
        !opponent.contains("private-a") && !opponent.contains("private-b"),
        "the opponent receives no private deck coordinates"
    );
    let mut play = game
        .answer_internal_card(
            capacity.actor,
            capacity.token,
            Answer::SearchPartition(vec![1]),
        )
        .unwrap();
    while let CardPlay::Awaiting(choice) = play {
        let InputDomain::Order(cards) = choice.domain else {
            panic!("only printed remainder ordering follows capacity")
        };
        play = game
            .answer_internal_card(
                choice.actor,
                choice.token,
                Answer::Order(cards.iter().map(|card| card.handle).collect()),
            )
            .unwrap();
    }
    resolved(play);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        state["P1"]["ex"].as_array().unwrap().last(),
        Some(&json!("private-b"))
    );
    assert_eq!(
        state["P1"]["deck"],
        json!(["private-a"]),
        "the unplaced looked card still follows the bottom instruction"
    );
    game.resume_internal_card().unwrap();
    game.play_internal_card("old-ex-0", PlayerId { seat: 1 })
        .unwrap_err();
    assert!(
        matches!(
            game.play_internal_card("private-b", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ),
        "the successful arrival alone gets the zero play cost"
    );
}

#[test]
fn field_capacity_is_saved_across_the_selected_entrants_ward_posture_choice() {
    let mut tree = field_tree();
    let mut ward = tree["cards"]["unit-follower"]["abilities"][0].clone();
    ward.as_object_mut().unwrap().remove("activate");
    ward["line"] = json!(2);
    ward["static"] = json!({"effects":[{"grant":{"keywords":["ward"]}}]});
    tree["cards"]["unit-follower"]["abilities"]
        .as_array_mut()
        .unwrap()
        .push(ward);
    let program = lowerer()
        .lower(&tree)
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    let mut game = evolution_linked_movement::loaded(&field_position(1), &program);
    let targets = question(
        game.activate_internal_card(
            "source",
            program.card().abilities[0].node.id,
            PlayerId { seat: 1 },
        )
        .unwrap(),
    );
    let capacity = question(
        game.answer_internal_card(targets.actor, targets.token, Answer::Targets(vec![0, 1]))
            .unwrap(),
    );
    assert!(
        matches!(capacity.domain, InputDomain::SearchPartition(_)),
        "capacity precedes the entry posture of any entrant"
    );
    let ward = question(
        game.answer_internal_card(
            capacity.actor,
            capacity.token,
            Answer::SearchPartition(vec![1]),
        )
        .unwrap(),
    );
    assert!(
        matches!(ward.domain, InputDomain::WardEntry { .. }),
        "only the chosen entrant asks for Ward posture"
    );
    let before = game.projection(View::Referee).unwrap();
    assert_eq!(
        before["P1"]["cemetery"],
        json!(["grave-a", "grave-b"]),
        "movement waits until posture is chosen"
    );
    game.answer_internal_card(ward.actor, ward.token, Answer::SearchPartition(vec![0]))
        .unwrap_err();
    assert_eq!(game.projection(View::Referee).unwrap(), before);
    resolved(
        game.answer_internal_card(ward.actor, ward.token, Answer::WardEntry(true))
            .unwrap(),
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["cemetery"], json!(["grave-a"]));
    assert_eq!(state["objects"]["grave-b"]["acted"], true);
    assert_eq!(state["objects"]["grave-b"]["power"], 3);
    assert_eq!(state["objects"]["grave-a"]["generation"], 0);
}

#[test]
fn two_remaining_field_spaces_reject_duplicate_handles_and_ignore_answer_order() {
    let mut tree = field_tree();
    tree["cards"]["unit-follower"]["abilities"][0]["activate"]["targets"][0]["n"] = json!(3);
    let program = lowerer()
        .lower(&tree)
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    let mut initial = field_position(2);
    initial["players"]["P1"]["zones"]["cemetery"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"grave-c","card":"unit-follower"}));
    let mut game = evolution_linked_movement::loaded(&initial, &program);
    let targets = question(
        game.activate_internal_card(
            "source",
            program.card().abilities[0].node.id,
            PlayerId { seat: 1 },
        )
        .unwrap(),
    );
    let capacity = question(
        game.answer_internal_card(targets.actor, targets.token, Answer::Targets(vec![0, 1, 2]))
            .unwrap(),
    );
    let InputDomain::SearchPartition(domain) = &capacity.domain else {
        panic!("two of three capacity allocation")
    };
    assert_eq!(
        domain.cardinality,
        Cardinality {
            minimum: Nat(2),
            maximum: Some(Nat(2))
        }
    );
    let before = game.projection(View::Referee).unwrap();
    game.answer_internal_card(
        capacity.actor,
        capacity.token,
        Answer::SearchPartition(vec![0, 0]),
    )
    .unwrap_err();
    assert_eq!(
        game.projection(View::Referee).unwrap(),
        before,
        "a repeated handle cannot fill two field spaces"
    );
    resolved(
        game.answer_internal_card(
            capacity.actor,
            capacity.token,
            Answer::SearchPartition(vec![2, 0]),
        )
        .unwrap(),
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        state["P1"]["field"],
        json!(["source", "old-1", "old-2", "grave-a", "grave-c"]),
        "allocation order does not reorder a simultaneous move"
    );
    assert_eq!(state["P1"]["cemetery"], json!(["grave-b"]));
    assert_eq!(state["objects"]["grave-a"]["power"], 3);
    assert_eq!(state["objects"]["grave-c"]["power"], 3);
}
