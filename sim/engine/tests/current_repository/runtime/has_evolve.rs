#![expect(
    clippy::panic,
    reason = "Unexpected questions invalidate these card scenarios."
)]
use super::*;
use sve_engine::game::internal::SearchAnswer;

const fn actor() -> PlayerId {
    PlayerId { seat: 1 }
}

fn pending(game: &mut Game, card: &str) -> CardPlay {
    let q = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &q.domain else {
        panic!("pending trigger");
    };
    let option = options.iter().find(|option| option.card == card).unwrap();
    game.answer_internal_card(q.actor, q.token, Answer::Pending(option.handle))
        .unwrap()
}

fn enter(game: &mut Game) {
    let CardPlay::Entered(entry) = game.play_internal_card("source", actor()).unwrap() else {
        panic!("ordinary follower play");
    };
    if entry.resume == ResumeStage::CheckTiming {
        game.resume_internal_card().unwrap();
    }
}

#[test]
fn has_evolve_maid_reprints_search_only_followers_with_evolution() {
    for form in ["BP01-035", "BP01-P08"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP01-034"}]);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
        initial["players"]["P1"]["zones"]["deck"] = json!([
            {"id":"eligible","card":"BP22-078"},
            {"id":"without-evolution","card":"BP01-037"},
            {"id":"spell","card":"BP01-025"}
        ]);
        let mut game = game(&initial);
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":2_u8}})
            )
            .unwrap()
            .outcome,
            "resolved"
        );
        let CardPlay::Awaiting(q) = pending(&mut game, form) else {
            panic!("private search");
        };
        let InputDomain::Search(domain) = &q.domain else {
            panic!("search domain");
        };
        assert_eq!(
            domain.cards.len(),
            1,
            "a follower without evolution and a spell are excluded"
        );
        assert_eq!(domain.cards[0].card, "BP22-078");
        resolved(
            game.answer_internal_card(
                q.actor,
                q.token,
                Answer::Search(SearchAnswer::Ungrouped(vec![domain.cards[0].handle])),
            )
            .unwrap(),
        );
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["hand"], json!(["eligible"]));
        assert_eq!(state["P1"]["deck"].as_array().unwrap().len(), 2);
    }
}

#[test]
fn has_evolve_detective_returns_an_evolution_follower_and_gains_rush_on_evolution() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP13-115"}]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"base","card":"BP01-034"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP01-035"}]);
    initial["players"]["P1"]["zones"]["cemetery"] = json!([
        {"id":"eligible","card":"BP22-078"}, {"id":"without-evolution","card":"BP01-037"}
    ]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-037","state":{"acted":true}}]);
    let mut game = game(&initial);
    enter(&mut game);
    let CardPlay::Awaiting(q) = pending(&mut game, "BP13-115") else {
        panic!("cemetery target");
    };
    let InputDomain::Targets(domain) = &q.domain else {
        panic!("target domain");
    };
    assert_eq!(
        domain.members.len(),
        1,
        "a follower without evolution is excluded"
    );
    resolved(
        game.answer_internal_card(
            q.actor,
            q.token,
            Answer::Targets(vec![domain.members[0].handle]),
        )
        .unwrap(),
    );
    game.resume_internal_card().unwrap();
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["eligible"]));
    assert_eq!(state["P1"]["cemetery"], json!(["without-evolution"]));
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"enemy"}))
            .unwrap()
            .outcome,
        "cannot-attack"
    );
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"base","evolve_card":"form","pay":{"pp":2_u8}})
        )
        .unwrap()
        .outcome,
        "resolved"
    );
    resolved(pending(&mut game, "BP13-115"));
    game.resume_internal_card().unwrap();
    let CardPlay::Awaiting(search_question) = pending(&mut game, "BP01-035") else {
        panic!("empty deck search");
    };
    let InputDomain::Search(search_domain) = &search_question.domain else {
        panic!("search domain");
    };
    assert_eq!(search_domain.cards, vec![]);
    resolved(
        game.answer_internal_card(
            search_question.actor,
            search_question.token,
            Answer::Search(SearchAnswer::Ungrouped(vec![])),
        )
        .unwrap(),
    );
    game.resume_internal_card().unwrap();
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"enemy"}))
            .unwrap()
            .outcome,
        "resolved",
        "the actual evolution grants rush to the newly played detective"
    );
}

#[test]
fn has_evolve_field_target_uses_the_same_filter() {
    let (snapshot, keywords, mut input) = inputs().clone();
    let author = input["authors"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|author| author["input"]["card_no"] == "BP13-115")
        .unwrap();
    let face = author["input"]["tree"]["cards"]["BP13-115"]["faces"]
        .as_object_mut()
        .unwrap()
        .values_mut()
        .next()
        .unwrap();
    face["abilities"][1]["fanfare"]["targets"][0]["from"]["zone"] = json!("field");
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP13-115"}]);
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"eligible","card":"BP22-078"}, {"id":"without-evolution","card":"BP01-037"}
    ]);
    let mut game = game_from_selection(&initial, &snapshot, &keywords, &input);
    enter(&mut game);
    let CardPlay::Awaiting(q) = pending(&mut game, "BP13-115") else {
        panic!("field target");
    };
    let InputDomain::Targets(domain) = &q.domain else {
        panic!("target domain");
    };
    assert_eq!(
        domain.members.len(),
        1,
        "only the field follower with evolution is selectable"
    );
    resolved(
        game.answer_internal_card(
            q.actor,
            q.token,
            Answer::Targets(vec![domain.members[0].handle]),
        )
        .unwrap(),
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["hand"],
        json!(["eligible"])
    );
}
