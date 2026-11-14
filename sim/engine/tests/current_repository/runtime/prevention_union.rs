#![expect(
    clippy::panic,
    reason = "Unexpected questions invalidate the whole-card scenario."
)]
use super::*;

fn finish(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}

fn enter(game: &mut Game, id: &str) {
    assert!(
        matches!(
            game.play_internal_card(id, PlayerId { seat: 1 }).unwrap(),
            CardPlay::Entered(_)
        ),
        "the native Castel play must enter"
    );
    finish(game);
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("fanfare pending");
    };
    assert_eq!(options.len(), 1, "only this Castel contributes a Fanfare");
    assert_eq!(
        options[0].card, "BP21-001",
        "the printed Castel supplies the Fanfare"
    );
    resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap(),
    );
    finish(game);
}

#[test]
fn bp21_001_whole_card_deduplicates_dual_traits_and_prevents_only_other_named_allies() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"castel-1","card":"BP21-001"},
        {"id":"castel-2","card":"BP21-001"}
    ]);
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"ally","card":"BP21-005","state":{"hp":20_u8,"max_hp":20_u8}}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"enemy","card":"BP21-005","state":{"hp":20_u8,"max_hp":20_u8}}
    ]);
    let action = Action::Damage {
        recipients: RecipientSet::Objects {
            objects: subjects(),
        },
        amount: DamageAmount::Uniform {
            amount: Expr::Literal(Int(1)),
        },
        flags: DamageFlags::Ability,
        source: ObjectView::SourceInformation {},
        batch: BatchPolicy::Instruction {},
    };
    let mut game = with_producer(&initial, action);
    enter(&mut game, "castel-1");
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["ex"],
        json!([]),
        "one dual-trait peer and Castel are only two followers"
    );
    enter(&mut game, "castel-2");
    let before = game.projection(View::Referee).unwrap();
    let ex = before["P1"]["ex"].as_array().unwrap();
    assert_eq!(
        ex.len(),
        1,
        "three distinct qualifying followers create one token"
    );
    assert_eq!(
        before["objects"][ex[0].as_str().unwrap()]["name"],
        "緑の顕現",
        "the printed Fanfare creates the named token"
    );
    assert_eq!(
        before["P1"]["pp"]["current"], 8_u8,
        "both proper plays pay their own PP"
    );
    let mut play = game
        .activate_internal_card("producer", AbilityId(1), PlayerId { seat: 1 })
        .unwrap();
    while let CardPlay::Awaiting(question) = play {
        let InputDomain::DamageReplacements(options) = &question.domain else {
            panic!("replacement ordering");
        };
        let order = options.iter().map(|option| option.handle).collect();
        play = game
            .answer_internal_card(question.actor, question.token, Answer::Order(order))
            .unwrap();
    }
    resolved(play);
    finish(&mut game);
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(
        after["objects"]["ally"]["damage"], 0_u8,
        "a qualifying other-named ally is protected"
    );
    for id in ["castel-1", "castel-2", "enemy"] {
        assert_eq!(after["objects"][id]["damage"], 1_u8, "unprotected {id}");
    }
}
