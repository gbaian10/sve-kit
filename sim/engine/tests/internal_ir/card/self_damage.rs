use super::*;

fn program(amount: u8) -> WellFormedCardProgram {
    lowerer()
        .lower(&evolution_linked_movement::tree(&[(
            "activate",
            json!({
                "cost":{"pp":0},"do":[{"damage":{"to":"this","n":amount}}]
            }),
        )]))
        .unwrap()
        .remove("unit-follower")
        .unwrap()
}

#[test]
fn source_damage_needs_no_target_and_keeps_the_original_controller_and_owner() {
    for seat in [1_u32, 2] {
        let actor = if seat == 1 { "P1" } else { "P2" };
        let other = if seat == 1 { "P2" } else { "P1" };
        for owner in [actor, other] {
            for amount in [1_u8, 3] {
                let program = program(amount);
                let mut initial = setup(seat, json!([]));
                initial["turn"]["active"] = json!(actor);
                initial["players"][actor]["zones"]["hand"] = json!([]);
                initial["players"][actor]["zones"]["field"] = json!([
                    {"id":"source","card":"unit-follower","owner":owner}
                ]);
                let mut game = evolution_linked_movement::loaded(&initial, &program);
                resolved(
                    game.activate_internal_card(
                        "source",
                        program.card().abilities[0].node.id,
                        PlayerId { seat },
                    )
                    .unwrap(),
                );
                assert!(game.internal_decision(View::P1).is_none());
                assert!(game.internal_decision(View::P2).is_none());
                while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
                    game.resume_internal_card().unwrap();
                }
                let view = game.projection(View::Referee).unwrap();
                assert_eq!(view[other]["field"], json!(["f"]));
                assert_eq!(view["objects"]["f"]["damage"], 0_u8);
                if amount == 1 {
                    assert_eq!(view[actor]["field"], json!(["source"]));
                    assert_eq!(view["objects"]["source"]["damage"], 1_u8);
                } else {
                    assert_eq!(view[owner]["cemetery"], json!(["source"]));
                    assert!(
                        !view[actor]["field"]
                            .as_array()
                            .unwrap()
                            .contains(&json!("source"))
                    );
                }
            }
        }
    }
}
