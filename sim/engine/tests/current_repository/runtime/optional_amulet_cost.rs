#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate these game scenarios."
)]
use super::*;
use sve_engine::game::internal::SearchAnswer;

fn position(card: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([]);
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

fn finish(game: &mut Game, mut play: CardPlay, paid: bool) {
    while let CardPlay::Awaiting(question) = play {
        let answer = if matches!(&question.domain, InputDomain::WardEntry { .. }) {
            Answer::WardEntry(false)
        } else if matches!(&question.domain, InputDomain::OptionalCost) {
            Answer::OptionalCost(paid)
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
        finish(game, step, true);
        game.resume_internal_card().unwrap();
    }
}

#[test]
fn bp13_098_optional_amulet_payment_replaces_three_damage_with_five() {
    for paid in [false, true] {
        let mut initial = position("BP13-098");
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"amulet","card":"BP01-016"}]);
        let mut game = game(&initial);
        let mut step = game.play_internal_card("source", actor()).unwrap();
        while let CardPlay::Awaiting(question) = step {
            let answer = if matches!(&question.domain, InputDomain::OptionalCost) {
                Answer::OptionalCost(paid)
            } else if let InputDomain::CostMaterials(domain) = &question.domain {
                Answer::CostMaterials(vec![domain.members[0].handle])
            } else if let InputDomain::Targets(domain) = &question.domain {
                Answer::Targets(vec![domain.members[0].handle])
            } else {
                panic!("spell payment or target")
            };
            step = game
                .answer_internal_card(question.actor, question.token, answer)
                .unwrap();
        }
        game.resume_internal_card().unwrap();
        triggers(&mut game);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P2"]["field"],
            if paid { json!([]) } else { json!(["enemy"]) }
        );
        if !paid {
            assert_eq!(state["objects"]["enemy"]["damage"], 3_u8);
        }
        assert_eq!(
            state["P1"]["field"],
            if paid { json!([]) } else { json!(["amulet"]) }
        );
        assert_eq!(state["P1"]["pp"]["current"], 8_u8);
    }
}
