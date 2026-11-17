#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate these game scenarios."
)]
use super::*;
use sve_engine::game::internal::SearchAnswer;

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
        } else if let InputDomain::CostMaterials(domain) = &question.domain {
            Answer::CostMaterials(
                domain
                    .members
                    .iter()
                    .take(usize::try_from(domain.cardinality.maximum.unwrap().0).unwrap())
                    .map(|member| member.handle)
                    .collect(),
            )
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

fn pass_quick(game: &mut Game) {
    let q = game.internal_quick_choice().unwrap().unwrap();
    let step = game
        .answer_internal_card(q.actor, q.token, Answer::Quick(None))
        .unwrap();
    finish(game, step);
}

#[test]
fn bp11_085_acts_enemy_draws_and_blocks_its_next_rule_stand() {
    let mut initial = position("BP11-085");
    initial["players"]["P2"]["zones"]["deck"] = json!([{ "id":"opp-draw", "card":"BP01-171" }]);
    let mut game = game(&initial);
    play(&mut game);
    game.resume_internal_card().unwrap();
    let before_turn = game.projection(View::Referee).unwrap();
    assert_eq!(before_turn["objects"]["enemy"]["acted"], true);
    assert_eq!(before_turn["P1"]["hand"], json!(["first"]));
    assert_eq!(
        game.internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap()
            .outcome,
        "resolved",
        "the current player can end the turn"
    );
    pass_quick(&mut game);
    let after_turn = game.projection(View::Referee).unwrap();
    assert_eq!(after_turn["turn"]["active"], "P2");
    assert_eq!(after_turn["objects"]["enemy"]["acted"], true);
}

#[test]
fn bp11_100_fanfare_checks_amulet_and_attack_draws_before_discarding() {
    for amulet in [false, true] {
        let mut initial = position("BP11-100");
        if amulet {
            initial["players"]["P1"]["zones"]["field"] = json!([{"id":"amulet","card":"BP01-016"}]);
        }
        let mut played = game(&initial);
        play(&mut played);
        triggers(&mut played);
        let state = played.projection(View::Referee).unwrap();
        assert_eq!(
            state["objects"]["source"]["power"],
            if amulet { 2_u8 } else { 1_u8 }
        );
        assert_eq!(
            state["objects"]["source"]["hp"],
            if amulet { 3_u8 } else { 2_u8 }
        );
    }
    let mut initial = position("BP11-100");
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP11-100"}]);
    let mut attacking = game(&initial);
    assert_eq!(
        attacking
            .internal_rule_action(&json!({"do":"attack","attacker":"source","target":"P2.leader"}))
            .unwrap()
            .outcome,
        "resolved",
        "the standing follower legally attacks"
    );
    triggers(&mut attacking);
    let after_attack = attacking.projection(View::Referee).unwrap();
    assert_eq!(after_attack["P1"]["hand"], json!([]));
    assert_eq!(after_attack["P1"]["cemetery"], json!(["first"]));
    assert_eq!(
        after_attack["P1"]["deck"],
        json!(["second", "third", "fourth"])
    );
}
