use super::*;

#[test]
fn bp04_103_prevents_repeated_ability_hits_to_the_leader_and_captured_followers() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"ally","card":"BP01-037","state":{"hp":10_u8,"max_hp":10_u8}}]);
    initial["players"]["P1"]["zones"]["hand"] =
        json!([{"id":"source","card":"BP04-103"},{"id":"late","card":"BP01-037"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"own-draw","card":"BP01-037"}]);
    initial["players"]["P2"]["zones"]["deck"] = json!([{"id":"opp-draw","card":"BP01-037"}]);
    let hit = |recipients| Action::Damage {
        recipients,
        amount: DamageAmount::Uniform {
            amount: Expr::Literal(Int(1)),
        },
        flags: DamageFlags::Ability,
        source: ObjectView::SourceInformation {},
        batch: BatchPolicy::Instruction {},
    };
    let program = producer_steps(vec![
        hit(RecipientSet::Leaders {
            players: PlayerSet {
                relation: PlayerRelation::SelfPlayer,
                relative_to: PlayerRef::Controller {},
                cardinality: Cardinality {
                    minimum: Nat(1),
                    maximum: Some(Nat(1)),
                },
                text_quantifier: TextQuantifier::One,
            },
        }),
        hit(RecipientSet::Objects {
            objects: subjects(),
        }),
    ]);
    let mut game = with_selected_producer_program(&initial, &program, producer_selection());
    play_grant_source(&mut game, None);
    resolve_grant_trigger(&mut game, 1, None);
    assert!(matches!(
        game.play_internal_card("late", actor()).unwrap(),
        CardPlay::Entered(_)
    ));
    finish_checks(&mut game);
    resolve_grant_trigger(&mut game, 1, None);
    for _ in 0_u8..2 {
        activate(&mut game, "P1");
        finish_checks(&mut game);
    }
    let protected = game.projection(View::Referee).unwrap();
    assert_eq!(protected["P1"]["leader"]["life"], 20_u8);
    assert_eq!(protected["objects"]["ally"]["damage"], 0_u8);
    assert_eq!(protected["objects"]["source"]["damage"], 0_u8);
    assert_eq!(protected["objects"]["late"]["damage"], 2_u8);

    for _ in 0_u8..2 {
        next_turn(&mut game);
        finish_checks(&mut game);
    }
    activate(&mut game, "P1");
    let expired = game.projection(View::Referee).unwrap();
    assert_eq!(expired["P1"]["leader"]["life"], 19_u8);
    assert_eq!(expired["objects"]["ally"]["damage"], 1_u8);
}
