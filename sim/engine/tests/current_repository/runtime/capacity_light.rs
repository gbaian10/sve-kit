#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "The printed cemetery seeds have explicit target, payment and ordering choices."
)]
use super::*;
use sve_engine::game::internal::DecisionView;

fn awaiting(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(question) = play else {
        panic!("printed seed choice")
    };
    question
}

fn pending(game: &mut Game, card: &str) -> CardPlay {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("printed pending")
    };
    let handle = options
        .iter()
        .find(|option| option.card == card)
        .unwrap()
        .handle;
    game.answer_internal_card(question.actor, question.token, Answer::Pending(handle))
        .unwrap()
}

fn finish(game: &mut Game, mut play: CardPlay, take: usize) -> CardPlay {
    while let CardPlay::Awaiting(question) = play {
        let answer = match &question.domain {
            InputDomain::OptionalCost => Answer::OptionalCost(true),
            InputDomain::Targets(domain) => Answer::Targets(
                domain
                    .members
                    .iter()
                    .take(take)
                    .map(|member| member.handle)
                    .collect(),
            ),
            InputDomain::SearchPartition(domain) => Answer::SearchPartition(
                domain
                    .cards
                    .iter()
                    .take(usize::try_from(domain.cardinality.minimum.0).unwrap())
                    .map(|card| card.handle)
                    .collect(),
            ),
            InputDomain::Order(cards) => {
                Answer::Order(cards.iter().rev().map(|card| card.handle).collect())
            }
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            other => panic!("unexpected printed seed choice {other:?}"),
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    play
}

fn truth_initial() -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP08-041"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"a","card":"BP01-046"}, {"id":"b","card":"BP11-023"},
        {"id":"wrong","card":"BP01-037","state":{"cost":1_u8}},
        {"id":"d","card":"BP11-109"}, {"id":"e","card":"BP01-043"}
    ]);
    initial
}

#[test]
fn bp08_041_looks_for_original_cost_cards_and_reduces_their_ex_play_cost() {
    let mut game = game(&truth_initial());
    let question = awaiting(
        game.play_internal_card("source", PlayerId { seat: 1 })
            .unwrap(),
    );
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("look cost selection")
    };
    assert_eq!(
        domain.members.len(),
        4,
        "the original five-cost card is excluded despite its changed cost"
    );
    assert!(game.internal_decision(View::P2).is_none());
    let next = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Targets(
                domain
                    .members
                    .iter()
                    .take(2)
                    .map(|member| member.handle)
                    .collect(),
            ),
        )
        .unwrap();
    resolved(finish(&mut game, next, 2));
    game.resume_internal_card().unwrap();
    let selected = game.projection(View::Referee).unwrap();
    assert_eq!(selected["P1"]["ex"], json!(["a", "b"]));
    assert_eq!(selected["P1"]["deck"], json!(["e", "d", "wrong"]));
    let entering = game.play_internal_card("a", PlayerId { seat: 1 }).unwrap();
    let completed = finish(&mut game, entering, 1);
    assert!(matches!(completed, CardPlay::Entered(_)));
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        5_u8
    );
}

#[test]
fn bp08_041_pays_its_cemetery_copy_and_adds_ep_above_three_on_raio_entry() {
    let mut initial = draw_position();
    initial["players"]["P1"]["ep"] = json!(3_u8);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"raio","card":"BP05-035"}]);
    initial["players"]["P1"]["zones"]["cemetery"] = json!([{"id":"source","card":"BP08-041"}]);
    let mut game = game(&initial);
    assert!(matches!(
        game.play_internal_card("raio", PlayerId { seat: 1 })
            .unwrap(),
        CardPlay::Entered(_)
    ));
    let grave = pending(&mut game, "BP08-041");
    resolved(finish(&mut game, grave, 1));
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["ep"], 4_u8);
    assert_eq!(state["P1"]["banish"], json!(["source"]));
}

