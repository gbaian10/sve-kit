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
            let mut members: Vec<_> = d.members.iter().collect();
            members.sort_by_key(|member| {
                !matches!(member.card.as_deref(), Some("BP01-007" | "BP01-015"))
            });
            Answer::CostMaterials(
                members
                    .into_iter()
                    .take(usize::try_from(d.cardinality.maximum.unwrap().0).unwrap())
                    .map(|member| member.handle)
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

fn remove_source(game: &mut Game) {
    let CardPlay::Awaiting(q) = game.play_internal_card("removal", actor()).unwrap() else {
        panic!("Soul Conversion target")
    };
    let InputDomain::Targets(d) = &q.domain else {
        panic!("one allied follower target")
    };
    assert_eq!(
        d.members.len(),
        1,
        "the scenario exposes only its intended source to Soul Conversion"
    );
    let step = game
        .answer_internal_card(q.actor, q.token, Answer::Targets(vec![d.members[0].handle]))
        .unwrap();
    finish(game, step);
    finish_checks(game);
}

#[test]
fn bp04_055_fanfare_and_lastword_each_discard_a_spell_before_drawing() {
    let mut initial = position("BP04-055");
    initial["players"]["P1"]["zones"]["hand"]
        .as_array_mut()
        .unwrap()
        .extend([
            json!({"id":"spell-a","card":"BP01-007"}),
            json!({"id":"spell-b","card":"BP01-015"}),
            json!({"id":"removal","card":"BP01-116"}),
        ]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"d1","card":"BP01-171"},{"id":"d2","card":"BP01-171"},{"id":"d3","card":"BP01-171"},{"id":"d4","card":"BP01-171"},{"id":"tail","card":"BP01-171"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    trigger(&mut game);
    remove_source(&mut game);
    trigger(&mut game);
    let s = game.projection(View::Referee).unwrap();
    assert_eq!(
        s["P1"]["deck"],
        json!(["tail"]),
        "Fanfare, Soul Conversion and Lastword draw one plus two plus one cards"
    );
    let grave = s["P1"]["cemetery"].as_array().unwrap();
    assert!(
        grave.contains(&json!("spell-a")) && grave.contains(&json!("spell-b")),
        "each printed trigger pays a real spell from hand before its draw"
    );
}
