use super::*;

#[test]
fn token_trio_printed_rush_and_ward_control_real_combat() {
    let mut initial = setup("P1", "P1", [20, 20]);
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"rush","card":"BP08-T09","state":{"entered_this_turn":true}},
        {"id":"both","card":"BP15-T06","state":{"entered_this_turn":true}},
        {"id":"attacker","card":"BP06-048"}
    ]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"ward","card":"BP17-T13","state":{"acted":true}}]);
    let mut game = game(&initial);
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["objects"]["rush"]["keywords"], json!(["突進"]));
    assert_eq!(view["objects"]["both"]["keywords"], json!(["守護", "突進"]));
    assert_eq!(view["objects"]["ward"]["keywords"], json!(["守護"]));
    for attacker in ["rush", "both", "attacker"] {
        assert_eq!(
            game.internal_rule_action(&json!({
                "do":"attack","attacker":attacker,"target":"P2.leader"
            }))
            .unwrap()
            .outcome,
            "cannot-attack",
            "Rush cannot attack a leader on entry; rested Ward also protects it"
        );
    }
    assert_eq!(
        game.internal_rule_action(&json!({
            "do":"attack","attacker":"rush","target":"ward"
        }))
        .unwrap()
        .outcome,
        "resolved",
        "Rush can attack the rested Ward immediately"
    );
}
