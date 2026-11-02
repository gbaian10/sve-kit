#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate these printed payment scenarios."
)]
use super::*;
use sve_engine::game::internal::SearchAnswer;

fn finish(game: &mut Game, mut play: CardPlay) -> CardPlay {
    while let CardPlay::Awaiting(question) = play {
        let answer = match &question.domain {
            InputDomain::OptionalCost => Answer::OptionalCost(true),
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            InputDomain::Targets(domain) => Answer::Targets(
                domain
                    .members
                    .iter()
                    .take(usize::try_from(domain.cardinality.minimum.0).unwrap())
                    .map(|member| member.handle)
                    .collect(),
            ),
            InputDomain::CostMaterials(domain) => Answer::CostMaterials(
                domain
                    .members
                    .iter()
                    .take(usize::try_from(domain.cardinality.minimum.0).unwrap())
                    .map(|member| member.handle)
                    .collect(),
            ),
            InputDomain::Search(domain) => Answer::Search(SearchAnswer::Ungrouped(
                domain.cards.iter().map(|card| card.handle).collect(),
            )),
            _ => panic!("unexpected multi-pay decision"),
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    play
}
fn position(number: &str, hand: Value) -> Value {
    let mut initial = lesson::position();
    initial["players"]["P1"]["zones"]["hand"] = hand;
    initial["players"]["P1"]["zones"]["hand"]
        .as_array_mut()
        .unwrap()
        .insert(0, json!({"id":"source","card":number}));
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-037","state":{"hp":10_u8,"max_hp":10_u8}}]);
    initial
}
fn enter_and_resolve(number: &str, initial: &Value) -> (Game, Value) {
    let mut game = lesson::game(initial);
    let play = game
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap();
    assert!(
        matches!(finish(&mut game, play), CardPlay::Entered(_)),
        "the real card enters before its Fanfare"
    );
    let before = game.projection(View::Referee).unwrap();
    loop {
        if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
        let Some(pending) = game.internal_pending_choice().unwrap() else {
            break;
        };
        let InputDomain::Pending(options) = &pending.domain else {
            panic!("source Fanfare")
        };
        let handle = options
            .iter()
            .find(|option| option.card == number)
            .unwrap()
            .handle;
        let trigger = game
            .answer_internal_card(pending.actor, pending.token, Answer::Pending(handle))
            .unwrap();
        resolved(finish(&mut game, trigger));
    }
    (game, before)
}
fn lesson_buff(numbers: &[&str]) {
    for number in numbers {
        let (game, before) = enter_and_resolve(number, &position(number, json!([])));
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(
            view["P1"]["pp"]["current"], 6_u8,
            "two PP for entry and two for the ability"
        );
        assert_eq!(
            view["P1"]["ex"],
            json!(["second", "wrong"]),
            "one Lesson material is banished"
        );
        assert_eq!(
            view["objects"]["source"]["power"],
            before["objects"]["source"]["power"]
                .as_i64()
                .unwrap()
                .checked_add(1)
                .unwrap(),
            "paid ability buffs attack"
        );
        assert_eq!(
            view["objects"]["source"]["hp"],
            before["objects"]["source"]["hp"]
                .as_i64()
                .unwrap()
                .checked_add(1)
                .unwrap(),
            "paid ability buffs life"
        );
    }
}
#[test]
fn csd02b_005_pays_pp_and_lesson_for_all_printings() {
    lesson_buff(&["CSD02b-005", "CSD02b-P02"]);
}
#[test]
fn csd02c_007_pays_pp_and_lesson_for_all_printings() {
    lesson_buff(&["CSD02c-007", "CSD02c-P04"]);
}
#[test]
fn pr_442_pays_all_three_cost_steps_before_damage() {
    let initial = position("PR-442", json!([{"id":"fuel","card":"CSD02c-004"}]));
    let (game, _) = enter_and_resolve("PR-442", &initial);
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["pp"]["current"], 7_u8);
    assert_eq!(view["P1"]["ex"], json!(["second", "wrong"]));
    assert_eq!(view["P1"]["cemetery"], json!(["fuel"]));
    assert_eq!(view["objects"]["enemy"]["damage"], 3_u8);
}
#[test]
fn bp12_065_creates_tree_then_pays_pp_and_discard_for_all_printings() {
    for number in ["BP12-065", "PR-333"] {
        let initial = position(number, json!([{"id":"fuel","card":"BP01-037"}]));
        let (game, _) = enter_and_resolve(number, &initial);
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["P1"]["pp"]["current"], 7_u8);
        assert_eq!(view["P1"]["cemetery"], json!(["fuel"]));
        assert_eq!(view["objects"]["enemy"]["damage"], 3_u8);
        assert_eq!(view["P1"]["field"].as_array().unwrap().len(), 2);
        assert!(
            view["P1"]["field"]
                .as_array()
                .unwrap()
                .iter()
                .any(|id| view["objects"][id.as_str().unwrap()]["name"] == "ナテラの大樹")
        );
    }
}
#[test]
fn cp03_063_attack_cost_stands_the_attacker_for_all_printings() {
    for number in ["CP03-063", "CP03-SL12", "CP03-U04"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["pp"] = json!({"current":3_u8,"max":3_u8});
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":number}]);
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"fuel","card":number}]);
        initial["players"]["P1"]["zones"]["deck"] =
            json!([{"id":"draw0","card":"BP01-037"},{"id":"draw1","card":"BP01-037"}]);
        let mut game = lesson::game(&initial);
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"P2.leader"}))
            .unwrap();
        let pending = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = &pending.domain else {
            panic!("attack triggers")
        };
        let handle = options
            .iter()
            .find(|option| option.card == number && option.line == 2)
            .unwrap()
            .handle;
        let trigger = game
            .answer_internal_card(pending.actor, pending.token, Answer::Pending(handle))
            .unwrap();
        resolved(finish(&mut game, trigger));
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["P1"]["pp"]["current"], 0_u8);
        assert_eq!(view["P1"]["cemetery"], json!(["fuel"]));
        assert_eq!(view["objects"]["source"]["acted"], false);
    }
}
fn search_discount(numbers: &[&str]) {
    for number in numbers {
        let mut initial = position(number, json!([]));
        initial["players"]["P1"]["pp"] = json!({"current":4_u8,"max":4_u8});
        initial["players"]["P1"]["zones"]["field"] =
            json!([{"id":"cool0","card":"CSD02b-004"},{"id":"cool1","card":"CSD02b-004"}]);
        initial["players"]["P1"]["zones"]["deck"] =
            json!([{"id":"found","card":"CP02-069"},{"id":"wrong-deck","card":"BP01-037"}]);
        let (mut game, _) = enter_and_resolve(number, &initial);
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(
            view["P1"]["pp"]["current"], 0_u8,
            "entry and two PP for Lesson consume the available PP"
        );
        assert!(
            view["objects"]["source"]["keywords"]
                .as_array()
                .unwrap()
                .contains(&json!("疾走")),
            "three Cool followers enable Storm"
        );
        assert!(
            view["P1"]["ex"]
                .as_array()
                .unwrap()
                .contains(&json!("found")),
            "the searched named follower enters EX"
        );
        assert!(
            matches!(
                game.play_internal_card("found", PlayerId { seat: 1 })
                    .unwrap(),
                CardPlay::Entered(_)
            ),
            "the searched four-cost card can be played with zero PP"
        );
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
            0_u8,
            "the linked discount is applied"
        );
    }
}
#[test]
fn ecp02_052_pays_pp_lesson_then_discounts_search_result_for_all_printings() {
    search_discount(&["ECP02-052", "ECP02-SL27"]);
}
#[test]
fn ecp02_sp10_pays_pp_lesson_then_discounts_search_result_for_all_printings() {
    search_discount(&["ECP02-SP10", "ECP02-U10"]);
}
