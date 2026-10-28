#![expect(
    clippy::panic,
    reason = "A malformed pending domain fails this test scenario."
)]
use super::*;
use sve_engine::game::internal::SearchAnswer;

fn actor(seat: &str) -> PlayerId {
    PlayerId {
        seat: seat_number(seat),
    }
}

fn play_merlin_fanfare(game: &mut Game, seat: &str) -> CardPlay {
    assert!(
        matches!(
            game.play_internal_card("merlin", actor(seat)).unwrap(),
            CardPlay::Entered(_)
        ),
        "the real follower enters after paying its printed cost"
    );
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("one Fanfare");
    };
    assert_eq!(options.len(), 1, "only the played card has a Fanfare");
    assert_eq!(
        options[0].line, 2,
        "the Fanfare belongs to printed line two"
    );
    game.answer_internal_card(
        pending.actor,
        pending.token,
        Answer::Pending(options[0].handle),
    )
    .unwrap()
}

#[test]
fn merlin_fanfare_searches_only_its_own_spells_and_finishes_once() {
    for seat in ["P1", "P2"] {
        let opponent = if seat == "P1" { "P2" } else { "P1" };
        for has_spell in [true, false] {
            let mut initial = draw_position();
            initial["turn"]["active"] = json!(seat);
            initial["players"][seat]["zones"]["hand"] = json!([{"id":"merlin","card":"EBD02-004"}]);
            initial["players"][seat]["zones"]["deck"] =
                json!([{"id":"follower","card":"CP03-016"}]);
            if has_spell {
                initial["players"][seat]["zones"]["deck"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!({"id":"found","card":"SD03-005"}));
            }
            initial["players"][opponent]["zones"]["deck"] =
                json!([{"id":"enemy-spell","card":"SD03-005"}]);
            if has_spell {
                let mut failed = initial.clone();
                failed["players"][seat]["pp"]["current"] = json!(2_u64);
                let mut rejected = game(&failed);
                let before_payment = format!("{rejected:?}");
                rejected
                    .play_internal_card("merlin", actor(seat))
                    .unwrap_err();
                assert!(
                    format!("{rejected:?}") == before_payment,
                    "rejected input must preserve authority state"
                );
            }
            let mut game = game(&initial);
            let CardPlay::Awaiting(question) = play_merlin_fanfare(&mut game, seat) else {
                panic!("Search");
            };
            let InputDomain::Search(domain) = &question.domain else {
                panic!("private Search");
            };
            assert_eq!(domain.cards.len(), usize::from(has_spell));
            let view = if seat == "P1" { View::P2 } else { View::P1 };
            assert!(game.projection(view).unwrap()["awaiting"]["domain"].is_null());
            let before = format!("{game:?}");
            game.answer_internal_card(
                actor(opponent),
                question.token,
                Answer::Search(SearchAnswer::Ungrouped(vec![])),
            )
            .unwrap_err();
            game.answer_internal_card(
                question.actor,
                question.token,
                Answer::Search(SearchAnswer::Ungrouped(vec![99])),
            )
            .unwrap_err();
            assert!(
                format!("{game:?}") == before,
                "rejected input must preserve authority state"
            );
            let chosen = domain.cards.iter().map(|card| card.handle).collect();
            let result = resolved(
                game.answer_internal_card(
                    question.actor,
                    question.token,
                    Answer::Search(SearchAnswer::Ungrouped(chosen)),
                )
                .unwrap(),
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
            game.resume_internal_card().unwrap();
            assert!(game.internal_pending_choice().unwrap().is_none());
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state[seat]["hand"],
                if has_spell {
                    json!(["found"])
                } else {
                    json!([])
                }
            );
            assert_eq!(state[seat]["deck"], json!(["follower"]));
            assert_eq!(state[opponent]["deck"], json!(["enemy-spell"]));
            assert_eq!(state[seat]["pp"]["current"], 7_u64);
        }
    }
}

#[test]
fn holy_wish_chooses_one_printed_activation_and_pays_before_drawing() {
    for seat in ["P1", "P2"] {
        let opponent = if seat == "P1" { "P2" } else { "P1" };
        for (line, pp, drawn) in [(1_u32, 0_u8, 1_usize), (2, 2, 2)] {
            let mut initial = draw_position();
            initial["turn"]["active"] = json!(seat);
            initial["players"][seat]["pp"] = json!({"current":pp,"max":2_u64});
            initial["players"][seat]["zones"]["field"] =
                json!([{"id":"amulet","card":"EBD04-014"}]);
            initial["players"][seat]["zones"]["deck"] = json!([
                {"id":"draw1","card":"CP03-016"},{"id":"draw2","card":"CP03-016"},{"id":"draw3","card":"CP03-016"}
            ]);
            let (snapshot, keywords, input) = inputs();
            let selected = Catalog::from_current_selection(&snapshot, &keywords, &input).unwrap();
            let [first, second] = [1_u32, 2].map(|printed_line| {
                selected.current["EBD04-014"]
                    .program
                    .card()
                    .abilities
                    .iter()
                    .find(|entry| entry.source.line == printed_line)
                    .unwrap()
                    .node
                    .id
            });
            let ability = if line == 1 { first } else { second };
            let mut game = Game::from_selected_effects(
                selected,
                &initial,
                &Value::Null,
                &Value::Null,
                "current-runtime",
            )
            .unwrap();
            let before_actor = format!("{game:?}");
            game.activate_internal_card("amulet", ability, actor(opponent))
                .unwrap_err();
            if line == 1 {
                game.activate_internal_card("amulet", second, actor(seat))
                    .unwrap_err();
            }
            assert!(
                format!("{game:?}") == before_actor,
                "rejected input must preserve authority state"
            );
            resolved(
                game.activate_internal_card("amulet", ability, actor(seat))
                    .unwrap(),
            );
            game.resume_internal_card().unwrap();
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state[seat]["field"], json!([]));
            assert_eq!(state[seat]["cemetery"], json!(["amulet"]));
            assert_eq!(state[seat]["hand"].as_array().unwrap().len(), drawn);
            assert_eq!(state[seat]["deck"].as_array().unwrap().len(), 3 - drawn);
            assert_eq!(state[seat]["pp"]["current"], 0_u64);
            let before = format!("{game:?}");
            game.activate_internal_card("amulet", ability, actor(seat))
                .unwrap_err();
            assert!(
                format!("{game:?}") == before,
                "rejected input must preserve authority state"
            );
        }
    }
}
