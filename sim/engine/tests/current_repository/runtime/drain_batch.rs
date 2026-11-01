#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate these real-card scenarios."
)]
use super::*;

fn pending_line(game: &mut Game, number: &str, line: u64) {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("printed pending ability")
    };
    assert_eq!(options.len(), 1, "one printed ability is pending");
    assert_eq!(
        options[0].card, number,
        "the effective printing supplies the pending ability"
    );
    assert_eq!(
        options[0].line, line,
        "the expected printed line is pending"
    );
    resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap(),
    );
    game.resume_internal_card().unwrap();
}

fn declare(game: &mut Game, source: &str, target: &str) {
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":source,"target":target}))
            .unwrap()
            .outcome,
        "resolved",
        "the real follower declares an attack"
    );
}

fn combat(game: &mut Game) {
    let question = game.internal_quick_choice().unwrap().unwrap();
    game.answer_internal_card(question.actor, question.token, Answer::Quick(None))
        .unwrap();
}

#[test]
fn hazan_all_printings_stand_once_and_only_combat_damage_recovers_life() {
    for number in ["BP11-073", "PR-308"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
        initial["players"]["P1"]["zones"]["field"] = json!([
            {"id":"source","card":"BP11-072","state":{"evolved":true,"evolved_with":"form","power":1_u8,"hp":6_u8,"max_hp":6_u8}}
        ]);
        initial["players"]["P1"]["zones"]["evolution"] = json!([{"id":"form","card":number}]);
        let mut game = game(&initial);
        declare(&mut game, "source", "P2.leader");
        pending_line(&mut game, number, 2);
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(
            view["P2"]["leader"]["life"], 18_u8,
            "the attack ability deals two before combat"
        );
        assert_eq!(
            view["P1"]["leader"]["life"], 10_u8,
            "ability damage does not trigger Drain"
        );
        assert_eq!(
            view["objects"]["source"]["acted"], false,
            "the first attack ability stands its holder"
        );
        combat(&mut game);
        pending_line(&mut game, number, 1);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
            11_u8,
            "Drain heals the first one-point combat hit"
        );
        declare(&mut game, "source", "P2.leader");
        assert!(
            game.internal_pending_choice().unwrap().is_none(),
            "the attack ability is limited to once per turn"
        );
        combat(&mut game);
        pending_line(&mut game, number, 1);
        let final_view = game.projection(View::Referee).unwrap();
        assert_eq!(
            final_view["P2"]["leader"]["life"], 16_u8,
            "only one attack ability and two combat hits resolve"
        );
        assert_eq!(
            final_view["P1"]["leader"]["life"], 12_u8,
            "each combat hit heals exactly once"
        );
        assert_eq!(
            final_view["objects"]["source"]["acted"], true,
            "the second attack cannot stand again"
        );
    }
}

#[test]
fn alouette_all_printings_create_a_rush_artifact_that_recovers_combat_damage() {
    for number in ["BP16-116", "BP16-P27"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":number}]);
        initial["players"]["P2"]["zones"]["field"] = json!([
            {"id":"enemy","card":"BP01-037","state":{"acted":true,"power":0_u8}}
        ]);
        let mut game = game(&initial);
        let CardPlay::Entered(entry) = game
            .play_internal_card("source", PlayerId { seat: 1 })
            .unwrap()
        else {
            panic!("the real source enters for its printed cost")
        };
        if matches!(entry.resume, ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
        pending_line(&mut game, number, 1);
        let view = game.projection(View::Referee).unwrap();
        let token = view["P1"]["field"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(Value::as_str)
            .find(|id| *id != "source")
            .unwrap()
            .to_owned();
        assert_eq!(
            view["objects"][&token]["card"], "BP13-T05",
            "the actual artifact printing is created"
        );
        assert_eq!(
            view["P1"]["pp"]["current"], 6_u8,
            "the fanfare follows the four-PP play"
        );
        declare(&mut game, &token, "enemy");
        combat(&mut game);
        pending_line(&mut game, "BP13-T05", 1);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
            13_u8,
            "the newly created Rush artifact heals three actual combat damage"
        );
    }
}
