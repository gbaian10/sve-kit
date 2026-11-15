use super::player_evolution::{form, start};
use super::*;
use crate::player_pipeline::question;
use core::error::Error;
use sve_battle_contract::{CardRef, Nat as WireNat, Seat, Subject, observation as wire};
use sve_engine::battle::{BattleReferences, PrintingFaceIdentity, ReferenceInput};
use sve_engine::catalog::EngineIdentityInput;
use sve_engine::game::player::observation::ObservedZone;
use sve_engine::game::player::{
    Command, EvolutionAnswer, EvolutionQuestion, MainActionAnswer, MainIntentKind, QuestionShape,
    SubmissionKind, Viewer,
};

pub(super) fn fixture(names: &[&str]) -> Result<(Arc<Catalog>, BattleReferences), Box<dyn Error>> {
    let (input, registry, document) = evolution::documents_for_forms(names);
    let mut rows = Vec::new();
    for line in input.lines() {
        let mut row: Value = serde_json::from_str(line)?;
        if row["number"] == "unit-form" {
            for face in row["faces"].as_array_mut().ok_or("fixture faces")? {
                face["title"] = json!("synthetic-evolution-title");
            }
        }
        rows.push(row.to_string());
    }
    let snapshot = rows.join("\n");
    let rules = rule_fixtures::rules(
        &snapshot,
        json!([rule_fixtures::title("synthetic", "unit-form", json!([]))]),
        json!([]),
    );
    let catalog = Arc::new(Catalog::from_documents_with_rules(
        &snapshot,
        &registry,
        &[("evolution.yaml".into(), document)],
        &rules.to_string(),
        &EngineIdentityInput::LegacyJp,
    )?);
    let mut faces = Vec::new();
    for (number, printing, count) in [
        ("unit-follower", "printing-base", 1_u64),
        ("unit-form", "printing-form", u64::try_from(names.len())?),
        ("unit-spell", "printing-spell", 1),
        ("unit-amulet", "printing-amulet", 1),
        ("unit-other", "printing-other", 1),
    ] {
        for native_face in 0..count {
            faces.push(PrintingFaceIdentity {
                card_no: number.into(),
                native_face,
                card: CardRef {
                    printing_id: printing.into(),
                    face: WireNat::try_from(native_face)?,
                },
            });
        }
    }
    let references = BattleReferences::new(
        &catalog,
        ReferenceInput {
            region: "jp".into(),
            snapshot_sha256: rule_fixtures::hash(snapshot.as_bytes()),
            faces,
        },
    )?;
    Ok((catalog, references))
}

fn position(
    catalog: Arc<Catalog>,
    actor: Viewer,
    secret: &str,
    base: &WellFormedCardProgram,
) -> Game {
    let own = if actor == Viewer::P1 { "P1" } else { "P2" };
    let mut zones = json!({});
    zones[own] = json!({
        "hand":[{"id":"fee","card":secret}],
        "cemetery":[{"id":"material","card":"unit-spell"}]
    });
    evolution::position_with_catalog(
        catalog,
        actor.player().seat,
        base,
        &card(vec![]),
        false,
        &zones,
    )
}

fn declaring(
    game: &Game,
    actor: Viewer,
    viewer: Viewer,
    references: &BattleReferences,
) -> Option<wire::BattleObservation> {
    let before = exact_authority(game);
    let board = game.battle_observation(viewer, references).ok()?;
    let actor_seat = if actor == Viewer::P1 {
        Seat::P1
    } else {
        Seat::P2
    };
    let own = board
        .players
        .iter()
        .find(|player| player.seat == actor_seat)?;
    let subject = own.zones.field.cards.first()?.object.clone();
    assert_eq!(
        board.objects.get(&subject)?.effective_face,
        CardRef {
            printing_id: "printing-base".into(),
            face: WireNat::ZERO,
        },
        "the declaration identifies the public base, never its private evolution form"
    );
    assert_eq!(
        board.processing,
        vec![wire::ProcessingContext {
            stage: wire::ProcessingStage::Declare,
            controller: Some(if actor == Viewer::P1 {
                Seat::P1
            } else {
                Seat::P2
            }),
            source: None,
            subject: Some(Subject::Card {
                card: CardRef {
                    printing_id: "printing-base".into(),
                    face: WireNat::ZERO,
                },
                object: Some(subject),
            }),
            rule: None,
            procedure: None,
            declared: vec![],
            payment: None,
        }],
        "unpaid evolution parameters keep one public declaration without private details"
    );
    assert!(
        exact_authority(game) == before,
        "reading an evolution declaration cannot advance it"
    );
    assert_eq!(own.pp.get(), 2, "declaration has not paid PP");
    assert_eq!(
        own.zones.hand.count.get(),
        1,
        "choosing a discard has not discarded it"
    );
    assert_eq!(
        own.zones.cemetery.count.get(),
        1,
        "choosing a material has not moved it"
    );
    if viewer != actor {
        assert!(
            board
                .objects
                .values()
                .all(|card| card.card.printing_id != "printing-form"),
            "choosing a private evolution face does not publish its object"
        );
        assert!(
            board
                .remembered
                .iter()
                .all(|memory| memory.card.printing_id != "printing-form"),
            "an unpaid evolution does not publish face memory"
        );
    }
    Some(board)
}

