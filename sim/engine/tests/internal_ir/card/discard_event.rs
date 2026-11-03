use super::*;
use sve_engine::game::internal::InputDomain;

fn tree(query: &Value, hand: bool, ordinary: bool) -> Value {
    let mut tree = fanfare::expanded::tree().clone();
    let mut observer = tree["cards"]["BP01-041"].clone();
    let entry = &mut observer["abilities"][0];
    entry.as_object_mut().unwrap().remove("activate");
    entry["auto"] = json!({"on":{"discarded":query},"do":[{"life":{"who":"self","delta":1_u8}}]});
    let mut producer = tree["cards"]["BP01-041"].clone();
    let body = if ordinary {
        json!([
            {"choose":{"bind":"chosen","from":{"who":"self","zone":"hand"},"n":1_u8}},
            {"banish":{"ref":"chosen"}}
        ])
    } else {
        json!([{"discard":{"who":"self","all":true}}])
    };
    producer["abilities"][0]["activate"] = json!({"cost":{"pp":0_u8},"do":body});
    let observer_number = if hand { "unit-spell" } else { "unit-follower" };
    tree["cards"] = json!({observer_number:observer,"unit-amulet":producer});
    tree
}

fn fixture(tree: &Value, hand: bool, actor: u32) -> Game {
    fixture_for_discarder(tree, hand, actor, actor)
}

fn fixture_for_discarder(tree: &Value, hand: bool, actor: u32, discarder: u32) -> Game {
    let programs = lowerer().lower(tree).unwrap();
    let seat = if actor == 1 { "P1" } else { "P2" };
    let mut initial = setup(actor, json!([]));
    initial["turn"]["active"] = json!(seat);
    for player in ["P1", "P2"] {
        initial["players"][player]["zones"]["field"] = json!([]);
        initial["players"][player]["zones"]["hand"] = json!([]);
    }
    initial["players"][seat]["zones"]["field"] = json!([{"id":"p","card":"unit-amulet"}]);
    if !hand {
        initial["players"]["P1"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"s","card":"unit-follower"}));
    }
    let hand_seat = if discarder == 1 { "P1" } else { "P2" };
    initial["players"][hand_seat]["zones"]["hand"] = if hand {
        json!([{"id":"h","card":"unit-spell"},{"id":"other","card":"unit-follower"}])
    } else {
        json!([{"id":"h0","card":"unit-spell"},{"id":"h1","card":"unit-spell"}])
    };
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "discard-event",
    )
    .unwrap();
    let empty = card(vec![]);
    for number in ["unit-follower", "unit-spell", "unit-amulet"] {
        game.load_internal_card(number, programs.get(number).unwrap_or(&empty))
            .unwrap();
    }
    game
}

fn finish_check(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}

#[expect(
    clippy::panic,
    reason = "An unexpected domain invalidates the pending authority scenario."
)]
fn take_one(game: &mut Game, expected: usize) -> CardPlay {
    finish_check(game);
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("discard pending options")
    };
    assert_eq!(
        options.len(),
        expected,
        "each successful card has its own occurrence"
    );
    assert!(
        options.iter().all(|option| option.event == "Discard"),
        "no private object identity is exposed"
    );
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::Pending(options[0].handle),
    )
    .unwrap()
}

fn resolve_one(game: &mut Game, expected: usize) {
    resolved(take_one(game, expected));
}

fn agree(game: &mut Game, play: CardPlay) -> CardPlay {
    if let CardPlay::Awaiting(question) = play {
        assert_eq!(
            question.domain,
            InputDomain::OptionalCost,
            "only the printed head payment may intervene"
        );
        game.answer_internal_card(question.actor, question.token, Answer::OptionalCost(true))
            .unwrap()
    } else {
        play
    }
}

#[test]
fn committed_discard_is_per_card_and_filters_the_discarder_independently_of_turn() {
    for who in [Some("self"), Some("opp"), None] {
        for actor in [1_u32, 2_u32] {
            let mut query = json!({"from":"hand"});
            if let Some(who) = who {
                query["who"] = json!(who);
            }
            let mut game = fixture(&tree(&query, false, false), false, actor);
            resolved(
                game.activate_internal_card("p", AbilityId(1), PlayerId { seat: actor })
                    .unwrap(),
            );
            let eligible = who.is_none_or(|who| (who == "self") == (actor == 1));
            if eligible {
                resolve_one(&mut game, 2);
                resolve_one(&mut game, 1);
            }
            finish_check(&mut game);
            assert!(game.internal_pending_choice().unwrap().is_none());
            assert_eq!(
                game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
                if eligible { 22_u8 } else { 20_u8 }
            );
        }
    }
}

