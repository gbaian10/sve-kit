#![expect(
    clippy::panic,
    reason = "Unexpected evolution stages fail the scenario."
)]

use super::*;
use crate::player_pipeline::question;
use sve_engine::game::player::events::{ActionKind, EventKind};
use sve_engine::game::player::{
    Command, EvolutionAnswer, EvolutionQuestion, Input, MainActionAnswer, MainIntentKind,
    QuestionShape, SubmissionKind, Viewer,
};

fn start(game: &mut Game, actor: Viewer) {
    game.advance_player_input();
    let main = question(game, actor);
    let QuestionShape::ActionWindow(actions) = main.shape else {
        panic!("main evolution intents");
    };
    let intent = actions
        .intents
        .iter()
        .find(|intent| matches!(intent.kind, MainIntentKind::Evolve { .. }))
        .unwrap();
    assert_eq!(
        game.submit_player(
            actor,
            Command::MainAction {
                token: main.token,
                intent: intent.id.clone(),
                answer: MainActionAnswer::None
            }
        )
        .kind,
        SubmissionKind::Applied,
        "The selected evolution intent begins its declaration"
    );
}

fn form(game: &mut Game, actor: Viewer) {
    let first = question(game, actor);
    let QuestionShape::Evolution(EvolutionQuestion::Form(forms)) = first.shape else {
        panic!("evolution forms");
    };
    assert_eq!(
        forms.len(),
        1,
        "The fixture has one eligible evolution resource"
    );
    assert_eq!(
        game.submit_player(
            actor,
            Command::Evolution {
                token: first.token,
                answer: EvolutionAnswer::Form(forms[0].id)
            }
        )
        .kind,
        SubmissionKind::Applied,
        "The selected form advances to the existing payment stages"
    );
}

#[test]
fn static_source_evolution_quotes_and_payment_reread_the_public_hand_count() {
    let mut tree = fanfare::expanded::tree().clone();
    let mut base = tree["cards"]["synthetic-evolution"].clone();
    let mut modifier = tree["cards"]["BP08-038"]["abilities"][0].clone();
    modifier.as_object_mut().unwrap().remove("fanfare");
    modifier["line"] = json!(2);
    modifier["static"] = json!({"effects":[{"evolve_cost_mod":{
        "on":"this","delta":{"count":{"who":"self","zone":"hand"}}
    }}]});
    let mut draw = modifier.clone();
    draw.as_object_mut().unwrap().remove("static");
    draw["line"] = json!(3);
    draw["activate"] = json!({"cost":{"pp":0},"do":[{"draw":{"n":1}}]});
    base["abilities"]
        .as_array_mut()
        .unwrap()
        .extend([modifier, draw]);
    tree["cards"] = json!({"unit-follower":base});
    let program = lowerer()
        .lower(&tree)
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    let draw = program
        .card()
        .abilities
        .iter()
        .find(|a| {
            a.node.kind == AbilityKind::Activated
                && matches!(a.node.source.origin, AbilityOrigin::Printed { .. })
        })
        .unwrap()
        .node
        .id;
    let mut game = evolution::position(1, &program, &card(vec![]));
    let quote = |game: &Game| {
        let mut copy = game.clone();
        start(&mut copy, Viewer::P1);
        form(&mut copy, Viewer::P1);
        let QuestionShape::Evolution(EvolutionQuestion::Payment(payment)) =
            question(&copy, Viewer::P1).shape
        else {
            panic!("evolution price")
        };
        payment.total_pp
    };
    assert_eq!(quote(&game), 1);
    resolved(
        game.activate_internal_card("s", draw, PlayerId { seat: 1 })
            .unwrap(),
    );
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
    assert_eq!(quote(&game), 2, "the new card changes the next price");
    start(&mut game, Viewer::P1);
    form(&mut game, Viewer::P1);
    let payment = question(&game, Viewer::P1);
    let before = exact_authority(&game);
    assert_eq!(
        game.submit_player(
            Viewer::P1,
            Command::Evolution {
                token: payment.token,
                answer: EvolutionAnswer::Payment {
                    pp: 1,
                    ep: false,
                    super_evolution: false
                },
            }
        )
        .kind,
        SubmissionKind::Rejected
    );
    assert_eq!(exact_authority(&game), before);
    assert_eq!(
        game.submit_player(
            Viewer::P1,
            Command::Evolution {
                token: payment.token,
                answer: EvolutionAnswer::Payment {
                    pp: 2,
                    ep: false,
                    super_evolution: false
                },
            }
        )
        .kind,
        SubmissionKind::Applied
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["pp"]["current"], 0);
    assert_eq!(state["objects"]["s"]["evolved_with"], "e");
    assert_eq!(state["P1"]["hand"], json!(["d"]));
}

