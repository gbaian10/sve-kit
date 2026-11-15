use super::*;
use crate::player_pipeline;
use sve_battle_contract::{
    CardRef, Nat,
    observation::{BattleObservation, ProcessingStage},
};
use sve_engine::battle::BattleReferences;
use sve_engine::game::player::{EvolutionAnswer, EvolutionForm, EvolutionQuestion};

fn make_form(catalog: &Arc<Catalog>, actor: Viewer, secret: &str) -> Game {
    evolution_payment::at_form(
        catalog,
        actor,
        (2, 0, 1),
        secret,
        json!([
            {"id":"physical-form-0","card":"unit-form"}, {"id":"physical-form-1","card":"unit-form"}
        ]),
    )
}

fn projected(
    game: &Game,
    actor: Viewer,
    references: &BattleReferences,
    id: &str,
) -> QuestionProjection {
    game.battle_question(actor, &QuestionId::new(id.into()), references)
        .unwrap()
        .unwrap()
}

fn options(projection: &QuestionProjection) -> Option<&[wire::ChoiceOption]> {
    if let wire::QuestionShape::Choose { options, .. } = &projection.question().shape {
        Some(options)
    } else {
        None
    }
}

fn expected_face(native_face: u64) -> CardRef {
    CardRef {
        printing_id: "printing-form".into(),
        face: Nat::from(u32::from(native_face == 0)),
    }
}

fn assert_unpaid_secret(
    game: &Game,
    actor: Viewer,
    references: &BattleReferences,
) -> BattleObservation {
    let view = game
        .battle_observation(opposite(actor), references)
        .unwrap();
    assert!(
        view.objects
            .values()
            .all(|object| object.card.printing_id != "printing-form"),
        "choosing a private form cannot publish its object before payment"
    );
    assert!(
        view.remembered
            .iter()
            .all(|memory| memory.card.printing_id != "printing-form"),
        "choosing a private form cannot publish its face memory before payment"
    );
    view
}

fn finish_payment(game: &mut Game, actor: Viewer, references: &BattleReferences) -> Option<()> {
    let payment = projected(game, actor, references, "host-form-payment");
    let wire::QuestionShape::EvolutionPayment { payments } = &payment.question().shape else {
        return None;
    };
    let choice = payments.first()?;
    let answer = WireAnswer::EvolutionPayment {
        pp: choice.pp,
        use_ep: choice.use_ep,
        super_evolution: choice.super_evolution,
    };
    let command = payment
        .answer(actor, &payment.question().id, &answer)
        .ok()?;
    assert_eq!(
        game.submit_player(actor, command).kind,
        SubmissionKind::Applied,
        "the selected form commits after the existing wire payment"
    );
    Some(())
}

fn assert_committed(
    game: &Game,
    actor: Viewer,
    references: &BattleReferences,
    destination: &CardRef,
) {
    for viewer in [Viewer::P1, Viewer::P2] {
        let board = game.battle_observation(viewer, references).unwrap();
        let evolution = &board
            .players
            .iter()
            .find(|player| player.seat == seat(actor))
            .unwrap()
            .zones
            .evolution;
        assert_eq!(
            evolution.cards.len(),
            1,
            "exactly the selected physical form is attached"
        );
        let member = evolution.cards.first().unwrap();
        assert_eq!(
            &board.objects[&member.object].card, destination,
            "both seats see the chosen permanent destination face"
        );
    }
}

fn assert_choice(
    projection: &QuestionProjection,
    form: &EvolutionForm,
    choice: &wire::ChoiceOption,
    actor: Viewer,
) -> Option<CardRef> {
    let wire::ChoiceValue::EvolutionForm { object, face } = &choice.value else {
        return None;
    };
    assert_eq!(
        object.as_str(),
        form.card.handle.as_str(),
        "the option identifies this physical form capability"
    );
    assert_eq!(
        *face,
        expected_face(form.face),
        "destination faces use the supplied permanent face map"
    );
    let own = projection
        .observation()
        .players
        .iter()
        .find(|player| player.seat == seat(actor))?;
    assert!(
        own.zones
            .evolve_deck
            .cards
            .iter()
            .any(|member| &member.object == object),
        "the capability belongs to this board's evolution deck"
    );
    assert_eq!(
        projection.observation().objects.get(object)?.card,
        expected_face(0),
        "the destination does not replace the currently observed face"
    );
    Some(face.clone())
}

