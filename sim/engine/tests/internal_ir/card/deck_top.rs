use super::*;
use sve_engine::ir::lowering::ExpandedLowerer;

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
        let error = ExpandedLowerer::new()
            .unwrap()
            .lower(&tree(&movement))
            .unwrap_err();
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
        movement::put(1, bottom, OwnerZone::Cemetery),
        movement::put(1, foreign, OwnerZone::Cemetery),
        movement::put(1, top, OwnerZone::Hand),
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
