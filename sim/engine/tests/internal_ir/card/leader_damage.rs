use super::*;
use sve_engine::ir::lowering::ExpandedLowerer;

fn tree(recipient: &Value) -> Value {
    let mut tree = fanfare::expanded::tree().clone();
    let mut entry = tree["cards"]["BP08-038"]["abilities"][0].clone();
    entry.as_object_mut().unwrap().remove("fanfare");
    entry["activate"] = json!({"cost":{"pp":0},"do":[{"damage":{"to":recipient,"n":2}}]});
    let meta = tree["cards"]["BP08-038"]["meta"].clone();
    tree["cards"] = json!({"unit-follower":{"meta":meta,"abilities":[entry]}});
    tree
}

#[test]
fn synthetic_author_single_leader_recipients_damage_exactly_the_named_side() {
    let lowerer = ExpandedLowerer::new().unwrap();
    for seat in [1, 2] {
        for own in [true, false] {
            let program = lowerer
                .lower(&tree(&json!({"leader":if own {"self"} else {"opp"}})))
                .unwrap();
            let mut game = position_with_card(seat, &program["unit-follower"]);
            let result = activate(&mut game, seat);
            let hit_seat = if own {
                seat
            } else if seat == 1 {
                2
            } else {
                1
            };
            let hits = result
                .progress
                .facts
                .iter()
                .filter_map(|fact| {
                    if let Fact::Damage(hit) = fact {
                        Some(hit)
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();
            assert!(
                matches!(hits.as_slice(), [hit] if hit.recipient == (RuntimeDamageRecipient::Leader { player: PlayerId { seat: hit_seat } }) && hit.amount.0.get() == 2)
            );
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["P1"]["leader"]["life"],
                if hit_seat == 1 { 18 } else { 20 }
            );
            assert_eq!(
                state["P2"]["leader"]["life"],
                if hit_seat == 2 { 18 } else { 20 }
            );
        }
    }
}

#[test]
fn leader_lowering_rejects_player_names_in_the_wrong_recipient_domain() {
    let lowerer = ExpandedLowerer::new().unwrap();
    for recipient in [json!({"leader":"current"}), json!({"leaders":"self"})] {
        lowerer.lower(&tree(&recipient)).unwrap_err();
    }
}