#[test]
fn wire_evolution_form_round_trips_physical_twins_and_reindexed_destinations() {
    let (catalog, references) =
        battle_evolution::fixture_with_face_map(&["unit-follower", "unit-follower"], &[1, 0])
            .unwrap();
    for actor in [Viewer::P1, Viewer::P2] {
        let game = make_form(&catalog, actor, "unit-spell");
        let native = player_pipeline::question(&game, actor);
        let QuestionShape::Evolution(EvolutionQuestion::Form(forms)) = native.shape else {
            panic!("native form question")
        };
        assert_eq!(forms.len(), 4);
        assert!(
            game.battle_observation(actor, &references).is_ok(),
            "the native declaration board is independently projectable"
        );
        let before = exact_authority(&game);
        let projection = projected(&game, actor, &references, "host-form");
        assert!(
            exact_authority(&game) == before,
            "form projection cannot advance evolution"
        );
        assert_eq!(
            projection.observation(),
            &game.battle_observation(actor, &references).unwrap()
        );
        assert_eq!(
            projection.question().usage,
            wire::QuestionUse::EvolutionForm
        );
        assert_eq!(
            projection.question().purpose,
            wire::QuestionPurpose::PlayParameter
        );
        assert_eq!(
            projection.question().processing.stage,
            ProcessingStage::Declare
        );
        assert_eq!(projection.question().processing.payment, None);
        assert!(projection.question().is_coherent());
        let choices = options(&projection).unwrap();
        assert_eq!(choices.len(), forms.len());
        let wire::QuestionShape::Choose { bounds, .. } = &projection.question().shape else {
            panic!("form choice")
        };
        assert_eq!(
            *bounds,
            wire::Bounds {
                min: Nat::from(1_u32),
                max: Some(Nat::from(1_u32))
            }
        );
        let mut other_views = Vec::new();
        for (choice, form) in choices.iter().zip(&forms) {
            let destination = assert_choice(&projection, form, choice, actor).unwrap();
            let mut branch = game.clone();
            let command = projection
                .answer(
                    actor,
                    &projection.question().id,
                    &WireAnswer::Choose {
                        chosen: vec![choice.id.clone()],
                    },
                )
                .unwrap();
            let Command::Evolution {
                token,
                answer: EvolutionAnswer::Form(selected),
            } = &command
            else {
                panic!("native form capability")
            };
            assert_eq!(*token, native.token);
            assert_eq!(*selected, form.id);
            assert_eq!(
                branch.submit_player(actor, command.clone()).kind,
                SubmissionKind::Applied
            );
            let after = exact_authority(&branch);
            assert_eq!(
                branch.submit_player(actor, command).kind,
                SubmissionKind::Rejected
            );
            assert!(
                exact_authority(&branch) == after,
                "the retired form question cannot choose again"
            );
            other_views.push(assert_unpaid_secret(&branch, actor, &references));
            finish_payment(&mut branch, actor, &references).unwrap();
            assert_committed(&branch, actor, &references, &destination);
        }
        assert!(
            other_views
                .windows(2)
                .all(|pair| pair.first() == pair.last())
        );
    }
}

#[test]
fn wire_evolution_form_rejects_invalid_choices_and_wrong_bindings() {
    let (catalog, references) =
        battle_evolution::fixture_with_face_map(&["unit-follower", "unit-follower"], &[1, 0])
            .unwrap();
    let actor = Viewer::P1;
    let mut game = make_form(&catalog, actor, "unit-spell");
    let projection = projected(&game, actor, &references, "host-form");
    let choices = options(&projection).unwrap();
    assert!(
        choices.len() > 1,
        "invalid-set cases require distinct form choices"
    );
    let first = choices[0].id.clone();
    let before = exact_authority(&game);
    for answer in [
        WireAnswer::Choose { chosen: vec![] },
        WireAnswer::Choose {
            chosen: vec![first.clone(), choices[1].id.clone()],
        },
        WireAnswer::Choose {
            chosen: vec![first.clone(), first.clone()],
        },
        WireAnswer::Choose {
            chosen: vec![OptionId::new("foreign".into())],
        },
        WireAnswer::Select {
            selected: vec![first.clone()],
            groups: None,
            roles: vec![],
        },
    ] {
        assert!(matches!(
            projection.answer(actor, &projection.question().id, &answer),
            Err(AnswerError)
        ));
    }
    let valid = WireAnswer::Choose {
        chosen: vec![first],
    };
    assert!(matches!(
        projection.answer(Viewer::P2, &projection.question().id, &valid),
        Err(AnswerError)
    ));
    assert!(matches!(
        projection.answer(actor, &QuestionId::new("another".into()), &valid),
        Err(AnswerError)
    ));
    let another = projected(&game, actor, &references, "another");
    assert!(matches!(
        another.answer(actor, &another.question().id, &valid),
        Err(AnswerError)
    ));
    let command = projection
        .answer(actor, &projection.question().id, &valid)
        .unwrap();
    assert_eq!(
        game.submit_player(Viewer::P2, command).kind,
        SubmissionKind::Rejected
    );
    assert!(
        exact_authority(&game) == before,
        "invalid form bindings preserve the current authority"
    );
}

#[test]
fn wire_evolution_form_question_and_board_ignore_the_other_hand() {
    let (catalog, references) =
        battle_evolution::fixture_with_face_map(&["unit-follower", "unit-follower"], &[1, 0])
            .unwrap();
    for actor in [Viewer::P1, Viewer::P2] {
        let mut views = Vec::new();
        for secret in ["unit-spell", "unit-amulet"] {
            let game = make_form(&catalog, actor, secret);
            let projection = projected(&game, actor, &references, "host-form");
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
            for private in ["physical-", "token", "serial", "epoch", "ordinal"] {
                assert!(!serialized.contains(private));
            }
            assert!(!format!("{projection:?}").contains("vobj:"));
            views.push(projection);
        }
        assert_eq!(views[0].observation(), views[1].observation());
        assert_eq!(views[0].question(), views[1].question());
    }
}
