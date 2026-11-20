use super::*;

#[test]
fn bp02_067_doubles_its_outgoing_leader_attack_and_clash_damage() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"leader-attacker","card":"BP02-067"},
        {"id":"clash-attacker","card":"BP02-067"}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"defender","card":"BP01-037","state":{"acted":true,"power":1_u8,"hp":10_u8,"max_hp":10_u8}}
    ]);
    let mut game = game(&initial);
    for (attacker, target) in [
        ("leader-attacker", "P2.leader"),
        ("clash-attacker", "defender"),
    ] {
        assert_eq!(
            game.internal_rule_action(&json!({"do":"attack","attacker":attacker,"target":target}))
                .unwrap()
                .outcome,
            "resolved"
        );
        pass_quick(&mut game);
        finish_checks(&mut game);
    }
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["P2"]["leader"]["life"], 16_u8);
    assert_eq!(after["objects"]["defender"]["damage"], 4_u8);
    assert_eq!(after["objects"]["clash-attacker"]["damage"], 1_u8);
}
