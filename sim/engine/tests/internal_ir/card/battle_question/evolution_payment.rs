use super::*;
use crate::board_fixtures;
use sve_battle_contract::{Int as WireInt, observation::ProcessingStage};
use sve_engine::battle::BattleReferences;
use sve_engine::game::player::{EvolutionAnswer, EvolutionQuestion, QuestionPurpose};

type Resources = (i64, i64, i64);

fn make_payment(catalog: &Arc<Catalog>, actor: Viewer, resources: Resources, secret: &str) -> Game {
    let mut game = at_form(
        catalog,
        actor,
        resources,
        secret,
        json!([{"id":"physical-form","card":"unit-form"}]),
    );
    player_evolution::form(&mut game, actor);
    game
}

pub(super) fn at_form(
    catalog: &Arc<Catalog>,
    actor: Viewer,
    resources: Resources,
    secret: &str,
    evolve_deck: Value,
) -> Game {
    let (own, other) = if actor == Viewer::P1 {
        ("P1", "P2")
    } else {
        ("P2", "P1")
    };
    let mut initial = board_fixtures::setup(actor.player().seat, json!([]));
    initial["turn"]["active"] = json!(own);
    initial["turn"]["elapsed_turns"][own] = json!(7_i32);
    initial["players"][own]["pp"] = json!({"current":resources.0,"max":2_i32});
    initial["players"][own]["ep"] = json!(resources.1);
    initial["players"][own]["sep"] = json!(resources.2);
    initial["players"][own]["zones"]["hand"] = json!([]);
    initial["players"][own]["zones"]["field"] =
        json!([{"id":"physical-base","card":"unit-follower"}]);
    initial["players"][own]["zones"]["evolve_deck"] = evolve_deck;
    initial["players"][other]["zones"]["field"] = json!([]);
    initial["players"][other]["zones"]["hand"] = json!([{"id":"physical-secret","card":secret}]);
    let mut game = Game::new(
        Arc::clone(catalog),
        &initial,
        &Value::Null,
        &Value::Null,
        "wire-evolution-payment",
    )
    .unwrap();
    let (base, _) = evolution::programs();
    game.load_internal_card("unit-follower", base).unwrap();
    for number in ["unit-form", "unit-spell", "unit-amulet", "unit-other"] {
        game.load_internal_card(number, &card(vec![])).unwrap();
    }
    player_evolution::start(&mut game, actor);
    game
}

fn project_payment(
    game: &Game,
    actor: Viewer,
    references: &BattleReferences,
) -> QuestionProjection {
    game.battle_question(
        actor,
        &QuestionId::new("host-evolution-payment".into()),
        references,
    )
    .unwrap()
    .unwrap()
}

fn payments(projection: &QuestionProjection) -> Option<&[wire::EvolutionPayment]> {
    if let wire::QuestionShape::EvolutionPayment { payments } = &projection.question().shape {
        Some(payments)
    } else {
        None
    }
}

const fn answer(payment: wire::EvolutionPayment) -> WireAnswer {
    WireAnswer::EvolutionPayment {
        pp: payment.pp,
        use_ep: payment.use_ep,
        super_evolution: payment.super_evolution,
    }
}

fn expected(resources: Resources) -> Vec<wire::EvolutionPayment> {
    let tuples = match resources {
        (2, 0, 0) => vec![(1_i32, false, false)],
        (2, 0, 1) => vec![(1, false, false), (1, false, true)],
        (2, 1, 1) => vec![
            (1, false, false),
            (1, false, true),
            (0, true, false),
            (0, true, true),
        ],
        (0, 1, 1) => vec![(0, true, false), (0, true, true)],
        _ => vec![],
    };
    tuples
        .into_iter()
        .map(|(pp, use_ep, super_evolution)| wire::EvolutionPayment {
            pp: WireInt::from(pp),
            use_ep,
            super_evolution,
        })
        .collect()
}

fn assert_paid(game: &Game, actor: Viewer, resources: Resources, payment: wire::EvolutionPayment) {
    let board = game.observe(actor).unwrap();
    let paid = board
        .players
        .iter()
        .find(|player| player.player == actor)
        .unwrap()
        .resources;
    assert_eq!(
        paid.pp.checked_add(payment.pp.get()),
        Some(resources.0),
        "the exact tuple pays its PP component"
    );
    assert_eq!(
        paid.ep.checked_add(i64::from(payment.use_ep)),
        Some(resources.1),
        "EP replaces one PP only in the selected tuple"
    );
    assert_eq!(
        paid.sep.checked_add(i64::from(payment.super_evolution)),
        Some(resources.2),
        "SEP is spent only when selecting super evolution"
    );
}

