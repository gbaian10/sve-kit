#![expect(
    clippy::panic,
    reason = "A missing decision invalidates the private choice scenarios."
)]
use super::*;
use alloc::collections::BTreeMap;
use sve_engine::game::internal::{DecisionView, InputDomain};
use sve_engine::game::player::{Input, Viewer};

fn tree(number: &str, head: &Value) -> Value {
    let mut tree = fanfare::expanded::tree().clone();
    let card = &tree["cards"]["BP08-038"];
    let meta = card["meta"].clone();
    let mut entry = card["abilities"][0].clone();
    entry.as_object_mut().unwrap().remove("fanfare");
    for (key, value) in head.as_object().unwrap() {
        entry[key] = value.clone();
    }
    tree["cards"] = json!({ number: {"meta":meta,"abilities":[entry]} });
    tree
}

fn awaiting(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(question) = play else {
        panic!("a decision must be offered");
    };
    question
}

fn pending(game: &mut Game) -> CardPlay {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("one pending occurrence");
    };
    game.answer_internal_card(
        pending.actor,
        pending.token,
        Answer::Pending(options[0].handle),
    )
    .unwrap()
}

const fn seat_name(seat: u32) -> &'static str {
    if seat == 1 { "P1" } else { "P2" }
}

const fn other(seat: u32) -> u32 {
    if seat == 1 { 2 } else { 1 }
}

fn fanfare(steps: &Value) -> Value {
    tree(
        "unit-follower",
        &json!({"fanfare":{
            "targets":[{"bind":"picked","from":{"who":"opp","zone":"field","kind":"follower"},"n":1}],
            "do":steps
        }}),
    )
}

fn hand_pick(bind: &str, by: &str, who: &str, upto: u8) -> Value {
    json!({"choose":{"bind":bind,"from":{"who":who,"zone":"hand","kind":"follower"},"upto":upto,"by":by}})
}

fn enter(bind: &str, result: &str) -> Value {
    json!({"move":{"ref":bind,"to":"field","result":result}})
}

/// BP07-050's shape: the chosen card's player may put a follower from their own hand.
fn hand_entry(by: &str, who: &str) -> Value {
    fanfare(&json!([
        {"move":{"ref":"picked","to":"deck","position":"bottom"}},
        hand_pick("put", by, who, 1),
        enter("put", "entered")
    ]))
}

/// `seat` plays `s`; the other seat holds `hand` and one field follower `f`.
fn position(programs: &BTreeMap<String, WellFormedCardProgram>, seat: u32, hand: &Value) -> Game {
    let (me, them) = (seat_name(seat), seat_name(other(seat)));
    let mut initial = setup(seat, json!([]));
    initial["turn"]["active"] = json!(me);
    initial["players"][me]["zones"]["hand"] = json!([{"id":"s","card":"unit-follower"}]);
    initial["players"][me]["pp"] = json!({"current":5,"max":5});
    initial["players"][them]["zones"]["hand"] = hand.clone();
    let mut game = Game::new(catalog(), &initial, &Value::Null, &Value::Null, "opp-hand").unwrap();
    game.load_internal_card("unit-follower", &programs["unit-follower"])
        .unwrap();
    for number in ["unit-spell", "unit-amulet"] {
        game.load_internal_card(number, &card(vec![])).unwrap();
    }
    game
}

fn targets(question: &DecisionView) -> Vec<u64> {
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("a target question");
    };
    domain.members.iter().map(|member| member.handle).collect()
}

fn pick_field_target(game: &mut Game, seat: u32) -> DecisionView {
    game.play_internal_card("s", PlayerId { seat }).unwrap();
    let target = awaiting(pending(game));
    let first = targets(&target)[0];
    awaiting(
        game.answer_internal_card(target.actor, target.token, Answer::Targets(vec![first]))
            .unwrap(),
    )
}

