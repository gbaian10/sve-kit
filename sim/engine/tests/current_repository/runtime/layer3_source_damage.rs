use super::event_cemetery::finish_checks;
use super::union_burst_batch1::{finish, pending, question};
use super::*;
use sve_engine::game::internal::SearchAnswer;

#[test]
fn grea_evolves_searches_heat_and_discounts_only_the_next_play_while_increasing_its_damage() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"DSD01a-002"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"DSD01a-003"}]);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"second","card":"DSD01a-009"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"heat","card":"DSD01a-009"}, {"id":"other","card":"BP01-037"}
    ]);
    initial["players"]["P1"]["zones"]["cemetery"] = json!(
        (0..5_u8)
            .map(|i| json!({"id":format!("cemetery-{i}"),"card":"DSD01a-009"}))
            .collect::<Vec<_>>()
    );
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"enemy","card":"BP01-037","state":{"hp":20_u8}}
    ]);
    let mut game = game(&initial);
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
        )
        .unwrap()
        .outcome,
        "resolved"
    );
    let search = question(pending(&mut game, "DSD01a-003"));
    let InputDomain::Search(domain) = &search.domain else {
        panic!("heat search");
    };
    assert_eq!(domain.cards.len(), 1);
    assert_eq!(domain.cards[0].card, "DSD01a-009");
    resolved(
        game.answer_internal_card(
            search.actor,
            search.token,
            Answer::Search(SearchAnswer::Ungrouped(vec![domain.cards[0].handle])),
        )
        .unwrap(),
    );
    finish_checks(&mut game);
    for (spell, pp, hp) in [("heat", 9_u8, 16_u8), ("second", 8_u8, 12_u8)] {
        let play = game
            .play_internal_card(spell, PlayerId { seat: 1 })
            .unwrap();
        resolved(finish(&mut game, play));
        finish_checks(&mut game);
        let after = game.projection(View::Referee).unwrap();
        assert_eq!(after["P1"]["pp"]["current"], pp);
        assert_eq!(after["objects"]["enemy"]["hp"], hp);
    }
}
