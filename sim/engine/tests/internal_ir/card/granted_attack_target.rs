use super::*;
use sve_engine::game::internal::InputDomain;

#[test]
fn granted_attack_damage_uses_the_original_attack_recipient_and_attacker_controller() {
    let tree = equipment_played::tree(
        &json!({"static":{"effects":[{"grant":{
            "on":{"who":"self","zone":"field","kind":"follower"},"abilities":[{"attack_time":{"do":[
                {"if":{"cond":{"is":{"ref":"attack_target","kind":"follower"}},"then":[{"damage":{"to":"attack_target","n":2}}]}}
            ]}}]
        }}]}}),
        "unit-amulet",
    );
    let provider = lowerer()
        .lower(&tree)
        .unwrap()
        .remove("unit-amulet")
        .unwrap();
    for leader in [false, true] {
        let mut initial = setup(1, json!([]));
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] = json!([
            {"id":"aura","card":"unit-amulet"},
            {"id":"attacker","card":"unit-follower","owner":"P2","state":{"entered_this_turn":false}}
        ]);
        initial["players"]["P2"]["zones"]["field"] = json!([
            {"id":"defender","card":"unit-follower","state":{"acted":true}}
        ]);
        let mut game = Game::new(
            catalog(),
            &initial,
            &Value::Null,
            &Value::Null,
            "granted-attack-target",
        )
        .unwrap();
        game.load_internal_card("unit-amulet", &provider).unwrap();
        game.load_internal_card("unit-follower", &card(vec![]))
            .unwrap();
        game.internal_rule_action(&json!({"do":"attack","attacker":"attacker","target":if leader {"P2.leader"} else {"defender"}})).unwrap();
        let question = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = &question.domain else {
            panic!("granted occurrence");
        };
        assert_eq!(options.len(), 1);
        let result = resolved(
            game.answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap(),
        );
        assert_eq!(result.source.controller, PlayerId { seat: 1 });
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["defender"]["damage"],
            if leader { 0 } else { 2 }
        );
        assert_eq!(
            game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
            20,
            "a leader attack cannot turn the captured recipient into a follower"
        );
        assert!(
            result
                .progress
                .facts
                .iter()
                .filter_map(|fact| if let Fact::Damage(hit) = fact {
                    Some(hit)
                } else {
                    None
                })
                .all(|hit| hit.source.object == result.source.holder
                    && hit.controller == PlayerId { seat: 1 })
        );
    }
}
