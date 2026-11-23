use super::event_cemetery::finish_checks;
use super::*;
use sve_engine::game::player::observation::ProcessingStage;

#[test]
fn bp03_002_fanfare_uses_hand_when_its_own_entry_fills_the_field() {
    for field_full in [true, false] {
        let mut initial = position_with_source("BP03-002", "hand");
        initial["players"]["P1"]["zones"]["field"] = json!(
            (0_u8..if field_full { 4 } else { 3 })
                .map(|i| json!({"id":format!("prior-{i}"),"card":"BP01-037"}))
                .collect::<Vec<_>>()
        );
        initial["players"]["P1"]["zones"]["deck"] = json!([
            {"id":"beast","card":"BP03-005"},
            {"id":"spare","card":"BP01-037"}
        ]);
        let mut game = game(&initial);
        let play = enter_with_fanfare(&mut game, "BP03-002");
        let search = await_card_choice(play);
        let InputDomain::Search(domain) = &search.domain else {
            panic!("printed deck search");
        };
        let beast = domain
            .cards
            .iter()
            .find(|card| card.card == "BP03-005")
            .unwrap();
        let after_search = game
            .answer_internal_card(
                search.actor,
                search.token,
                Answer::Search(SearchAnswer::Ungrouped(vec![beast.handle])),
            )
            .unwrap();
        let result = if field_full {
            resolved(after_search)
        } else {
            let replacement = await_card_choice(after_search);
            assert_eq!(replacement.domain, InputDomain::SearchReplacement);
            resolved(
                game.answer_internal_card(
                    replacement.actor,
                    replacement.token,
                    Answer::SearchReplacement(SearchReplacementAnswer::Replacement),
                )
                .unwrap(),
            )
        };
        let state = game.projection(View::Referee).unwrap();
        let destination = if field_full { "hand" } else { "field" };
        assert!(
            state["P1"][destination]
                .as_array()
                .unwrap()
                .contains(&json!("beast"))
        );
        assert_eq!(
            result
                .progress
                .facts
                .iter()
                .filter(|fact| matches!(fact, Fact::Shuffled { .. }))
                .count(),
            1
        );
    }
}

#[test]
fn bp01_097_enters_before_its_fanfare_declares_or_cancels() {
    for seat in ["P1", "P2"] {
        let opponent = if seat == "P1" { "P2" } else { "P1" };
        for enemy in [false, true] {
            let mut initial = draw_position();
            initial["turn"]["active"] = json!(seat);
            initial["players"][seat]["zones"]["hand"] = json!([{"id":"dragon","card":"BP01-097"}]);
            if enemy {
                initial["players"][opponent]["zones"]["field"] =
                    json!([{"id":"enemy","card":"BP01-037"}]);
            }
            let mut game = game(&initial);
            let entered = game
                .play_internal_card(
                    "dragon",
                    PlayerId {
                        seat: seat_number(seat),
                    },
                )
                .unwrap();
            assert!(
                matches!(entered, CardPlay::Entered(_)),
                "entry has no play-time targets"
            );
            assert!(game.internal_decision(View::Referee).is_none());
            let entered_state = game.projection(View::Referee).unwrap();
            assert_eq!(entered_state[seat]["field"], json!(["dragon"]));
            assert_eq!(entered_state[seat]["pp"]["current"], 3_u8);
            let pending = game.internal_pending_choice().unwrap().unwrap();
            let InputDomain::Pending(options) = &pending.domain else {
                panic!("pending Fanfare")
            };
            assert_eq!(options.len(), 1);
            assert_eq!(options[0].card, "BP01-097");
            let fanfare = game
                .answer_internal_card(
                    pending.actor,
                    pending.token,
                    Answer::Pending(options[0].handle),
                )
                .unwrap();
            if enemy {
                let viewer = if seat == "P1" { Viewer::P1 } else { Viewer::P2 };
                let other_player = PlayerId {
                    seat: seat_number(opponent),
                };
                resolve_fanfare_target(&mut game, fanfare, viewer, other_player);
            } else {
                assert_eq!(fanfare, CardPlay::Cancelled);
                assert!(game.internal_decision(View::Referee).is_none());
            }
            finish_checks(&mut game);
            assert!(game.internal_pending_choice().unwrap().is_none());
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state[seat]["field"], json!(["dragon"]));
            assert_eq!(state[seat]["pp"]["current"], 3_u8);
            if enemy {
                assert_eq!(state[opponent]["cemetery"], json!(["enemy"]));
            }
        }
    }
}

#[expect(
    clippy::panic,
    reason = "The printed Fanfare must declare a legal target."
)]
fn resolve_fanfare_target(game: &mut Game, play: CardPlay, viewer: Viewer, opponent: PlayerId) {
    let CardPlay::Awaiting(question) = play else {
        panic!("the Fanfare declares a field target");
    };
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("field target");
    };
    assert_eq!(
        domain.members.len(),
        1,
        "the opposing follower is the only candidate"
    );
    assert_eq!(
        domain.cardinality.minimum,
        Nat(1),
        "the target is mandatory"
    );
    let processing = game.observe(viewer).unwrap().processing;
    assert_eq!(
        processing.last().unwrap().stage,
        Some(ProcessingStage::Target),
        "the Fanfare declares the target before resolution"
    );
    let answer = Answer::Targets(vec![domain.members[0].handle]);
    let before = format!("{game:?}");
    game.answer_internal_card(opponent, question.token, answer.clone())
        .unwrap_err();
    assert!(
        format!("{game:?}") == before,
        "another actor cannot change the target declaration"
    );
    let result = resolved(
        game.answer_internal_card(question.actor, question.token, answer)
            .unwrap(),
    );
    assert_eq!(
        result
            .progress
            .facts
            .iter()
            .filter(|fact| matches!(fact, Fact::Damage(_)))
            .count(),
        1,
        "resolving the declared target produces one damage event"
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
        7_u8,
        "the Fanfare deals its printed damage"
    );
}
