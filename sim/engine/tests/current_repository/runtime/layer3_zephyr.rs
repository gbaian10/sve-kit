use super::event_cemetery::finish_checks;
use super::union_burst_batch1::{pending, question};
use super::*;
use sve_engine::game::internal::{DecisionView, SearchAnswer};

const NUMBER: &str = "ECP01-006";
const SMALL: &str = "ECP01-007";
const TEIO: &str = "ECP01-018";

fn position() -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["leader"]["class"] = json!("エルフ");
    initial["players"]["P1"]["construction"] = json!("title");
    initial["players"]["P1"]["title_code"] = json!("umamusume_pretty_derby");
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":NUMBER}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"small","card":SMALL},
        {"id":"spell-a","card":TEIO},{"id":"spell-b","card":TEIO},
        {"id":"spell-c","card":TEIO},{"id":"spell-d","card":TEIO},{"id":"spell-e","card":TEIO}
    ]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"carrot","card":"CSD01-032"}]);
    initial
}

fn ward(board: &mut Game, play: CardPlay) -> CardPlay {
    let q = question(play);
    assert!(matches!(q.domain, InputDomain::WardEntry { .. }), "the printed Ward installs its normal entry option");
    board.answer_internal_card(q.actor, q.token, Answer::WardEntry(false)).unwrap()
}

fn search(board: &mut Game, q: &DecisionView) {
    let InputDomain::Search(domain) = &q.domain else { panic!("Zephyr's printed deck search"); };
    assert_eq!(domain.cards.len(), 1, "only the cheap Uma follower qualifies");
    assert_eq!(domain.cards[0].card, SMALL, "the search offers the cheap Uma follower");
    resolved(board.answer_internal_card(q.actor, q.token,
        Answer::Search(SearchAnswer::Ungrouped(vec![domain.cards[0].handle]))).unwrap());
    finish_checks(board);
}

#[test]
fn zephyr_normal_play_requires_search_without_a_bounce_target_and_keeps_ward_and_meal() {
    let mut board = game(&position());
    let play = board.play_internal_card("source", PlayerId { seat: 1 }).unwrap();
    assert!(matches!(ward(&mut board, play), CardPlay::Entered(_)));
    let modes = question(pending(&mut board, NUMBER));
    let InputDomain::Modes { modes: available, count, cardinality } = &modes.domain else { panic!("Zephyr modes"); };
    assert_eq!(*count, Nat(1));
    assert_eq!(cardinality.minimum, Nat(1));
    assert_eq!(available.len(), 1, "Q1817: no legal target removes the bounce option");
    let q = question(board.answer_internal_card(modes.actor, modes.token, Answer::Modes(available.clone())).unwrap());
    search(&mut board, &q);
    let after = board.projection(View::Referee).unwrap();
    assert_eq!(after["P1"]["field"], json!(["source","small"]));
    assert_eq!(after["P1"]["pp"]["current"], 6_u8);
    meal_race::feed(&mut board, NUMBER, 0, false);
    finish_checks(&mut board);
    let fed = board.projection(View::Referee).unwrap();
    assert_eq!(fed["P1"]["pp"]["current"], 5_u8);
    assert_eq!(fed["objects"]["source"]["racing"], true);
    assert_eq!(fed["P1"]["race"], json!(["carrot"]));
}

#[test]
fn q1818_teio_spell_source_keeps_two_modes_after_the_spell_has_left_resolution() {
    let mut initial = position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"teio","card":TEIO}]);
    initial["players"]["P1"]["zones"]["deck"].as_array_mut().unwrap().insert(0, json!({"id":"source","card":NUMBER}));
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-171"}]);
    let mut board = game(&initial);
    let pick = question(board.play_internal_card("teio", PlayerId { seat: 1 }).unwrap());
    let InputDomain::Targets(look) = &pick.domain else { panic!("Teio's private Look"); };
    let selected = look.members.iter().find(|member| member.card.as_deref() == Some(NUMBER)).unwrap().handle;
    let play = board.answer_internal_card(pick.actor, pick.token, Answer::Targets(vec![selected])).unwrap();
    let ordering = question(ward(&mut board, play));
    let InputDomain::Order(cards) = &ordering.domain else { panic!("Teio orders its six remaining cards"); };
    assert_eq!(cards.len(), 6);
    resolved(board.answer_internal_card(ordering.actor, ordering.token, Answer::Order(cards.iter().map(|card| card.handle).collect())).unwrap());
    let modes = question(pending(&mut board, NUMBER));
    assert_eq!(board.projection(View::Referee).unwrap()["P1"]["cemetery"], json!(["teio"]));
    let InputDomain::Modes { modes: available, count, cardinality } = &modes.domain else { panic!("Zephyr's enhanced mode limit"); };
    assert_eq!(*count, Nat(2), "Q1818: the spell is an Uma card ability source");
    assert_eq!(cardinality.minimum, Nat(1));
    assert_eq!(cardinality.maximum, Some(Nat(2)));
    assert_eq!(available.len(), 2);
    let before = board.projection(View::Referee).unwrap();
    board.answer_internal_card(modes.actor, modes.token, Answer::Modes(vec![available[0], available[0]])).unwrap_err();
    assert_eq!(board.projection(View::Referee).unwrap(), before, "Q1816: no repeated mode");
    let target = question(board.answer_internal_card(modes.actor, modes.token, Answer::Modes(available.clone())).unwrap());
    let InputDomain::Targets(bounce) = &target.domain else { panic!("the required bounce target"); };
    assert_eq!(bounce.members.len(), 1);
    let q = question(board.answer_internal_card(target.actor, target.token, Answer::Targets(vec![bounce.members[0].handle])).unwrap());
    search(&mut board, &q);
    let after = board.projection(View::Referee).unwrap();
    assert_eq!(after["P2"]["hand"], json!(["enemy"]));
    assert_eq!(after["P1"]["field"], json!(["source","small"]));
}
