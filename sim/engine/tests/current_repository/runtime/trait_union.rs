#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate the printed scenarios."
)]
use super::*;
use sve_engine::game::internal::DecisionView;

const PLAYER: PlayerId = PlayerId { seat: 1 };

fn rules_game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "trait-union",
    )
    .unwrap()
}

fn position(card: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"draw","card":"BP01-037"},
        {"id":"draw_b","card":"BP01-037"}
    ]);
    initial
}

/// Card numbers offered by a target or material question, sorted.
fn offered(question: &DecisionView) -> Vec<String> {
    let members = match &question.domain {
        InputDomain::Targets(domain) | InputDomain::CostMaterials(domain) => &domain.members,
        other => panic!("expected a card choice, got {other:?}"),
    };
    let mut cards = members
        .iter()
        .map(|member| member.card.clone().unwrap())
        .collect::<Vec<_>>();
    cards.sort_unstable();
    cards
}

fn pick(game: &mut Game, question: &DecisionView, card: &str) -> CardPlay {
    let (members, cost) = match &question.domain {
        InputDomain::Targets(domain) => (&domain.members, false),
        InputDomain::CostMaterials(domain) => (&domain.members, true),
        other => panic!("expected a card choice, got {other:?}"),
    };
    let handle = members
        .iter()
        .find(|member| member.card.as_deref() == Some(card))
        .unwrap_or_else(|| panic!("{card} must be offered"))
        .handle;
    let answer = if cost {
        Answer::CostMaterials(vec![handle])
    } else {
        Answer::Targets(vec![handle])
    };
    game.answer_internal_card(question.actor, question.token, answer)
        .unwrap()
}

/// Plays the source and takes its fanfare; Ward entry and optional payment prompts are accepted.
fn fanfare(game: &mut Game, card: &str) -> CardPlay {
    let mut entry = game.play_internal_card("source", PLAYER).unwrap();
    while let CardPlay::Awaiting(question) = &entry
        && matches!(question.domain, InputDomain::WardEntry { .. })
    {
        entry = game
            .answer_internal_card(question.actor, question.token, Answer::WardEntry(false))
            .unwrap();
    }
    assert!(
        matches!(entry, CardPlay::Entered(_)),
        "{card} enters before its fanfare"
    );
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("expected the fanfare of {card}");
    };
    let mut step = game
        .answer_internal_card(
            pending.actor,
            pending.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap();
    while let CardPlay::Awaiting(question) = &step
        && matches!(question.domain, InputDomain::OptionalCost)
    {
        step = game
            .answer_internal_card(question.actor, question.token, Answer::OptionalCost(true))
            .unwrap();
    }
    step
}

fn awaiting(step: CardPlay, why: &str) -> DecisionView {
    let CardPlay::Awaiting(question) = step else {
        panic!("{why}");
    };
    question
}

#[test]
fn bp13_051_returns_a_cheap_beast_or_magic_creature_from_the_cemetery() {
    let mut initial = position("BP13-051");
    initial["players"]["P1"]["zones"]["cemetery"] = json!([
        {"id":"beast","card":"BP12-076"},
        {"id":"creature","card":"BP19-050"},
        {"id":"both","card":"BP16-050"},
        {"id":"expensive","card":"DSD01a-010"},
        {"id":"other","card":"BP01-037"}
    ]);
    let mut game = rules_game(&initial);
    let question = awaiting(fanfare(&mut game, "BP13-051"), "the cemetery target");
    assert_eq!(
        offered(&question),
        ["BP12-076", "BP16-050", "BP19-050"],
        "a card with both traits is one candidate; cost 4 and other traits are excluded"
    );
    resolved(pick(&mut game, &question, "BP19-050"));
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["field"], json!(["source", "creature"]));
}

#[test]
fn bp10_121_discards_an_angel_or_fallen_angel_to_heal_draw_and_recover() {
    for printing in ["BP10-121", "BP10-P28"] {
        let mut initial = position(printing);
        initial["players"]["P1"]["zones"]["hand"] = json!([
            {"id":"source","card":printing},
            {"id":"angel","card":"BP01-176"},
            {"id":"fallen","card":"BP09-114"},
            {"id":"other","card":"BP01-037"}
        ]);
        initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
        let mut game = rules_game(&initial);
        let question = awaiting(fanfare(&mut game, printing), "the discard cost");
        assert_eq!(offered(&question), ["BP01-176", "BP09-114"]);
        let paid = game.projection(View::Referee).unwrap()["P1"]["pp"]["current"]
            .as_u64()
            .unwrap();
        resolved(pick(&mut game, &question, "BP09-114"));
        let state = game.projection(View::Referee).unwrap();
        assert!(state["P1"]["cemetery"].as_array().unwrap().contains(&json!("fallen")));
        assert_eq!(state["P1"]["leader"]["life"], 13_u8);
        assert!(state["P1"]["hand"].as_array().unwrap().contains(&json!("draw")));
        assert_eq!(state["P1"]["pp"]["current"].as_u64().unwrap(), paid + 2);
    }
}

#[test]
fn bp14_107_banishes_a_demon_or_demon_lord_to_destroy_or_gain_storm() {
    for printing in ["BP14-107", "BP14-SL23", "BP14-U07"] {
        let mut initial = position(printing);
        initial["players"]["P1"]["zones"]["cemetery"] = json!([
            {"id":"demon","card":"BP01-167"},
            {"id":"lord","card":"BP02-116"},
            {"id":"other","card":"BP01-037"}
        ]);
        initial["players"]["P2"]["zones"]["field"] = json!([
            {"id":"enemy","card":"BP01-037"},
            {"id":"enemy_b","card":"BP01-037"}
        ]);
        let mut game = rules_game(&initial);
        let mut step = fanfare(&mut game, printing);
        // The fanfare declares its targets and pays one banished card, in either order.
        loop {
            let question = awaiting(step, "the fanfare target or cost");
            step = match &question.domain {
                InputDomain::Targets(domain) => {
                    let handles = domain.members.iter().map(|member| member.handle).collect();
                    game.answer_internal_card(question.actor, question.token, Answer::Targets(handles))
                        .unwrap()
                }
                InputDomain::CostMaterials(_) => {
                    assert_eq!(offered(&question), ["BP01-167", "BP02-116"]);
                    let next = pick(&mut game, &question, "BP02-116");
                    resolved(next);
                    break;
                }
                other => panic!("unexpected question {other:?}"),
            };
        }
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P2"]["field"], json!([]), "both enemy cards are destroyed");
        assert!(state["P1"]["banish"].as_array().unwrap().contains(&json!("lord")));

        while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
        let ability = selection_with_rules().current[printing]
            .program
            .card()
            .abilities[1]
            .node
            .id;
        let question = awaiting(
            game.activate_internal_card("source", ability, PLAYER).unwrap(),
            "the activation cost",
        );
        assert_eq!(offered(&question), ["BP01-167"]);
        resolved(pick(&mut game, &question, "BP01-167"));
        let activated = game.projection(View::Referee).unwrap();
        assert!(
            activated["objects"]["source"]["keywords"]
                .as_array()
                .unwrap()
                .contains(&json!("疾走"))
        );
    }
}
