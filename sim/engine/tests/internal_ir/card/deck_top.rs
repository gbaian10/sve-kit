use super::*;

fn tree(movement: &Value) -> Value {
    let mut tree = fanfare::expanded::tree().clone();
    tree["cards"]["BP08-038"]["abilities"][0]["fanfare"]["do"] = json!([{"move":movement}]);
    tree
}

#[test]
fn deck_top_lowering_rejects_unadmitted_source_destination_and_followup() {
    for movement in [
        json!({"top":{"who":"opp","n":2},"to":"ex"}),
        json!({"top":{"who":{"owner_of":"unbound"},"n":1},"to":"cemetery"}),
        json!({"top":{"who":"each","n":1},"to":"cemetery"}),
        json!({"top":{"who":"opponents","n":1},"to":"ex"}),
        json!({"top":{"who":"self","n":2},"to":"hand"}),
        json!({"top":{"who":"self","n":0},"to":"cemetery"}),
        json!({"top":{"who":"self","n":2},"to":"cemetery","result":"m"}),
    ] {
        let error = lowerer().lower(&tree(&movement)).unwrap_err();
        assert_eq!(error.code, "unsupported_construct");
    }
}

#[test]
fn deck_top_authority_does_not_open_other_hidden_selector_shapes() {
    let top = Selector::Ordered {
        zone: ZoneRef {
            player: PlayerRef::Controller {},
            kind: ZoneKind::Deck,
        },
        start: Position::Top {},
        count: Expr::Literal(Nat(2)),
        direction: SliceDirection::Forward,
    };
    let mut bottom = top.clone();
    if let Selector::Ordered { start, .. } = &mut bottom {
        *start = Position::Bottom {};
    }
    let mut foreign = top.clone();
    if let Selector::Ordered { zone, .. } = &mut foreign {
        zone.player = PlayerRef::Active {};
    }
    let actions = [
        Action::Destroy {
            objects: top.clone(),
        },
        Action::Reveal {
            objects: top.clone(),
        },
    ];
    let mut steps = actions
        .into_iter()
        .map(|action| instruction(1, action))
        .collect::<Vec<_>>();
    steps.extend([
        movement::put(1, bottom.clone(), OwnerZone::Cemetery),
        movement::put(1, top, OwnerZone::Hand),
        movement::put(1, bottom.clone(), OwnerZone::Ex),
        movement::put(1, foreign.clone(), OwnerZone::Ex),
        instruction(1, Action::Banish { objects: bottom }),
        instruction(1, Action::Banish { objects: foreign }),
    ]);
    for step in steps {
        let mut game = Game::new(
            catalog(),
            &setup(1, json!([])),
            &Value::Null,
            &Value::Null,
            "top-spine",
        )
        .unwrap();
        let before = format!("{game:?}");
        game.load_internal_card("unit-follower", &card(vec![ability(1, vec![step])]))
            .unwrap_err();
        assert!(
            format!("{game:?}") == before,
            "rejection must not change authority"
        );
    }
}

const fn top(count: u64) -> Selector {
    Selector::Ordered {
        zone: ZoneRef {
            player: PlayerRef::Controller {},
            kind: ZoneKind::Deck,
        },
        start: Position::Top {},
        count: Expr::Literal(Nat(count)),
        direction: SliceDirection::Forward,
    }
}

fn position_with_top(step: Located<Node>, full_ex: bool) -> Game {
    let mut initial = setup(
        1,
        json!([
            {"id":"d0","card":"unit-amulet"},
            {"id":"d1","card":"unit-spell"},
            {"id":"tail","card":"unit-amulet"}
        ]),
    );
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
    if full_ex {
        initial["players"]["P1"]["zones"]["ex"] = json!(
            (0_u8..5)
                .map(|i| json!({"id":format!("e{i}"),"card":"unit-amulet"}))
                .collect::<Vec<_>>()
        );
    }
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "top-movement",
    )
    .unwrap();
    game.load_internal_card("unit-follower", &card(vec![ability(1, vec![step])]))
        .unwrap();
    for number in ["unit-spell", "unit-amulet"] {
        game.load_internal_card(number, &card(vec![])).unwrap();
    }
    game
}