#[test]
fn bp08_041_allocates_one_ex_slot_and_returns_the_unplaced_choice_to_deck() {
    let mut initial = truth_initial();
    initial["players"]["P1"]["zones"]["ex"] = json!([{"filler":4_u8}]);
    let mut game = game(&initial);
    let question = awaiting(
        game.play_internal_card("source", PlayerId { seat: 1 })
            .unwrap(),
    );
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("capacity probe look selection")
    };
    let answer = Answer::Targets(
        domain
            .members
            .iter()
            .take(2)
            .map(|member| member.handle)
            .collect(),
    );
    let result = game
        .answer_internal_card(question.actor, question.token, answer)
        .unwrap();
    resolved(finish(&mut game, result, 2));
    game.resume_internal_card().unwrap();
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        projected_zone_count(&state["P1"]["ex"]),
        5_u64,
        "only one selected card fits"
    );
    assert_eq!(
        state["P1"]["deck"],
        json!(["e", "d", "wrong", "b"]),
        "the unallocated choice joins the other unplaced cards at the bottom"
    );
    assert_eq!(state["P1"]["pp"]["current"], 5_u8, "the spell remains paid");
    let entering = game.play_internal_card("a", PlayerId { seat: 1 }).unwrap();
    assert!(
        matches!(finish(&mut game, entering, 1), CardPlay::Entered(_)),
        "the allocated card can be played through its Ward choice"
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        5_u8,
        "the discount follows the successful arrival"
    );
}

#[test]
fn bp20_088_evolves_then_allocates_the_last_ex_slot_and_waives_only_that_play_cost() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP20-087"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP20-088"}]);
    initial["players"]["P1"]["zones"]["cemetery"] = json!([
        {"id":"first","card":"BP06-084"},{"id":"second","card":"BP06-084","state":{"cost":9_u8}},
        {"id":"wrong_trait","card":"BP01-046"},{"id":"too_expensive","card":"BP06-073"}
    ]);
    initial["players"]["P1"]["zones"]["ex"] = json!([{"filler":4_u8}]);
    let mut game = game(&initial);
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
        )
        .unwrap()
        .outcome,
        "resolved"
    );
    let triggered = pending(&mut game, "BP20-088");
    let question = awaiting(triggered);
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("original-cost yokai targets")
    };
    assert_eq!(
        domain.members.len(),
        3,
        "the trait filters candidates before the joint original-cost bound"
    );
    let expensive = domain
        .members
        .iter()
        .find(|member| member.card.as_deref() == Some("BP06-073"))
        .unwrap()
        .handle;
    let before = game.projection(View::Referee).unwrap();
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::Targets(vec![expensive]),
    )
    .unwrap_err();
    assert_eq!(
        game.projection(View::Referee).unwrap(),
        before,
        "a six-cost yokai exceeds the sum even on its own"
    );
    let selected = domain
        .members
        .iter()
        .filter(|member| member.card.as_deref() == Some("BP06-084"))
        .map(|member| member.handle)
        .collect();
    let next = game
        .answer_internal_card(question.actor, question.token, Answer::Targets(selected))
        .unwrap();
    resolved(finish(&mut game, next, 2));
    game.resume_internal_card().unwrap();
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(projected_zone_count(&state["P1"]["ex"]), 5_u64);
    assert!(
        state["P1"]["cemetery"]
            .as_array()
            .unwrap()
            .contains(&json!("second"))
    );
    let pp_before_play = state["P1"]["pp"]["current"].clone();
    assert!(matches!(
        game.play_internal_card("first", PlayerId { seat: 1 })
            .unwrap(),
        CardPlay::Entered(_)
    ));
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        pp_before_play
    );
}

fn moon_initial() -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["cemetery"] = json!([
        {"id":"source","card":"BP09-105"},{"id":"sun","card":"BP09-108"},
        {"id":"sun2","card":"BP09-108"},{"id":"moon","card":"BP09-109"}
    ]);
    initial
}

