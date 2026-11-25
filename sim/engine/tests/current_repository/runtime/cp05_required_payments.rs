#![expect(
    clippy::panic,
    reason = "An unexpected decision invalidates the printed payment scenario."
)]
use super::event_cemetery::finish_checks;
use super::*;

const P1: PlayerId = PlayerId { seat: 1 };

fn waiting(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(question) = play else {
        panic!("expected the printed payment scenario's next decision");
    };
    question
}

fn choose(game: &mut Game, question: &DecisionView, answer: Answer) -> CardPlay {
    game.answer_internal_card(question.actor, question.token, answer)
        .unwrap()
}

fn pick_discard(game: &mut Game, play: CardPlay, printing: &str) -> CardPlay {
    let question = waiting(play);
    let InputDomain::CostMaterials(domain) = &question.domain else {
        panic!("the discard material is chosen privately");
    };
    assert_eq!(
        domain.members.len(),
        1,
        "only the eligible material is offered"
    );
    let material = domain.members.first().unwrap();
    assert_eq!(
        material.card.as_deref(),
        Some(printing),
        "the matching other card pays"
    );
    assert!(
        game.internal_decision(View::P2).is_none(),
        "the other player cannot see the hand choices"
    );
    choose(
        game,
        &question,
        Answer::CostMaterials(vec![material.handle]),
    )
}

fn discard_position(printing: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"amulet","card":"CP05-079"}]);
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"discarded","card":printing}, {"id":"plain","card":"BP01-171"}
    ]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"top","card":"BP01-171"}]);
    initial
}

fn activate_amulet(game: &mut Game, printing: &str) {
    let activation = selection().current["CP05-079"]
        .program
        .card()
        .abilities
        .iter()
        .find(|ability| ability.source.line == 2_u32)
        .unwrap()
        .node
        .id;
    let play = game
        .activate_internal_card("amulet", activation, P1)
        .unwrap();
    let look = waiting(pick_discard(game, play, printing));
    assert!(
        matches!(look.domain, InputDomain::Integer(_)),
        "Celeste offers the optional bottom movement"
    );
    resolved(choose(game, &look, Answer::Integer(Nat(0))));
    finish_checks(game);
}

fn discard_trigger(game: &mut Game, printing: &str) -> CardPlay {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("the self-discard ability is pending at check timing");
    };
    let option = options
        .iter()
        .find(|option| option.card == printing)
        .unwrap();
    assert_eq!(
        option.line, 1_u64,
        "the ability is the card's self-discard line"
    );
    assert_eq!(
        option.event, "Discard",
        "the trigger carries the actual discard"
    );
    choose(game, &question, Answer::Pending(option.handle))
}

#[test]
fn six_pays_three_pp_on_resolution_to_enter_from_the_committed_discard() {
    let printing = "CP05-GBF45";
    for accept in [false, true] {
        let mut game = game(&discard_position(printing));
        activate_amulet(&mut game, printing);
        let fee = waiting(discard_trigger(&mut game, printing));
        assert_eq!(fee.domain, InputDomain::OptionalCost);
        let before = game.projection(View::Referee).unwrap();
        assert_eq!(before["P1"]["cemetery"], json!(["discarded"]));
        assert_eq!(before["P1"]["pp"]["current"], 10_u8);
        resolved(choose(&mut game, &fee, Answer::OptionalCost(accept)));
        let after = game.projection(View::Referee).unwrap();
        assert_eq!(
            after["P1"]["pp"]["current"],
            if accept { 7_u8 } else { 10_u8 }
        );
        assert_eq!(
            after["P1"]["field"],
            if accept {
                json!(["amulet", "discarded"])
            } else {
                json!(["amulet"])
            }
        );
    }
}

#[test]
fn vaseraga_declares_a_follower_before_its_one_pp_resolution_payment() {
    let printing = "PR-637";
    let mut no_target = game(&discard_position(printing));
    activate_amulet(&mut no_target, printing);
    assert!(matches!(
        discard_trigger(&mut no_target, printing),
        CardPlay::Cancelled
    ));
    assert_eq!(
        no_target.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        10_u8
    );

    let mut initial = discard_position(printing);
    initial["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"ally","card":"BP01-171"}));
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"chosen","card":"BP14-022"}, {"id":"other","card":"BP01-037"}
    ]);
    let mut game = game(&initial);
    activate_amulet(&mut game, printing);
    let targets = waiting(discard_trigger(&mut game, printing));
    let InputDomain::Targets(domain) = &targets.domain else {
        panic!("the printed ability first declares a formal target");
    };
    assert_eq!(domain.members.len(), 2);
    let recipient = domain
        .members
        .iter()
        .find(|member| member.card.as_deref() == Some("BP14-022"))
        .unwrap();
    let fee = waiting(choose(
        &mut game,
        &targets,
        Answer::Targets(vec![recipient.handle]),
    ));
    assert_eq!(fee.domain, InputDomain::OptionalCost);
    let before = game.projection(View::Referee).unwrap();
    assert_eq!(before["P1"]["pp"]["current"], 10_u8);
    assert_eq!(before["objects"]["chosen"]["damage"], 0_u8);
    resolved(choose(&mut game, &fee, Answer::OptionalCost(true)));
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["P1"]["pp"]["current"], 9_u8);
    assert_eq!(after["objects"]["chosen"]["damage"], 3_u8);
    assert_eq!(after["objects"]["other"]["damage"], 0_u8);
    assert_eq!(after["P1"]["cemetery"], json!(["discarded"]));
}

#[test]
fn corruption_requires_another_dark_card_and_deals_damage_only_after_payment() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] =
        json!([{"id":"spell","card":"CP05-P63"},{"id":"plain","card":"BP01-171"}]);
    let mut denied = game(&initial);
    let before = denied.projection(View::Referee).unwrap();
    denied.play_internal_card("spell", P1).unwrap_err();
    assert_eq!(denied.projection(View::Referee).unwrap(), before);

    initial["players"]["P1"]["zones"]["hand"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"material","card":"CP05-074"}));
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"ally","card":"BP01-037"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    let mut game = game(&initial);
    let play = game.play_internal_card("spell", P1).unwrap();
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
        20_u8
    );
    resolved(pick_discard(&mut game, play, "CP05-074"));
    finish_checks(&mut game);
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["pp"]["current"], 5_u8);
    assert_eq!(view["P1"]["hand"], json!(["plain"]));
    assert_eq!(view["P1"]["field"], json!([]));
    assert_eq!(view["P2"]["field"], json!([]));
    assert_eq!(view["P2"]["leader"]["life"], 18_u8);
}
