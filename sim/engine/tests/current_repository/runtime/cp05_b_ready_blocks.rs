use super::*;

fn discard_owned_hand() -> Action {
    Action::Discard {
        player: PlayerRef::Controller {},
        mode: DiscardMode::All {
            eligible: Selector::Zone {
                players: PlayerSet {
                    relation: PlayerRelation::SelfPlayer,
                    relative_to: PlayerRef::Controller {},
                    cardinality: Cardinality {
                        minimum: Nat(1),
                        maximum: Some(Nat(1)),
                    },
                    text_quantifier: TextQuantifier::Implicit,
                },
                zone: ZoneKind::Hand,
                view: SelectionView::Current {},
                binding: Binding::new(400, ScopeId(1)),
                predicate: Box::new(Expr::Literal(Bool(true))),
            },
        },
    }
}

#[test]
fn manamel_damages_the_formal_target_and_its_leader_only_after_a_dark_discard() {
    for (discarded, follower_life, leader_life) in [
        ("CP05-017", 4_u8, 19_u8),
        ("BP01-037", 6_u8, 20_u8),
    ] {
        let mut initial = position_with_source("CP05-017", "field");
        initial["players"]["P1"]["zones"]["hand"] =
            json!([{"id":"discarded","card":discarded}]);
        initial["players"]["P2"]["zones"]["field"] = json!([
            {"id":"enemy","card":"BP01-037","state":{"hp":6_u8,"max_hp":6_u8}}
        ]);
        let mut game = with_producer(&initial, discard_owned_hand());
        run_grant_producer(&mut game);
        assert_eq!(
            game.internal_rule_action(&json!({"do":"end-phase"}))
                .unwrap()
                .outcome,
            "resolved",
            "Manamel's printed head waits at the owner's end phase",
        );
        resolve_grant_trigger(&mut game, 2, Some("BP01-037"));
        let view = state(&game);
        assert_eq!(view["objects"]["enemy"]["hp"], follower_life);
        assert_eq!(view["P2"]["leader"]["life"], leader_life);
    }
}
