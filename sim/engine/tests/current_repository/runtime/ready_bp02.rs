#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate the printed card scenarios."
)]
use super::*;
use sve_engine::game::internal::SearchAnswer;

const PLAYER: PlayerId = PlayerId { seat: 1 };
fn position(number: &str, zone: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"][zone] = json!([{"id":"source","card":number}]);
    initial["players"]["P1"]["zones"]["deck"] = json!(
        (0..6_u8)
            .map(|i| json!({"id":format!("deck{i}"),"card":"BP01-037"}))
            .collect::<Vec<_>>()
    );
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-037","state":{"hp":12_u8,"max_hp":12_u8}}]);
    initial
}
fn new_game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "ready-bp02",
    )
    .unwrap()
}
fn finish(game: &mut Game, mut play: CardPlay) -> CardPlay {
    while let CardPlay::Awaiting(question) = play {
        let answer = match &question.domain {
            InputDomain::Targets(domain) => Answer::Targets(vec![
                domain
                    .members
                    .iter()
                    .find(|m| m.card.is_some())
                    .unwrap()
                    .handle,
            ]),
            InputDomain::CostMaterials(domain) => {
                Answer::CostMaterials(vec![domain.members[0].handle])
            }
            InputDomain::Search(domain) => {
                assert_eq!(
                    domain.cards.len(),
                    1,
                    "only the Neutral follower is searchable"
                );
                Answer::Search(SearchAnswer::Ungrouped(vec![domain.cards[0].handle]))
            }
            InputDomain::OptionalCost => Answer::OptionalCost(true),
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            _ => panic!("unexpected printed decision: {:?}", question.domain),
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    play
}
fn play(game: &mut Game, source: &str) {
    let step = game.play_internal_card(source, PLAYER).unwrap();
    assert!(
        matches!(
            finish(game, step),
            CardPlay::Entered(_) | CardPlay::Resolved(_)
        ),
        "the printed card play completes"
    );
    if matches!(game.internal_card_stage(), Some(ResumeStage::CheckTiming)) {
        game.resume_internal_card().unwrap();
    }
}
fn trigger(game: &mut Game) {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("printed trigger")
    };
    assert_eq!(options.len(), 1, "only this printed trigger is pending");
    let play = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap();
    resolved(finish(game, play));
    game.resume_internal_card().unwrap();
}
#[test]
fn cynthia_attack_banishes_ex_material_damages_a_follower_and_creates_two_fairies() {
    let mut initial = position("BP02-005", "field");
    initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"fee","card":"BP01-171"}]);
    let mut game = new_game(&initial);
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"P2.leader"}))
            .unwrap()
            .outcome,
        "resolved"
    );
    trigger(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["damage"], 2_u8);
    assert_eq!(state["P1"]["ex"], json!([]));
    assert_eq!(state["P1"]["banish"], json!(["fee"]));
    let names: Vec<_> = state["P1"]["field"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|id| **id != json!("source"))
        .map(|id| {
            state["objects"][id.as_str().unwrap()]["name"]
                .as_str()
                .unwrap()
        })
        .collect();
    assert_eq!(names, vec!["フェアリー", "フェアリー"]);
}
#[test]
fn selwyn_fanfare_deals_five_only_after_the_fourth_play() {
    for prior in [2_u8, 3] {
        let mut initial = position("BP02-006", "hand");
        for i in 0..prior {
            initial["players"]["P1"]["zones"]["hand"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":format!("prior{i}"),"card":"BP01-023"}));
        }
        let mut game = new_game(&initial);
        for i in 0..prior {
            play(&mut game, &format!("prior{i}"));
        }
        play(&mut game, "source");
        trigger(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
            if prior == 3 { 5_u8 } else { 0 }
        );
    }
}
#[test]
fn witch_bolt_draws_only_with_an_evolved_ally() {
    for evolved in [false, true] {
        let mut initial = position("BP02-049", "hand");
        if evolved {
            initial["players"]["P1"]["zones"]["field"] = json!([{"id":"ally","card":"BP01-105"}]);
            initial["players"]["P1"]["zones"]["evolve_deck"] =
                json!([{"id":"form","card":"BP01-106"}]);
        }
        let mut game = new_game(&initial);
        if evolved {
            assert_eq!(
                game.internal_rule_action(
                    &json!({"do":"evolve","source":"ally","evolve_card":"form","pay":{"pp":0_u8}})
                )
                .unwrap()
                .outcome,
                "resolved"
            );
        }
        if matches!(game.internal_card_stage(), Some(ResumeStage::CheckTiming)) {
            game.resume_internal_card().unwrap();
        }
        play(&mut game, "source");
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["enemy"]["damage"], 5_u8);
        assert_eq!(
            state["P1"]["hand"],
            if evolved { json!(["deck0"]) } else { json!([]) }
        );
    }
}
#[test]
fn bone_chimera_mills_three_and_recomputes_necrocharge_seven() {
    for graves in [3_u8, 4] {
        let mut initial = position("BP02-087", "hand");
        initial["players"]["P1"]["zones"]["cemetery"] = json!(
            (0..graves)
                .map(|i| json!({"id":format!("grave{i}"),"card":"BP01-037"}))
                .collect::<Vec<_>>()
        );
        let mut game = new_game(&initial);
        play(&mut game, "source");
        trigger(&mut game);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P1"]["cemetery"].as_array().unwrap().len(),
            usize::from(graves) + 3
        );
        for keyword in ["突進", "必殺"] {
            assert_eq!(
                state["objects"]["source"]["keywords"]
                    .as_array()
                    .unwrap()
                    .contains(&json!(keyword)),
                graves == 4
            );
        }
    }
}
#[test]
fn call_of_cocytus_destroys_then_searches_a_neutral_follower() {
    let mut initial = position("BP02-116", "hand");
    initial["players"]["P1"]["zones"]["deck"] =
        json!([{"id":"royal","card":"BP01-037"},{"id":"neutral","card":"BP01-171"}]);
    let mut game = new_game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["field"], json!([]));
    assert_eq!(state["P1"]["hand"], json!(["neutral"]));
    assert_eq!(state["P1"]["deck"], json!(["royal"]));
}
