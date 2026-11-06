#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Printed box seeds ask only their declared targets and searches."
)]
use super::*;
use sve_engine::game::internal::{DecisionView, SearchAnswer};

fn finish_search(game: &mut Game, mut play: CardPlay) {
    while let CardPlay::Awaiting(question) = play {
        let answer = match &question.domain {
            InputDomain::Search(domain) => {
                assert_eq!(
                    domain.cards.len(),
                    1,
                    "only the printed search predicate matches"
                );
                Answer::Search(SearchAnswer::Ungrouped(vec![domain.cards[0].handle]))
            }
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            other => panic!("unexpected printed question {other:?}"),
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    resolved(play);
    game.resume_internal_card().unwrap();
}

fn pending(game: &mut Game) -> CardPlay {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("printed fanfare")
    };
    assert_eq!(options.len(), 1, "the source has one printed Fanfare");
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::Pending(options[0].handle),
    )
    .unwrap()
}

fn question(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(question) = play else {
        panic!("printed target")
    };
    question
}

fn initial() -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"source","card":"BP11-018"}, {"id":"spell","card":"BP11-024"}
    ]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"royal","card":"BP11-023"}, {"id":"wilderness","card":"BP11-109"},
        {"id":"wrong","card":"BP01-037"}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"enemy","card":"BP01-046"}, {"id":"unboxed","card":"BP01-037"}
    ]);
    initial
}

#[test]
fn bp11_018_searches_a_one_cost_royal_card_and_boxes_once_before_bp11_024_selects_it() {
    let mut game = game(&initial());
    assert!(matches!(
        game.play_internal_card("source", PlayerId { seat: 1 })
            .unwrap(),
        CardPlay::Entered(_)
    ));
    let fanfare = pending(&mut game);
    finish_search(&mut game, fanfare);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["ex"],
        json!(["royal"])
    );
    let ability = selection().current["BP11-018"]
        .program
        .card()
        .abilities
        .iter()
        .find(|ability| ability.source.line == 2)
        .unwrap()
        .node
        .id;
    let before = game.projection(View::Referee).unwrap();
    let before_pp = before["P1"]["pp"].clone();
    assert_eq!(before["objects"]["enemy"]["keywords"], json!(["守護"]));
    let targets = question(
        game.activate_internal_card("source", ability, PlayerId { seat: 1 })
            .unwrap(),
    );
    let InputDomain::Targets(domain) = &targets.domain else {
        panic!("opponent followers")
    };
    let enemy = domain
        .members
        .iter()
        .find(|member| member.card.as_deref() == Some("BP01-046"))
        .unwrap()
        .handle;
    resolved(
        game.answer_internal_card(targets.actor, targets.token, Answer::Targets(vec![enemy]))
            .unwrap(),
    );
    game.resume_internal_card().unwrap();
    let boxed = game.projection(View::Referee).unwrap();
    assert_eq!(boxed["objects"]["enemy"]["boxed"], true);
    assert_eq!(boxed["objects"]["enemy"]["keywords"], json!([]));
    assert_eq!(boxed["objects"]["enemy"]["acted"], true);
    assert_eq!(boxed["P1"]["pp"], before_pp);
    game.activate_internal_card("source", ability, PlayerId { seat: 1 })
        .unwrap_err();
    let spell_target = question(
        game.play_internal_card("spell", PlayerId { seat: 1 })
            .unwrap(),
    );
    let InputDomain::Targets(only_boxed) = &spell_target.domain else {
        panic!("boxed target")
    };
    assert_eq!(only_boxed.members.len(), 1);
    assert_eq!(only_boxed.members[0].card.as_deref(), Some("BP01-046"));
    let search = game
        .answer_internal_card(
            spell_target.actor,
            spell_target.token,
            Answer::Targets(vec![only_boxed.members[0].handle]),
        )
        .unwrap();
    finish_search(&mut game, search);
    let completed = game.projection(View::Referee).unwrap();
    assert_eq!(completed["P2"]["cemetery"], json!(["enemy"]));
    assert_eq!(completed["P2"]["field"], json!(["unboxed"]));
    assert_eq!(completed["P1"]["hand"], json!(["wilderness"]));
}

#[test]
fn bp11_024_with_no_boxed_target_still_searches_a_wilderness_follower() {
    let mut setup = initial();
    setup["players"]["P1"]["zones"]["hand"] = json!([{"id":"spell","card":"BP11-024"}]);
    setup["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"wilderness","card":"BP11-109"}, {"id":"wrong","card":"BP01-037"}
    ]);
    let mut game = game(&setup);
    let targets = question(
        game.play_internal_card("spell", PlayerId { seat: 1 })
            .unwrap(),
    );
    let InputDomain::Targets(domain) = &targets.domain else {
        panic!("empty boxed domain")
    };
    assert_eq!(domain.members, vec![]);
    let search = game
        .answer_internal_card(targets.actor, targets.token, Answer::Targets(vec![]))
        .unwrap();
    finish_search(&mut game, search);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["field"], json!(["enemy", "unboxed"]));
    assert_eq!(state["P1"]["hand"], json!(["wilderness"]));
}
