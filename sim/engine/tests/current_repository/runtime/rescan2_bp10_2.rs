#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate these game scenarios."
)]
use super::*;
use sve_engine::game::internal::SearchAnswer;

const fn actor() -> PlayerId {
    PlayerId { seat: 1 }
}

fn position(card: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"first","card":"BP01-171"},
        {"id":"second","card":"BP01-171"},
        {"id":"third","card":"BP01-171"},
        {"id":"fourth","card":"BP01-171"}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    initial
}

fn finish(game: &mut Game, mut play: CardPlay) {
    while let CardPlay::Awaiting(question) = play {
        let answer = if matches!(&question.domain, InputDomain::WardEntry { .. }) {
            Answer::WardEntry(false)
        } else if matches!(&question.domain, InputDomain::OptionalCost) {
            Answer::OptionalCost(true)
        } else if let InputDomain::Search(domain) = &question.domain {
            Answer::Search(SearchAnswer::Ungrouped(
                domain
                    .cards
                    .iter()
                    .take(usize::try_from(domain.cardinality.maximum.unwrap().0).unwrap())
                    .map(|card| card.handle)
                    .collect(),
            ))
        } else if let InputDomain::Order(cards) = &question.domain {
            Answer::Order(cards.iter().rev().map(|card| card.handle).collect())
        } else if let InputDomain::Targets(domain) = &question.domain {
            Answer::Targets(
                domain
                    .members
                    .iter()
                    .take(usize::try_from(domain.cardinality.maximum.unwrap().0).unwrap())
                    .map(|member| member.handle)
                    .collect(),
            )
        } else {
            panic!("unexpected decision")
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
}

fn play(game: &mut Game) {
    let step = game.play_internal_card("source", actor()).unwrap();
    finish(game, step);
}

fn triggers(game: &mut Game) {
    while let Some(question) = game.internal_pending_choice().unwrap() {
        let InputDomain::Pending(options) = &question.domain else {
            panic!("pending ability expected")
        };
        let step = game
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap();
        finish(game, step);
        game.resume_internal_card().unwrap();
    }
}

fn named(state: &Value, zone: &str, name: &str) -> usize {
    state["P1"][zone]
        .as_array()
        .unwrap()
        .iter()
        .filter(|id| state["objects"][id.as_str().unwrap()]["name"] == name)
        .count()
}

#[test]
fn bp10_013_searches_an_arcana_card_into_ex() {
    let mut initial = position("BP10-013");
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"wrong","card":"BP01-037"},{"id":"arcana","card":"BP10-086"}
    ]);
    let mut game = game(&initial);
    play(&mut game);
    triggers(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["ex"], json!(["arcana"]));
    assert_eq!(state["P1"]["deck"], json!(["wrong"]));
}

#[test]
fn bp10_014_evolution_searches_arcana_into_ex() {
    let mut initial = position("BP10-013");
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP10-013"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP10-014"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"arcana","card":"BP10-086"}]);
    let mut game = game(&initial);
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
        )
        .unwrap()
        .outcome,
        "resolved",
        "the printed evolution is legal"
    );
    triggers(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["ex"],
        json!(["arcana"])
    );
}

#[test]
fn bp10_021_searches_ilmisuna_directly_into_field() {
    let mut initial = position("BP10-021");
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"wrong","card":"BP01-037"},{"id":"merchant","card":"BP10-025"}
    ]);
    let mut game = game(&initial);
    play(&mut game);
    triggers(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert!(
        state["P1"]["field"]
            .as_array()
            .unwrap()
            .contains(&json!("merchant"))
    );
    assert_eq!(state["P1"]["deck"], json!(["wrong"]));
}

#[test]
fn bp10_012_search_requires_combo_three() {
    for prior in [0_usize, 2] {
        let mut initial = position("BP10-012");
        initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"copy","card":"BP10-012"}]);
        for n in 0..prior {
            initial["players"]["P1"]["zones"]["hand"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":format!("prior{n}"),"card":"BP01-171"}));
        }
        let mut game = game(&initial);
        for n in 0..prior {
            let step = game
                .play_internal_card(&format!("prior{n}"), actor())
                .unwrap();
            finish(&mut game, step);
            game.resume_internal_card().unwrap();
        }
        play(&mut game);
        triggers(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["hand"],
            if prior == 0 {
                json!([])
            } else {
                json!(["copy"])
            }
        );
    }
}

#[test]
fn bp10_018_counts_fairy_tokens_after_creating_two() {
    let mut initial = position("BP10-018");
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"existing","card":"BP01-T03"},{"id":"physical-fairy","card":"SD01-011"}
    ]);
    let mut game = game(&initial);
    play(&mut game);
    game.resume_internal_card().unwrap();
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(named(&state, "field", "フェアリー"), 3);
    assert_eq!(state["P2"]["leader"]["life"], 17_u8);
}

#[test]
fn bp10_025_merchant_bonus_excludes_every_copy_with_its_own_name() {
    for ally in [None, Some("BP10-025"), Some("BP10-021")] {
        let mut initial = position("BP10-025");
        if let Some(card) = ally {
            initial["players"]["P1"]["zones"]["field"] = json!([{"id":"merchant","card":card}]);
        }
        let mut game = game(&initial);
        play(&mut game);
        game.resume_internal_card().unwrap();
        let id = selection().current["BP10-025"]
            .program
            .card()
            .abilities
            .iter()
            .find(|ability| ability.node.kind == AbilityKind::Activated)
            .unwrap()
            .node
            .id;
        let step = game.activate_internal_card("source", id, actor()).unwrap();
        finish(&mut game, step);
        game.resume_internal_card().unwrap();
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["source"]["acted"], true);
        assert_eq!(
            state["P2"]["field"],
            if ally == Some("BP10-021") {
                json!([])
            } else {
                json!(["enemy"])
            }
        );
        if ally != Some("BP10-021") {
            assert_eq!(state["objects"]["enemy"]["damage"], 3_u8);
        }
    }
}
