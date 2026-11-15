use super::*;
use sve_engine::game::internal::InputDomain;

fn tree(delta: i64) -> Value {
    let mut tree = next_damage::tree(json!({"cost":{"pp":0},"do":[
        {"apply":{"on":{"who":"opp","zone":"field","kind":"follower"},
            "effect":{"damage_mod":{"delta":delta}},"until":"turn"}}
    ]}));
    tree["cards"]["unit-follower"]["abilities"][0]["line"] = json!(1);
    tree
}

fn board(seat: u32, reduction: bool) -> Game {
    let mut tree = tree(2);
    if reduction {
        let extra = equipment_played::tree(
            &json!({"line":2,"static":{"effects":[{"damage_mod":{"delta":-1}}]}}),
            "unit-follower",
        );
        tree["cards"]["unit-follower"]["abilities"]
            .as_array_mut()
            .unwrap()
            .push(extra["cards"]["unit-follower"]["abilities"][0].clone());
    }
    prepared_board(seat, &tree)
}

fn prepared_board(seat: u32, tree: &Value) -> Game {
    let programs = lowerer().lower(tree).unwrap();
    let own = if seat == 1 { "P1" } else { "P2" };
    let other = if seat == 1 { "P2" } else { "P1" };
    let mut initial = setup(seat, json!([]));
    initial["turn"]["active"] = json!(own);
    initial["players"][own]["zones"]["hand"] = json!([]);
    initial["players"][own]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
    initial["players"][other]["zones"]["field"] = json!([
        {"id":"f","card":"unit-follower","state":{"hp":40_i32}}
    ]);
    for player in [own, other] {
        initial["players"][player]["zones"]["deck"] = json!([
            {"id":format!("{player}-deck"),"card":"unit-spell"}
        ]);
    }
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "damage-increase",
    )
    .unwrap();
    game.load_internal_card("unit-follower", &programs["unit-follower"])
        .unwrap();
    game.load_internal_card("unit-spell", &card(vec![]))
        .unwrap();
    game
}

fn checks(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}

fn install(game: &mut Game, seat: u32) {
    activate(game, seat);
    checks(game);
}

fn opponent_zone(zone: ZoneKind) -> Selector {
    Selector::Zone {
        players: players(PlayerRelation::Opponents),
        zone,
        view: SelectionView::Current {},
        binding: Binding::new(9, ScopeId(if zone == ZoneKind::Field { 599 } else { 600 })),
        predicate: Box::new(Expr::Literal(Bool(true))),
    }
}

fn hit(amount: i64) -> WellFormedProgram {
    checked(vec![damage(
        100,
        RecipientSet::Objects {
            objects: opponent_zone(ZoneKind::Field),
        },
        amount,
    )])
}

#[test]
fn positive_received_damage_modifier_stacks_but_never_creates_a_zero_damage_hit() {
    for seat in [1, 2] {
        let mut game = board(seat, false);
        install(&mut game, seat);
        install(&mut game, seat);
        let zero = game
            .execute_internal(&hit(0), &source("synthetic"), context(seat))
            .unwrap();
        assert!(
            !zero
                .facts
                .iter()
                .any(|fact| matches!(fact, Fact::Damage(_)))
        );
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["f"]["hp"],
            40
        );
        game.execute_internal(&hit(1), &source("synthetic"), context(seat))
            .unwrap();
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["f"]["hp"],
            35
        );
        game.execute_internal(&hit(2), &source("synthetic"), context(seat))
            .unwrap();
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["f"]["hp"],
            29
        );
    }
}

#[test]
fn recipient_orders_increase_and_reduction_and_zero_ends_the_damage_event() {
    for seat in [1, 2] {
        for increase_first in [false, true] {
            let mut game = board(seat, true);
            install(&mut game, seat);
            let progress = game
                .execute_internal(&hit(1), &source("synthetic"), context(seat))
                .unwrap();
            assert!(!progress.complete);
            let question = game.internal_decision(View::Referee).unwrap();
            assert_eq!(
                question.actor,
                PlayerId {
                    seat: if seat == 1 { 2 } else { 1 }
                }
            );
            let InputDomain::DamageReplacements(options) = &question.domain else {
                panic!("recipient chooses replacement order");
            };
            assert_eq!(options.len(), 2);
            let increase = options
                .iter()
                .find(|option| option.line == Some(1))
                .unwrap()
                .handle;
            let reduction = options
                .iter()
                .find(|option| option.line == Some(2))
                .unwrap()
                .handle;
            let order = if increase_first {
                vec![increase, reduction]
            } else {
                vec![reduction, increase]
            };
            game.answer_internal(question.actor, question.token, Answer::Order(order))
                .unwrap();
            assert_eq!(
                game.projection(View::Referee).unwrap()["objects"]["f"]["hp"],
                if increase_first { 38 } else { 40 }
            );
        }
    }
}