#[test]
fn only_the_holder_may_choose_from_a_private_hand() {
    lowerer().lower(&hand_entry("opp", "opp")).unwrap();
    lowerer().lower(&hand_entry("self", "self")).unwrap();
    for (by, who) in [("opp", "self"), ("self", "opp")] {
        if let Ok(programs) = lowerer().lower(&hand_entry(by, who)) {
            let mut game = Game::new(
                catalog(),
                &setup(1, json!([])),
                &Value::Null,
                &Value::Null,
                "foreign-hand",
            )
            .unwrap();
            game.load_internal_card("unit-follower", &programs["unit-follower"])
                .unwrap_err();
        }
    }
}

#[test]
fn the_opponent_puts_a_follower_from_their_own_hand_while_the_controller_waits() {
    let programs = lowerer().lower(&hand_entry("opp", "opp")).unwrap();
    for seat in [1_u32, 2] {
        let (me, them) = (seat_name(seat), seat_name(other(seat)));
        let (viewer, holder) = if seat == 1 {
            (Viewer::P1, Viewer::P2)
        } else {
            (Viewer::P2, Viewer::P1)
        };
        for accept in [true, false] {
            let mut game = position(
                &programs,
                seat,
                &json!([{"id":"h1","card":"unit-follower"},{"id":"h2","card":"unit-spell"}]),
            );
            let choice = pick_field_target(&mut game, seat);
            assert_eq!(
                choice.actor,
                PlayerId { seat: other(seat) },
                "the hand's holder chooses"
            );
            assert_eq!(targets(&choice).len(), 1, "only the follower qualifies");
            assert!(
                matches!(game.input(viewer), Input::Waiting(_)),
                "the controller sees no private candidates"
            );
            assert!(
                matches!(game.input(holder), Input::Question(_)),
                "the holder is asked"
            );
            let seen = game
                .projection(if seat == 1 { View::P1 } else { View::P2 })
                .unwrap()
                .to_string();
            assert!(
                !seen.contains("\"h1\"") && !seen.contains("\"h2\""),
                "the controller's projection carries no hidden hand identity"
            );
            let before = exact_authority(&game);
            game.answer_internal_card(
                PlayerId { seat },
                choice.token,
                Answer::Targets(vec![targets(&choice)[0]]),
            )
            .unwrap_err();
            assert_eq!(
                exact_authority(&game),
                before,
                "the controller cannot answer"
            );
            let picked = if accept {
                vec![targets(&choice)[0]]
            } else {
                vec![]
            };
            resolved(
                game.answer_internal_card(choice.actor, choice.token, Answer::Targets(picked))
                    .unwrap(),
            );
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state[them]["deck"],
                json!(["f"]),
                "seat={seat} accept={accept}"
            );
            assert_eq!(
                state[them]["field"],
                if accept { json!(["h1"]) } else { json!([]) },
                "seat={seat} accept={accept}"
            );
            assert_eq!(state[me]["field"], json!(["s"]));
        }
    }
}

