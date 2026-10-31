use super::union_burst_batch1::{build, enter, evolve, finish, pending, position, ub};
use super::*;

#[test]
fn misato_pays_an_amulet_to_heal_and_retains_ward() {
    for card in ["CP04-101", "CP04-P77", "PCS01-043", "PR-501"] {
        let mut initial = position(card, "hand");
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"fee","card":"ub-amulet"}]);
        let mut game = build(&initial);
        enter(&mut game);
        let play = pending(&mut game, card);
        ub(&resolved(finish(&mut game, play)));
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["leader"]["life"], 12_u8);
        assert_eq!(state["P1"]["cemetery"], json!(["fee"]));
        assert!(
            state["objects"]["source"]["keywords"]
                .as_array()
                .unwrap()
                .contains(&json!("守護")),
            "the non-UB ward keyword remains available"
        );
    }
}
#[test]
fn kurumi_fanfare_acts_the_declared_opposing_follower() {
    for card in ["CP04-107", "CP04-P83", "PCS01-045"] {
        let mut game = build(&position(card, "hand"));
        enter(&mut game);
        let play = pending(&mut game, card);
        ub(&resolved(finish(&mut game, play)));
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["enemy"]["acted"],
            true
        );
    }
}
#[test]
fn kaiser_insight_evolution_damages_every_opposing_follower() {
    for card in ["CP04-110", "CP04-SL26"] {
        let mut initial = position(card, "field");
        initial["players"]["P2"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"enemy2","card":"ub-enemy"}));
        let mut game = evolve(card, initial);
        let play = pending(&mut game, card);
        ub(&resolved(finish(&mut game, play)));
        let state = game.projection(View::Referee).unwrap();
        for enemy in ["enemy", "enemy2"] {
            assert_eq!(state["objects"][enemy]["damage"], 7_u8);
        }
    }
}
fn end(game: &mut Game) {
    if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
    while let Some(attack_window) = game.internal_quick_choice().unwrap() {
        game.answer_internal_card(
            attack_window.actor,
            attack_window.token,
            Answer::Quick(None),
        )
        .unwrap();
    }
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    let window = game.internal_quick_choice().unwrap().unwrap();
    game.answer_internal_card(window.actor, window.token, Answer::Quick(None))
        .unwrap();
}
fn opponent_draws(initial: &mut Value) {
    initial["players"]["P2"]["zones"]["deck"] = json!(
        (0_u8..4)
            .map(|n| json!({"id":format!("p2draw{n}"),"card":"BP01-037"}))
            .collect::<Vec<_>>()
    );
}
#[test]
fn kasumi_evolution_acts_the_target_and_skips_only_its_next_rule_stand() {
    for card in ["CP04-117", "CP04-P89"] {
        let mut initial = position(card, "field");
        opponent_draws(&mut initial);
        let mut game = evolve(card, initial);
        let play = pending(&mut game, card);
        ub(&resolved(finish(&mut game, play)));
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["enemy"]["acted"],
            true
        );
        end(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["enemy"]["acted"],
            true
        );
        end(&mut game);
        end(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["enemy"]["acted"],
            false
        );
    }
}
#[test]
fn ayumi_attack_preserves_an_already_acted_target_through_its_next_start() {
    for card in ["CP04-119", "CP04-P91"] {
        let mut initial = position(card, "field");
        opponent_draws(&mut initial);
        initial["players"]["P2"]["zones"]["field"][0]["state"] = json!({"acted":true});
        let mut game = build(&initial);
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"P2.leader"}))
            .unwrap();
        let play = pending(&mut game, card);
        ub(&resolved(finish(&mut game, play)));
        end(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["enemy"]["acted"],
            true
        );
    }
}
#[test]
fn shizuru_and_rino_evolution_damages_all_opposing_followers() {
    let card = "PCS01-005";
    let mut initial = position(card, "field");
    initial["players"]["P2"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"enemy2","card":"ub-enemy"}));
    let mut game = evolve(card, initial);
    let play = pending(&mut game, card);
    ub(&resolved(finish(&mut game, play)));
    let state = game.projection(View::Referee).unwrap();
    for enemy in ["enemy", "enemy2"] {
        assert_eq!(state["objects"][enemy]["damage"], 1_u8);
    }
}