#[test]
fn bp09_105_requires_both_names_before_paying_and_then_allocates_the_last_field_slot() {
    let ability = selection().current["BP09-105"].program.card().abilities[0]
        .node
        .id;
    for missing_card in [Some("BP09-108"), Some("BP09-109"), None] {
        let mut initial = moon_initial();
        initial["players"]["P1"]["zones"]["field"] = json!([{"filler":4_u8}]);
        if let Some(missing) = missing_card {
            initial["players"]["P1"]["zones"]["cemetery"]
                .as_array_mut()
                .unwrap()
                .retain(|object| object["card"] != missing);
        }
        let mut game = game(&initial);
        let before_attempt = game.projection(View::Referee).unwrap();
        let attempt = game.activate_internal_card("source", ability, PlayerId { seat: 1 });
        if missing_card.is_some() {
            assert!(
                matches!(attempt, Err(EngineFailure::IllegalPlay(_))),
                "a direct activated ability cannot be declared without both names"
            );
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state, before_attempt,
                "the whole rejected declaration is atomic"
            );
            assert_eq!(state["P1"]["pp"]["current"], 10_u8);
            assert_eq!(state["P1"]["banish"], json!([]));
            assert!(
                game.internal_decision(View::P1).is_none(),
                "a rejected declaration leaves no target question"
            );
            continue;
        }
        let question = awaiting(attempt.unwrap());
        let InputDomain::Targets(domain) = &question.domain else {
            panic!("named pair")
        };
        assert_eq!(domain.members.len(), 3);
        let before = game.projection(View::Referee).unwrap();
        let suns: Vec<_> = domain
            .members
            .iter()
            .filter(|member| member.card.as_deref() == Some("BP09-108"))
            .map(|member| member.handle)
            .collect();
        game.answer_internal_card(question.actor, question.token, Answer::Targets(suns))
            .unwrap_err();
        assert_eq!(game.projection(View::Referee).unwrap(), before);
        let sun = domain
            .members
            .iter()
            .find(|member| member.card.as_deref() == Some("BP09-108"))
            .unwrap()
            .handle;
        let moon = domain
            .members
            .iter()
            .find(|member| member.card.as_deref() == Some("BP09-109"))
            .unwrap()
            .handle;
        let capacity = awaiting(
            game.answer_internal_card(
                question.actor,
                question.token,
                Answer::Targets(vec![sun, moon]),
            )
            .unwrap(),
        );
        let InputDomain::SearchPartition(allocation) = &capacity.domain else {
            panic!("one of two named targets may enter")
        };
        assert_eq!(allocation.cards.len(), 2);
        assert_eq!(allocation.cardinality.minimum.0, 1);
        let moon_arrival = allocation
            .cards
            .iter()
            .find(|card| card.card == "BP09-109")
            .unwrap()
            .handle;
        let next = game
            .answer_internal_card(
                capacity.actor,
                capacity.token,
                Answer::SearchPartition(vec![moon_arrival]),
            )
            .unwrap();
        resolved(finish(&mut game, next, 1));
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(projected_zone_count(&state["P1"]["field"]), 5_u64);
        assert_eq!(state["P1"]["cemetery"], json!(["sun", "sun2"]));
        assert_eq!(state["P1"]["banish"], json!(["source"]));
        assert_eq!(state["P1"]["pp"]["current"], 6_u8);
    }
}

#[test]
fn bp09_105_spell_searches_for_either_name_and_puts_the_selected_card_on_the_field() {
    use sve_engine::game::internal::SearchAnswer;
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP09-105"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"sun","card":"BP09-108"},{"id":"moon","card":"BP09-109"},{"id":"other","card":"BP01-046"}
    ]);
    let mut game = game(&initial);
    let question = awaiting(
        game.play_internal_card("source", PlayerId { seat: 1 })
            .unwrap(),
    );
    let InputDomain::Search(domain) = &question.domain else {
        panic!("named deck search")
    };
    assert_eq!(domain.cards.len(), 2);
    let moon = domain
        .cards
        .iter()
        .find(|card| card.card == "BP09-109")
        .unwrap()
        .handle;
    let next = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Search(SearchAnswer::Ungrouped(vec![moon])),
        )
        .unwrap();
    resolved(finish(&mut game, next, 1));
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["field"], json!(["moon"]));
    assert_eq!(state["P1"]["cemetery"], json!(["source"]));
    assert_eq!(state["P1"]["pp"]["current"], 8_u8);
}
