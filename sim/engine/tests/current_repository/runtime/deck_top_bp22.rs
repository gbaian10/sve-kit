#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate the real top-one banish scenario."
)]
use super::*;

fn position(number: &str, has_top: bool) -> Value {
    let mut state = draw_position();
    state["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":number}]);
    state["players"]["P1"]["zones"]["deck"] = if has_top {
        json!([{"id":"top","card":"BP01-168"},{"id":"tail","card":"BP01-171"}])
    } else {
        json!([])
    };
    state
}

fn play_and_trigger(game: &mut Game, number: &str) {
    assert!(
        matches!(
            game.play_internal_card("source", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ),
        "the real two-cost Elina enters before her fanfare"
    );
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("real printed fanfare")
    };
    let handle = options
        .iter()
        .find(|option| option.card == number)
        .unwrap()
        .handle;
    resolved(
        game.answer_internal_card(pending.actor, pending.token, Answer::Pending(handle))
            .unwrap(),
    );
}

fn assert_result(game: &Game, has_top: bool) {
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(
        view["P1"]["banish"],
        if has_top { json!(["top"]) } else { json!([]) },
        "the actual fanfare banishes exactly the first available card, including a three-cost spell"
    );
    assert_eq!(
        view["P1"]["deck"],
        if has_top { json!(["tail"]) } else { json!([]) },
        "the remaining follower stays on top and an empty deck is unchanged"
    );
    assert_eq!(
        view["P1"]["cemetery"],
        json!([]),
        "banishing does not move the card to the cemetery"
    );
    assert_eq!(
        view["P1"]["field"],
        json!(["source"]),
        "the real source stays on the field after its fanfare"
    );
    assert_eq!(
        view["P1"]["pp"]["current"], 8_u8,
        "the actual card play pays exactly two PP"
    );
    assert!(
        game.internal_decision(View::P1).is_none(),
        "direct banishing creates no private Look decision"
    );
    let public = game.projection(View::P2).unwrap();
    let known = public["known_cards"].as_array().unwrap();
    assert_eq!(
        known.iter().any(|card| card["id"] == "top"),
        has_top,
        "the banished top becomes public only when present"
    );
    assert!(
        !known.iter().any(|card| card["id"] == "tail"),
        "the unbanished tail remains private"
    );
}

#[test]
fn bp22_046_and_p23_banish_the_actual_top_one_and_resolve_with_an_empty_deck() {
    for number in ["BP22-046", "BP22-P23"] {
        for has_top in [true, false] {
            let mut game = game(&position(number, has_top));
            play_and_trigger(&mut game, number);
            assert_result(&game, has_top);
        }
    }
}