#[test]
fn a_reused_name_or_an_inherited_pick_enters_the_field_of_its_own_selection() {
    // An optional's inner opponent pick never redirects a later own pick with the same name.
    let reuse = lowerer()
        .lower(&fanfare(&json!([
            {"optional":{"who":"opp","do":[hand_pick("x", "opp", "opp", 1), enter("x", "theirs")]}},
            {"choose":{"bind":"x","from":{"who":"self","zone":"hand","kind":"follower"},"upto":1}},
            enter("x", "mine")
        ])))
        .unwrap();
    for inner in [false, true] {
        let mut initial = setup(1, json!([]));
        initial["players"]["P1"]["zones"]["hand"] =
            json!([{"id":"s","card":"unit-follower"},{"id":"m","card":"unit-follower"}]);
        initial["players"]["P1"]["pp"] = json!({"current":5,"max":5});
        initial["players"]["P2"]["zones"]["hand"] = json!([{"id":"h1","card":"unit-follower"}]);
        let mut game = Game::new(catalog(), &initial, &Value::Null, &Value::Null, "reuse").unwrap();
        game.load_internal_card("unit-follower", &reuse["unit-follower"])
            .unwrap();
        let may = pick_field_target(&mut game, 1);
        let mut next = awaiting(
            game.answer_internal_card(may.actor, may.token, Answer::Integer(Nat(u64::from(inner))))
                .unwrap(),
        );
        if inner {
            next = awaiting(
                game.answer_internal_card(
                    next.actor,
                    next.token,
                    Answer::Targets(vec![targets(&next)[0]]),
                )
                .unwrap(),
            );
        }
        assert_eq!(
            next.actor,
            PlayerId { seat: 1 },
            "the outer pick is the controller's"
        );
        resolved(
            game.answer_internal_card(
                next.actor,
                next.token,
                Answer::Targets(vec![targets(&next)[0]]),
            )
            .unwrap(),
        );
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["field"], json!(["s", "m"]), "inner={inner}");
        assert_eq!(
            state["P2"]["field"],
            if inner {
                json!(["f", "h1"])
            } else {
                json!(["f"])
            },
            "inner={inner}"
        );
    }
    // Either branch consuming an outer opponent pick still enters the holder's field.
    let either = lowerer()
        .lower(&fanfare(&json!([
            hand_pick("x", "opp", "opp", 1),
            {"either":{"options":[[enter("x", "one")],[enter("x", "two")]]}}
        ])))
        .unwrap();
    for mode in [0_usize, 1] {
        let mut game = position(&either, 1, &json!([{"id":"h1","card":"unit-follower"}]));
        let pick = pick_field_target(&mut game, 1);
        let choice = awaiting(
            game.answer_internal_card(
                pick.actor,
                pick.token,
                Answer::Targets(vec![targets(&pick)[0]]),
            )
            .unwrap(),
        );
        let InputDomain::Modes { modes, .. } = &choice.domain else {
            panic!("either");
        };
        resolved(
            game.answer_internal_card(choice.actor, choice.token, Answer::Modes(vec![modes[mode]]))
                .unwrap(),
        );
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P2"]["field"], json!(["f", "h1"]), "mode={mode}");
    }
}

#[test]
fn a_multi_card_pick_is_capped_by_the_receiving_field() {
    // (controller field, holder field) -> how many the holder may put.
    let programs = lowerer()
        .lower(&tree(
            "unit-follower",
            &json!({"fanfare":{"do":[hand_pick("put", "opp", "opp", 2), enter("put", "entered")]}}),
        ))
        .unwrap();
    let fill = |prefix: &str, n: usize| {
        Value::Array(
            (0..n)
                .map(|i| json!({"id":format!("{prefix}{i}"),"card":"unit-follower"}))
                .collect(),
        )
    };
    for (mine, theirs, maximum) in [(4_usize, 0_usize, 2_u64), (0, 4, 1)] {
        let mut initial = setup(1, json!([]));
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"s","card":"unit-follower"}]);
        initial["players"]["P1"]["pp"] = json!({"current":5,"max":5});
        initial["players"]["P1"]["zones"]["field"] = fill("m", mine);
        initial["players"]["P2"]["zones"]["field"] = fill("t", theirs);
        initial["players"]["P2"]["zones"]["hand"] =
            json!([{"id":"h1","card":"unit-follower"},{"id":"h2","card":"unit-follower"}]);
        let mut game =
            Game::new(catalog(), &initial, &Value::Null, &Value::Null, "capacity").unwrap();
        game.load_internal_card("unit-follower", &programs["unit-follower"])
            .unwrap();
        game.play_internal_card("s", PlayerId { seat: 1 }).unwrap();
        let choice = awaiting(pending(&mut game));
        let InputDomain::Targets(domain) = &choice.domain else {
            panic!("own hand candidates");
        };
        assert_eq!(choice.actor, PlayerId { seat: 2 });
        assert_eq!(
            domain.cardinality.maximum,
            Some(Nat(maximum)),
            "mine={mine} theirs={theirs}"
        );
        let picked = targets(&choice)[..usize::try_from(maximum).unwrap()].to_vec();
        resolved(
            game.answer_internal_card(choice.actor, choice.token, Answer::Targets(picked))
                .unwrap(),
        );
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P2"]["field"].as_array().unwrap().len(),
            theirs + usize::try_from(maximum).unwrap()
        );
        assert_eq!(state["P1"]["field"].as_array().unwrap().len(), mine + 1);
    }
}

