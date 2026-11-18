use super::*;
use crate::player_pipeline::question;
use alloc::collections::BTreeSet;
use sve_battle_contract::{
    Int as WireInt, Nat, RoleChoice,
    observation::{ProcessingStage, ZoneView},
};
use sve_engine::battle::BattleReferences;
use sve_engine::game::player::{EvolutionAnswer, EvolutionQuestion, SelectionQuestion};

#[path = "evolution_discard/privacy.rs"]
mod privacy;

fn program() -> WellFormedCardProgram {
    let mut tree = fanfare::expanded::tree().clone();
    let mut base = tree["cards"]["synthetic-evolution"].clone();
    base["abilities"][0]["evolve"]["cost"] = json!({"discard":{
        "sel":{"who":"self","zone":"hand","kind":"spell","orig_cost_eq":1},"n":1
    }});
    tree["cards"] = json!({"unit-follower":base});
    lowerer()
        .lower(&tree)
        .unwrap()
        .remove("unit-follower")
        .unwrap()
}

fn make(
    catalog: &Arc<Catalog>,
    base: &WellFormedCardProgram,
    actor: Viewer,
    hand: &[&str],
    secret: &str,
) -> Game {
    let (own, other) = if actor == Viewer::P1 {
        ("P1", "P2")
    } else {
        ("P2", "P1")
    };
    let mut zones = json!({});
    zones[own]["hand"] = json!(
        hand.iter()
            .enumerate()
            .map(|(index, number)| { json!({"id":format!("fee-{index}"),"card":number}) })
            .collect::<Vec<_>>()
    );
    zones[other]["hand"] = json!([{"id":"opponent-secret","card":secret}]);
    let mut game = evolution::position_with_catalog(
        Arc::clone(catalog),
        actor.player().seat,
        base,
        &card(vec![]),
        false,
        &zones,
    );
    player_evolution::start(&mut game, actor);
    player_evolution::form(&mut game, actor);
    game
}

fn projected(game: &Game, actor: Viewer, references: &BattleReferences) -> QuestionProjection {
    game.battle_question(
        actor,
        &QuestionId::new("host-evolution-discard".into()),
        references,
    )
    .unwrap()
    .unwrap()
}

const fn selection(projection: &QuestionProjection) -> Option<&wire::Selection> {
    if let wire::QuestionShape::Select { selection } = &projection.question().shape {
        Some(selection)
    } else {
        None
    }
}

const fn select(selected: Vec<OptionId>) -> WireAnswer {
    WireAnswer::Select {
        selected,
        groups: None,
        roles: vec![],
    }
}

fn discard_domain(game: &Game, actor: Viewer) -> Option<SelectionQuestion> {
    if let QuestionShape::Evolution(EvolutionQuestion::Discard(domain)) =
        question(game, actor).shape
    {
        Some(domain)
    } else {
        None
    }
}

fn assert_groups(view: &wire::Selection, domain: &SelectionQuestion) -> Option<()> {
    assert_eq!(
        view.groups.len(),
        domain.groups.len(),
        "no native group is dropped"
    );
    for (group, native) in view.groups.iter().zip(&domain.groups) {
        assert_eq!(
            group.bounds.min.get(),
            native.cardinality.minimum.0,
            "group minimum remains native"
        );
        assert_eq!(
            group.bounds.max.map(Nat::get),
            native.cardinality.maximum.map(|max| max.0),
            "group maximum remains native"
        );
        let expected = native
            .handles
            .iter()
            .map(|handle| {
                let index = domain
                    .members
                    .iter()
                    .position(|member| member.handle == *handle)?;
                Some(view.candidates.get(index)?.id.clone())
            })
            .collect::<Option<Vec<_>>>()?;
        assert_eq!(
            group.eligible, expected,
            "group eligibility uses the same coordinate mapping"
        );
    }
    Some(())
}

