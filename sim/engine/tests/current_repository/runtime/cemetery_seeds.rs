#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "The printed cemetery seeds have explicit target, payment and ordering choices."
)]
use super::*;

fn pending(game: &mut Game, card: &str) -> CardPlay {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("printed pending")
    };
    let handle = options
        .iter()
        .find(|option| option.card == card)
        .unwrap()
        .handle;
    game.answer_internal_card(question.actor, question.token, Answer::Pending(handle))
        .unwrap()
}

fn finish(game: &mut Game, mut play: CardPlay, take: usize) -> CardPlay {
    while let CardPlay::Awaiting(question) = play {
        let answer = match &question.domain {
            InputDomain::OptionalCost => Answer::OptionalCost(true),
            InputDomain::Targets(domain) => Answer::Targets(
                domain
                    .members
                    .iter()
                    .take(take)
                    .map(|member| member.handle)
                    .collect(),
            ),
            InputDomain::Order(cards) => {
                Answer::Order(cards.iter().rev().map(|card| card.handle).collect())
            }
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            other => panic!("unexpected printed seed choice {other:?}"),
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    play
}

#[test]
fn bp08_068_spell_enters_cemetery_then_banishes_itself_to_damage_an_own_follower() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] =
        json!([{"id":"source","card":"BP08-068"},{"id":"galmieux","card":"BP05-052"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    let mut game = game(&initial);
    let spell = game
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap();
    resolved(finish(&mut game, spell, 1));
    game.resume_internal_card().unwrap();
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["cemetery"],
        json!([])
    );
    assert!(matches!(
        game.play_internal_card("galmieux", PlayerId { seat: 1 })
            .unwrap(),
        CardPlay::Entered(_)
    ));
    let before = game.projection(View::Referee).unwrap()["objects"]["galmieux"]["hp"]
        .as_i64()
        .unwrap();
    let grave = pending(&mut game, "BP08-068");
    resolved(finish(&mut game, grave, 1));
    game.resume_internal_card().unwrap();
    let galmieux = pending(&mut game, "BP05-052");
    resolved(finish(&mut game, galmieux, 1));
    game.resume_internal_card().unwrap();
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["banish"], json!(["source"]));
    assert_eq!(state["objects"]["galmieux"]["hp"], before - 1);
    assert_eq!(state["objects"]["enemy"]["hp"], 4_u8);
    assert_eq!(state["P1"]["pp"]["current"], 4_u8);
}
