use super::event_cemetery::finish_checks;
use super::*;
use sve_engine::game::internal::{DecisionView, InstructionOutcome, SearchAnswer};

fn position() -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"source","card":"BP10-111"},
        {"id":"payment","card":"BP01-171"},
        {"id":"spell","card":"BP01-057"}
    ]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"first","card":"BP01-171"},
        {"id":"second","card":"BP01-171"},
        {"id":"different","card":"BP01-042"}
    ]);
    initial
}

fn game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "goddess",
    )
    .unwrap()
}

fn variant(evolved: bool, material: &str, found: &str, count: usize, occupied: usize) -> Value {
    let mut initial = position();
    initial["players"]["P1"]["zones"]["hand"][1]["card"] = json!(material);
    if evolved {
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP10-111"}]);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP10-112"}]);
        initial["players"]["P1"]["zones"]["cemetery"] =
            json!([{"id":"payment","card":material},{"id":"spell","card":"BP01-057"}]);
    }
    let mut deck = (0..count)
        .map(|n| json!({"id":format!("found{n}"),"card":found}))
        .collect::<Vec<_>>();
    deck.push(json!({"id":"different","card":"BP01-042"}));
    initial["players"]["P1"]["zones"]["deck"] = json!(deck);
    initial["players"]["P1"]["zones"]["ex"] = json!(
        (0..occupied)
            .map(|n| json!({"id":format!("old{n}"),"card":"BP01-171"}))
            .collect::<Vec<_>>()
    );
    initial
}

fn start(game: &mut Game, evolved: bool) -> CardPlay {
    if !evolved {
        return fanfare(game);
    }
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":3_u8}})
        )
        .unwrap()
        .outcome,
        "resolved",
        "the printed evolution costs three PP"
    );
    finish_checks(game);
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("Goddess evolution trigger")
    };
    let play = game
        .answer_internal_card(
            pending.actor,
            pending.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap();
    declare(game, play)
}

fn find(
    game: &mut Game,
    search: &DecisionView,
    card: &str,
    available: usize,
    take: usize,
) -> Activation {
    let InputDomain::Search(domain) = &search.domain else {
        panic!("paid-name search")
    };
    assert_eq!(
        domain.cards.len(),
        available,
        "only cards matching the material's rules names are offered"
    );
    assert!(
        domain.cards.iter().all(|candidate| candidate.card == card),
        "different names are absent even after the material changed generation"
    );
    assert!(
        game.internal_decision(View::P2).is_none(),
        "the deck search is private to its controller"
    );
    let handles = domain
        .cards
        .iter()
        .take(take)
        .map(|candidate| candidate.handle)
        .collect();
    let mut play = game
        .answer_internal_card(
            search.actor,
            search.token,
            Answer::Search(SearchAnswer::Ungrouped(handles)),
        )
        .unwrap();
    while let CardPlay::Awaiting(question) = play {
        let InputDomain::SearchPartition(allocation) = &question.domain else {
            panic!("EX capacity allocation")
        };
        let count = usize::try_from(allocation.cardinality.minimum.0).unwrap();
        play = game
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::SearchPartition(
                    allocation
                        .cards
                        .iter()
                        .take(count)
                        .map(|candidate| candidate.handle)
                        .collect(),
                ),
            )
            .unwrap();
    }
    resolved(play)
}

fn fanfare(game: &mut Game) -> CardPlay {
    let mut play = game
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap();
    while let CardPlay::Awaiting(question) = play {
        let InputDomain::WardEntry { .. } = question.domain else {
            panic!("printed Ward entry")
        };
        play = game
            .answer_internal_card(question.actor, question.token, Answer::WardEntry(false))
            .unwrap();
    }
    assert!(
        matches!(play, CardPlay::Entered(_)),
        "the base follower enters before its fanfare"
    );
    finish_checks(game);
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("Goddess fanfare")
    };
    let declared = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap();
    declare(game, declared)
}

fn declare(game: &mut Game, play: CardPlay) -> CardPlay {
    let CardPlay::Awaiting(question) = play else {
        panic!("the triggered cost declaration")
    };
    assert_eq!(
        question.domain,
        InputDomain::OptionalCost,
        "the head cost must be accepted before choosing materials"
    );
    game.answer_internal_card(question.actor, question.token, Answer::OptionalCost(true))
        .unwrap()
}

fn pay(game: &mut Game, play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(question) = play else {
        panic!("mandatory follower payment")
    };
    assert!(
        game.internal_decision(View::P2).is_none(),
        "only the payer chooses its materials"
    );
    let InputDomain::CostMaterials(domain) = &question.domain else {
        panic!("cost material selection")
    };
    assert_eq!(
        domain.members.len(),
        1,
        "the spell is not a follower payment"
    );
    let CardPlay::Awaiting(search) = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::CostMaterials(vec![domain.members[0].handle]),
        )
        .unwrap()
    else {
        panic!("same-name search after payment")
    };
    search
}