#[test]
fn the_holder_may_pick_a_follower_or_an_amulet_but_not_a_spell() {
    let union = |by: &str, who: &str| {
        fanfare(&json!([
            {"choose":{"bind":"put","by":by,"upto":1,"from":{"any_of":[
                {"who":who,"zone":"hand","kind":"follower"},
                {"who":who,"zone":"hand","kind":"amulet"}
            ]}}},
            enter("put", "entered")
        ]))
    };
    lowerer().lower(&union("self", "self")).unwrap();
    let programs = lowerer().lower(&union("opp", "opp")).unwrap();
    let mut game = position(
        &programs,
        1,
        &json!([
            {"id":"h1","card":"unit-follower"},
            {"id":"h2","card":"unit-amulet"},
            {"id":"h3","card":"unit-spell"}
        ]),
    );
    let choice = pick_field_target(&mut game, 1);
    assert_eq!(choice.actor, PlayerId { seat: 2 });
    let InputDomain::Targets(domain) = &choice.domain else {
        panic!("own hand candidates");
    };
    let cards = domain
        .members
        .iter()
        .map(|member| member.card.clone().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(cards.len(), 2, "follower and amulet, no spell: {cards:?}");
    assert!(cards.contains(&"unit-amulet".to_owned()), "{cards:?}");
    let amulet = domain
        .members
        .iter()
        .find(|member| member.card.as_deref() == Some("unit-amulet"))
        .unwrap()
        .handle;
    resolved(
        game.answer_internal_card(choice.actor, choice.token, Answer::Targets(vec![amulet]))
            .unwrap(),
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["field"], json!(["f", "h2"]));
}

#[test]
fn a_public_choice_is_answered_by_the_selection_time_controller() {
    let programs = lowerer()
        .lower(&tree(
            "unit-follower",
            &json!({"fanfare":{
                "targets":[{"bind":"victim","from":{"who":"opp","zone":"field","kind":"follower"},"n":1}],
                "do":[
                    {"choose":{"bind":"next","from":{"who":"opp","zone":"field","kind":"follower"},
                        "n":1,"by":{"master_of":"victim"}}},
                    {"damage":{"n":1,"to":"next"}}
                ]
            }}),
        ))
        .unwrap();
    for stolen in [false, true] {
        let mut initial = setup(1, json!([]));
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"s","card":"unit-follower"}]);
        initial["players"]["P1"]["pp"] = json!({"current":5,"max":5});
        initial["players"]["P2"]["zones"]["field"] = if stolen {
            json!([{"id":"f","card":"unit-follower","owner":"P1"}])
        } else {
            json!([{"id":"f","card":"unit-follower"}])
        };
        let mut game = Game::new(
            catalog(),
            &initial,
            &Value::Null,
            &Value::Null,
            "public-master",
        )
        .unwrap();
        game.load_internal_card("unit-follower", &programs["unit-follower"])
            .unwrap();
        let choice = pick_field_target(&mut game, 1);
        assert_eq!(choice.actor, PlayerId { seat: 2 }, "stolen={stolen}");
        let before = exact_authority(&game);
        game.answer_internal_card(
            PlayerId { seat: 1 },
            choice.token,
            Answer::Targets(vec![targets(&choice)[0]]),
        )
        .unwrap_err();
        assert_eq!(
            exact_authority(&game),
            before,
            "the effect controller cannot answer"
        );
        resolved(
            game.answer_internal_card(
                choice.actor,
                choice.token,
                Answer::Targets(vec![targets(&choice)[0]]),
            )
            .unwrap(),
        );
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["f"]["damage"],
            1
        );
    }
}

/// BP01-111's shape: the opponent reveals their hand, the controller picks, the holder discards.
fn revealed_discard() -> Value {
    tree(
        "unit-follower",
        &json!({"fanfare":{"do":[
            {"damage":{"n":2,"to":{"leader":"self"}}},
            {"reveal":{"bind":"shown","ref":{"who":"opp","zone":"hand"}}},
            {"choose":{"bind":"picked","from":{"in":"shown"},"n":1}},
            {"discard":{"ref":"picked","who":"opp"}}
        ]}}),
    )
}

