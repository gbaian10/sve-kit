use super::*;

fn tree(movement: &Value) -> Value {
    let mut tree = fanfare::expanded::tree().clone();
    tree["cards"]["BP08-038"]["abilities"][0]["fanfare"]["do"] = json!([{"move":movement}]);
    tree
}

#[test]
fn deck_top_lowering_rejects_unadmitted_source_destination_and_followup() {
    for movement in [
        json!({"top":{"who":"opp","n":2},"to":"cemetery"}),
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
        movement::put(1, foreign.clone(), OwnerZone::Cemetery),
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