#[test]
fn fixed_top_to_ex_preserves_order_without_observing_the_tail_and_respects_full_ex() {
    let mut game = position_with_top(movement::put(1, top(2), OwnerZone::Ex), false);
    assert!(
        matches!(
            game.activate_internal_card("s", AbilityId(1), PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Resolved(_)
        ),
        "fixed top movement does not ask a Look or arbitrary selection question"
    );
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(
        view["P1"]["ex"],
        json!(["d0", "d1"]),
        "the first two cards enter EX in deck order without a kind restriction"
    );
    assert_eq!(
        view["P1"]["deck"],
        json!(["tail"]),
        "the untouched tail remains in the deck"
    );
    let public = game.projection(View::P2).unwrap();
    assert!(
        !public["known_cards"]
            .as_array()
            .unwrap()
            .iter()
            .any(|card| card["id"] == "tail"),
        "direct top movement cannot publish the untouched tail"
    );
    let mut full = position_with_top(movement::put(1, top(2), OwnerZone::Ex), true);
    assert!(
        matches!(
            full.activate_internal_card("s", AbilityId(1), PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Resolved(_)
        ),
        "the common full-EX rule prevents the move without a Look"
    );
    let unchanged = full.projection(View::Referee).unwrap();
    assert_eq!(
        unchanged["P1"]["deck"],
        json!(["d0", "d1", "tail"]),
        "full EX leaves all intended cards in their original deck order"
    );
    assert_eq!(
        unchanged["P1"]["ex_count"], 5_u8,
        "the EX capacity is never exceeded"
    );
}

#[test]
fn fixed_top_banish_uses_available_cards_without_a_draw_or_look() {
    let mut game = position_with_top(instruction(1, Action::Banish { objects: top(5) }), false);
    assert!(
        matches!(
            game.activate_internal_card("s", AbilityId(1), PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Resolved(_)
        ),
        "banishing the fixed top slice does not ask a Look or draw question"
    );
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(
        view["P1"]["banish"],
        json!(["d0", "d1", "tail"]),
        "a short deck banishes every available card in original order"
    );
    assert_eq!(
        view["P1"]["deck_count"], 0_u8,
        "only actual deck cards move"
    );
    assert_eq!(
        view["P1"]["cemetery"],
        json!([]),
        "banishment does not become cemetery movement"
    );
    assert_ne!(
        view["game"]["ended"], true,
        "moving the last card is not a failed draw"
    );
}

#[test]
fn deck_top_lowering_accepts_ex_and_banish_but_rejects_other_top_banish_shapes() {
    assert!(
        lowerer()
            .lower(&tree(&json!({"top":{"who":"self","n":2},"to":"ex"})))
            .is_ok(),
        "the schema's existing fixed own top-to-EX syntax lowers through ordinary movement"
    );
    for banish in [
        json!({"top":{"who":"opp","n":1}}),
        json!({"top":{"who":"self","n":0}}),
        json!({"top":{"who":"self","n":1},"result":"unadmitted"}),
    ] {
        let mut input = tree(&json!({"top":{"who":"self","n":1},"to":"cemetery"}));
        input["cards"]["BP08-038"]["abilities"][0]["fanfare"]["do"] = json!([{"banish":banish}]);
        assert!(
            lowerer().lower(&input).is_err(),
            "top banishment does not admit foreign decks, zero counts or unimplemented result syntax"
        );
    }
}

#[test]
fn deck_top_lowering_reads_the_opponents_deck_only_for_cemetery_movement() {
    let program = lowerer()
        .lower(&tree(&json!({"top":{"who":"opp","n":2},"to":"cemetery"})))
        .unwrap();
    let text = format!("{program:?}");
    assert!(
        text.contains("RequireOne") && text.contains("Opponents"),
        "the opponent's slice names exactly one opponent instead of the controller"
    );
    let all = format!(
        "{:?}",
        lowerer()
            .lower(&tree(
                &json!({"top":{"who":"opponents","n":1},"to":"cemetery"})
            ))
            .unwrap()
    );
    assert!(
        all.contains("Opponents") && all.contains("text_quantifier: All"),
        "「相手プレイヤーすべて」 keeps its printed set inside the single-seat requirement"
    );
}

#[test]
fn opponent_deck_top_moves_to_its_owners_cemetery_without_publishing_the_tail() {
    let opponent = Selector::Ordered {
        zone: ZoneRef {
            player: PlayerRef::RequireOne {
                players: Box::new(PlayerSet {
                    relation: PlayerRelation::Opponents,
                    relative_to: PlayerRef::Controller {},
                    cardinality: Cardinality {
                        minimum: Nat(1),
                        maximum: Some(Nat(1)),
                    },
                    text_quantifier: TextQuantifier::One,
                }),
            },
            kind: ZoneKind::Deck,
        },
        start: Position::Top {},
        count: Expr::Literal(Nat(2)),
        direction: SliceDirection::Forward,
    };
    let mut initial = setup(1, json!([{"id":"own","card":"unit-amulet"}]));
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
    initial["players"]["P2"]["zones"]["deck"] = json!([
        {"id":"o0","card":"unit-amulet"},
        {"id":"o1","card":"unit-spell"},
        {"id":"otail","card":"unit-amulet"}
    ]);
    let mut game = Game::new(catalog(), &initial, &Value::Null, &Value::Null, "opp-top").unwrap();
    game.load_internal_card(
        "unit-follower",
        &card(vec![ability(
            1,
            vec![movement::put(1, opponent, OwnerZone::Cemetery)],
        )]),
    )
    .unwrap();
    for number in ["unit-spell", "unit-amulet"] {
        game.load_internal_card(number, &card(vec![])).unwrap();
    }
    assert!(
        matches!(
            game.activate_internal_card("s", AbilityId(1), PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Resolved(_)
        ),
        "the opponent's fixed top slice moves without a Look or choice"
    );
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P2"]["cemetery"], json!(["o0", "o1"]));
    assert_eq!(view["P2"]["deck"], json!(["otail"]));
    assert_eq!(
        view["P1"]["deck"],
        json!(["own"]),
        "the controller's deck is untouched"
    );
    assert_eq!(view["P1"]["cemetery"], json!([]));
    let public = game.projection(View::P1).unwrap();
    assert!(
        !public["known_cards"]
            .as_array()
            .unwrap()
            .iter()
            .any(|card| card["id"] == "otail"),
        "the untouched opponent tail stays hidden from the controller"
    );
}

/// A synthetic follower whose fanfare is `body`, lowered through the ordinary author path.
fn fanfare_follower(body: &Value) -> WellFormedCardProgram {
    let mut expanded = fanfare::expanded::tree().clone();
    expanded["cards"]["BP08-038"]["abilities"][0]["fanfare"] = body.clone();
    expanded["cards"] = json!({"unit-follower":expanded["cards"]["BP08-038"]});
    lowerer()
        .lower(&expanded)
        .unwrap()
        .remove("unit-follower")
        .unwrap()
}

fn cards(prefix: &str, count: usize) -> Value {
    json!(
        (0..count)
            .map(|n| json!({"id":format!("{prefix}{n}"),"card":"unit-amulet"}))
            .collect::<Vec<_>>()
    )
}

/// Plays the synthetic follower and answers its pending fanfare and any first-target question.
fn play_fanfare(initial: &Value, body: &Value) -> Value {
    let mut game = Game::new(catalog(), initial, &Value::Null, &Value::Null, "deck-top").unwrap();
    game.load_internal_card("unit-follower", &fanfare_follower(body))
        .unwrap();
    for number in ["unit-spell", "unit-amulet"] {
        game.load_internal_card(number, &card(vec![])).unwrap();
    }
    assert!(
        matches!(
            game.play_internal_card("s", PlayerId { seat: 1 }).unwrap(),
            CardPlay::Entered(_)
        ),
        "the follower enters before its fanfare resolves"
    );
    if let Some(pending) = game.internal_pending_choice().unwrap() {
        let mut step = game
            .answer_internal_card(pending.actor, pending.token, Answer::Pending(1))
            .unwrap();
        while let CardPlay::Awaiting(question) = step {
            step = game
                .answer_internal_card(question.actor, question.token, Answer::Targets(vec![0]))
                .unwrap();
        }
    }
    game.projection(View::Referee).unwrap()
}

/// P1 plays the follower; P2 has `deck` hidden cards and `grave` cemetery cards.
fn mill_position(deck: usize, grave: usize) -> Value {
    let mut initial = setup(1, cards("mine", 2));
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"s","card":"unit-follower"}]);
    initial["players"]["P2"]["zones"]["deck"] = cards("top", deck);
    initial["players"]["P2"]["zones"]["cemetery"] = cards("old", grave);
    initial
}

fn zone_len(view: &Value, seat: &str, zone: &str) -> usize {
    view[seat][zone].as_array().unwrap().len()
}

#[test]
fn opponent_mill_follow_up_reads_the_cemetery_after_the_move() {
    let body = json!({"do":[
        {"move":{"top":{"who":"opp","n":2},"to":"cemetery"}},
        {"if":{"cond":{"cmp":{"left":{"count":{"who":"opp","zone":"cemetery"}},"op":"ge","right":10}},
            "then":[{"draw":{"n":1}}]}}
    ]});
    for (grave, drawn) in [(7_usize, 0_usize), (8, 1)] {
        let view = play_fanfare(&mill_position(3, grave), &body);
        assert_eq!(view["P2"]["deck"], json!(["top2"]));
        assert_eq!(
            zone_len(&view, "P2", "cemetery"),
            grave.checked_add(2).unwrap()
        );
        assert_eq!(
            zone_len(&view, "P1", "hand"),
            drawn,
            "the condition sees the two milled cards"
        );
    }
}

#[test]
fn opponent_mill_takes_only_the_cards_a_short_or_empty_deck_has() {
    let body = json!({"do":[{"move":{"top":{"who":"opp","n":2},"to":"cemetery"}}]});
    for (deck, cemetery) in [(1_usize, json!(["top0"])), (0, json!([]))] {
        let view = play_fanfare(&mill_position(deck, 0), &body);
        assert_eq!(view["P2"]["cemetery"], cemetery);
        assert_eq!(view["P2"]["deck"], json!([]));
        assert_ne!(
            view["game"]["ended"], true,
            "Q696: milling a short or empty deck is not a failed draw"
        );
    }
}

#[test]
fn owner_of_mills_the_chosen_followers_owner_not_its_controller() {
    let body = json!({
        "targets":[{"bind":"enemy","from":{"who":"opp","zone":"field","kind":"follower"},"n":1}],
        "do":[
            {"damage":{"to":"enemy","n":2}},
            {"move":{"top":{"who":{"owner_of":"enemy"},"n":1},"to":"cemetery"}}
        ]
    });
    let mut initial = mill_position(1, 0);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"unit-follower","owner":"P1"}]);
    let view = play_fanfare(&initial, &body);
    assert_eq!(
        view["objects"]["enemy"]["damage"], 2_u8,
        "the chosen follower is damaged"
    );
    assert_eq!(
        view["P1"]["cemetery"],
        json!(["mine0"]),
        "P1 owns the follower"
    );
    assert_eq!(view["P1"]["deck"], json!(["mine1"]));
    assert_eq!(
        view["P2"]["deck"],
        json!(["top0"]),
        "the controller's deck is untouched"
    );
    assert_eq!(view["P2"]["cemetery"], json!([]));

    initial["players"]["P2"]["zones"]["field"] = json!([]);
    let untargeted = play_fanfare(&initial, &body);
    assert_eq!(
        untargeted["P1"]["deck"],
        json!(["mine0", "mine1"]),
        "no chosen follower, no owner"
    );
    assert_eq!(untargeted["P2"]["deck"], json!(["top0"]));
}
