use super::*;
use sve_battle_contract::{MAX_SAFE_INTEGER, observation as wire};
use sve_engine::battle::{ProjectionError, project_costs};
use sve_engine::game::player::{ObjectHandle, Viewer, observation as native};

pub(super) fn board(viewer: Viewer, secret: &str) -> Game {
    let mut initial = setup(viewer.player().seat, json!([]));
    let (actor, opponent) = if viewer == Viewer::P1 {
        ("P1", "P2")
    } else {
        ("P2", "P1")
    };
    initial["turn"]["active"] = json!(actor);
    initial["players"][actor]["zones"]["hand"] = json!([
        {"id":"quoted-a","card":"unit-follower"}, {"id":"quoted-b","card":"unit-follower"}
    ]);
    initial["players"][actor]["zones"]["ex"] = json!([{"id":"quoted-ex","card":"unit-follower"}]);
    initial["players"][actor]["zones"]["field"] =
        json!([{"id":"quote-source","card":"unit-follower"}]);
    initial["players"][opponent]["zones"]["hand"] = json!([{"id":"opponent-secret","card":secret}]);
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "wire-costs",
    )
    .unwrap();
    let empty = WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![],
    })
    .unwrap();
    for number in ["unit-follower", "unit-spell", "unit-amulet"] {
        game.load_internal_card(number, &empty).unwrap();
    }
    game
}

pub(super) fn object(
    observation: &native::Observation,
    seat: Viewer,
    zone: native::ObservedZone,
) -> ObjectHandle {
    observation
        .players
        .iter()
        .find(|player| player.player == seat)
        .unwrap()
        .zones
        .iter()
        .find(|entry| entry.kind == zone)
        .unwrap()
        .cards[0]
        .card
        .handle
        .clone()
}

fn summary(lines: Vec<native::CostLine>) -> native::CostSummary {
    native::CostSummary {
        basis: native::QuoteBasis::CurrentContext,
        total_pp: None,
        groups: vec![native::CostGroup {
            kind: native::CostGroupKind::Original,
            lines,
        }],
        pending: vec![native::PendingCost::Unavailable],
    }
}

#[test]
fn wire_cost_quotes_keep_distinct_copies_and_ignore_opponent_secrets_read_only() {
    for viewer in [Viewer::P1, Viewer::P2] {
        let game = board(viewer, "unit-spell");
        let other = board(viewer, "unit-amulet");
        let before = exact_authority(&game);
        let native = game.observe(viewer).unwrap();
        let projected = project_costs(&native).unwrap();
        assert_eq!(
            projected,
            project_costs(&other.observe(viewer).unwrap()).unwrap()
        );
        assert_eq!(projected.cards.len(), 3);
        assert_ne!(projected.cards[0].object, projected.cards[1].object);
        for quote in &projected.cards {
            assert_eq!(quote.cost.basis, wire::QuoteBasis::CurrentContext);
            assert_eq!(quote.cost.total_pp.unwrap().get(), 1);
            assert_eq!(quote.cost.pending, Vec::<wire::PendingCost>::new());
        }
        assert!(projected.current_play.is_none());
        let json = serde_json::to_string(&projected).unwrap();
        for private in [
            "quoted-a",
            "quoted-b",
            "quoted-ex",
            "quote-source",
            "opponent-secret",
            "generation",
            "unit-",
        ] {
            assert!(!json.contains(private));
        }
        assert_eq!(projected, project_costs(&native).unwrap());
        assert_eq!(exact_authority(&game), before);
    }
}

