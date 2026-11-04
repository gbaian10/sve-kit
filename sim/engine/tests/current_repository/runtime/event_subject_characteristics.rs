#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate the complete printed event scenarios."
)]
use super::event_cemetery::finish_checks;
use super::*;
use sve_engine::game::internal::DecisionView;

const WILBERT: [&str; 6] = [
    "BP06-090",
    "BP06-SL16",
    "SP01-042",
    "SP01-SL42",
    "SP01-SP28",
    "SP01-U26",
];
const LADY: [&str; 2] = ["BP21-084", "BP21-P45"];

fn actual(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "printed-event-subject-characteristics",
    )
    .unwrap()
}

fn initial() -> Value {
    let mut state = draw_position();
    state["players"]["P1"]["leader"]["class"] = json!("ビショップ");
    state["players"]["P2"]["zones"]["field"] = json!([]);
    state
}

fn wards(game: &mut Game, mut play: CardPlay) -> CardPlay {
    loop {
        let CardPlay::Awaiting(q) = &play else {
            return play;
        };
        if !matches!(q.domain, InputDomain::WardEntry { .. }) {
            return play;
        }
        play = game
            .answer_internal_card(q.actor, q.token, Answer::WardEntry(false))
            .unwrap();
    }
}

fn play(game: &mut Game, id: &str, actor: PlayerId) {
    let declaration = game.play_internal_card(id, actor).unwrap();
    assert!(
        matches!(
            wards(game, declaration),
            CardPlay::Entered(_) | CardPlay::Resolved(_)
        ),
        "the real printed card must finish entry or resolution"
    );
}

fn awaiting(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(q) = play else {
        panic!("a printed choice must be offered");
    };
    q
}

fn pending(game: &mut Game, number: &str, line: u64) -> CardPlay {
    finish_checks(game);
    let q = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &q.domain else {
        panic!("printed trigger order");
    };
    let selected = options
        .iter()
        .find(|option| option.card == number && option.line == line)
        .unwrap();
    game.answer_internal_card(q.actor, q.token, Answer::Pending(selected.handle))
        .unwrap()
}

fn damage_events(game: &mut Game, printings: &[&str], line: u64) -> u8 {
    let mut count = 0_u8;
    loop {
        finish_checks(game);
        let Some(q) = game.internal_pending_choice().unwrap() else {
            return count;
        };
        let InputDomain::Pending(options) = &q.domain else {
            panic!("only damage occurrences remain");
        };
        assert!(
            options
                .iter()
                .all(|option| printings.contains(&option.card.as_str()) && option.line == line),
            "only the requested printed damage occurrences may remain"
        );
        resolved(
            game.answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
                .unwrap(),
        );
        count = count.checked_add(1).unwrap();
    }
}

#[test]
fn every_wilbert_printing_counts_both_departed_copies_and_excludes_foreign_and_plain_followers() {
    for printing in WILBERT {
        let mut state = initial();
        state["players"]["P1"]["zones"]["field"] = json!([
            {"id":"first","card":printing},{"id":"second","card":"BP06-090"},
            {"id":"plain","card":"BP01-171"}
        ]);
        state["players"]["P2"]["zones"]["field"] = json!([{"id":"foreign","card":"BP01-046"}]);
        state["players"]["P1"]["zones"]["hand"] = json!([{"id":"judgment","card":"BP01-131"}]);
        let mut game = actual(&state);
        play(&mut game, "judgment", PlayerId { seat: 1 });
        assert_eq!(damage_events(&mut game, &WILBERT, 2), 4);
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["P1"]["field"], json!([]));
        assert_eq!(view["P2"]["field"], json!([]));
        assert_eq!(view["P2"]["leader"]["life"], 16_u8);
        assert_eq!(view["P1"]["leader"]["life"], 20_u8);
    }
}