#[test]
fn own_hand_discard_freezes_the_departed_source_and_does_not_follow_its_new_generation() {
    let source = tree(
        &json!({"ref":"this","from":"hand","who":"self"}),
        true,
        false,
    );
    let mut game = fixture(&source, true, 1);
    let activation = resolved(
        game.activate_internal_card("p", AbilityId(1), PlayerId { seat: 1 })
            .unwrap(),
    );
    let discard = activation
        .progress
        .receipts
        .values()
        .find_map(|outcome| {
            if let InstructionOutcome::Discard(discard) = outcome {
                Some(discard)
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!(discard.moves.len(), 2);
    assert!(discard.moves.iter().all(|movement| {
        movement
            .after
            .is_some_and(|after| after.generation != movement.before.generation)
    }));
    resolve_one(&mut game, 1);
    finish_check(&mut game);
    assert!(game.internal_pending_choice().unwrap().is_none());
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["leader"]["life"], 21_u8);
    assert_eq!(state["P1"]["cemetery"], json!(["h", "other"]));
}

#[test]
fn ordinary_private_hand_banish_does_not_produce_a_discard_occurrence() {
    let source = tree(
        &json!({"ref":"this","from":"hand","who":"self"}),
        true,
        true,
    );
    let mut game = fixture(&source, true, 1);
    let CardPlay::Awaiting(question) = game
        .activate_internal_card("p", AbilityId(1), PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("private hand choice")
    };
    resolved(
        game.answer_internal_card(question.actor, question.token, Answer::Targets(vec![0]))
            .unwrap(),
    );
    finish_check(&mut game);
    assert!(game.internal_pending_choice().unwrap().is_none());
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        20_u8
    );
}

#[test]
fn discard_headers_reject_unimplemented_filters_and_nonself_hand_observers() {
    for query in [
        json!({"from":"cemetery"}),
        json!({"ref":"this","who":"opp"}),
        json!({"ref":"another"}),
        json!({"sel":{"kind":"follower"}}),
        json!({"during":{"turn":"self"}}),
        json!({"by":{"ability_of":{"who":"self","trait":"学院"}}}),
    ] {
        lowerer().lower(&tree(&query, false, false)).unwrap_err();
    }
    let mut hand = tree(&json!({"who":"self"}), false, false);
    hand["cards"]["unit-follower"]["abilities"][0]["auto"]["zone"] = json!("hand");
    lowerer().lower(&hand).unwrap_err();
    let programs = lowerer()
        .lower(&tree(&json!({"ref":"this"}), true, false))
        .unwrap();
    assert_eq!(
        programs["unit-spell"].card().abilities[0].node.active_zones,
        [ZoneKind::Hand]
    );
}

#[test]
fn simultaneous_discards_trigger_twice_but_the_source_can_act_to_pay_only_once() {
    let mut source = tree(&json!({"who":"self"}), false, false);
    source["cards"]["unit-follower"]["abilities"][0]["auto"]["do"] =
        json!([{"pay":{"cost":{"act_self":true},"do":[{"life":{"who":"self","delta":1_u8}}]}}]);
    let mut game = fixture(&source, false, 1);
    resolved(
        game.activate_internal_card("p", AbilityId(1), PlayerId { seat: 1 })
            .unwrap(),
    );
    let first = take_one(&mut game, 2);
    resolved(agree(&mut game, first));
    let second = take_one(&mut game, 1);
    let second = agree(&mut game, second);
    if !matches!(second, CardPlay::Cancelled) {
        resolved(second);
    }
    finish_check(&mut game);
    assert!(game.internal_pending_choice().unwrap().is_none());
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["leader"]["life"], 21_u8);
    assert_eq!(state["objects"]["s"]["acted"], true);
}

#[test]
fn own_discard_by_an_opponents_effect_triggers_on_the_opponents_turn() {
    let mut source = tree(&json!({"who":"self"}), false, false);
    source["cards"]["unit-amulet"]["abilities"][0]["activate"]["do"][0]["discard"]["who"] =
        json!("opp");
    let mut game = fixture_for_discarder(&source, false, 2, 1);
    resolved(
        game.activate_internal_card("p", AbilityId(1), PlayerId { seat: 2 })
            .unwrap(),
    );
    resolve_one(&mut game, 2);
    resolve_one(&mut game, 1);
    finish_check(&mut game);
    assert!(game.internal_pending_choice().unwrap().is_none());
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["leader"]["life"], 22_u8);
    assert_eq!(state["P2"]["leader"]["life"], 20_u8);
    assert_eq!(state["turn"]["active"], "P2");
}

#[test]
fn opponent_chosen_discard_keeps_the_choice_private_to_the_discarder() {
    let mut source = tree(&json!({"who":"self"}), false, false);
    source["cards"]["unit-amulet"]["abilities"][0]["activate"]["do"] =
        json!([{"discard":{"who":"opp","n":1_u8}}]);
    let mut game = fixture_for_discarder(&source, false, 2, 1);
    let CardPlay::Awaiting(question) = game
        .activate_internal_card("p", AbilityId(1), PlayerId { seat: 2 })
        .unwrap()
    else {
        panic!("the discarding opponent must choose their own card");
    };
    assert_eq!(question.actor, PlayerId { seat: 1 });
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("private discard candidates");
    };
    assert_eq!(
        domain.members.len(),
        2,
        "only the discarder sees both hand candidates"
    );
    assert!(game.internal_decision(View::P1).is_some());
    assert!(
        game.internal_decision(View::P2).is_none(),
        "the ability's controller cannot see the choice"
    );
    let before = game.projection(View::Referee).unwrap();
    game.answer_internal_card(
        PlayerId { seat: 2 },
        question.token,
        Answer::Targets(vec![0]),
    )
    .unwrap_err();
    assert_eq!(
        game.projection(View::Referee).unwrap(),
        before,
        "the wrong actor cannot change the pending choice"
    );
    resolved(
        game.answer_internal_card(question.actor, question.token, Answer::Targets(vec![0]))
            .unwrap(),
    );
    resolve_one(&mut game, 1);
    finish_check(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["cemetery"], json!(["h0"]));
    assert_eq!(state["P1"]["hand"], json!(["h1"]));
    assert_eq!(
        state["P1"]["leader"]["life"], 21_u8,
        "only the selected discard triggers"
    );
}
