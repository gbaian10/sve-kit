#![expect(
    clippy::panic,
    reason = "Unexpected choices invalidate the printed-card scenarios."
)]
use super::*;

const ACTOR: PlayerId = PlayerId { seat: 1 };

fn position(card: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"first","card":"BP01-171"}, {"id":"second","card":"BP01-171"}
    ]);
    initial
}

fn finish(game: &mut Game, mut step: CardPlay) {
    while let CardPlay::Awaiting(question) = step {
        let answer = if let InputDomain::Targets(domain) = &question.domain {
            Answer::Targets(
                domain
                    .members
                    .iter()
                    .take(
                        usize::try_from(
                            domain
                                .cardinality
                                .maximum
                                .unwrap_or(domain.cardinality.minimum)
                                .0,
                        )
                        .unwrap(),
                    )
                    .map(|member| member.handle)
                    .collect(),
            )
        } else if let InputDomain::CostMaterials(domain) = &question.domain {
            Answer::CostMaterials(
                domain
                    .members
                    .iter()
                    .take(
                        usize::try_from(
                            domain
                                .cardinality
                                .maximum
                                .unwrap_or(domain.cardinality.minimum)
                                .0,
                        )
                        .unwrap(),
                    )
                    .map(|member| member.handle)
                    .collect(),
            )
        } else if matches!(question.domain, InputDomain::OptionalCost) {
            Answer::OptionalCost(true)
        } else if matches!(question.domain, InputDomain::WardEntry { .. }) {
            Answer::WardEntry(false)
        } else if matches!(question.domain, InputDomain::Modes { .. }) {
            Answer::Modes(vec![ModeId(1)])
        } else {
            panic!("unexpected choice")
        };
        step = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    checks(game);
}

fn play(game: &mut Game, id: &str) {
    let step = game.play_internal_card(id, ACTOR).unwrap();
    finish(game, step);
}

fn trigger(game: &mut Game, card: &str) {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("expected pending ability")
    };
    let chosen = options
        .iter()
        .filter(|option| option.card == card)
        .min_by_key(|option| option.line)
        .unwrap();
    let step = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(chosen.handle),
        )
        .unwrap();
    finish(game, step);
}

fn activate(game: &mut Game, card: &str) {
    let ability = selection().current[card]
        .program
        .card()
        .abilities
        .iter()
        .find(|ability| {
            ability.node.kind == AbilityKind::Activated
                && !matches!(
                    &ability.node.source.origin,
                    AbilityOrigin::Keyword {
                        keyword: KeywordId::Evolution,
                        ..
                    }
                )
        })
        .unwrap()
        .node
        .id;
    let step = game
        .activate_internal_card("source", ability, ACTOR)
        .unwrap();
    finish(game, step);
}

#[test]
fn eleanor_banishes_a_witch_to_draw_then_uses_the_five_and_ten_card_damage_thresholds() {
    for banished in [4_u8, 5, 9, 10] {
        let mut initial = position("BP17-039");
        initial["players"]["P1"]["zones"]["cemetery"] =
            json!([{"id":"material","card":"BP01-060"}]);
        initial["players"]["P1"]["zones"]["banish"] = json!(
            (0_u8..(banished - 1))
                .map(|i| json!({"id":format!("banished-{i}"),"card":"BP01-171"}))
                .collect::<Vec<_>>()
        );
        let mut game = game(&initial);
        play(&mut game, "source");
        trigger(&mut game, "BP17-039");
        let drawn = game.projection(View::Referee).unwrap();
        assert_eq!(drawn["P1"]["hand"], json!(["first"]));
        assert_eq!(
            drawn["P1"]["banish"].as_array().unwrap().len(),
            usize::from(banished)
        );
        assert_eq!(drawn["P1"]["cemetery"], json!([]));
        activate(&mut game, "BP17-039");
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["objects"]["enemy"]["damage"],
            if banished >= 10 {
                4_u8
            } else {
                u8::from(banished >= 5)
            }
        );
        assert_eq!(state["objects"]["source"]["acted"], true);
    }
}