fn unresolved_cost() -> native::CostSummary {
    let mut cost = summary(vec![
        native::CostLine::Pp(native::PpQuote::Known(1)),
        native::CostLine::Pp(native::PpQuote::Variable {
            fixed: 2,
            copies: 2,
            x: None,
        }),
        native::CostLine::Pp(native::PpQuote::Variable {
            fixed: 0,
            copies: 1,
            x: Some(3),
        }),
        native::CostLine::Pp(native::PpQuote::Unavailable),
        native::CostLine::Posture { cards: None },
        native::CostLine::Life { change: Some(-3) },
        native::CostLine::Counter {
            name: "energy".into(),
            count: Some(0),
        },
        native::CostLine::Move { cards: Some(2) },
        native::CostLine::Discard {
            cards: None,
            selected: false,
        },
        native::CostLine::Banish { cards: Some(1) },
        native::CostLine::Reveal { cards: Some(0) },
        native::CostLine::Fusion {
            cards: Some(3),
            selected: true,
        },
        native::CostLine::EntryResources {
            cards: 2,
            use_ep: None,
        },
        native::CostLine::Other,
    ]);
    cost.basis = native::QuoteBasis::BeforePayment;
    for kind in [
        native::CostGroupKind::Additional,
        native::CostGroupKind::Mode,
        native::CostGroupKind::Optional {
            required: false,
            accepted: None,
        },
        native::CostGroupKind::Optional {
            required: false,
            accepted: Some(false),
        },
        native::CostGroupKind::Optional {
            required: true,
            accepted: Some(true),
        },
    ] {
        cost.groups.push(native::CostGroup {
            kind,
            lines: vec![],
        });
    }
    cost.pending = vec![
        native::PendingCost::X,
        native::PendingCost::Optional,
        native::PendingCost::Modes,
        native::PendingCost::Materials,
        native::PendingCost::Exchange,
        native::PendingCost::EntryResources,
        native::PendingCost::Unavailable,
    ];
    cost
}

#[test]
fn wire_cost_components_preserve_unknowns_declarations_and_payment_status() {
    let mut observed = board(Viewer::P1, "unit-spell").observe(Viewer::P1).unwrap();
    let holder = object(&observed, Viewer::P1, native::ObservedZone::Field);
    let cost = unresolved_cost();
    observed.costs.cards.clear();
    observed.costs.current_play = Some(native::PlayCostQuote {
        source: Some(holder.clone()),
        cost,
    });
    let projected = project_costs(&observed).unwrap();
    let play = projected.current_play.unwrap();
    assert_eq!(play.source.unwrap().as_str(), holder.as_str());
    assert_eq!(
        serde_json::to_value(play.cost).unwrap(),
        json!({
            "basis":"before_payment", "total_pp":null,
            "groups":[
                {"role":{"kind":"original"},"lines":[
                    {"kind":"pp","quote":{"kind":"known","amount":1}},
                    {"kind":"pp","quote":{"kind":"variable","fixed":2,"copies":2,"x":null}},
                    {"kind":"pp","quote":{"kind":"variable","fixed":0,"copies":1,"x":3}},
                    {"kind":"pp","quote":{"kind":"unavailable"}},
                    {"kind":"posture","cards":null},{"kind":"life","change":-3},
                    {"kind":"counter","name":"energy","count":0},{"kind":"move","cards":2},
                    {"kind":"discard","cards":null,"selected":false},{"kind":"banish","cards":1},
                    {"kind":"reveal","cards":0},{"kind":"fusion","cards":3,"selected":true},
                    {"kind":"entry_resources","cards":2,"use_ep":null},{"kind":"other"}
                ]},
                {"role":{"kind":"additional"},"lines":[]},
                {"role":{"kind":"mode"},"lines":[]},
                {"role":{"kind":"optional","required":false,"accepted":null},"lines":[]},
                {"role":{"kind":"optional","required":false,"accepted":false},"lines":[]},
                {"role":{"kind":"optional","required":true,"accepted":true},"lines":[]}
            ],
            "pending":["x","optional","modes","materials","exchange","entry_resources","unavailable"]
        })
    );
    observed.costs.current_play.as_mut().unwrap().source = None;
    assert!(
        project_costs(&observed)
            .unwrap()
            .current_play
            .unwrap()
            .source
            .is_none()
    );
}

