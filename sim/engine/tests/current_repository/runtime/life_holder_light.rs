#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate these game scenarios."
)]
use super::*;

const fn actor() -> PlayerId {
    PlayerId { seat: 1 }
}

fn finish(game: &mut Game, mut play: CardPlay) {
    while let CardPlay::Awaiting(question) = play {
        let answer = if let InputDomain::Targets(domain) = &question.domain {
            Answer::Targets(vec![domain.members[0].handle])
        } else if matches!(&question.domain, InputDomain::OptionalCost) {
            Answer::OptionalCost(true)
        } else if matches!(&question.domain, InputDomain::WardEntry { .. }) {
            Answer::WardEntry(false)
        } else {
            panic!("unexpected decision")
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    triggers(game);
}

fn triggers(game: &mut Game) {
    loop {
        while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
        let Some(question) = game.internal_pending_choice().unwrap() else {
            return;
        };
        let InputDomain::Pending(options) = &question.domain else {
            panic!("pending ability expected")
        };
        let step = game
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap();
        finish(game, step);
    }
}

#[test]
fn bp02_115_destroys_a_cheap_follower_and_heals_its_controllers_leader() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP02-115"}]);
    initial["players"]["P2"]["leader"]["life"] = json!(12_u8);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-171"}]);
    let mut game = game(&initial);
    let step = game.play_internal_card("source", actor()).unwrap();
    finish(&mut game, step);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["cemetery"], json!(["enemy"]));
    assert_eq!(state["P2"]["leader"]["life"], 15_u8, "それのリーダー heals 3");
    assert_eq!(state["P1"]["leader"]["life"], 20_u8);
}