fn assert_domain(
    projection: &QuestionProjection,
    domain: &SelectionQuestion,
    actor: Viewer,
) -> Option<()> {
    let question = projection.question();
    assert_eq!(
        question.usage,
        wire::QuestionUse::EvolutionDiscard,
        "the selection is a normal evolution discard"
    );
    assert_eq!(
        question.purpose,
        wire::QuestionPurpose::PlayParameter,
        "discard selects an unpaid parameter"
    );
    assert_eq!(
        question.processing.stage,
        Some(ProcessingStage::Declare),
        "selection is not a payment receipt"
    );
    assert_eq!(
        question.processing.payments,
        vec![],
        "nothing has been paid"
    );
    assert!(
        question.is_coherent(),
        "the use retains its strict answer shape"
    );
    let view = selection(projection)?;
    assert_eq!(
        view.candidates.len(),
        domain.members.len(),
        "every native member is retained"
    );
    assert_groups(view, domain)?;
    assert_eq!(
        view.bounds,
        wire::Bounds {
            min: Nat::from(1_u32),
            max: Some(Nat::from(1_u32))
        },
        "the cost requires exactly one card"
    );
    let hand = &projection
        .observation()
        .players
        .iter()
        .find(|p| p.seat == seat(actor))?
        .zones
        .hand;
    for (candidate, member) in view.candidates.iter().zip(&domain.members) {
        assert_eq!(
            candidate.rules_names, member.rules_names,
            "captured name constraints are retained"
        );
        assert_eq!(
            candidate.original_cost.map(WireInt::get),
            member.original_cost,
            "captured cost facts are retained"
        );
        let object = member.object.as_ref()?;
        let located = hand
            .cards
            .iter()
            .find(|card| card.object.as_str() == object.as_str())?;
        assert_eq!(
            candidate.classes, None,
            "the native domain supplies no captured class constraint facts"
        );
        assert_eq!(
            candidate.subject,
            Some(Subject::Card {
                card: projection
                    .observation()
                    .objects
                    .get(&located.object)?
                    .effective_face
                    .clone(),
                object: Some(located.object.clone()),
            }),
            "each candidate refers to this board's effective face and physical object"
        );
        assert_eq!(
            candidate.controller,
            Some(seat(actor)),
            "controller is read from the same object table"
        );
        assert_eq!(
            candidate.origin,
            Some(wire::Origin {
                player: seat(actor),
                zone: Zone::Hand,
                position: located.position
            }),
            "origin is the authorized current hand position"
        );
    }
    Some(())
}

fn pay(game: &mut Game, actor: Viewer, references: &BattleReferences) -> Option<()> {
    let projection = game
        .battle_question(
            actor,
            &QuestionId::new("host-evolution-discard-payment".into()),
            references,
        )
        .ok()??;
    let wire::QuestionShape::EvolutionPayment { payments } = &projection.question().shape else {
        return None;
    };
    let tuple = payments.first()?;
    let command = projection
        .answer(
            actor,
            &projection.question().id,
            &WireAnswer::EvolutionPayment {
                pp: tuple.pp,
                use_ep: tuple.use_ep,
                super_evolution: tuple.super_evolution,
            },
        )
        .ok()?;
    assert_eq!(
        game.submit_player(actor, command).kind,
        SubmissionKind::Applied,
        "wire payment commits the selected cost"
    );
    Some(())
}

fn assert_paid(
    game: &Game,
    actor: Viewer,
    references: &BattleReferences,
    before: &ZoneView,
    selected: &str,
) -> Option<()> {
    let expected = before
        .cards
        .iter()
        .filter(|card| card.object.as_str() != selected)
        .map(|card| card.object.clone())
        .collect::<BTreeSet<_>>();
    for viewer in [Viewer::P1, Viewer::P2] {
        let board = game.battle_observation(viewer, references).ok()?;
        let own = board.players.iter().find(|p| p.seat == seat(actor))?;
        assert_eq!(
            own.zones.hand.count,
            Nat::from(2_u32),
            "payment discards one of three cards"
        );
        assert_eq!(
            own.zones.cemetery.cards.len(),
            1,
            "only the selected card reaches the cemetery"
        );
        let discarded = own.zones.cemetery.cards.first()?;
        assert_eq!(
            board.objects.get(&discarded.object)?.card.printing_id,
            "printing-spell",
            "the cost is now public for both viewers"
        );
        assert_eq!(
            own.zones.evolution.cards.len(),
            1,
            "the selected evolution form is attached"
        );
        if viewer == actor {
            assert_eq!(
                own.zones
                    .hand
                    .cards
                    .iter()
                    .map(|card| card.object.clone())
                    .collect::<BTreeSet<_>>(),
                expected,
                "only the selected twin leaves the hand"
            );
        }
    }
    Some(())
}

