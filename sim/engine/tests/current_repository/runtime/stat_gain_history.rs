use super::*;
use sve_engine::game::internal::{DecisionView, SearchAnswer};

fn evolve(board: &mut Game, sep: u8) -> String {
    event_cemetery::finish_checks(board);
    board
        .internal_rule_action(&json!({
            "do": "evolve", "source": "vincent", "evolve_card": "form",
            "pay": {"pp": 1_u8, "sep": sep}
        }))
        .unwrap()
        .outcome
}

#[expect(
    clippy::panic,
    reason = "The printed spell must present a formal target question."
)]
fn cast_at(board: &mut Game, spell: &str, target: &str) {
    let CardPlay::Awaiting(question) = board
        .play_internal_card(spell, PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("the spell requires a follower target");
    };
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("the spell has a formal target");
    };
    let handle = domain
        .members
        .iter()
        .find(|member| member.card.as_deref() == Some(target))
        .unwrap()
        .handle;
    resolved(
        board
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Targets(vec![handle]),
            )
            .unwrap(),
    );
    event_cemetery::finish_checks(board);
}

#[expect(
    clippy::panic,
    reason = "The printed Fanfare must reach its search question."
)]
fn enter_vincent(board: &mut Game) -> DecisionView {
    assert!(
        matches!(
            board
                .play_internal_card("vincent", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ),
        "Vincent enters before its Fanfare is offered"
    );
    let pending = board.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("the printed Fanfare must be pending");
    };
    let CardPlay::Awaiting(search) = board
        .answer_internal_card(
            pending.actor,
            pending.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap()
    else {
        panic!("the Fanfare must ask for the searched card");
    };
    search
}

#[test]
fn vincent_requires_its_own_current_turn_stat_gain_before_evolving() {
    for printing in ["BP11-035", "BP11-SL07"] {
        let mut initial = draw_position();
        initial["turn"]["elapsed_turns"] = json!({"P1":7_u8,"P2":6_u8});
        initial["players"]["P1"]["sep"] = json!(1_u8);
        initial["players"]["P1"]["zones"]["field"] = json!([
            {"id":"vincent","card":printing,"state":{"damage":2_u8}},
            {"id":"other","card":"BP01-012"}
        ]);
        initial["players"]["P1"]["zones"]["hand"] = json!([
            {"id":"power-buff","card":"BP06-072"},
            {"id":"hp-buff","card":"BP08-096"},
            {"id":"bounce","card":"BP03-016"}
        ]);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP11-036"}]);
        for seat in ["P1", "P2"] {
            initial["players"][seat]["zones"]["deck"] = json!(
                (0_u8..4)
                    .map(|index| json!({"id":format!("{seat}-deck-{index}"),"card":"BP01-012"}))
                    .collect::<Vec<_>>()
            );
        }
        initial["players"]["P1"]["zones"]["deck"][3]["card"] = json!("BP11-044");
        let mut board = game(&initial);
        assert_eq!(
            board.projection(View::Referee).unwrap()["objects"]["vincent"]["hp"],
            2_u8
        );
        assert_eq!(evolve(&mut board, 1), "cannot-evolve");
        assert_eq!(board.projection(View::Referee).unwrap()["P1"]["sep"], 1_u8);
        let mut attack_gain = board.clone();
        cast_at(&mut attack_gain, "power-buff", printing);
        let mut reentered = attack_gain.clone();
        cast_at(&mut reentered, "bounce", printing);
        let search = enter_vincent(&mut reentered);
        resolved(
            reentered
                .answer_internal_card(
                    search.actor,
                    search.token,
                    Answer::Search(SearchAnswer::Ungrouped(vec![])),
                )
                .unwrap(),
        );
        assert_eq!(evolve(&mut reentered, 0), "cannot-evolve");
        assert_eq!(evolve(&mut attack_gain, 1), "resolved");
        let super_evolved = attack_gain.projection(View::Referee).unwrap();
        assert_eq!(super_evolved["objects"]["vincent"]["power"], 8_u8);
        assert_eq!(super_evolved["P1"]["sep"], 0_u8);
        cast_at(&mut board, "power-buff", "BP01-012");
        assert_eq!(evolve(&mut board, 0), "cannot-evolve");
        cast_at(&mut board, "hp-buff", printing);
        assert_eq!(
            board.projection(View::Referee).unwrap()["objects"]["vincent"]["hp"],
            4_u8
        );
        let mut next_turn = board.clone();
        assert_eq!(evolve(&mut board, 0), "resolved");
        assert_eq!(
            board.projection(View::Referee).unwrap()["objects"]["vincent"]["hp"],
            5_u8
        );
        attack_batch::end_turn(&mut next_turn);
        attack_batch::end_turn(&mut next_turn);
        assert_eq!(evolve(&mut next_turn, 0), "cannot-evolve");
    }
}

#[test]
fn vincent_fanfare_searches_only_the_printed_trait_into_the_cemetery() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"vincent","card":"BP11-035"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"other","card":"BP01-012"}, {"id":"wild","card":"BP11-044"}
    ]);
    let mut board = game(&initial);
    let search = enter_vincent(&mut board);
    let InputDomain::Search(domain) = &search.domain else {
        panic!("the Fanfare must use the search domain");
    };
    assert_eq!(domain.cards.len(), 1);
    assert_eq!(domain.cards[0].card, "BP11-044");
    resolved(
        board
            .answer_internal_card(
                search.actor,
                search.token,
                Answer::Search(SearchAnswer::Ungrouped(vec![domain.cards[0].handle])),
            )
            .unwrap(),
    );
    let state = board.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["cemetery"], json!(["wild"]));
    assert_eq!(state["P1"]["deck"], json!(["other"]));
}