#[test]
fn real_granted_ward_and_a_standing_evolve_follower_both_count_before_their_batch_departs() {
    for (victim, granted) in [("BP01-171", true), ("BP06-098", false)] {
        let mut state = initial();
        state["players"]["P1"]["zones"]["field"] = json!([
            {"id":"wilbert","card":"BP06-090"},
            {"id":"victim","card":victim,"state":{"acted":false}}
        ]);
        let mut hand = vec![json!({"id":"judgment","card":"BP01-131"})];
        if granted {
            hand.push(json!({"id":"sunlight","card":"BP01-149"}));
        }
        state["players"]["P1"]["zones"]["hand"] = json!(hand);
        let mut game = actual(&state);
        if granted {
            play(&mut game, "sunlight", PlayerId { seat: 1 });
            let q = awaiting(pending(&mut game, "BP01-149", 1));
            let InputDomain::Targets(domain) = &q.domain else {
                panic!("Sunlight's printed formal target");
            };
            let selected = domain
                .members
                .iter()
                .find(|member| member.card.as_deref() == Some(victim))
                .unwrap();
            resolved(
                game.answer_internal_card(q.actor, q.token, Answer::Targets(vec![selected.handle]))
                    .unwrap(),
            );
            finish_checks(&mut game);
        }
        let before = game.projection(View::Referee).unwrap();
        assert_eq!(before["objects"]["victim"]["acted"], false);
        assert!(
            before["objects"]["victim"]["keywords"]
                .as_array()
                .unwrap()
                .contains(&json!("守護"))
        );
        play(&mut game, "judgment", PlayerId { seat: 1 });
        assert_eq!(damage_events(&mut game, &WILBERT, 2), 2);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
            18_u8
        );
    }
}

#[test]
fn all_wilbert_printings_choose_only_a_cheap_differently_named_ward_and_order_the_private_remainder()
 {
    for printing in WILBERT {
        for accept in [true, false] {
            let mut state = initial();
            state["players"]["P1"]["zones"]["hand"] = json!([{"id":"wilbert","card":printing}]);
            state["players"]["P1"]["zones"]["deck"] = json!([
                {"id":"eligible","card":"BP01-176","state":{"cost":9_u8}},
                {"id":"expensive","card":"BP01-152","state":{"cost":1_u8}},
                {"id":"same","card":"BP06-SL16"},
                {"id":"plain","card":"BP01-171"},
                {"id":"spell","card":"BP01-169"},
                {"id":"tail","card":"BP01-176"}
            ]);
            let mut game = actual(&state);
            play(&mut game, "wilbert", PlayerId { seat: 1 });
            let q = awaiting(pending(&mut game, printing, 3));
            let InputDomain::Targets(domain) = &q.domain else {
                panic!("private Look selection");
            };
            assert_eq!(domain.members.len(), 1);
            assert_eq!(domain.members[0].card.as_deref(), Some("BP01-176"));
            assert_eq!(domain.cardinality.minimum, Nat(0));
            assert_eq!(domain.cardinality.maximum, Some(Nat(1)));
            assert!(game.internal_decision(View::P2).is_none());
            let answer = if accept {
                vec![domain.members[0].handle]
            } else {
                vec![]
            };
            let declaration = game
                .answer_internal_card(q.actor, q.token, Answer::Targets(answer))
                .unwrap();
            let order = awaiting(wards(&mut game, declaration));
            let InputDomain::Order(cards) = &order.domain else {
                panic!("private remainder order");
            };
            assert_eq!(cards.len(), if accept { 4 } else { 5 });
            resolved(
                game.answer_internal_card(
                    order.actor,
                    order.token,
                    Answer::Order(cards.iter().rev().map(|card| card.handle).collect()),
                )
                .unwrap(),
            );
            finish_checks(&mut game);
            assert!(game.internal_pending_choice().unwrap().is_none());
            let view = game.projection(View::Referee).unwrap();
            assert_eq!(view["P1"]["pp"]["current"], 5_u8);
            assert_eq!(view["P1"]["hand"], json!([]));
            assert_eq!(
                view["P1"]["field"],
                if accept {
                    json!(["wilbert", "eligible"])
                } else {
                    json!(["wilbert"])
                }
            );
            assert_eq!(
                view["P1"]["deck"],
                if accept {
                    json!(["tail", "spell", "plain", "same", "expensive"])
                } else {
                    json!(["tail", "spell", "plain", "same", "expensive", "eligible"])
                }
            );
        }
    }
}

