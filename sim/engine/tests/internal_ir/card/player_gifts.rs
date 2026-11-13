//! BP13-043's shape: each player specifies one own follower, then each gives it away in turn.
use super::*;

fn gift_steps() -> Value {
    json!([{"each_player":{"as":"p","stages":[
        [{"choose":{"bind":"gift","from":{"who":"p","zone":"field","kind":"follower"},"n":1,"by":"p"}}],
        [{"give":{"ref":"gift","to":"other_players"}}]
    ]}}])
}

/// `seat` plays `s` during its own turn with `fields` extra followers for itself and the other.
fn gift(seat: u32, fields: (usize, usize)) -> (Game, DecisionView, &'static str, &'static str) {
    let (game, first, me, them) = staged(seat, "unit-follower", &gift_steps(), fields);
    (game, first.unwrap(), me, them)
}

/// `seat` plays `s`, a `source` card whose fanfare is `steps`, during its own turn.
fn staged(
    seat: u32,
    source: &str,
    steps: &Value,
    fields: (usize, usize),
) -> (Game, Option<DecisionView>, &'static str, &'static str) {
    let (me, them) = if seat == 1 {
        ("P1", "P2")
    } else {
        ("P2", "P1")
    };
    let programs = lowerer()
        .lower(&tree(source, &json!({"fanfare":{"do":steps}})))
        .unwrap();
    let mut initial = setup(seat, json!([]));
    initial["turn"]["active"] = json!(me);
    initial["players"][me]["zones"]["hand"] = json!([{"id":"s","card":source}]);
    initial["players"][me]["pp"] = json!({"current":5,"max":5});
    initial["players"][me]["zones"]["field"] = cards("m", fields.0, "unit-follower");
    initial["players"][them]["zones"]["field"] = cards("t", fields.1, "unit-follower");
    let mut game = Game::new(catalog(), &initial, &Value::Null, &Value::Null, "gift").unwrap();
    game.load_internal_card(source, &programs[source]).unwrap();
    for number in ["unit-follower", "unit-amulet", "unit-spell"] {
        if number != source {
            game.load_internal_card(number, &card(vec![])).unwrap();
        }
    }
    game.play_internal_card("s", PlayerId { seat }).unwrap();
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("one pending fanfare");
    };
    let first = game
        .answer_internal_card(
            pending.actor,
            pending.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap();
    let first = if let CardPlay::Awaiting(question) = first {
        Some(question)
    } else {
        None
    };
    (game, first, me, them)
}

/// Answers every question with its first `n` candidates, or none when it has fewer.
fn answer_all(game: &mut Game, first: Option<DecisionView>, n: usize) {
    let mut question = first;
    while let Some(current) = question {
        let offered = handles(&current);
        let picked = if offered.len() >= n {
            offered[..n].to_vec()
        } else {
            vec![]
        };
        question = answer(game, &current, picked);
    }
}

fn originals(state: &Value, player: &str, prefix: char) -> usize {
    ids(state, player, "field")
        .iter()
        .filter(|id| id.starts_with(prefix))
        .count()
}

#[test]
fn both_players_specify_before_either_gives_and_the_turn_player_gives_first() {
    // Q1766: the turn player specifies, then the other player, then each gives in turn order.
    for seat in [1_u32, 2] {
        let (mut game, first, me, them) = gift(seat, (1, 2));
        assert_eq!(
            first.actor,
            PlayerId { seat },
            "the turn player specifies first"
        );
        assert_eq!(handles(&first).len(), 2, "its own s and m0");
        let second = answer(&mut game, &first, vec![handles(&first)[0]]).unwrap();
        assert_ne!(second.actor, first.actor);
        assert_eq!(
            handles(&second).len(),
            2,
            "the other player's own followers"
        );
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            (count(&state, me, "field"), count(&state, them, "field")),
            (2, 2),
            "nothing is given before both players have specified"
        );
        assert!(answer(&mut game, &second, vec![handles(&second)[0]]).is_none());
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            (originals(&state, me, 't'), originals(&state, them, 't')),
            (1, 1),
            "seat={seat}: {state}"
        );
        assert_eq!(
            originals(&state, them, 's') + originals(&state, them, 'm'),
            1,
            "seat={seat}: {state}"
        );
        assert_eq!(count(&state, me, "cemetery"), 0, "a gift is no move");
    }
}