#[test]
fn increase_survives_the_source_leaving_but_does_not_follow_a_reentered_target() {
    let mut game = board(1, false);
    install(&mut game, 1);
    let departure = checked(vec![movement::put(
        101,
        Selector::Objects {
            objects: vec![ObjectOperand::SourceHolder {}],
        },
        OwnerZone::Cemetery,
    )]);
    game.execute_internal(&departure, &source("synthetic"), context(1))
        .unwrap();
    let mut after_departure = context(1);
    after_departure.generation = Generation(1);
    game.execute_internal(&hit(1), &source("synthetic"), after_departure.clone())
        .unwrap();
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["f"]["hp"],
        37
    );
    let reentry = checked(vec![
        movement::put(102, opponent_zone(ZoneKind::Field), OwnerZone::Cemetery),
        instruction(
            103,
            Action::Move {
                objects: opponent_zone(ZoneKind::Cemetery),
                movement: MoveSpec {
                    to: MoveDestination::Shared {
                        zone: ZoneRef {
                            player: PlayerRef::RequireOne {
                                players: Box::new(PlayerSet {
                                    cardinality: Cardinality {
                                        minimum: Nat(1),
                                        maximum: Some(Nat(1)),
                                    },
                                    text_quantifier: TextQuantifier::One,
                                    ..players(PlayerRelation::Opponents)
                                }),
                            },
                            kind: ZoneKind::Field,
                        },
                    },
                    initial_posture: None,
                    entry_effects: vec![],
                    position: None,
                    order: None,
                },
                reason: MoveReason::Put,
            },
        ),
    ]);
    game.execute_internal(&reentry, &source("synthetic"), after_departure.clone())
        .unwrap();
    game.execute_internal(&hit(1), &source("synthetic"), after_departure)
        .unwrap();
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["objects"]["f"]["generation"], 2);
    assert_eq!(after["objects"]["f"]["hp"], 2);
}

#[test]
fn received_damage_increase_expires_at_the_end_of_the_installation_turn() {
    let mut game = board(1, false);
    install(&mut game, 1);
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    quick::pass(&mut game);
    game.execute_internal(&hit(1), &source("synthetic"), context(1))
        .unwrap();
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["f"]["hp"],
        39
    );
}

#[test]
fn object_increase_keeps_other_damage_modifier_shapes_closed() {
    let valid = tree(2);
    for (pointer, value) in [
        (
            "/cards/unit-follower/abilities/0/activate/do/0/apply/effect/damage_mod/delta",
            json!(-1),
        ),
        (
            "/cards/unit-follower/abilities/0/activate/do/0/apply/effect/damage_mod/delta",
            json!(0),
        ),
        (
            "/cards/unit-follower/abilities/0/activate/do/0/apply/until",
            json!("next_turn"),
        ),
    ] {
        let mut invalid = valid.clone();
        *invalid.pointer_mut(pointer).unwrap() = value;
        lowerer().lower(&invalid).unwrap_err();
    }
    for extra in [
        json!({"source":"this"}),
        json!({"kind":"ability"}),
        json!({"next_only":true}),
    ] {
        let mut invalid = valid.clone();
        invalid["cards"]["unit-follower"]["abilities"][0]["activate"]["do"][0]["apply"]["effect"]["damage_mod"]
            .as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
        lowerer().lower(&invalid).unwrap_err();
    }
}

#[test]
fn losing_target_abilities_does_not_remove_the_received_damage_increase() {
    let mut tree = tree(2);
    let activate = &mut tree["cards"]["unit-follower"]["abilities"][0]["activate"];
    activate["targets"] =
        json!([{"bind":"enemy","from":{"who":"opp","zone":"field","kind":"follower"},"n":1}]);
    activate["do"][0]["apply"]["on"] = json!("enemy");
    activate["do"]
        .as_array_mut()
        .unwrap()
        .push(json!({"apply":{"on":"enemy","effect":{"lose_abilities":{}},"until":"turn"}}));
    let mut game = prepared_board(1, &tree);
    let CardPlay::Awaiting(target) = game
        .activate_internal_card("s", AbilityId(1), PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("formal target");
    };
    resolved(
        game.answer_internal_card(target.actor, target.token, Answer::Targets(vec![0]))
            .unwrap(),
    );
    checks(&mut game);
    game.execute_internal(&hit(1), &source("synthetic"), context(1))
        .unwrap();
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["f"]["hp"],
        37
    );
}