#[test]
fn goddess_reveals_a_hand_follower_then_searches_only_its_name_using_the_payment_snapshot() {
    let mut game = game(&position());
    let play = fanfare(&mut game);
    assert!(
        !game.projection(View::P2).unwrap()["known_cards"]
            .as_array()
            .unwrap()
            .iter()
            .any(|card| card["id"] == "payment"),
        "the hand material is private before the reveal cost is paid"
    );
    let search = pay(&mut game, play);
    assert!(
        game.projection(View::P2).unwrap()["known_cards"]
            .as_array()
            .unwrap()
            .iter()
            .any(|card| card["id"] == "payment"),
        "the cost publishes the selected follower before search resolution"
    );
    let InputDomain::Search(domain) = &search.domain else {
        panic!("same-name deck candidates")
    };
    assert_eq!(
        domain.cards.len(),
        2,
        "only the two cards matching the revealed follower are offered"
    );
    assert!(
        domain.cards.iter().all(|card| card.card == "BP01-171"),
        "the source Goddess name and different-name follower are excluded"
    );
    let result = resolved(
        game.answer_internal_card(
            search.actor,
            search.token,
            Answer::Search(SearchAnswer::Ungrouped(vec![domain.cards[0].handle])),
        )
        .unwrap(),
    );
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["hand"], json!(["payment", "spell"]));
    assert_eq!(view["P1"]["ex"].as_array().unwrap().len(), 1);
    assert!(result.progress.receipts.values().any(|receipt| matches!(receipt, InstructionOutcome::Payment { subjects, characters, .. } if subjects.len() == 1 && subjects[0].generation.0 == 0 && characters.len() == 1 && characters[0].names.iter().any(|name| name.0 == "ゴブリン"))), "the public payment retains the original generation and rules names");
}

#[test]
fn goddess_evolution_banishes_one_grave_follower_and_searches_zero_one_or_two_copies() {
    for (available, take) in [(0_usize, 0_usize), (1, 1), (3, 0), (3, 1), (3, 2)] {
        let mut game = game(&variant(true, "BP01-171", "BP01-171", available, 0));
        let play = start(&mut game, true);
        let search = pay(&mut game, play);
        let result = find(&mut game, &search, "BP01-171", available, take);
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["P1"]["ex"].as_array().unwrap().len(), take);
        assert_eq!(view["P1"]["banish"], json!(["payment"]));
        assert_eq!(view["objects"]["payment"]["generation"], 1_u8);
        assert_eq!(view["P1"]["cemetery"], json!(["spell"]));
        assert!(result.progress.receipts.values().any(|receipt| matches!(receipt, InstructionOutcome::Payment { subjects, characters, .. } if subjects.len() == 1 && subjects[0].generation.0 == 0 && characters.len() == 1 && characters[0].names.iter().any(|name| name.0 == "ゴブリン"))), "search uses the grave member's paid-before snapshot");
    }
}

#[test]
fn goddess_search_preserves_cards_when_ex_has_one_or_no_free_slots() {
    for evolved in [false, true] {
        for occupied in [4_usize, 5] {
            let count = if evolved { 2_usize } else { 1 };
            let mut game = game(&variant(evolved, "BP01-171", "BP01-171", count, occupied));
            let play = start(&mut game, evolved);
            let search = pay(&mut game, play);
            find(&mut game, &search, "BP01-171", count, count);
            let view = game.projection(View::Referee).unwrap();
            let moved = count.min(5 - occupied);
            assert_eq!(view["P1"]["ex"].as_array().unwrap().len(), occupied + moved);
            assert_eq!(
                view["P1"]["deck"].as_array().unwrap().len(),
                count + 1 - moved,
                "unplaced finds stay in the deck"
            );
        }
    }
}

#[test]
fn goddess_rejects_incomplete_material_payment_without_starting_the_search() {
    for evolved in [false, true] {
        let mut game = game(&variant(evolved, "BP01-171", "BP01-171", 2, 0));
        let CardPlay::Awaiting(payment) = start(&mut game, evolved) else {
            panic!("mandatory material")
        };
        assert!(
            game.answer_internal_card(payment.actor, payment.token, Answer::CostMaterials(vec![]))
                .is_err(),
            "the mandatory one-card cost cannot be paid with zero cards"
        );
        assert_eq!(
            game.internal_decision(View::P1).unwrap().token,
            payment.token,
            "an invalid payment preserves the material question"
        );
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["ex"],
            json!([])
        );
        let search = pay(&mut game, CardPlay::Awaiting(payment));
        find(&mut game, &search, "BP01-171", 2, 0);
    }
}

#[test]
#[ignore = "Pending canonical card-name/collaboration-name identity data for Q1355/Q1356; not a payment-name DSL gap"]
fn goddess_uses_quon_and_koyori_rules_aliases_in_both_payment_directions() {
    for evolved in [false, true] {
        for (material, found) in [("BP06-035", "BP06-036"), ("BP06-036", "BP06-035")] {
            let count = if evolved { 2_usize } else { 1 };
            let mut game = game(&variant(evolved, material, found, count, 0));
            let play = start(&mut game, evolved);
            let search = pay(&mut game, play);
            find(&mut game, &search, found, count, count);
            assert_eq!(
                game.projection(View::Referee).unwrap()["P1"]["ex"]
                    .as_array()
                    .unwrap()
                    .len(),
                count,
                "Q1355/Q1356 allow the alternate printed name"
            );
        }
    }
}
