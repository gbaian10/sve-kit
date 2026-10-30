#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate the printed card scenarios."
)]
use super::*;
use sve_engine::game::internal::SearchAnswer;

const fn actor() -> PlayerId {
    PlayerId { seat: 1 }
}

fn position(card: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    for seat in ["P1", "P2"] {
        initial["players"][seat]["zones"]["deck"] = json!([{"id":format!("{seat}-first"),"card":"BP01-171"},{"id":format!("{seat}-second"),"card":"BP01-171"}]);
    }
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-037","state":{"acted":true}}]);
    initial
}

fn field(card: &str) -> Value {
    let mut initial = position(card);
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":card}]);
    initial
}

fn finish_checks(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}

fn finish(game: &mut Game, mut step: CardPlay) {
    while let CardPlay::Awaiting(q) = step {
        let answer = if matches!(&q.domain, InputDomain::WardEntry { .. }) {
            Answer::WardEntry(false)
        } else if matches!(&q.domain, InputDomain::OptionalCost) {
            Answer::OptionalCost(true)
        } else if let InputDomain::Targets(d) = &q.domain {
            Answer::Targets(
                d.members
                    .iter()
                    .take(usize::try_from(d.cardinality.maximum.unwrap().0).unwrap())
                    .map(|m| m.handle)
                    .collect(),
            )
        } else if let InputDomain::CostMaterials(d) = &q.domain {
            Answer::CostMaterials(
                d.members
                    .iter()
                    .take(usize::try_from(d.cardinality.maximum.unwrap().0).unwrap())
                    .map(|m| m.handle)
                    .collect(),
            )
        } else if let InputDomain::Search(d) = &q.domain {
            Answer::Search(SearchAnswer::Ungrouped(
                d.cards
                    .iter()
                    .take(usize::try_from(d.cardinality.maximum.unwrap().0).unwrap())
                    .map(|m| m.handle)
                    .collect(),
            ))
        } else if let InputDomain::Order(cards) = &q.domain {
            Answer::Order(cards.iter().rev().map(|m| m.handle).collect())
        } else {
            panic!("unexpected decision")
        };
        step = game.answer_internal_card(q.actor, q.token, answer).unwrap();
    }
}

fn play(game: &mut Game, id: &str) {
    let step = game.play_internal_card(id, actor()).unwrap();
    finish(game, step);
}

fn trigger(game: &mut Game) {
    let q = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &q.domain else {
        panic!("pending printed ability")
    };
    assert_eq!(
        options.len(),
        1,
        "only the intended printed ability is pending"
    );
    let step = game
        .answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
        .unwrap();
    finish(game, step);
    finish_checks(game);
}

fn quick_pass(game: &mut Game) {
    let q = game.internal_quick_choice().unwrap().unwrap();
    let step = game
        .answer_internal_card(q.actor, q.token, Answer::Quick(None))
        .unwrap();
    finish(game, step);
}

fn trigger_line(game: &mut Game, line: u64) {
    let q = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &q.domain else {
        panic!("printed Fanfare order")
    };
    let handle = options.iter().find(|o| o.line == line).unwrap().handle;
    let step = game
        .answer_internal_card(q.actor, q.token, Answer::Pending(handle))
        .unwrap();
    finish(game, step);
    finish_checks(game);
}

fn end(game: &mut Game) {
    assert_eq!(
        game.internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap()
            .outcome,
        "resolved",
        "the normal End transition advances timing before the next Start"
    );
    quick_pass(game);
}

fn frozen_fanfare(card: &str) {
    for (acted, max) in [(false, 6_u8), (true, 6), (false, 7), (true, 7)] {
        let mut initial = position(card);
        initial["players"]["P1"]["pp"] = json!({"current":max,"max":max});
        initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037","state":{"acted":acted}},{"id":"control","card":"BP01-037","state":{"acted":true}}]);
        let mut game = game(&initial);
        play(&mut game, "source");
        trigger_line(&mut game, 1);
        trigger_line(&mut game, 2);
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["enemy"]["acted"],
            true,
            "Fanfare rests the target regardless of its initial posture"
        );
        end(&mut game);
        let first = game.projection(View::Referee).unwrap();
        assert_eq!(
            first["objects"]["enemy"]["acted"], true,
            "the selected follower skips its next opposing Start stand"
        );
        assert_eq!(
            first["objects"]["control"]["acted"], false,
            "the untargeted follower must stand normally"
        );
        end(&mut game);
        end(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["enemy"]["acted"],
            false,
            "the stand restriction expires after the first opposing Start"
        );
    }
}

#[test]
fn bp06_056_fanfare_locks_either_initial_posture_for_exactly_one_opposing_start() {
    frozen_fanfare("BP06-056");
}

#[test]
fn bp06_057_fanfare_locks_either_initial_posture_for_exactly_one_opposing_start() {
    frozen_fanfare("BP06-057");
}

#[test]
fn bp06_033_searches_a_levion_follower_past_an_ineligible_card() {
    let mut initial = position("BP06-033");
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"wrong","card":"BP01-171"},{"id":"found","card":"BP02-025"},{"id":"tail","card":"BP01-171"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    trigger(&mut game);
    let s = game.projection(View::Referee).unwrap();
    assert_eq!(s["P1"]["hand"], json!(["found"]));
    assert_eq!(s["P1"]["deck"], json!(["wrong", "tail"]));
}