#[test]
fn wire_evolution_discard_round_trips_the_later_twin_and_pays_after_selection() {
    let (catalog, references) = battle_evolution::fixture(&["unit-follower"]).unwrap();
    let base = program();
    for actor in [Viewer::P1, Viewer::P2] {
        let mut game = make(
            &catalog,
            &base,
            actor,
            &["unit-spell", "unit-spell", "unit-amulet"],
            "unit-other",
        );
        let native = question(&game, actor);
        let domain = discard_domain(&game, actor).unwrap();
        assert_eq!(domain.members.len(), 2);
        assert!(
            game.battle_observation(actor, &references).is_ok(),
            "the real Discard board is independently available"
        );
        let before = exact_authority(&game);
        let projection = projected(&game, actor, &references);
        assert!(
            exact_authority(&game) == before,
            "projection cannot pay or select costs"
        );
        assert_domain(&projection, &domain, actor).unwrap();
        let hand = &projection
            .observation()
            .players
            .iter()
            .find(|p| p.seat == seat(actor))
            .unwrap()
            .zones
            .hand;
        let chosen = &selection(&projection).unwrap().candidates[1];
        let command = projection
            .answer(
                actor,
                &projection.question().id,
                &select(vec![chosen.id.clone()]),
            )
            .unwrap();
        assert_eq!(
            command,
            Command::Evolution {
                token: native.token,
                answer: EvolutionAnswer::Discard(vec![domain.members[1].handle])
            }
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
            "the retired discard token cannot select again"
        );
        let unpaid = game.battle_observation(actor, &references).unwrap();
        assert_eq!(
            &unpaid
                .players
                .iter()
                .find(|p| p.seat == seat(actor))
                .unwrap()
                .zones
                .hand,
            hand
        );
        let waiting = game
            .battle_observation(opposite(actor), &references)
            .unwrap();
        assert!(
            waiting
                .remembered
                .iter()
                .all(|memory| memory.card.printing_id != "printing-spell")
        );
        pay(&mut game, actor, &references).unwrap();
        assert_paid(
            &game,
            actor,
            &references,
            hand,
            domain.members[1].object.as_ref().unwrap().as_str(),
        )
        .unwrap();
    }
}

#[test]
fn wire_evolution_discard_rejects_bad_selections_and_bindings() {
    let (catalog, references) = battle_evolution::fixture(&["unit-follower"]).unwrap();
    let base = program();
    let actor = Viewer::P1;
    let mut game = make(
        &catalog,
        &base,
        actor,
        &["unit-spell", "unit-spell", "unit-amulet"],
        "unit-other",
    );
    let projection = projected(&game, actor, &references);
    let candidates = &selection(&projection).unwrap().candidates;
    let first = candidates[0].id.clone();
    let before = exact_authority(&game);
    for answer in [
        select(vec![]),
        select(vec![first.clone(), candidates[1].id.clone()]),
        select(vec![first.clone(), first.clone()]),
        select(vec![OptionId::new("foreign".into())]),
        WireAnswer::Select {
            selected: vec![first.clone()],
            groups: Some(vec![]),
            roles: vec![],
        },
        WireAnswer::Select {
            selected: vec![first.clone()],
            groups: None,
            roles: vec![RoleChoice {
                role: Nat::from(0_u32),
                selected: vec![first.clone()],
            }],
        },
        WireAnswer::Choose {
            chosen: vec![first.clone()],
        },
    ] {
        assert!(matches!(
            projection.answer(actor, &projection.question().id, &answer),
            Err(AnswerError)
        ));
    }
    let valid = select(vec![first]);
    assert!(matches!(
        projection.answer(opposite(actor), &projection.question().id, &valid),
        Err(AnswerError)
    ));
    assert!(matches!(
        projection.answer(actor, &QuestionId::new("wrong".into()), &valid),
        Err(AnswerError)
    ));
    let other = game
        .battle_question(actor, &QuestionId::new("other".into()), &references)
        .unwrap()
        .unwrap();
    assert!(matches!(
        other.answer(actor, &other.question().id, &valid),
        Err(AnswerError)
    ));
    let command = projection
        .answer(actor, &projection.question().id, &valid)
        .unwrap();
    assert_eq!(
        game.submit_player(opposite(actor), command).kind,
        SubmissionKind::Rejected
    );
    assert!(
        exact_authority(&game) == before,
        "all invalid bindings preserve the unpaid declaration"
    );
}

