use super::*;
use sve_engine::game::internal::InstructionOutcome;

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

#[test]
fn bp01_010_evolution_creates_two_fairy_wisps_in_own_ex() {
    let mut initial = setup("P1", "P1", [20, 20]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"base","card":"BP01-009"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP01-010"}]);
    let mut game = game(&initial);
    assert_eq!(
        game.internal_rule_action(&json!({
            "do":"evolve","source":"base","evolve_card":"form","pay":{"pp":1_u8}
        }))
        .unwrap()
        .outcome,
        "resolved"
    );
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("evolution trigger");
    };
    assert_eq!(options.len(), 1);
    assert_eq!(options[0].card, "BP01-010");
    let result = resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap(),
    );
    let created = result
        .progress
        .receipts
        .values()
        .find_map(|outcome| {
            if let InstructionOutcome::Token(outcome) = outcome {
                Some(outcome.created.len())
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!(created, 2);
    let view = game.projection(View::Referee).unwrap();
    let ex = view["P1"]["ex"].as_array().unwrap();
    assert_eq!(ex.len(), 2);
    for id in ex {
        let object = &view["objects"][id.as_str().unwrap()];
        assert_eq!(object["card"], "BP01-T02");
        assert_eq!(object["name"], "フェアリーウィスプ");
    }
    assert_eq!(view["P2"]["ex"], json!([]));
}
