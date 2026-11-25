#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate the printed discard-history scenario."
)]

use super::*;

const ACTOR: PlayerId = PlayerId { seat: 1 };

fn activation_id(printing: &str) -> AbilityId {
    selection_with_rules().current[printing]
        .program
        .card()
        .abilities
        .iter()
        .find(|ability| ability.node.kind == AbilityKind::Activated)
        .unwrap()
        .node
        .id
}

fn pay_dark_discard(game: &mut Game) {
    let mut play = game
        .activate_internal_card("enabler", activation_id("CP05-079"), ACTOR)
        .unwrap();
    let mut discarded = false;
    loop {
        while let CardPlay::Awaiting(question) = play {
            let answer = match &question.domain {
                InputDomain::CostMaterials(domain) => {
                    let [dark] = domain.members.as_slice() else {
                        panic!("only the dark amulet pays the discard cost");
                    };
                    assert_eq!(
                        dark.card.as_deref(),
                        Some("CP05-071"),
                        "the known dark card is offered"
                    );
                    discarded = true;
                    Answer::CostMaterials(vec![dark.handle])
                }
                InputDomain::Integer(_) => Answer::Integer(Nat(0)),
                _ => panic!("unexpected discard setup question"),
            };
            play = game
                .answer_internal_card(question.actor, question.token, answer)
                .unwrap();
        }
        while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
        let Some(question) = game.internal_pending_choice().unwrap() else {
            break;
        };
        let InputDomain::Pending(options) = &question.domain else {
            panic!("expected a pending discard observer");
        };
        play = game
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap();
    }
    assert!(
        discarded,
        "the activation must have completed its discard cost"
    );
}

#[test]
fn cp05_071_requires_a_dark_discard_before_spending_its_source() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"enabler","card":"CP05-079"}]);
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"amulet","card":"CP05-P55"}, {"id":"dark","card":"CP05-071"}
    ]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"drawn","card":"BP01-037"}, {"id":"tail","card":"BP01-037"}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy_amulet","card":"BP13-087"}]);
    let mut game = Game::from_selected_effects(
        selection_with_rules().clone(),
        &initial,
        &Value::Null,
        &Value::Null,
        "cp05-dragon-discard",
    )
    .unwrap();
    assert!(matches!(
        game.play_internal_card("amulet", ACTOR).unwrap(),
        CardPlay::Entered(_)
    ));
    let fanfare = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &fanfare.domain else {
        panic!("printed fanfare must enter check timing");
    };
    let target = game
        .answer_internal_card(
            fanfare.actor,
            fanfare.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap();
    let CardPlay::Awaiting(target) = target else {
        panic!("fanfare must ask for a formal target");
    };
    let InputDomain::Targets(domain) = &target.domain else {
        panic!("fanfare targets enemy field cards");
    };
    assert_eq!(domain.members[0].card.as_deref(), Some("BP13-087"));
    resolved(
        game.answer_internal_card(
            target.actor,
            target.token,
            Answer::Targets(vec![domain.members[0].handle]),
        )
        .unwrap(),
    );
    game.resume_internal_card().unwrap();

    let ability = activation_id("CP05-P55");
    assert!(matches!(
        game.activate_internal_card("amulet", ability, ACTOR),
        Err(_) | Ok(CardPlay::Cancelled)
    ));
    pay_dark_discard(&mut game);
    resolved(
        game.activate_internal_card("amulet", ability, ACTOR)
            .unwrap(),
    );
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P2"]["cemetery"], json!(["enemy_amulet"]));
    assert_eq!(view["P1"]["cemetery"], json!(["dark", "amulet"]));
    assert_eq!(view["P1"]["hand"], json!(["drawn"]));
    assert_eq!(view["P1"]["deck"], json!(["tail"]));
    let tokens = view["P1"]["ex"].as_array().unwrap();
    let [token] = tokens.as_slice() else {
        panic!("expected one material in EX");
    };
    assert_eq!(view["objects"][token.as_str().unwrap()]["card"], "CP05-T14");
}