#[test]
fn wire_evolution_payment_round_trips_every_legal_tuple_for_both_seats() {
    let (catalog, references) = battle_evolution::fixture(&["unit-follower"]).unwrap();
    for actor in [Viewer::P1, Viewer::P2] {
        for resources in [(2, 0, 0), (2, 0, 1), (2, 1, 1), (0, 1, 1)] {
            let game = make_payment(&catalog, actor, resources, "unit-spell");
            let Input::Question(native) = game.input(actor) else {
                panic!("native evolution payment")
            };
            let QuestionShape::Evolution(EvolutionQuestion::Payment(quote)) = native.shape else {
                panic!("payment stage")
            };
            assert_eq!(quote.total_pp, 1);
            assert_eq!(quote.available_pp, resources.0);
            assert_eq!(native.purpose, QuestionPurpose::PlayParameter);
            assert!(
                game.battle_observation(actor, &references).is_ok(),
                "the native payment board is independently projectable"
            );
            let before = exact_authority(&game);
            let projection = project_payment(&game, actor, &references);
            assert!(
                exact_authority(&game) == before,
                "projection cannot pay or advance evolution"
            );
            assert_eq!(payments(&projection).unwrap(), expected(resources));
            assert_eq!(
                projection.question().usage,
                wire::QuestionUse::EvolutionPayment
            );
            assert_eq!(
                projection.question().purpose,
                wire::QuestionPurpose::PlayParameter
            );
            assert_eq!(
                projection.question().processing.stage,
                Some(ProcessingStage::Declare)
            );
            assert_eq!(projection.question().processing.payments, vec![]);
            assert_eq!(
                projection.observation(),
                &game.battle_observation(actor, &references).unwrap()
            );
            assert!(projection.question().is_coherent());
            for payment in payments(&projection).unwrap() {
                let mut branch = game.clone();
                let command = projection
                    .answer(actor, &projection.question().id, &answer(*payment))
                    .unwrap();
                let Command::Evolution {
                    token,
                    answer:
                        EvolutionAnswer::Payment {
                            pp,
                            ep,
                            super_evolution,
                        },
                } = &command
                else {
                    panic!("native evolution payment answer")
                };
                assert_eq!(*token, native.token);
                assert_eq!(
                    (*pp, *ep, *super_evolution),
                    (payment.pp.get(), payment.use_ep, payment.super_evolution)
                );
                assert_eq!(
                    branch.submit_player(actor, command).kind,
                    SubmissionKind::Applied
                );
                assert_paid(&branch, actor, resources, *payment);
            }
        }
    }
}

#[test]
fn wire_evolution_payment_rejects_bad_tuples_bindings_and_stale_tokens() {
    let (catalog, references) = battle_evolution::fixture(&["unit-follower"]).unwrap();
    let actor = Viewer::P1;
    let mut game = make_payment(&catalog, actor, (2, 0, 0), "unit-spell");
    let projection = project_payment(&game, actor, &references);
    let before = exact_authority(&game);
    for (pp, use_ep, super_evolution) in [
        (WireInt::ZERO, false, false),
        (WireInt::from(2_i32), false, false),
        (WireInt::from(-1_i32), false, false),
        (WireInt::ZERO, true, false),
        (WireInt::from(1_i32), false, true),
        (WireInt::MAX, false, false),
    ] {
        assert!(matches!(
            projection.answer(
                actor,
                &projection.question().id,
                &answer(wire::EvolutionPayment {
                    pp,
                    use_ep,
                    super_evolution
                })
            ),
            Err(AnswerError)
        ));
    }
    assert!(matches!(
        projection.answer(
            actor,
            &projection.question().id,
            &WireAnswer::Choose { chosen: vec![] }
        ),
        Err(AnswerError)
    ));
    let valid = answer(payments(&projection).unwrap()[0]);
    assert!(matches!(
        projection.answer(Viewer::P2, &projection.question().id, &valid),
        Err(AnswerError)
    ));
    assert!(matches!(
        projection.answer(actor, &QuestionId::new("retired".into()), &valid),
        Err(AnswerError)
    ));
    let command = projection
        .answer(actor, &projection.question().id, &valid)
        .unwrap();
    assert_eq!(
        game.submit_player(Viewer::P2, command.clone()).kind,
        SubmissionKind::Rejected
    );
    assert!(
        exact_authority(&game) == before,
        "invalid wire answers and wrong native actor preserve the authority"
    );
    assert_eq!(
        game.submit_player(actor, command.clone()).kind,
        SubmissionKind::Applied
    );
    let after = exact_authority(&game);
    assert_eq!(
        game.submit_player(actor, command).kind,
        SubmissionKind::Rejected
    );
    assert!(
        exact_authority(&game) == after,
        "a retired evolution token cannot pay twice"
    );
}

#[test]
fn wire_evolution_payment_question_and_board_ignore_the_other_hand() {
    let (catalog, references) = battle_evolution::fixture(&["unit-follower"]).unwrap();
    for actor in [Viewer::P1, Viewer::P2] {
        let mut views = Vec::new();
        for secret in ["unit-spell", "unit-amulet"] {
            let game = make_payment(&catalog, actor, (2, 1, 1), secret);
            let projection = project_payment(&game, actor, &references);
            assert!(
                game.battle_question(
                    opposite(actor),
                    &QuestionId::new("waiting".into()),
                    &references
                )
                .unwrap()
                .is_none()
            );
            let serialized =
                serde_json::to_string(&(projection.observation(), projection.question())).unwrap();
            for private in ["physical-", "token", "serial", "epoch"] {
                assert!(!serialized.contains(private));
            }
            assert!(!format!("{projection:?}").contains("vobj:"));
            views.push(projection);
        }
        assert_eq!(views[0].observation(), views[1].observation());
        assert_eq!(views[0].question(), views[1].question());
    }
}
