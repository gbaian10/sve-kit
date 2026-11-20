use super::*;

#[test]
fn bp06_107_finishes_the_turn_players_choice_before_asking_the_opponent() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP06-SL19"}]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"ally","card":"BP01-037"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"first","card":"BP01-037"},{"id":"second","card":"BP01-006"}
    ]);
    let mut game = game(&initial);
    play_grant_source(&mut game, None);
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("printed Fanfare")
    };
    let CardPlay::Awaiting(first) = game
        .answer_internal_card(
            pending.actor,
            pending.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap()
    else {
        panic!("turn player chooses")
    };
    assert_eq!(first.actor.seat, 1);
    assert!(game.internal_decision(View::P2).is_none());
    let InputDomain::Targets(candidates) = &first.domain else {
        panic!("own followers")
    };
    assert_eq!(candidates.members.len(), 2);
    assert!(
        candidates
            .members
            .iter()
            .all(|member| member.player == first.actor)
    );
    let mammoth = candidates
        .members
        .iter()
        .find(|candidate| candidate.card.as_deref() == Some("BP06-SL19"))
        .unwrap();
    let CardPlay::Awaiting(second) = game
        .answer_internal_card(
            first.actor,
            first.token,
            Answer::Targets(vec![mammoth.handle]),
        )
        .unwrap()
    else {
        panic!("opponent chooses")
    };
    let between = game.projection(View::Referee).unwrap();
    assert_eq!(between["P1"]["cemetery"], json!(["ally"]));
    assert_eq!(between["P2"]["field"], json!(["first", "second"]));
    assert_eq!(second.actor.seat, 2);
    assert!(game.internal_decision(View::P1).is_none());
    let InputDomain::Targets(opposing_candidates) = &second.domain else {
        panic!("opponent's own followers")
    };
    assert_eq!(opposing_candidates.members.len(), 2);
    assert!(
        opposing_candidates
            .members
            .iter()
            .all(|member| member.player == second.actor)
    );
    resolved(
        game.answer_internal_card(
            second.actor,
            second.token,
            Answer::Targets(vec![opposing_candidates.members[1].handle]),
        )
        .unwrap(),
    );
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["P1"]["field"], json!(["source"]));
    assert_eq!(after["P2"]["field"], json!(["second"]));
    assert_eq!(after["P2"]["cemetery"], json!(["first"]));
}