#[test]
fn a_chosen_card_is_discarded_only_by_reference() {
    let discard = |value: Value| {
        tree(
            "unit-follower",
            &json!({"fanfare":{"do":[
                {"reveal":{"bind":"shown","ref":{"who":"opp","zone":"hand"}}},
                {"choose":{"bind":"picked","from":{"in":"shown"},"n":1}},
                {"discard":value}
            ]}}),
        )
    };
    lowerer()
        .lower(&discard(json!({"ref":"picked","who":"opp"})))
        .unwrap();
    assert_eq!(
        lowerer()
            .lower(&discard(json!({"ref":"missing","who":"opp"})))
            .unwrap_err()
            .code,
        "unsupported_construct",
        "an unbound reference names no card"
    );
    lowerer()
        .lower(&discard(json!({"ref":"picked","who":"opp","n":1})))
        .unwrap_err();
}

#[test]
fn the_controller_picks_from_a_revealed_hand_and_its_holder_discards_it() {
    let programs = lowerer().lower(&revealed_discard()).unwrap();
    for seat in [1_u32, 2] {
        let (me, them) = (seat_name(seat), seat_name(other(seat)));
        for picked in ["h1", "h2"] {
            let mut game = position(
                &programs,
                seat,
                &json!([{"id":"h1","card":"unit-follower"},{"id":"h2","card":"unit-spell"}]),
            );
            game.play_internal_card("s", PlayerId { seat }).unwrap();
            let choice = awaiting(pending(&mut game));
            assert_eq!(
                choice.actor,
                PlayerId { seat },
                "the effect controller picks"
            );
            let InputDomain::Targets(domain) = &choice.domain else {
                panic!("revealed candidates");
            };
            let card = if picked == "h1" {
                "unit-follower"
            } else {
                "unit-spell"
            };
            let handle = domain
                .members
                .iter()
                .find(|member| member.card.as_deref() == Some(card))
                .unwrap()
                .handle;
            assert_eq!(
                domain.members.len(),
                2,
                "every revealed card, spells included"
            );
            let before = exact_authority(&game);
            game.answer_internal_card(
                PlayerId { seat: other(seat) },
                choice.token,
                Answer::Targets(vec![handle]),
            )
            .unwrap_err();
            assert_eq!(exact_authority(&game), before, "the holder cannot pick");
            resolved(
                game.answer_internal_card(
                    choice.actor,
                    choice.token,
                    Answer::Targets(vec![handle]),
                )
                .unwrap(),
            );
            let state = game.projection(View::Referee).unwrap();
            let kept = if picked == "h1" { "h2" } else { "h1" };
            assert_eq!(state[them]["cemetery"], json!([picked]), "seat={seat}");
            assert_eq!(state[them]["hand"], json!([kept]), "seat={seat}");
            assert_eq!(state[me]["leader"]["life"], 18);
        }
    }
}

#[test]
fn an_empty_revealed_hand_still_lets_the_rest_resolve() {
    // Q340: the self damage happens even when the opponent has no hand.
    let programs = lowerer().lower(&revealed_discard()).unwrap();
    let mut game = position(&programs, 1, &json!([]));
    game.play_internal_card("s", PlayerId { seat: 1 }).unwrap();
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
    let pending_play = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending_play.domain else {
        panic!("one pending occurrence");
    };
    let played = game
        .answer_internal_card(
            pending_play.actor,
            pending_play.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap();
    let empty = awaiting(played);
    assert!(
        targets(&empty).is_empty(),
        "no card can be picked from an empty hand"
    );
    resolved(
        game.answer_internal_card(empty.actor, empty.token, Answer::Targets(vec![]))
            .unwrap(),
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["leader"]["life"], 18);
    assert_eq!(state["P2"]["cemetery"], json!([]));
}

/// Plays `s` for P1 against `hand` held by P2, resolving the fanfare up to its first question.
fn reveal_position(steps: &Value, hand: &Value, deck: &Value) -> (Game, DecisionView) {
    let programs = lowerer()
        .lower(&tree("unit-follower", &json!({"fanfare":{"do":steps}})))
        .unwrap();
    let mut initial = setup(1, deck.clone());
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"s","card":"unit-follower"}]);
    initial["players"]["P1"]["pp"] = json!({"current":5,"max":5});
    initial["players"]["P2"]["zones"]["hand"] = hand.clone();
    let mut game = Game::new(catalog(), &initial, &Value::Null, &Value::Null, "reveal").unwrap();
    game.load_internal_card("unit-follower", &programs["unit-follower"])
        .unwrap();
    for number in ["unit-spell", "unit-amulet"] {
        game.load_internal_card(number, &card(vec![])).unwrap();
    }
    game.play_internal_card("s", PlayerId { seat: 1 }).unwrap();
    let question = awaiting(pending(&mut game));
    (game, question)
}

