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
        } else if matches!(&question.domain, InputDomain::Modes { .. }) {
            Answer::Modes(vec![ModeId(1)])
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

fn evolved_position(base: &str, form: &str) -> Value {
    let mut initial = position(base);
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":base}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
    initial
}

#[test]
fn bp15_089_real_evolution_revives_rulunai_or_damages_and_lastword_mills_one() {
    for mode in [ModeId(1), ModeId(2)] {
        let mut initial = evolved_position("BP15-088", "BP15-089");
        initial["players"]["P1"]["zones"]["cemetery"] = json!([{"id":"rulunai","card":"BP20-074"}]);
        initial["players"]["P2"]["zones"]["field"][0]["state"] = json!({"acted":true});
        let mut game = game(&initial);
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
            )
            .unwrap()
            .outcome,
            "resolved",
            "the real base evolves for one PP"
        );
        let pending = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = &pending.domain else {
            panic!("evolution ability")
        };
        let CardPlay::Awaiting(question) = game
            .answer_internal_card(
                pending.actor,
                pending.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap()
        else {
            panic!("mode choice")
        };
        assert!(
            matches!(&question.domain, InputDomain::Modes { .. }),
            "the evolved follower offers its printed modes"
        );
        let step = game
            .answer_internal_card(question.actor, question.token, Answer::Modes(vec![mode]))
            .unwrap();
        finish(&mut game, step);
        game.resume_internal_card().unwrap();
        let before = game.projection(View::Referee).unwrap();
        assert_eq!(
            before["objects"]["enemy"]["damage"],
            if mode == ModeId(2) { 2_u8 } else { 0_u8 }
        );
        assert_eq!(
            before["P1"]["field"]
                .as_array()
                .unwrap()
                .contains(&json!("rulunai")),
            mode == ModeId(1)
        );
        if mode == ModeId(2) {
            assert_eq!(
                game.internal_rule_action(
                    &json!({"do":"attack","attacker":"source","target":"enemy"})
                )
                .unwrap()
                .outcome,
                "resolved",
                "the evolved follower legally attacks the rested enemy"
            );
            pass_quick(&mut game);
            triggers(&mut game);
            let after = game.projection(View::Referee).unwrap();
            assert!(
                after["P1"]["cemetery"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("first")),
                "the Lastword puts the real top card into the cemetery"
            );
            assert_eq!(after["P1"]["deck"], json!(["second", "third", "fourth"]));
        }
    }
}