#[test]
fn bp06_088_takes_one_yokai_follower_and_orders_only_the_remaining_four() {
    let mut initial = position("BP06-088");
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"wrong","card":"BP01-171"},{"id":"found","card":"BP06-084"},{"id":"third","card":"BP01-171"},{"id":"fourth","card":"BP01-171"},{"id":"fifth","card":"BP01-171"},{"id":"tail","card":"BP01-171"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    finish_checks(&mut game);
    let s = game.projection(View::Referee).unwrap();
    assert_eq!(s["P1"]["hand"], json!(["found"]));
    assert_eq!(
        s["P1"]["deck"],
        json!(["tail", "fifth", "fourth", "third", "wrong"])
    );
}

#[test]
fn bp06_101_real_lastword_searches_a_copy_after_soul_conversion_draws_two() {
    let mut initial = position("BP06-101");
    initial["players"]["P1"]["zones"]["hand"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"removal","card":"BP01-116"}));
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"first","card":"BP01-171"},{"id":"second","card":"BP01-171"},{"id":"found","card":"BP06-101"},{"id":"tail","card":"BP01-171"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    finish_checks(&mut game);
    play(&mut game, "removal");
    finish_checks(&mut game);
    trigger(&mut game);
    let s = game.projection(View::Referee).unwrap();
    assert_eq!(s["P1"]["hand"], json!(["first", "second", "found"]));
    assert_eq!(s["P1"]["deck"], json!(["tail"]));
    assert!(
        s["P1"]["cemetery"]
            .as_array()
            .unwrap()
            .contains(&json!("source")),
        "Lastword resolves from the real sacrificed follower"
    );
}

fn named(s: &Value, name: &str) -> usize {
    s["P1"]["field"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|id| s["objects"][id.as_str().unwrap()]["name"] == name)
        .count()
}

#[test]
fn bp06_083_a_vampire_amulet_reduces_cost_and_a_full_field_still_draws_two() {
    for fill in [0_usize, 1, 4, 5] {
        let mut initial = position("BP06-083");
        let mut allies = Vec::new();
        if fill > 0 {
            allies.push(json!({"id":"castle","card":"BP02-076"}));
        }
        for n in 1..fill {
            allies.push(json!({"id":format!("other{n}"),"card":"BP01-171"}));
        }
        initial["players"]["P1"]["zones"]["field"] = Value::Array(allies);
        let mut game = game(&initial);
        play(&mut game, "source");
        finish_checks(&mut game);
        let s = game.projection(View::Referee).unwrap();
        assert_eq!(named(&s, "フォレストバット"), (5 - fill).min(3));
        assert_eq!(s["P1"]["hand"], json!(["P1-first", "P1-second"]));
        assert_eq!(
            s["P1"]["pp"]["current"],
            if fill == 0 { 6_u8 } else { 7_u8 }
        );
    }
}

#[test]
fn bp06_102_end_creates_a_ward_token_whose_fanfare_draws_before_end_can_continue() {
    for pp in [1_u8, 2] {
        let mut initial = field("BP06-102");
        initial["players"]["P1"]["pp"]["current"] = json!(pp);
        let mut game = game(&initial);
        assert_eq!(
            game.internal_rule_action(&json!({"do":"end-phase"}))
                .unwrap()
                .outcome,
            "resolved"
        );
        trigger(&mut game);
        if pp == 2 {
            let q = game.internal_pending_choice().unwrap().unwrap();
            let InputDomain::Pending(options) = &q.domain else {
                panic!("token Fanfare precedes End continuation")
            };
            assert_eq!(options.len(), 1);
            assert_eq!(options[0].card, "BP05-T05");
            trigger(&mut game);
        }
        let s = game.projection(View::Referee).unwrap();
        assert_eq!(
            named(&s, "ミスティックアーティファクト"),
            usize::from(pp == 2)
        );
        assert_eq!(
            s["P1"]["hand"].as_array().unwrap().len(),
            usize::from(pp == 2)
        );
    }
}

#[test]
fn bp06_100_activation_can_sacrifice_itself_or_another_own_amulet() {
    for other in [false, true] {
        let mut initial = position("BP06-100");
        if other {
            initial["players"]["P1"]["zones"]["field"] = json!([{"id":"other","card":"BP02-076"}]);
        }
        let mut game = game(&initial);
        play(&mut game, "source");
        trigger(&mut game);
        let id = selection().current["BP06-100"]
            .program
            .card()
            .abilities
            .iter()
            .find(|a| a.node.kind == AbilityKind::Activated)
            .unwrap()
            .node
            .id;
        let CardPlay::Awaiting(q) = game.activate_internal_card("source", id, actor()).unwrap()
        else {
            panic!("amulet head material")
        };
        let InputDomain::CostMaterials(d) = &q.domain else {
            panic!("own amulet material")
        };
        let wanted = if other { "BP02-076" } else { "BP06-100" };
        let selected = d
            .members
            .iter()
            .find(|m| m.card.as_deref() == Some(wanted))
            .unwrap()
            .handle;
        let step = game
            .answer_internal_card(q.actor, q.token, Answer::CostMaterials(vec![selected]))
            .unwrap();
        finish(&mut game, step);
        finish_checks(&mut game);
        let s = game.projection(View::Referee).unwrap();
        assert_eq!(named(&s, "ホーリーファルコン"), 2);
        assert_eq!(s["P1"]["pp"]["current"], 5_u8);
        assert_eq!(
            s["P1"]["cemetery"],
            if other {
                json!(["other"])
            } else {
                json!(["source"])
            }
        );
        if other {
            assert_eq!(s["objects"]["source"]["acted"], true);
        }
    }
}