fn all_declaring(game: &Game, actor: Viewer, references: &BattleReferences) -> Option<()> {
    let first = declaring(game, actor, Viewer::P1, references)?;
    let second = declaring(game, actor, Viewer::P2, references)?;
    assert_ne!(
        first.processing.first()?.subject,
        second.processing.first()?.subject,
        "the same public base uses each viewer's own handle"
    );
    Some(())
}

fn choose_form(game: &mut Game, actor: Viewer, ordinal: usize) -> Option<usize> {
    let question = question(game, actor);
    let QuestionShape::Evolution(EvolutionQuestion::Form(forms)) = question.shape else {
        return None;
    };
    let form = forms.get(ordinal)?;
    let submission = game.submit_player(
        actor,
        Command::Evolution {
            token: question.token,
            answer: EvolutionAnswer::Form(form.id),
        },
    );
    (submission.kind == SubmissionKind::Applied).then_some(forms.len())
}

fn choose_material(game: &mut Game, actor: Viewer) -> Option<()> {
    let question = question(game, actor);
    let QuestionShape::Evolution(EvolutionQuestion::Materials(domain)) = question.shape else {
        return None;
    };
    let submission = game.submit_player(
        actor,
        Command::Evolution {
            token: question.token,
            answer: EvolutionAnswer::Materials(vec![domain.members.first()?.handle]),
        },
    );
    (submission.kind == SubmissionKind::Applied).then_some(())
}

fn choose_discard(game: &mut Game, actor: Viewer) -> Option<()> {
    let question = question(game, actor);
    let QuestionShape::Evolution(EvolutionQuestion::Discard(domain)) = question.shape else {
        return None;
    };
    let submission = game.submit_player(
        actor,
        Command::Evolution {
            token: question.token,
            answer: EvolutionAnswer::Discard(vec![domain.members.first()?.handle]),
        },
    );
    (submission.kind == SubmissionKind::Applied).then_some(())
}

fn pay(game: &mut Game, actor: Viewer, pp: i64) -> Option<()> {
    let question = question(game, actor);
    if !matches!(
        &question.shape,
        QuestionShape::Evolution(EvolutionQuestion::Payment(_))
    ) {
        return None;
    }
    let submission = game.submit_player(
        actor,
        Command::Evolution {
            token: question.token,
            answer: EvolutionAnswer::Payment {
                pp,
                ep: false,
                super_evolution: false,
            },
        },
    );
    (submission.kind == SubmissionKind::Applied).then_some(())
}

fn question_shape(game: &Game, actor: Viewer) -> QuestionShape {
    question(game, actor).shape
}

#[test]
fn wire_board_keeps_normal_evolution_form_and_payment_in_declaration_without_private_faces() {
    let names = ["unit-follower", "unit-follower"];
    let (catalog, references) = fixture(&names).unwrap();
    let (base, _) = evolution::programs();
    for actor in [Viewer::P1, Viewer::P2] {
        let opponent = if actor == Viewer::P1 {
            Viewer::P2
        } else {
            Viewer::P1
        };
        let mut first = position(Arc::clone(&catalog), actor, "unit-spell", base);
        let mut second = position(Arc::clone(&catalog), actor, "unit-amulet", base);
        for game in [&mut first, &mut second] {
            assert!(
                game.battle_observation(actor, &references).is_ok(),
                "the same snapshot already projects the idle board"
            );
            start(game, actor);
            all_declaring(game, actor, &references)
                .expect("the face choice is a supported declaration stop");
        }
        assert_eq!(
            declaring(&first, actor, opponent, &references)
                .expect("the opponent declaration is available"),
            declaring(&second, actor, opponent, &references)
                .expect("the opponent declaration is available")
        );
        for (ordinal, game) in [&mut first, &mut second].into_iter().enumerate() {
            assert_eq!(
                choose_form(game, actor, ordinal).expect("the selected private face"),
                2
            );
            assert!(matches!(
                question_shape(game, actor),
                QuestionShape::Evolution(EvolutionQuestion::Payment(_))
            ));
            all_declaring(game, actor, &references)
                .expect("the payment choice remains an unspent declaration");
        }
        assert_eq!(
            declaring(&first, actor, opponent, &references)
                .expect("the opponent declaration is available"),
            declaring(&second, actor, opponent, &references)
                .expect("the opponent declaration is available"),
            "another private face choice and another same-sized hand do not change the opponent's board"
        );
        pay(&mut first, actor, 1).expect("the real evolution payment commits");
        for viewer in [Viewer::P1, Viewer::P2] {
            assert!(
                first
                    .battle_observation(viewer, &references)
                    .unwrap()
                    .processing
                    .is_empty(),
                "committing normal evolution retires its declaration context"
            );
        }
    }
}