#[test]
fn a_recipients_free_field_slot_is_read_when_its_gift_moves() {
    // The turn player's field is full; its own gift frees the slot the other player's gift needs.
    let (mut game, first, me, them) = gift(1, (4, 4));
    let second = answer(&mut game, &first, vec![handles(&first)[0]]).unwrap();
    assert!(answer(&mut game, &second, vec![handles(&second)[0]]).is_none());
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        (count(&state, me, "field"), count(&state, them, "field")),
        (5, 4),
        "{state}"
    );
    assert_eq!(originals(&state, me, 't'), 1, "{state}");

    // The other player's field is full: the turn player's gift fails, the other gift still moves.
    let (mut game, first, me, them) = gift(1, (1, 5));
    let second = answer(&mut game, &first, vec![handles(&first)[0]]).unwrap();
    assert!(answer(&mut game, &second, vec![handles(&second)[0]]).is_none());
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        (count(&state, me, "field"), count(&state, them, "field")),
        (3, 4),
        "{state}"
    );
    assert_eq!(
        originals(&state, them, 's') + originals(&state, them, 'm'),
        0
    );
}

#[test]
fn a_gift_names_a_chosen_object_and_one_recipient() {
    for (steps, case) in [
        (
            json!([{"give":{"ref":"nothing","to":"opp"}}]),
            "an unchosen object",
        ),
        (
            json!([
                {"choose":{"bind":"gift","from":{"who":"self","zone":"field","kind":"follower"},"n":1}},
                {"give":{"ref":"gift","to":"all"}}
            ]),
            "every player at once",
        ),
    ] {
        assert_eq!(
            lowerer()
                .lower(&tree("unit-follower", &json!({"fanfare":{"do":steps}})))
                .unwrap_err()
                .code,
            "unsupported_construct",
            "{case}"
        );
    }
}

#[test]
fn a_player_with_no_follower_gives_nothing_while_the_other_still_gives() {
    // The amulet source is no follower, so either player may have none to specify.
    for seat in [1_u32, 2] {
        // Each follower present is given away; the player without one gives nothing.
        for fields in [(1, 0), (0, 1), (0, 0)] {
            let (mut game, first, me, them) = staged(seat, "unit-amulet", &gift_steps(), fields);
            answer_all(&mut game, first, 1);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                (originals(&state, them, 'm'), originals(&state, me, 't')),
                fields,
                "seat={seat} {fields:?}: {state}"
            );
            assert_eq!(
                (originals(&state, me, 'm'), originals(&state, them, 't')),
                (0, 0),
                "seat={seat} {fields:?}: {state}"
            );
        }
    }
}

#[test]
fn every_specified_follower_is_given_while_the_recipient_has_room() {
    let steps = json!([{"each_player":{"as":"p","stages":[
        [{"choose":{"bind":"gift","from":{"who":"p","zone":"field","kind":"follower"},"n":2,"by":"p"}}],
        [{"give":{"ref":"gift","to":"other_players"}}]
    ]}}]);
    let (mut game, first, me, them) = staged(1, "unit-amulet", &steps, (2, 2));
    answer_all(&mut game, first, 2);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        (originals(&state, them, 'm'), originals(&state, me, 't')),
        (2, 2),
        "{state}"
    );

    // The other field has one free slot: only the first of the turn player's two gifts fits.
    let (mut game, first, me, them) = staged(1, "unit-amulet", &steps, (2, 4));
    answer_all(&mut game, first, 2);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(originals(&state, them, 'm'), 1, "{state}");
    assert_eq!(originals(&state, me, 't'), 2, "{state}");
}

#[test]
fn a_given_follower_still_leaves_to_its_owners_zone() {
    // P1 gives its own follower to P2, then destroys it: it goes to P1's cemetery.
    let steps = json!([
        {"choose":{"bind":"gift","from":{"who":"self","zone":"field","kind":"follower"},"n":1}},
        {"give":{"ref":"gift","to":"opp"}},
        {"destroy":"gift"}
    ]);
    let (mut game, first, me, them) = staged(1, "unit-amulet", &steps, (1, 0));
    answer_all(&mut game, first, 1);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(ids(&state, me, "cemetery"), ["m0"], "{state}");
    assert_eq!(count(&state, them, "cemetery"), 0, "{state}");
    assert_eq!(count(&state, them, "field"), 0, "{state}");
}