fn handle_of(question: &DecisionView, card: &str) -> u64 {
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("a target question");
    };
    domain
        .members
        .iter()
        .find(|member| member.card.as_deref() == Some(card))
        .unwrap()
        .handle
}

fn cards_of(question: &DecisionView) -> Vec<String> {
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("a target question");
    };
    domain
        .members
        .iter()
        .map(|member| member.card.clone().unwrap())
        .collect()
}

const SHOWN: &str = r#"{"reveal":{"bind":"shown","ref":{"who":"opp","zone":"hand"}}}"#;

#[test]
fn the_revealed_hands_holder_may_be_the_chooser() {
    let steps = json!([
        serde_json::from_str::<Value>(SHOWN).unwrap(),
        {"choose":{"bind":"picked","from":{"in":"shown"},"n":1,"by":"opp"}},
        {"discard":{"ref":"picked","who":"opp"}}
    ]);
    let (mut game, choice) = reveal_position(
        &steps,
        &json!([{"id":"h1","card":"unit-follower"},{"id":"h2","card":"unit-spell"}]),
        &json!([]),
    );
    assert_eq!(choice.actor, PlayerId { seat: 2 }, "the holder chooses");
    let spell = handle_of(&choice, "unit-spell");
    let before = exact_authority(&game);
    game.answer_internal_card(
        PlayerId { seat: 1 },
        choice.token,
        Answer::Targets(vec![spell]),
    )
    .unwrap_err();
    assert_eq!(
        exact_authority(&game),
        before,
        "the effect controller cannot answer"
    );
    resolved(
        game.answer_internal_card(choice.actor, choice.token, Answer::Targets(vec![spell]))
            .unwrap(),
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["cemetery"], json!(["h2"]));
    assert_eq!(state["P2"]["hand"], json!(["h1"]));
}

#[test]
fn a_revealed_choice_excludes_the_sources_name_even_after_it_left() {
    for departed in [false, true] {
        let mut steps = vec![];
        if departed {
            steps.push(json!({"destroy":"this"}));
        }
        steps.push(serde_json::from_str::<Value>(SHOWN).unwrap());
        steps.push(json!({"choose":{"bind":"picked","from":{"in":"shown","not_same_name_as_this":true},"n":1}}));
        steps.push(json!({"discard":{"ref":"picked","who":"opp"}}));
        let (mut game, choice) = reveal_position(
            &Value::Array(steps),
            &json!([{"id":"h1","card":"unit-follower"},{"id":"h2","card":"unit-spell"}]),
            &json!([]),
        );
        assert_eq!(cards_of(&choice), ["unit-spell"], "departed={departed}");
        resolved(
            game.answer_internal_card(
                choice.actor,
                choice.token,
                Answer::Targets(vec![handle_of(&choice, "unit-spell")]),
            )
            .unwrap(),
        );
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P2"]["cemetery"],
            json!(["h2"]),
            "departed={departed}"
        );
    }
}