#[test]
fn wire_board_keeps_evolution_materials_and_hand_discard_in_the_same_declaration() {
    let names = ["unit-follower"];
    let (catalog, references) = fixture(&names).unwrap();
    let program = player_evolution::material_program();
    for actor in [Viewer::P1, Viewer::P2] {
        let opponent = if actor == Viewer::P1 {
            Viewer::P2
        } else {
            Viewer::P1
        };
        let mut first = position(Arc::clone(&catalog), actor, "unit-spell", &program);
        let mut second = position(Arc::clone(&catalog), actor, "unit-amulet", &program);
        for game in [&mut first, &mut second] {
            game.battle_observation(actor, &references)
                .expect("the same snapshot projects before evolution starts");
            start(game, actor);
            form(game, actor);
            all_declaring(game, actor, &references)
                .expect("the real material selection is a declaration");
        }
        assert_eq!(
            declaring(&first, actor, opponent, &references)
                .expect("the first material declaration is available"),
            declaring(&second, actor, opponent, &references)
                .expect("the second material declaration is available"),
            "private material parameters do not change the opponent's complete board"
        );
        for game in [&mut first, &mut second] {
            choose_material(game, actor).expect("the real material answer");
            all_declaring(game, actor, &references)
                .expect("the private discard choice is a declaration");
        }
        assert_eq!(
            declaring(&first, actor, opponent, &references)
                .expect("the opponent declaration is available"),
            declaring(&second, actor, opponent, &references)
                .expect("the opponent declaration is available"),
            "private discard candidates never become an opponent processing parameter"
        );
        for game in [&mut first, &mut second] {
            choose_discard(game, actor).expect("the private discard answer");
            assert!(matches!(
                question_shape(game, actor),
                QuestionShape::Evolution(EvolutionQuestion::Payment(_))
            ));
            all_declaring(game, actor, &references)
                .expect("choosing a discard has not paid it yet");
        }
        assert_eq!(
            declaring(&first, actor, opponent, &references)
                .expect("the opponent declaration is available"),
            declaring(&second, actor, opponent, &references)
                .expect("the opponent declaration is available")
        );
    }
}

#[test]
fn wire_board_evolution_subject_selects_the_later_public_twin_for_each_viewer() {
    let (catalog, references) = fixture(&["unit-follower"]).unwrap();
    let (base, _) = evolution::programs();
    for actor in [Viewer::P1, Viewer::P2] {
        let mut game = position(Arc::clone(&catalog), actor, "unit-spell", base);
        game.advance_player_input();
        let actor_seat = if actor == Viewer::P1 {
            Seat::P1
        } else {
            Seat::P2
        };
        let observation = game.observe(actor).unwrap();
        let own = observation
            .players
            .iter()
            .find(|player| player.player == actor)
            .unwrap();
        let field = own
            .zones
            .iter()
            .find(|zone| zone.kind == ObservedZone::Field)
            .unwrap();
        let selected = &field.cards[1].card.handle;
        let main = question(&game, actor);
        let QuestionShape::ActionWindow(actions) = main.shape else {
            panic!("the real main window supplies evolution intents");
        };
        let intent = actions.intents.iter().find(|intent| {
            matches!(&intent.kind, MainIntentKind::Evolve { holder, .. } if &holder.handle == selected)
        }).unwrap();
        assert_eq!(
            game.submit_player(
                actor,
                Command::MainAction {
                    token: main.token,
                    intent: intent.id.clone(),
                    answer: MainActionAnswer::None,
                }
            )
            .kind,
            SubmissionKind::Applied
        );
        for stage in 0..2 {
            let before = exact_authority(&game);
            let mut handles = Vec::new();
            for viewer in [Viewer::P1, Viewer::P2] {
                let board = game.battle_observation(viewer, &references).unwrap();
                let own = board
                    .players
                    .iter()
                    .find(|player| player.seat == actor_seat)
                    .unwrap();
                let first = &own.zones.field.cards[0].object;
                let second = &own.zones.field.cards[1].object;
                if viewer == actor {
                    assert_eq!(
                        second.as_str(),
                        selected.as_str(),
                        "the wire subject is the selected native handle"
                    );
                }
                assert_eq!(
                    board.objects[first].effective_face, board.objects[second].effective_face,
                    "two public twins cannot be distinguished by printing"
                );
                assert_eq!(
                    board.processing[0].subject,
                    Some(Subject::Card {
                        card: board.objects[second].effective_face.clone(),
                        object: Some(second.clone()),
                    }),
                    "the subject follows the selected physical base rather than the first matching printing"
                );
                handles.push(second.clone());
            }
            assert_ne!(
                handles[0], handles[1],
                "each viewer owns its subject handle"
            );
            assert!(
                exact_authority(&game) == before,
                "reading the public subject is read-only"
            );
            if stage == 0 {
                choose_form(&mut game, actor, 0).expect("the real form answer reaches payment");
            }
        }
    }
}
