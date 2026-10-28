use super::*;
use alloc::collections::BTreeMap;
use sve_engine::ir::lowering::ExpandedLowerer;

fn opponent() -> PlayerRef {
    PlayerRef::RequireOne {
        players: Box::new(PlayerSet {
            relation: PlayerRelation::Opponents,
            relative_to: PlayerRef::Controller {},
            cardinality: Cardinality {
                minimum: Nat(1),
                maximum: Some(Nat(1)),
            },
            text_quantifier: TextQuantifier::One,
        }),
    }
}
fn combo() -> Expr<Nat> {
    Expr::Live(Box::new(NatRead::Combo {
        player: PlayerRef::Controller {},
        execution: ExecutionRef::Current {},
    }))
}
fn played(id: u64, player: Option<PlayerRef>) -> Expr<Nat> {
    let event = Binding::new(id, ScopeId(id));
    Expr::Live(Box::new(NatRead::HistoryCount {
        query: HistoryQuery {
            pattern: EventPattern::Play {
                binding: event,
                predicate: player.map_or(Expr::Literal(Bool(true)), |player| {
                    Expr::Compute(Box::new(Predicate::PlayerEquals {
                        left: Expr::Frozen(Box::new(PlayerFrozen::PlayController { event })),
                        right: Expr::Literal(player),
                    }))
                }),
            },
            window: HistoryWindow {
                range: HistoryRange::CurrentTurn {},
                cutoff: HistoryCutoff::CurrentPoint {},
                relative_player: None,
            },
            include_execution: InclusionPolicy::IncludeAll {},
        },
        count: HistoryCount::Facts,
    }))
}
fn gain(id: u64, value: Expr<Nat>) -> Located<Node> {
    instruction(
        id,
        Action::Life {
            player: PlayerRef::Controller {},
            change: NumericChange::Add {
                amount: Expr::Compute(Box::new(IntOperation::FromNat { value })),
            },
        },
    )
}
fn spell() -> WellFormedCardProgram {
    let frozen = Binding::new(81, ScopeId(1));
    let committed = Binding::new(82, ScopeId(1));
    let mut ability = play::spell(vec![
        gain(1, Expr::Captured(CaptureRef { binding: frozen })),
        gain(2, Expr::Captured(CaptureRef { binding: committed })),
        gain(3, combo()),
        gain(4, played(1002, Some(PlayerRef::Controller {}))),
    ]);
    ability.node.play.as_mut().unwrap().captures = vec![
        Capture::Natural {
            at: CaptureTime::AtPlay {
                execution: ExecutionRef::Current {},
            },
            value: combo(),
            into: CaptureRef { binding: frozen },
        },
        Capture::Natural {
            at: CaptureTime::AtPlay {
                execution: ExecutionRef::Current {},
            },
            value: played(1001, Some(PlayerRef::Controller {})),
            into: CaptureRef { binding: committed },
        },
    ];
    card(vec![ability])
}

#[test]
fn turn_history_distinguishes_pending_combo_from_committed_cards_and_resets() {
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"s0","card":"unit-spell"},{"id":"s1","card":"unit-spell"},{"id":"fail","card":"unit-spell"}
    ]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"a","card":"unit-follower"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"b","card":"unit-follower"}]);
    initial["players"]["P2"]["zones"]["deck"] = json!([{"id":"next","card":"unit-spell"}]);
    let reader = payment::paid_ability(
        vec![],
        vec![
            gain(1, played(2001, Some(PlayerRef::Controller {}))),
            gain(2, played(2002, Some(opponent()))),
            gain(3, played(2003, None)),
        ],
    );
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "history-turn",
    )
    .unwrap();
    game.load_internal_card("unit-spell", &spell()).unwrap();
    game.load_internal_card("unit-follower", &card(vec![reader]))
        .unwrap();
    resolved(game.play_internal_card("s0", PlayerId { seat: 1 }).unwrap());
    game.resume_internal_card().unwrap();
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        23
    );
    resolved(
        game.activate_internal_card("a", AbilityId(1), PlayerId { seat: 1 })
            .unwrap(),
    );
    game.resume_internal_card().unwrap();
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        25
    );
    resolved(game.play_internal_card("s1", PlayerId { seat: 1 }).unwrap());
    game.resume_internal_card().unwrap();
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        32
    );
    let before = game.projection(View::Referee).unwrap();
    game.play_internal_card("fail", PlayerId { seat: 1 })
        .unwrap_err();
    assert_eq!(game.projection(View::Referee).unwrap(), before);
    resolved(
        game.activate_internal_card("a", AbilityId(1), PlayerId { seat: 1 })
            .unwrap(),
    );
    game.resume_internal_card().unwrap();
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        36
    );
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    quick::pass(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["turn"]["active"],
        "P2"
    );
    resolved(
        game.activate_internal_card("b", AbilityId(1), PlayerId { seat: 2 })
            .unwrap(),
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
        20
    );
}

#[test]
fn turn_history_lowering_accepts_public_play_counts_and_rejects_wider_queries() {
    let mut tree = fanfare::expanded::spell_tree().clone();
    let body = &mut tree["cards"]["SD03-005"]["abilities"][0]["spell"]["do"];
    *body = json!([{"if":{"cond":{"combo":3},"then":[{"draw":{"n":{"history_count":{"window":"this_turn","event":"played","who":"self"}}}}]}}]);
    let lowerer = ExpandedLowerer::new().unwrap();
    let costs = BTreeMap::from([("SD03-005".to_owned(), Nat(1))]);
    lowerer.lower_with_printed_costs(&tree, &costs).unwrap();
    for query in [
        json!({"window":"game","event":"played","who":"self"}),
        json!({"window":"this_turn","event":"ability_played"}),
        json!({"window":"this_turn","event":"played","sel":{"name":"secret"}}),
        json!({"window":"this_turn","event":"played","distinct":"subjects"}),
        json!({"window":"this_turn","event":"played","exclude":"this_execution"}),
    ] {
        let mut unsupported = tree.clone();
        unsupported["cards"]["SD03-005"]["abilities"][0]["spell"]["do"] =
            json!([{"draw":{"n":{"history_count":query}}}]);
        lowerer
            .lower_with_printed_costs(&unsupported, &costs)
            .unwrap_err();
    }
}
