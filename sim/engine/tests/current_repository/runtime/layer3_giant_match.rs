use super::event_cemetery::finish_checks;
use super::union_burst_batch1::{pending, question};
use super::*;

const NUMBER: &str = "BP11-112";

fn activation() -> AbilityId {
    selection_with_rules().current[NUMBER].program.card().abilities.iter()
        .find(|ability| ability.node.kind == AbilityKind::Activated).unwrap().node.id
}

#[test]
fn giant_match_enters_acted_and_its_fanfare_takes_only_expensive_followers_then_orders_the_rest() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"match","card":NUMBER}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"cheap","card":"BP01-171"},
        {"id":"high-a","card":"BP01-158"},
        {"id":"spell-a","card":"BP01-179"},
        {"id":"high-b","card":"BP01-158"},
        {"id":"spell-b","card":"BP01-179"},
        {"id":"unseen","card":"BP01-179"}
    ]);
    let mut board = game(&initial);
    assert!(matches!(board.play_internal_card("match", PlayerId { seat: 1 }).unwrap(), CardPlay::Entered(_)));
    assert_eq!(board.projection(View::Referee).unwrap()["objects"]["match"]["acted"], true);
    let pick = question(pending(&mut board, NUMBER));
    let InputDomain::Targets(domain) = &pick.domain else { panic!("private Look selection"); };
    assert_eq!(domain.members.len(), 2, "only the two original-cost-eight followers qualify");
    assert_eq!(domain.cardinality.minimum, Nat(0));
    assert_eq!(domain.cardinality.maximum, Some(Nat(5)));
    let private = board.projection(View::P2).unwrap();
    for id in ["cheap", "high-a", "spell-a", "high-b", "spell-b", "unseen"] {
        assert!(private["objects"][id].is_null());
    }
    let ordering = question(board.answer_internal_card(pick.actor, pick.token,
        Answer::Targets(domain.members.iter().map(|member| member.handle).collect())).unwrap());
    let InputDomain::Order(cards) = &ordering.domain else { panic!("the three remaining looked cards are ordered"); };
    assert_eq!(cards.len(), 3);
    let shown = board.projection(View::P2).unwrap();
    for id in ["high-a", "high-b"] { assert_eq!(shown["objects"][id]["card"], "BP01-158"); }
    for id in ["cheap", "spell-a", "spell-b", "unseen"] { assert!(shown["objects"][id].is_null()); }
    resolved(board.answer_internal_card(ordering.actor, ordering.token,
        Answer::Order(cards.iter().rev().map(|card| card.handle).collect())).unwrap());
    finish_checks(&mut board);
    let after = board.projection(View::Referee).unwrap();
    assert_eq!(after["P1"]["hand"], json!(["high-a","high-b"]));
    assert_eq!(after["P1"]["deck"], json!(["unseen","spell-b","spell-a","cheap"]));
    assert_eq!(after["P1"]["pp"]["current"], 3_u8);
    board.activate_internal_card("match", activation(), PlayerId { seat: 1 }).unwrap_err();
    assert_eq!(board.projection(View::Referee).unwrap(), after, "an acted amulet cannot pay Act again");
}

#[test]
fn giant_match_pays_itself_first_and_q1577_requires_one_of_the_two_public_followers() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"match","card":NUMBER},
        {"id":"old-a","card":"BP01-171"},{"id":"old-b","card":"BP01-171"},
        {"id":"old-c","card":"BP01-171"},{"id":"old-d","card":"BP01-171"}
    ]);
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"a","card":"BP01-158"},{"id":"b","card":"BP01-158"},
        {"id":"private","card":"BP01-179"}
    ]);
    let script = json!({"random_selections":[{"n":1_u8,"from":"P1.hand","result":["a","b"]}]});
    let mut board = Game::from_selected_effects(selection_with_rules().clone(), &initial, &Value::Null, &script, "giant-match").unwrap();
    let q = question(board.activate_internal_card("match", activation(), PlayerId { seat: 1 }).unwrap());
    let InputDomain::SearchPartition(domain) = &q.domain else { panic!("capacity choice after paying the amulet"); };
    assert_eq!(domain.cardinality, Cardinality { minimum: Nat(1), maximum: Some(Nat(1)) });
    assert_eq!(domain.cards.len(), 2);
    let before = board.projection(View::Referee).unwrap();
    assert_eq!(before["P1"]["cemetery"], json!(["match"]));
    assert_eq!(before["P1"]["field"].as_array().unwrap().len(), 4);
    let public = board.projection(View::P2).unwrap();
    for id in ["a","b"] { assert_eq!(public["objects"][id]["card"], "BP01-158"); }
    assert!(public["objects"]["private"].is_null());
    board.answer_internal_card(q.actor, q.token, Answer::SearchPartition(vec![])).unwrap_err();
    assert_eq!(board.projection(View::Referee).unwrap(), before);
    resolved(board.answer_internal_card(q.actor, q.token, Answer::SearchPartition(vec![domain.cards[1].handle])).unwrap());
    finish_checks(&mut board);
    let after = board.projection(View::Referee).unwrap();
    assert_eq!(after["P1"]["field"], json!(["old-a","old-b","old-c","old-d","b"]));
    assert_eq!(after["P1"]["hand"], json!(["a","private"]));
    assert_eq!(after["P1"]["cemetery"], json!(["match"]));
    let after_public = board.projection(View::P2).unwrap();
    assert!(after_public["objects"]["a"].is_null());
    assert!(after_public["objects"]["private"].is_null());
}