#[test]
fn each_lady_printing_excludes_itself_foreign_entries_and_a_discounted_original_three() {
    for printing in LADY {
        let mut state = initial();
        state["players"]["P1"]["leader"]["class"] = json!("ナイトメア");
        state["semantic_state"]["counters_this_turn"] = json!({"P1":{"discarded":1_u8}});
        state["players"]["P1"]["zones"]["hand"] = json!([
            {"id":"lady","card":printing},{"id":"lancer","card":"BP01-046"},
            {"id":"girl","card":"BP02-063"}
        ]);
        let mut game = actual(&state);
        play(&mut game, "lady", PlayerId { seat: 1 });
        assert_eq!(damage_events(&mut game, &LADY, 1), 0);
        play(&mut game, "lancer", PlayerId { seat: 1 });
        assert_eq!(damage_events(&mut game, &LADY, 1), 1);
        play(&mut game, "girl", PlayerId { seat: 1 });
        assert_eq!(damage_events(&mut game, &LADY, 1), 0);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
            5_u8
        );
        assert_eq!(
            game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
            19_u8
        );
        let mut foreign = initial();
        foreign["turn"]["active"] = json!("P2");
        foreign["players"]["P1"]["zones"]["field"] = json!([{"id":"lady","card":printing}]);
        foreign["players"]["P2"]["zones"]["hand"] = json!([{"id":"foreign","card":"BP01-046"}]);
        let mut foreign_game = actual(&foreign);
        play(&mut foreign_game, "foreign", PlayerId { seat: 2 });
        assert_eq!(damage_events(&mut foreign_game, &LADY, 1), 0);
        assert_eq!(
            foreign_game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
            20_u8
        );
    }
}

#[test]
fn each_lady_printing_observes_a_real_lastword_return_during_the_opponents_turn() {
    for printing in LADY {
        let mut state = initial();
        state["turn"]["active"] = json!("P2");
        state["players"]["P1"]["zones"]["field"] = json!([
            {"id":"lady","card":printing},{"id":"alioch","card":"BP08-080"}
        ]);
        state["players"]["P1"]["zones"]["cemetery"] = json!([{"id":"returned","card":printing}]);
        state["players"]["P2"]["zones"]["hand"] = json!([{"id":"execution","card":"BP01-169"}]);
        let mut game = actual(&state);
        let q = awaiting(
            game.play_internal_card("execution", PlayerId { seat: 2 })
                .unwrap(),
        );
        let InputDomain::Targets(domain) = &q.domain else {
            panic!("Execution's formal target");
        };
        let selected = domain
            .members
            .iter()
            .find(|member| member.card.as_deref() == Some("BP08-080"))
            .unwrap();
        resolved(
            game.answer_internal_card(q.actor, q.token, Answer::Targets(vec![selected.handle]))
                .unwrap(),
        );
        let lastword_question = awaiting(pending(&mut game, "BP08-080", 2));
        let InputDomain::Targets(return_targets) = &lastword_question.domain else {
            panic!("Alioch's real Lastword return target");
        };
        assert_eq!(return_targets.members.len(), 1);
        assert_eq!(return_targets.members[0].card.as_deref(), Some(printing));
        resolved(
            game.answer_internal_card(
                lastword_question.actor,
                lastword_question.token,
                Answer::Targets(vec![return_targets.members[0].handle]),
            )
            .unwrap(),
        );
        assert_eq!(damage_events(&mut game, &LADY, 1), 1);
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["turn"]["active"], "P2");
        assert_eq!(view["P1"]["field"], json!(["lady", "returned"]));
        assert_eq!(view["P2"]["leader"]["life"], 19_u8);
    }
}