#[test]
fn revealed_cards_may_be_picked_in_groups() {
    let steps = json!([
        serde_json::from_str::<Value>(SHOWN).unwrap(),
        {"choose":{"bind":"picked","from":{"in":"shown"},
            "groups":[{"kind":"follower"},{"kind":"spell"}],"upto":2,"may":true}},
        {"discard":{"ref":"picked","who":"opp"}}
    ]);
    let hand = json!([
        {"id":"h1","card":"unit-follower"},
        {"id":"h2","card":"unit-spell"},
        {"id":"h3","card":"unit-amulet"}
    ]);
    let (mut game, choice) = reveal_position(&steps, &hand, &json!([]));
    assert_eq!(choice.actor, PlayerId { seat: 1 });
    let mut offered = cards_of(&choice);
    offered.sort();
    assert_eq!(
        offered,
        ["unit-follower", "unit-spell"],
        "an amulet fits no group"
    );
    let follower = handle_of(&choice, "unit-follower");
    let spell = handle_of(&choice, "unit-spell");
    resolved(
        game.answer_internal_card(
            choice.actor,
            choice.token,
            Answer::Targets(vec![follower, spell]),
        )
        .unwrap(),
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["cemetery"], json!(["h1", "h2"]));
    assert_eq!(state["P2"]["hand"], json!(["h3"]));
}

#[test]
fn only_cards_still_where_they_were_revealed_are_read_or_discarded() {
    // A card drawn after the reveal is never a member.
    let programs = lowerer()
        .lower(&tree(
            "unit-follower",
            &json!({"fanfare":{"do":[
                {"reveal":{"bind":"shown","ref":{"who":"self","zone":"hand"}}},
                {"draw":{"n":1}},
                {"choose":{"bind":"picked","from":{"in":"shown"},"n":1}},
                {"discard":{"ref":"picked"}}
            ]}}),
        ))
        .unwrap();
    let mut initial = setup(1, json!([{"id":"d0","card":"unit-spell"}]));
    initial["players"]["P1"]["zones"]["hand"] =
        json!([{"id":"s","card":"unit-follower"},{"id":"k","card":"unit-amulet"}]);
    initial["players"]["P1"]["pp"] = json!({"current":5,"max":5});
    let mut game = Game::new(catalog(), &initial, &Value::Null, &Value::Null, "drawn").unwrap();
    game.load_internal_card("unit-follower", &programs["unit-follower"])
        .unwrap();
    for number in ["unit-spell", "unit-amulet"] {
        game.load_internal_card(number, &card(vec![])).unwrap();
    }
    game.play_internal_card("s", PlayerId { seat: 1 }).unwrap();
    let choice = awaiting(pending(&mut game));
    assert_eq!(
        cards_of(&choice),
        ["unit-amulet"],
        "the drawn card was not revealed"
    );
    resolved(
        game.answer_internal_card(
            choice.actor,
            choice.token,
            Answer::Targets(targets(&choice)),
        )
        .unwrap(),
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["cemetery"], json!(["k"]));
    assert_eq!(state["P1"]["hand"], json!(["d0"]));
    // A chosen set loses members discarded meanwhile and discards only the rest.
    let (mut game, three) = reveal_position(
        &json!([
            serde_json::from_str::<Value>(SHOWN).unwrap(),
            {"choose":{"bind":"three","from":{"in":"shown"},"n":3}},
            {"choose":{"bind":"one","from":{"in":"shown"},"n":1}},
            {"discard":{"ref":"one","who":"opp"}},
            {"choose":{"bind":"rest","from":{"in":"shown"},"upto":3}},
            {"discard":{"ref":"three","who":"opp"}}
        ]),
        &json!([
            {"id":"h1","card":"unit-follower"},
            {"id":"h2","card":"unit-spell"},
            {"id":"h3","card":"unit-amulet"}
        ]),
        &json!([]),
    );
    let one = awaiting(
        game.answer_internal_card(three.actor, three.token, Answer::Targets(targets(&three)))
            .unwrap(),
    );
    let rest = awaiting(
        game.answer_internal_card(
            one.actor,
            one.token,
            Answer::Targets(vec![handle_of(&one, "unit-spell")]),
        )
        .unwrap(),
    );
    assert_eq!(
        cards_of(&rest).len(),
        2,
        "the discarded card is no longer a revealed member"
    );
    resolved(
        game.answer_internal_card(rest.actor, rest.token, Answer::Targets(vec![]))
            .unwrap(),
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        state["P2"]["cemetery"].as_array().unwrap().len(),
        3,
        "{state}"
    );
    assert_eq!(state["P2"]["hand"], json!([]));
}
