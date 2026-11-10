use super::*;
use sve_engine::game::internal::{InputDomain, InstructionOutcome};

fn tree(limbs: &Value) -> Value {
    equipment_played::tree(
        &json!({"activate":{
            "cost":{"pp":0},
            "targets":[{"bind":"victim","from":{"who":"opp","zone":"field","kind":"follower"},"n":1}],
            "do":[{"damage":{"to":{"any_of":limbs},"n":1}}]
        }}),
        "unit-follower",
    )
}

#[test]
fn formal_target_and_source_receive_one_damage_batch_in_either_order() {
    for limbs in [json!(["victim", "this"]), json!(["this", "victim"])] {
        let program = lowerer()
            .lower(&tree(&limbs))
            .unwrap()
            .remove("unit-follower")
            .unwrap();
        let mut game = position_with_card(1, &program);
        let CardPlay::Awaiting(question) = game
            .activate_internal_card("s", AbilityId(1), PlayerId { seat: 1 })
            .unwrap()
        else {
            panic!("formal target question");
        };
        assert!(matches!(question.domain, InputDomain::Targets(_)));
        let outcome = resolved(
            game.answer_internal_card(question.actor, question.token, Answer::Targets(vec![0]))
                .unwrap(),
        );
        let hits = outcome
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
        assert_eq!(hits.len(), 2);
        let source = RuntimeDamageRecipient::Object {
            object: outcome.source.holder,
        };
        assert_eq!(
            hits.iter().filter(|hit| hit.recipient == source).count(),
            1,
            "exactly one hit belongs to the source holder s"
        );
        assert!(
            hits.iter().any(
                |hit| matches!(hit.recipient, RuntimeDamageRecipient::Object { .. })
                    && hit.recipient != source
            ),
            "the other hit belongs to the declared opposing follower"
        );
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["s"]["damage"], 1_u8);
        assert_eq!(state["objects"]["f"]["damage"], 1_u8);
        assert_eq!(
            outcome
                .progress
                .receipts
                .values()
                .filter(|receipt| matches!(receipt, InstructionOutcome::Damage(_)))
                .count(),
            1,
            "both hits belong to the same Damage instruction"
        );

        assert!(hits.iter().all(|hit| hit.amount.0.get() == 1));
    }
}

#[test]
fn source_pair_rejects_nonformal_or_nonsingle_follower_bindings() {
    for limbs in [
        json!(["this"]),
        json!(["this", "this"]),
        json!(["this", "victim", "victim"]),
        json!(["this",{"leader":"opp"}]),
        json!([{"any_of":["this","victim"]},{"leader":"opp"}]),
    ] {
        lowerer().lower(&tree(&limbs)).unwrap_err();
    }
    for case in 0..8 {
        let mut invalid = tree(&json!(["this", "victim"]));
        let head = &mut invalid["cards"]["unit-follower"]["abilities"][0]["activate"];
        match case {
            0 => {
                let target = head["targets"][0].clone();
                head.as_object_mut().unwrap().remove("targets");
                head["do"]
                    .as_array_mut()
                    .unwrap()
                    .insert(0, json!({"choose":target}));
            }
            1 => head["targets"][0]["n"] = json!(2),
            2 => head["targets"][0]["from"]["kind"] = json!("amulet"),
            3 => head["targets"][0]["from"]["zone"] = json!("cemetery"),
            4 => {
                head["targets"][0].as_object_mut().unwrap().remove("n");
                head["targets"][0]["upto"] = json!(1);
            }
            5 => head["targets"][0]["from"]["leader_or"] = json!(true),
            6 => head["targets"][0]["any"] = json!(true),
            _ => {
                head["targets"][0]["roles"] =
                    json!({"primary":{"if_nonempty":1},"rest":"remaining"});
            }
        }
        lowerer().lower(&invalid).unwrap_err();
    }
}