#[test]
fn player_evolution_normal_and_super_share_native_resources_and_events() {
    let (base, evolved) = evolution::programs();
    for (actor, super_evolution) in [(Viewer::P1, false), (Viewer::P2, true)] {
        let mut game = evolution::position(actor.player().seat, base, evolved);
        start(&mut game, actor);
        let first = question(&game, actor);
        let before = exact_authority(&game);
        for _ in 0..3 {
            assert_eq!(question(&game, actor), first);
        }
        assert_eq!(exact_authority(&game), before);
        let opponent = if actor == Viewer::P1 {
            Viewer::P2
        } else {
            Viewer::P1
        };
        assert!(matches!(game.input(opponent), Input::Waiting(waiting) if waiting.actor == actor));
        form(&mut game, actor);
        let payment = question(&game, actor);
        let QuestionShape::Evolution(EvolutionQuestion::Payment(domain)) = &payment.shape else {
            panic!("evolution payment");
        };
        assert_eq!(domain.total_pp, 1);
        assert!(domain.super_allowed);
        assert_eq!(
            game.submit_player(
                actor,
                Command::Evolution {
                    token: payment.token,
                    answer: EvolutionAnswer::Payment {
                        pp: 1,
                        ep: false,
                        super_evolution
                    }
                }
            )
            .kind,
            SubmissionKind::Applied
        );
        let state = game.projection(View::Referee).unwrap();
        let seat = if actor == Viewer::P1 { "P1" } else { "P2" };
        assert_eq!(state[seat]["pp"]["current"], 1);
        assert_eq!(state[seat]["sep"], i64::from(!super_evolution));
        assert_eq!(state["objects"]["s"]["evolved_with"], "e");
        assert_eq!(state["objects"]["s"]["super_evolved"], super_evolution);
        assert!(game.events(actor, None).unwrap().any(|event| matches!(&event.kind, EventKind::Action(action) if action.kind == ActionKind::Evolve)));
    }
}

#[test]
fn player_evolution_wrong_actor_shape_payment_and_stale_token_preserve_questions() {
    let (base, evolved) = evolution::programs();
    let mut game = evolution::position(1, base, evolved);
    start(&mut game, Viewer::P1);
    let first = question(&game, Viewer::P1);
    let QuestionShape::Evolution(EvolutionQuestion::Form(forms)) = &first.shape else {
        panic!("forms");
    };
    let answer = Command::Evolution {
        token: first.token,
        answer: EvolutionAnswer::Form(forms[0].id),
    };
    let before = exact_authority(&game);
    assert_eq!(
        game.submit_player(Viewer::P2, answer.clone()).kind,
        SubmissionKind::Rejected
    );
    assert_eq!(
        game.submit_player(
            Viewer::P1,
            Command::Evolution {
                token: first.token,
                answer: EvolutionAnswer::Materials(vec![])
            }
        )
        .kind,
        SubmissionKind::Rejected
    );
    assert_eq!(exact_authority(&game), before);
    assert_eq!(
        game.submit_player(Viewer::P1, answer.clone()).kind,
        SubmissionKind::Applied
    );
    let payment = question(&game, Viewer::P1);
    let before = exact_authority(&game);
    for invalid in [
        answer,
        Command::Evolution {
            token: payment.token,
            answer: EvolutionAnswer::Payment {
                pp: 1,
                ep: true,
                super_evolution: false,
            },
        },
        Command::Evolution {
            token: payment.token,
            answer: EvolutionAnswer::Payment {
                pp: -1,
                ep: false,
                super_evolution: false,
            },
        },
    ] {
        assert_eq!(
            game.submit_player(Viewer::P1, invalid).kind,
            SubmissionKind::Rejected
        );
        assert_eq!(question(&game, Viewer::P1), payment);
        assert_eq!(exact_authority(&game), before);
    }
}

