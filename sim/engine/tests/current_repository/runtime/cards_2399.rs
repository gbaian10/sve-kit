use super::*;

#[test]
#[expect(clippy::panic, reason = "The printed evolution must offer a private search.")]
fn hotaru_searches_for_exactly_one_printed_attack_when_she_evolves() {
    let mut initial = position_with_source("ECP02-044", "field");
    initial["players"]["P1"]["zones"]["evolve_deck"] =
        json!([{"id":"form","card":"ECP02-P24"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"higher","card":"ECP02-003"},
        {"id":"matching","card":"ECP02-010"},
        {"id":"other_trait","card":"BP01-171"}
    ]);
    let mut board = game(&initial);
    assert_eq!(
        board
            .internal_rule_action(&json!({
                "do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}
            }))
            .unwrap()
            .outcome,
        "resolved"
    );
    checks(&mut board);
    let pending = board.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("evolution trigger");
    };
    let search = await_card_choice(
        board
            .answer_internal_card(pending.actor, pending.token, Answer::Pending(options[0].handle))
            .unwrap(),
    );
    let InputDomain::Search(domain) = &search.domain else {
        panic!("private search");
    };
    assert_eq!(domain.cards.len(), 1);
    assert!(board.internal_decision(View::P2).is_none());
    let answer = Answer::Search(SearchAnswer::Ungrouped(vec![domain.cards[0].handle]));
    drop(resolved(
        board
            .answer_internal_card(search.actor, search.token, answer)
            .unwrap(),
    ));
    let state = board.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["matching"]));
}
