#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate the real-card scenario."
)]
use super::*;
use sve_engine::game::internal::SearchAnswer;

const PLAYER: PlayerId = PlayerId { seat: 1 };

/// BP10-007 in hand, one beast follower on the field to return and a サルビアパンサー to search.
fn position(returned: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"tiger","card":"BP10-007"},
        {"id":"fox","card":"BP13-016"}
    ]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"returned","card":returned}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"draw","card":"BP01-037"},
        {"id":"panther","card":"BP10-009"}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    initial
}

fn settle(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}

fn pp(game: &Game) -> u64 {
    game.projection(View::Referee).unwrap()["P1"]["pp"]["current"]
        .as_u64()
        .unwrap()
}

/// Plays BP10-007, returns the field beast as its Fanfare cost and searches サルビアパンサー.
fn return_beast(game: &mut Game) {
    let mut entry = game.play_internal_card("tiger", PLAYER).unwrap();
    while let CardPlay::Awaiting(question) = &entry
        && matches!(question.domain, InputDomain::WardEntry { .. })
    {
        entry = game
            .answer_internal_card(question.actor, question.token, Answer::WardEntry(false))
            .unwrap();
    }
    assert!(
        matches!(entry, CardPlay::Entered(_)),
        "BP10-007 enters before its Fanfare"
    );
    settle(game);
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("BP10-007 Fanfare");
    };
    let mut step = game
        .answer_internal_card(
            pending.actor,
            pending.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap();
    while let CardPlay::Awaiting(question) = step {
        let answer = match &question.domain {
            InputDomain::OptionalCost => Answer::OptionalCost(true),
            InputDomain::CostMaterials(domain) => {
                assert_eq!(domain.members.len(), 1, "only the other beast returns");
                Answer::CostMaterials(vec![domain.members[0].handle])
            }
            InputDomain::Search(domain) => {
                Answer::Search(SearchAnswer::Ungrouped(vec![domain.cards[0].handle]))
            }
            _ => panic!("unexpected decision: {:?}", question.domain),
        };
        step = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    settle(game);
    let hand = game.projection(View::Referee).unwrap()["P1"]["hand"].clone();
    for id in ["fox", "panther", "returned"] {
        assert!(
            hand.as_array().unwrap().contains(&json!(id)),
            "{id} is in hand after the Fanfare"
        );
    }
}

/// The PP actually paid when the hand card enters from hand.
fn paid(game: &mut Game, id: &str) -> u64 {
    let before = pp(game);
    let entry = game.play_internal_card(id, PLAYER).unwrap();
    assert!(
        matches!(entry, CardPlay::Entered(_)),
        "{id} enters from hand"
    );
    before.checked_sub(pp(game)).unwrap()
}

#[test]
fn returned_beast_discounts_only_differently_named_beasts_this_turn() {
    // A returned ガザニアフォックス discounts サルビアパンサー (3 - 2) but not another ガザニアフォックス.
    let mut fox_returned = game(&position("BP13-016"));
    assert_eq!(paid(&mut fox_returned.clone(), "fox"), 2, "no return yet");
    return_beast(&mut fox_returned);
    assert_eq!(paid(&mut fox_returned.clone(), "panther"), 1, "3 - 2");
    assert_eq!(paid(&mut fox_returned, "fox"), 2, "a same-named return does not count");

    // A returned サルビアパンサー discounts ガザニアフォックス (2 - 1) but not another サルビアパンサー.
    let mut panther_returned = game(&position("BP10-009"));
    return_beast(&mut panther_returned);
    assert_eq!(paid(&mut panther_returned.clone(), "fox"), 1, "2 - 1");
    assert_eq!(paid(&mut panther_returned, "panther"), 3, "a same-named return does not count");
}

#[test]
fn bp13_016_deals_four_and_gains_aura_only_beside_a_big_beast() {
    for (field, damage, aura) in [
        (json!([{"id":"cat","card":"BP20-017"}]), 4_u8, true),
        (json!([]), 1_u8, false),
    ] {
        let mut initial = position("BP10-009");
        initial["players"]["P1"]["zones"]["field"] = field;
        let mut game = game(&initial);
        let entry = game.play_internal_card("fox", PLAYER).unwrap();
        assert!(
            matches!(entry, CardPlay::Entered(_)),
            "BP13-016 enters before its Fanfare"
        );
        settle(&mut game);
        let pending = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = &pending.domain else {
            panic!("BP13-016 Fanfare");
        };
        let CardPlay::Awaiting(target) = game
            .answer_internal_card(
                pending.actor,
                pending.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap()
        else {
            panic!("the Fanfare chooses its target first");
        };
        let InputDomain::Targets(domain) = &target.domain else {
            panic!("enemy follower target: {:?}", target.domain);
        };
        resolved(
            game.answer_internal_card(
                target.actor,
                target.token,
                Answer::Targets(vec![domain.members[0].handle]),
            )
            .unwrap(),
        );
        settle(&mut game);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["enemy"]["damage"], damage, "1 or 4 damage");
        assert_eq!(
            state["objects"]["fox"]["keywords"]
                .as_array()
                .unwrap()
                .contains(&json!("オーラ")),
            aura,
            "Aura only with an original-cost 5+ beast"
        );
    }
}