#[test]
fn player_evolution_material_binding_precedes_the_existing_hand_discard_domain() {
    let mut tree = fanfare::expanded::tree().clone();
    let mut base = tree["cards"]["synthetic-evolution"].clone();
    base["abilities"][0]["evolve"]["cost"] = json!({"steps":[
        {"banish":{"sel":{"who":"self","zone":"cemetery"},"n":1}},
        {"discard":{"sel":{"who":"self","zone":"hand"},"n":1}}
    ]});
    tree["cards"] = json!({"unit-follower":base});
    let program = lowerer()
        .lower(&tree)
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    let zones = json!({"P1":{"hand":[{"id":"fee","card":"unit-spell"}],"cemetery":[{"id":"material","card":"unit-spell"}]}});
    let mut game = evolution::position_with_zones(1, &program, &card(vec![]), false, &zones);
    let mut native = game.clone();
    assert_eq!(native.internal_rule_action(&json!({"do":"evolve","source":"s","evolve_card":"e","pay":{"pp":0},"costs":{"materials":["material"],"discard":["fee"]}})).unwrap().outcome, "resolved");
    start(&mut game, Viewer::P1);
    form(&mut game, Viewer::P1);
    let material = question(&game, Viewer::P1);
    let QuestionShape::Evolution(EvolutionQuestion::Materials(domain)) = material.shape else {
        panic!("material before discard");
    };
    assert_eq!(domain.members.len(), 1);
    let answer = Command::Evolution {
        token: material.token,
        answer: EvolutionAnswer::Materials(vec![domain.members[0].handle]),
    };
    assert_eq!(
        game.submit_player(Viewer::P1, answer).kind,
        SubmissionKind::Applied
    );
    let discard = question(&game, Viewer::P1);
    let QuestionShape::Evolution(EvolutionQuestion::Discard(domain)) = discard.shape else {
        panic!("discard after binding material");
    };
    assert_eq!(domain.members.len(), 1);
    assert_eq!(domain.members[0].card.as_deref(), Some("unit-spell"));
    assert_eq!(
        game.submit_player(
            Viewer::P1,
            Command::Evolution {
                token: discard.token,
                answer: EvolutionAnswer::Discard(vec![domain.members[0].handle])
            }
        )
        .kind,
        SubmissionKind::Applied
    );
    let payment = question(&game, Viewer::P1);
    assert_eq!(
        game.submit_player(
            Viewer::P1,
            Command::Evolution {
                token: payment.token,
                answer: EvolutionAnswer::Payment {
                    pp: 0,
                    ep: false,
                    super_evolution: false
                }
            }
        )
        .kind,
        SubmissionKind::Applied
    );
    let state = game.projection(View::Referee).unwrap();
    let expected = native.projection(View::Referee).unwrap();
    for key in ["field", "hand", "cemetery", "banish", "pp"] {
        assert_eq!(state["P1"][key], expected["P1"][key]);
    }
    assert_eq!(state["objects"]["s"]["evolved_with"], "e");
}

#[test]
fn player_evolution_quote_uses_native_installed_cost_adjustments() {
    for set in [false, true] {
        let program = evolution_cost_mod::program(set);
        let definition = program
            .card()
            .abilities
            .iter()
            .find(|ability| matches!(ability.node.source.origin, AbilityOrigin::Printed { .. }))
            .unwrap()
            .node
            .id;
        let mut game = evolution::position(1, &program, &card(vec![]));
        resolved(
            game.activate_internal_card("s", definition, PlayerId { seat: 1 })
                .unwrap(),
        );
        while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
        start(&mut game, Viewer::P1);
        form(&mut game, Viewer::P1);
        let payment = question(&game, Viewer::P1);
        let QuestionShape::Evolution(EvolutionQuestion::Payment(domain)) = payment.shape else {
            panic!("adjusted evolution payment");
        };
        assert_eq!(domain.total_pp, 0);
        assert!(!domain.ep_allowed);
        assert_eq!(
            game.submit_player(
                Viewer::P1,
                Command::Evolution {
                    token: payment.token,
                    answer: EvolutionAnswer::Payment {
                        pp: 0,
                        ep: false,
                        super_evolution: false
                    }
                }
            )
            .kind,
            SubmissionKind::Applied
        );
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
            2
        );
    }
}

#[test]
fn player_evolution_opponent_private_cards_do_not_change_any_visible_stage() {
    let (base, evolved) = evolution::programs();
    let mut stages = Vec::new();
    for hidden in [
        json!([{"id":"private-a","card":"unit-spell"},{"id":"private-z","card":"unit-amulet"}]),
        json!([{"id":"hidden-z","card":"unit-amulet"},{"id":"hidden-a","card":"unit-spell"}]),
    ] {
        let zones =
            json!({"P2":{"hand":hidden,"deck":[{"id":"hidden-deck","card":hidden[0]["card"]}]}});
        let mut game = evolution::position_with_zones(1, base, evolved, false, &zones);
        game.advance_player_input();
        let main = question(&game, Viewer::P1);
        start(&mut game, Viewer::P1);
        let forms = question(&game, Viewer::P1);
        let before = exact_authority(&game);
        assert_eq!(question(&game, Viewer::P1), forms);
        assert_eq!(exact_authority(&game), before);
        form(&mut game, Viewer::P1);
        stages.push((
            main,
            forms,
            question(&game, Viewer::P1),
            game.input(Viewer::P2),
        ));
    }
    assert_eq!(stages[0], stages[1]);
}
