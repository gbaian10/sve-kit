#![expect(
    clippy::panic,
    reason = "Unexpected questions invalidate the printed Thornwood scenario."
)]
use super::*;

fn finish(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}
fn pending(game: &mut Game) -> Activation {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("printed pending");
    };
    resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap(),
    )
}
fn next_turn(game: &mut Game) {
    let turns = game.projection(View::Referee).unwrap()["turn"]["elapsed_turns"].clone();
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    for _ in 0_u8..8 {
        if game.projection(View::Referee).unwrap()["turn"]["elapsed_turns"] != turns {
            return;
        }
        if let Some(question) = game.internal_quick_choice().unwrap() {
            game.answer_internal_card(question.actor, question.token, Answer::Quick(None))
                .unwrap();
        } else {
            game.internal_rule_action(&json!({"do":"guard-act","select":[]}))
                .unwrap();
        }
    }
    panic!("the actual turn must advance before resolving Main-start triggers");
}
#[test]
fn bp03_011_whole_card_gives_combo_fairy_rush_and_granted_damage_before_combat() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"first","card":"BP01-009"},{"id":"second","card":"BP01-009"},{"id":"forest","card":"BP03-011"}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"enemy","card":"BP01-037","state":{"acted":true,"hp":20_u8,"max_hp":20_u8}}
    ]);
    for seat in ["P1", "P2"] {
        initial["players"][seat]["zones"]["deck"] = json!([
            {"id":format!("draw-{seat}"),"card":"BP01-009"}
        ]);
    }
    let mut game = Game::from_selected_effects(
        selection_with_rules().clone(),
        &initial,
        &Value::Null,
        &Value::Null,
        "native-granted-attack-target",
    )
    .unwrap();
    for id in ["first", "second", "forest"] {
        assert!(
            matches!(
                game.play_internal_card(id, PlayerId { seat: 1 }).unwrap(),
                CardPlay::Entered(_)
            ),
            "each native card enters normally"
        );
        finish(&mut game);
    }
    let forest = pending(&mut game);
    finish(&mut game);
    let before = game.projection(View::Referee).unwrap();
    let fairy = before["P1"]["field"]
        .as_array()
        .unwrap()
        .iter()
        .map(|id| id.as_str().unwrap())
        .find(|id| before["objects"][*id]["name"] == "フェアリー")
        .unwrap()
        .to_owned();
    assert!(
        before["objects"][&fairy]["keywords"]
            .as_array()
            .unwrap()
            .iter()
            .any(|keyword| keyword == "突進"),
        "Combo three grants Rush to the just-created Fairy"
    );
    game.internal_rule_action(&json!({"do":"attack","attacker":fairy,"target":"enemy"}))
        .unwrap();
    let hit = pending(&mut game);
    assert_ne!(
        hit.source.holder, forest.source.holder,
        "the granted attack ability belongs to the attacking Fairy"
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
        2_u8,
        "the captured defender takes ability damage before combat"
    );
    finish(&mut game);
    damage_prevent::pass_quick(&mut game);
    finish(&mut game);
    next_turn(&mut game);
    next_turn(&mut game);
    pending(&mut game);
    finish(&mut game);
    assert!(
        game.projection(View::Referee).unwrap()["P1"]["cemetery"]
            .as_array()
            .unwrap()
            .contains(&json!("forest")),
        "the same native amulet destroys itself at the next own main phase"
    );
}
