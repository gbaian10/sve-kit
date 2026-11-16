use super::super::*;
use super::select::{components, domain, project_question, question, seats};
use crate::game::internal::SearchAnswer;
use crate::game::player::{SubmissionKind, Viewer};
use crate::game::{Game, View};
use crate::ir::SearchReplacementAnswer;
use serde_json::json;
use sve_battle_contract::{QuestionId, Seat, Zone};

#[test]
fn found_movement_offers_real_original_and_replacement_destinations_for_both_seats() {
    for actor in [Viewer::P1, Viewer::P2] {
        for replacement in [false, true] {
            let (mut game, _) = searched(
                actor,
                &["unit-follower", "unit-spell"],
                "unit-amulet",
                vec![0],
                2,
            );
            let native = question(&game, actor).unwrap();
            let (usage, shape, binding) = project_question(
                &native,
                &components(&game, actor),
                &QuestionId::new("search-route".into()),
            )
            .unwrap();
            assert_eq!(usage, wire::QuestionUse::SearchReplacement);
            let wire::QuestionShape::Choose { options, .. } = shape else {
                panic!("route choice")
            };
            for (option, zone) in options.iter().zip([Zone::Hand, Zone::Field]) {
                let wire::ChoiceValue::Destination { destination, .. } = &option.value else {
                    panic!("native destination")
                };
                assert_eq!(
                    destination.player,
                    if actor == Viewer::P1 {
                        Seat::P1
                    } else {
                        Seat::P2
                    }
                );
                assert_eq!(destination.zone, zone);
                assert!(destination.position.is_none());
            }
            let answer = Answer::Choose {
                chosen: vec![options[usize::from(replacement)].id.clone()],
            };
            assert_eq!(
                binding.decode(&answer),
                Ok(player::Answer::SearchReplacement(if replacement {
                    SearchReplacementAnswer::Replacement
                } else {
                    SearchReplacementAnswer::Original
                }))
            );
            assert_eq!(
                game.submit_player(actor, binding.answer(native.token, &answer).unwrap())
                    .kind,
                SubmissionKind::Applied
            );
            assert!(
                game.projection(View::Referee).unwrap()[seats(actor).0]
                    [if replacement { "field" } else { "hand" }]
                .as_array()
                .unwrap()
                .contains(&json!("deck-0"))
            );
        }
    }
}

mod exchange;

fn searched(
    actor: Viewer,
    deck: &[&str],
    secret: &str,
    selected: Vec<u64>,
    limit: i64,
) -> (Game, player::Question) {
    let search = json!({"sel":{"who":"self","zone":"deck","kind":"follower"},"upto":1_i32,"to":{"zone":"hand","instead":{"if":{"cmp":{"left":{"cost_of":"found"},"op":"le","right":limit}},"zone":"field","may":true}}});
    let mut game = super::search::make(actor, &search, deck, secret);
    let native = question(&game, actor).unwrap();
    assert!(matches!(domain(&native), Some(DecisionShape::Search(_))));
    assert_eq!(
        game.submit_player(
            actor,
            player::Command::Answer {
                token: native.token,
                answer: player::Answer::Search(SearchAnswer::Ungrouped(selected))
            }
        )
        .kind,
        SubmissionKind::Applied
    );
    (game, native)
}

#[test]
fn empty_found_and_false_conditions_keep_the_native_search_replacement_skip() {
    for actor in [Viewer::P1, Viewer::P2] {
        for (selected, limit) in [(vec![], 2_i64), (vec![0], 0)] {
            let (game, _) = searched(
                actor,
                &["unit-follower", "unit-spell"],
                "unit-amulet",
                selected,
                limit,
            );
            assert!(!matches!(
                question(&game, actor).as_ref().and_then(domain),
                Some(DecisionShape::SearchReplacement(_))
            ));
            assert_eq!(
                game.projection(View::Referee).unwrap()[seats(actor).0]["leader"]["life"],
                21_i32
            );
        }
    }
}

mod privacy;
mod validation;