#[test]
fn wire_costs_refuse_duplicate_cross_zone_foreign_and_stale_references() {
    let game = board(Viewer::P1, "unit-spell");
    let observed = game.observe(Viewer::P1).unwrap();
    let foreign = game.observe(Viewer::P2).unwrap();
    for (source, owner, zone) in [
        (&observed, Viewer::P1, native::ObservedZone::Field),
        (&observed, Viewer::P2, native::ObservedZone::Field),
        (&foreign, Viewer::P2, native::ObservedZone::Hand),
    ] {
        let mut bad = observed.clone();
        bad.costs.cards[0].card = object(source, owner, zone);
        assert_eq!(
            project_costs(&bad).unwrap_err(),
            ProjectionError::Unavailable
        );
    }
    let mut duplicate = observed.clone();
    duplicate.costs.cards.push(duplicate.costs.cards[0].clone());
    assert_eq!(
        project_costs(&duplicate).unwrap_err(),
        ProjectionError::Unavailable
    );
    let mut stale = observed.clone();
    let handle = stale.costs.cards[0].card.clone();
    for player in &mut stale.players {
        for zone in &mut player.zones {
            zone.cards.retain(|card| card.card.handle != handle);
        }
    }
    assert_eq!(
        project_costs(&stale).unwrap_err(),
        ProjectionError::Unavailable
    );
    stale.costs.cards.clear();
    let mut cost = summary(vec![]);
    cost.basis = native::QuoteBasis::BeforePayment;
    stale.costs.current_play = Some(native::PlayCostQuote {
        source: Some(handle),
        cost,
    });
    assert_eq!(
        project_costs(&stale).unwrap_err(),
        ProjectionError::Unavailable
    );
    let mut wrong_basis = observed.clone();
    wrong_basis.costs.cards[0].cost.basis = native::QuoteBasis::BeforePayment;
    assert_eq!(
        project_costs(&wrong_basis).unwrap_err(),
        ProjectionError::Unavailable
    );
    let mut ambiguous = observed.clone();
    let member = ambiguous.players[0]
        .zones
        .iter()
        .find_map(|zone| zone.cards.first())
        .unwrap()
        .clone();
    ambiguous.players[0].zones[0].cards.push(member);
    assert_eq!(
        project_costs(&ambiguous).unwrap_err(),
        ProjectionError::Unavailable
    );
    let mut wrong_viewer = observed.clone();
    wrong_viewer.viewer = Viewer::P2;
    assert_eq!(
        project_costs(&wrong_viewer).unwrap_err(),
        ProjectionError::Unavailable
    );
    let mut wrong_seats = observed;
    wrong_seats.players.swap(0, 1);
    assert_eq!(
        project_costs(&wrong_seats).unwrap_err(),
        ProjectionError::Unavailable
    );
}

#[test]
fn wire_costs_check_every_numeric_component_without_clamping() {
    let observed = board(Viewer::P1, "unit-spell").observe(Viewer::P1).unwrap();
    let too_large = MAX_SAFE_INTEGER.checked_add(1).unwrap();
    for line in [
        native::CostLine::Pp(native::PpQuote::Known(i64::MAX)),
        native::CostLine::Pp(native::PpQuote::Variable {
            fixed: i64::MIN,
            copies: 1,
            x: None,
        }),
        native::CostLine::Pp(native::PpQuote::Variable {
            fixed: 0,
            copies: too_large,
            x: None,
        }),
        native::CostLine::Pp(native::PpQuote::Variable {
            fixed: 0,
            copies: 1,
            x: Some(too_large),
        }),
        native::CostLine::Posture {
            cards: Some(too_large),
        },
        native::CostLine::Life {
            change: Some(i64::MIN),
        },
        native::CostLine::Counter {
            name: "energy".into(),
            count: Some(too_large),
        },
        native::CostLine::Move {
            cards: Some(too_large),
        },
        native::CostLine::Discard {
            cards: Some(too_large),
            selected: false,
        },
        native::CostLine::Banish {
            cards: Some(too_large),
        },
        native::CostLine::Reveal {
            cards: Some(too_large),
        },
        native::CostLine::Fusion {
            cards: Some(too_large),
            selected: true,
        },
        native::CostLine::EntryResources {
            cards: too_large,
            use_ep: Some(true),
        },
    ] {
        let mut overflow = observed.clone();
        overflow.costs.cards[0].cost = summary(vec![line]);
        assert_eq!(
            project_costs(&overflow).unwrap_err(),
            ProjectionError::Range
        );
    }
    let mut total = observed;
    total.costs.cards[0].cost.total_pp = Some(i64::MAX);
    assert_eq!(project_costs(&total).unwrap_err(), ProjectionError::Range);
    total.costs.cards[0].cost.total_pp = Some(i64::try_from(MAX_SAFE_INTEGER).unwrap());
    assert_eq!(
        project_costs(&total).unwrap().cards[0]
            .cost
            .total_pp
            .unwrap()
            .get(),
        i64::try_from(MAX_SAFE_INTEGER).unwrap()
    );
}
