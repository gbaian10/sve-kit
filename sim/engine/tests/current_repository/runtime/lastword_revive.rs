#![expect(
    clippy::panic,
    reason = "Unexpected choices invalidate these printed resurrection paths."
)]
use super::*;

fn initial(card: &str, pp: u8) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["pp"]["current"] = json!(pp);
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    initial
}

fn enter(game: &mut Game) {
    let CardPlay::Entered(entry) = game
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("real follower play");
    };
    if matches!(entry.resume, ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}

fn trigger(game: &mut Game, card: &str) -> CardPlay {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("printed trigger");
    };
    assert_eq!(
        options.len(),
        1,
        "only this real card contributes a trigger"
    );
    assert_eq!(
        options[0].card, card,
        "the actual printing supplies the source"
    );
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::Pending(options[0].handle),
    )
    .unwrap()
}

fn destroy_own_field() -> Action {
    let mut objects = subjects();
    let Selector::Zone { players, .. } = &mut objects else {
        panic!("field selector");
    };
    players.relation = PlayerRelation::SelfPlayer;
    players.text_quantifier = TextQuantifier::Implicit;
    players.cardinality = Cardinality {
        minimum: Nat(1),
        maximum: Some(Nat(1)),
    };
    Action::Destroy { objects }
}

fn resolve_damage_target(game: &mut Game, play: CardPlay) {
    let CardPlay::Awaiting(target) = play else {
        panic!("Fanfare target");
    };
    let InputDomain::Targets(domain) = &target.domain else {
        panic!("formal target");
    };
    assert_eq!(
        domain.members.len(),
        1,
        "the same opposing follower is the only legal target"
    );
    resolved(
        game.answer_internal_card(
            target.actor,
            target.token,
            Answer::Targets(vec![domain.members[0].handle]),
        )
        .unwrap(),
    );
    game.resume_internal_card().unwrap();
}

#[test]
fn bp04_093_pays_two_pp_to_return_the_same_real_card_after_departure() {
    let mut game = with_producer(
        &initial("BP04-093", 5),
        Action::Destroy {
            objects: subjects(),
        },
    );
    enter(&mut game);
    let entered = game.projection(View::Referee).unwrap();
    assert_eq!(entered["P1"]["pp"]["current"], 2_u8);
    let generation = entered["objects"]["source"]["generation"].as_u64().unwrap();
    activate(&mut game, "P1");
    game.resume_internal_card().unwrap();
    let CardPlay::Awaiting(payment) = trigger(&mut game, "BP04-093") else {
        panic!("Lastword head payment");
    };
    assert!(matches!(payment.domain, InputDomain::OptionalCost));
    resolved(
        game.answer_internal_card(payment.actor, payment.token, Answer::OptionalCost(true))
            .unwrap(),
    );
    game.resume_internal_card().unwrap();
    let returned = game.projection(View::Referee).unwrap();
    assert_eq!(returned["P1"]["field"], json!(["source"]));
    assert_eq!(returned["P1"]["cemetery"], json!(["producer"]));
    assert_eq!(returned["objects"]["source"]["generation"], generation + 2);
    assert_eq!(returned["objects"]["source"]["power"], 3_u8);
    assert_eq!(returned["objects"]["source"]["hp"], 4_u8);
    assert_eq!(returned["objects"]["source"]["damage"], 0_u8);
    assert_eq!(returned["P1"]["pp"]["current"], 0_u8);
    assert!(game.internal_pending_choice().unwrap().is_none());
}

#[test]
fn bp19_003_and_sl03_return_and_trigger_fanfare_again_without_paying_six_again() {
    for card in ["BP19-003", "BP19-SL03"] {
        let mut game = with_producer(&initial(card, 10), destroy_own_field());
        enter(&mut game);
        let first_fanfare = trigger(&mut game, card);
        resolve_damage_target(&mut game, first_fanfare);
        let entered = game.projection(View::Referee).unwrap();
        assert_eq!(entered["objects"]["enemy"]["damage"], 4_u8);
        assert_eq!(entered["P1"]["pp"]["current"], 4_u8);
        let generation = entered["objects"]["source"]["generation"].as_u64().unwrap();
        activate(&mut game, "P1");
        game.resume_internal_card().unwrap();
        resolved(trigger(&mut game, card));
        game.resume_internal_card().unwrap();
        let returned = game.projection(View::Referee).unwrap();
        assert_eq!(returned["P1"]["field"], json!(["source"]));
        assert_eq!(returned["objects"]["source"]["generation"], generation + 2);
        assert_eq!(returned["objects"]["source"]["power"], 4_u8);
        assert_eq!(returned["objects"]["source"]["hp"], 5_u8);
        assert_eq!(returned["P1"]["pp"]["current"], 4_u8);
        assert_eq!(returned["P2"]["field"], json!(["enemy"]));
        assert_eq!(returned["objects"]["enemy"]["damage"], 4_u8);
        let second_fanfare = trigger(&mut game, card);
        resolve_damage_target(&mut game, second_fanfare);
        let after_fanfare = game.projection(View::Referee).unwrap();
        assert_eq!(after_fanfare["P2"]["field"], json!([]));
        assert_eq!(after_fanfare["P2"]["cemetery"], json!(["enemy"]));
        assert_eq!(after_fanfare["P1"]["pp"]["current"], 4_u8);
        assert!(game.internal_pending_choice().unwrap().is_none());
    }
}
