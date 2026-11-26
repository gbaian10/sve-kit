use super::*;
use alloc::collections::BTreeSet;
use sve_engine::game::internal::SearchAnswer;

fn position(card: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"enemy","card":"BP01-037","state":{"power":3_u8,"hp":10_u8,"max_hp":10_u8}}
    ]);
    for seat in ["P1", "P2"] {
        initial["players"][seat]["zones"]["deck"] = json!([
            {"id":format!("{seat}-top"),"card":"BP01-171"},
            {"id":format!("{seat}-tail"),"card":"BP01-171"}
        ]);
    }
    initial
}

fn finish(game: &mut Game, mut play: CardPlay, modes: &[ModeId]) {
    while let CardPlay::Awaiting(question) = play {
        let answer = if let InputDomain::Targets(domain) = &question.domain {
            let limit = domain
                .cardinality
                .maximum
                .unwrap_or(domain.cardinality.minimum);
            let mut names = BTreeSet::new();
            Answer::Targets(
                domain
                    .members
                    .iter()
                    .filter(|member| !domain.distinct_names || names.insert(member.name.clone()))
                    .take(usize::try_from(limit.0).unwrap())
                    .map(|m| m.handle)
                    .collect(),
            )
        } else if let InputDomain::CostMaterials(domain) = &question.domain {
            Answer::CostMaterials(
                domain
                    .members
                    .iter()
                    .take(usize::try_from(domain.cardinality.minimum.0).unwrap())
                    .map(|m| m.handle)
                    .collect(),
            )
        } else if let InputDomain::Search(domain) = &question.domain {
            Answer::Search(SearchAnswer::Ungrouped(
                domain
                    .cards
                    .iter()
                    .take(usize::try_from(domain.cardinality.maximum.unwrap_or(Nat(1)).0).unwrap())
                    .map(|m| m.handle)
                    .collect(),
            ))
        } else if let InputDomain::Order(cards) = &question.domain {
            Answer::Order(cards.iter().map(|m| m.handle).collect())
        } else if matches!(&question.domain, InputDomain::Modes { .. }) {
            Answer::Modes(modes.to_vec())
        } else if matches!(&question.domain, InputDomain::OptionalCost) {
            Answer::OptionalCost(true)
        } else if matches!(&question.domain, InputDomain::WardEntry { .. }) {
            Answer::WardEntry(false)
        } else if matches!(&question.domain, InputDomain::TokenCapacity { .. }) {
            Answer::TokenCapacity(0)
        } else {
            panic!("unexpected decision: {:?}", question.domain)
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
}

fn settle(game: &mut Game) {
    loop {
        while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
        let Some(question) = game.internal_pending_choice().unwrap() else {
            break;
        };
        let InputDomain::Pending(options) = &question.domain else {
            panic!("pending trigger")
        };
        let play = game
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap();
        finish(game, play, &[ModeId(1)]);
    }
}

fn evolved_position(base: &str, form: &str) -> Value {
    let mut initial = position(base);
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":base}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
    initial
}

fn evolve(game: &mut Game, pp: u8) {
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":pp}})
        )
        .unwrap()
        .outcome,
        "resolved",
        "the printed evolution cost permits this form"
    );
    settle(game);
}

#[test]
fn ecp02_007_returns_a_deremas_card_then_bottoms_an_enemy() {
    for form in ["ECP02-007", "ECP02-P02"] {
        let mut initial = evolved_position("ECP02-006", form);
        initial["players"]["P1"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"fuel","card":"CP02-007"}));
        let mut game = game(&initial);
        evolve(&mut game, 1);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["hand"].as_array().unwrap().len(), 1);
        assert_eq!(
            state["P2"]["deck"].as_array().unwrap().last(),
            Some(&json!("enemy"))
        );
    }
}
