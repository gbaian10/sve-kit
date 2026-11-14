#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate the printed Gigablade subscription scenario."
)]
use super::*;
use sve_engine::game::internal::SearchAnswer;
fn finish(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}
fn resolve(game: &mut Game, mut play: CardPlay) {
    while let CardPlay::Awaiting(question) = play {
        let answer = match &question.domain {
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            InputDomain::Search(domain) => {
                assert!(domain.cards.is_empty(), "this fixture has no deck candidates");
                Answer::Search(domain.groups.as_ref().map_or_else(
                    || SearchAnswer::Ungrouped(vec![]),
                    |groups| SearchAnswer::Grouped(vec![vec![]; groups.len()]),
                ))
            }
            _ => panic!("printed Ward or empty optional search"),
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    finish(game);
}
fn pending(game: &mut Game) {
    finish(game);
    while let Some(question) = game.internal_pending_choice().unwrap() {
        let InputDomain::Pending(options) = &question.domain else {
            panic!("printed pending");
        };
        let play = game
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap();
        resolve(game, play);
    }
}
#[test]
fn bp18_027_whole_card_shares_its_entry_and_evolution_observer_without_duplicating_it() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"first","card":"BP18-027"},{"id":"second","card":"BP18-027"},
        {"id":"examiner","card":"BP18-024"}
    ]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP18-025"}]);
    let mut game = with_producer(
        &initial,
        Action::Destroy {
            objects: damage_prevent::field_name("ギガブレード"),
        },
    );
    for id in ["first", "second"] {
        let play = game.play_internal_card(id, PlayerId { seat: 1 }).unwrap();
        resolve(&mut game, play);
        pending(&mut game);
    }
    let before = game.projection(View::Referee).unwrap();
    assert_eq!(
        before["objects"]["first"]["counters"]["ギガカウンター"], 4_u8,
        "each native Fanfare adds two to all own Gigablades"
    );
    assert_eq!(
        before["P1"]["cemetery"],
        json!(["second"]),
        "the duplicate puts itself into the cemetery"
    );
    let play = game
        .play_internal_card("examiner", PlayerId { seat: 1 })
        .unwrap();
    resolve(&mut game, play);
    pending(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["first"]["counters"]["ギガカウンター"],
        5_u8,
        "the Tokyo follower entry fires the shared observer once"
    );
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"examiner","evolve_card":"form","pay":{"pp":1_u8}})
        )
        .unwrap()
        .outcome,
        "resolved",
        "a real follower evolution supplies the other event family"
    );
    pending(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["first"]["counters"]["ギガカウンター"],
        6_u8,
        "evolution fires the same observer once"
    );
    activate(&mut game, "P1");
    finish(&mut game);
    assert!(
        game.projection(View::Referee).unwrap()["P1"]["field"]
            .as_array()
            .unwrap()
            .contains(&json!("first")),
        "the native ability-destruction prohibition remains active"
    );
}