#[test]
fn wire_evolution_discard_waiting_board_ignores_hidden_eligible_candidates() {
    let (catalog, references) = battle_evolution::fixture(&["unit-follower"]).unwrap();
    let base = program();
    for actor in [Viewer::P1, Viewer::P2] {
        let mut boards = Vec::new();
        let mut inputs = Vec::new();
        let mut questions = Vec::new();
        let mut handles = Vec::new();
        for (hand, count) in [
            (["unit-spell", "unit-spell", "unit-amulet"], 2_usize),
            (["unit-spell", "unit-amulet", "unit-amulet"], 1),
            (["unit-amulet", "unit-spell", "unit-amulet"], 1),
        ] {
            let game = make(&catalog, &base, actor, &hand, "unit-other");
            let before = exact_authority(&game);
            let domain = discard_domain(&game, actor).unwrap();
            assert_eq!(domain.members.len(), count);
            handles.push(
                domain
                    .members
                    .iter()
                    .map(|member| member.object.clone())
                    .collect::<BTreeSet<_>>(),
            );
            let projection = projected(&game, actor, &references);
            assert_eq!(selection(&projection).unwrap().candidates.len(), count);
            questions.push(projection.question().clone());
            let waiter = opposite(actor);
            let input = game.input(waiter);
            assert!(matches!(&input, Input::Waiting(summary) if summary.actor == actor));
            inputs.push(input);
            assert!(
                game.battle_question(waiter, &QuestionId::new("waiter".into()), &references)
                    .unwrap()
                    .is_none()
            );
            let board = game.battle_observation(waiter, &references).unwrap();
            let hidden = &board
                .players
                .iter()
                .find(|p| p.seat == seat(actor))
                .unwrap()
                .zones
                .hand;
            assert_eq!(hidden.count, Nat::from(3_u32));
            assert!(
                hidden.cards.is_empty(),
                "the waiter sees no hidden hand slots"
            );
            for printing in ["printing-spell", "printing-amulet"] {
                assert!(
                    board
                        .objects
                        .values()
                        .all(|object| object.card.printing_id != printing
                            && object.effective_face.printing_id != printing)
                );
                assert!(
                    board
                        .remembered
                        .iter()
                        .all(|memory| memory.card.printing_id != printing)
                );
            }
            boards.push(board);
            assert!(
                exact_authority(&game) == before,
                "projection reads preserve the hidden selection authority"
            );
        }
        assert_ne!(handles[0], handles[1]);
        assert_ne!(handles[1], handles[2]);
        assert_ne!(questions[0], questions[1]);
        assert_ne!(questions[1], questions[2]);
        assert!(inputs.windows(2).all(|pair| pair.first() == pair.last()));
        assert!(
            boards.windows(2).all(|pair| pair.first() == pair.last()),
            "hidden discard eligibility cannot change any waiting-seat board component"
        );
    }
}

#[test]
fn wire_evolution_discard_actor_board_and_question_ignore_the_other_hand() {
    let (catalog, references) = battle_evolution::fixture(&["unit-follower"]).unwrap();
    let base = program();
    for actor in [Viewer::P1, Viewer::P2] {
        let mut projections = Vec::new();
        for secret in ["unit-other", "unit-amulet"] {
            let game = make(
                &catalog,
                &base,
                actor,
                &["unit-spell", "unit-spell", "unit-amulet"],
                secret,
            );
            let projection = projected(&game, actor, &references);
            let serialized =
                serde_json::to_string(&(projection.observation(), projection.question())).unwrap();
            for private in ["fee-", "opponent-secret", "token", "serial", "epoch"] {
                assert!(!serialized.contains(private));
            }
            assert!(!format!("{projection:?}").contains("vobj:"));
            projections.push(projection);
        }
        assert_eq!(projections[0].observation(), projections[1].observation());
        assert_eq!(projections[0].question(), projections[1].question());
    }
}
