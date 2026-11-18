use super::*;

#[path = "evolution_materials/private_flip.rs"]
mod private_flip;
use crate::player_pipeline::question;
use sve_battle_contract::observation::ProcessingStage;
use sve_engine::battle::BattleReferences;
use sve_engine::game::player::{EvolutionAnswer, EvolutionQuestion, SelectionQuestion};

fn material_program(any: bool) -> WellFormedCardProgram {
    let mut tree = fanfare::expanded::tree().clone();
    let mut base = tree["cards"]["synthetic-evolution"].clone();
    let mut material = json!({"sel":{"who":"self","zone":"cemetery"}});
    if any {
        material["any"] = json!(true);
    } else {
        material["n"] = json!(1);
    }
    base["abilities"][0]["evolve"]["cost"] = json!({"banish":material});
    tree["cards"] = json!({"unit-follower":base});
    lowerer()
        .lower(&tree)
        .unwrap()
        .remove("unit-follower")
        .unwrap()
}

fn make(
    catalog: &Arc<Catalog>,
    program: &WellFormedCardProgram,
    actor: Viewer,
    hand: &[&str],
    cemetery: &[&str],
    secret: &str,
) -> Game {
    let me = if actor == Viewer::P1 { "P1" } else { "P2" };
    let them = if actor == Viewer::P1 { "P2" } else { "P1" };
    let mut zones = json!({});
    zones[me]["hand"] = json!(
        hand.iter()
            .enumerate()
            .map(|(i, card)| json!({"id":format!("fee-{i}"),"card":card}))
            .collect::<Vec<_>>()
    );
    zones[me]["cemetery"] = json!(
        cemetery
            .iter()
            .enumerate()
            .map(|(i, card)| json!({"id":format!("mat-{i}"),"card":card}))
            .collect::<Vec<_>>()
    );
    zones[them]["hand"] = json!([{"id":"other-secret","card":secret}]);
    let mut game = evolution::position_with_catalog(
        Arc::clone(catalog),
        actor.player().seat,
        program,
        &card(vec![]),
        false,
        &zones,
    );
    player_evolution::start(&mut game, actor);
    player_evolution::form(&mut game, actor);
    game
}

fn domain(game: &Game, actor: Viewer) -> Option<SelectionQuestion> {
    if let QuestionShape::Evolution(EvolutionQuestion::Materials(domain)) =
        question(game, actor).shape
    {
        Some(domain)
    } else {
        None
    }
}

fn projected(
    game: &Game,
    actor: Viewer,
    references: &BattleReferences,
    stop: &str,
) -> QuestionProjection {
    game.battle_question(
        actor,
        &QuestionId::new(format!("host-material-{stop}")),
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

fn chosen(projection: &QuestionProjection, indices: &[usize]) -> WireAnswer {
    let candidates = &selection(projection).unwrap().candidates;
    WireAnswer::Select {
        selected: indices
            .iter()
            .map(|index| candidates[*index].id.clone())
            .collect(),
        groups: None,
        roles: vec![],
    }
}

fn pay(game: &mut Game, actor: Viewer, references: &BattleReferences) -> Option<()> {
    let projection = projected(game, actor, references, "payment");
    let wire::QuestionShape::EvolutionPayment { payments } = &projection.question().shape else {
        return None;
    };
    let payment = payments.first()?;
    let answer = WireAnswer::EvolutionPayment {
        pp: payment.pp,
        use_ep: payment.use_ep,
        super_evolution: payment.super_evolution,
    };
    let command = projection
        .answer(actor, &projection.question().id, &answer)
        .ok()?;
    assert_eq!(
        game.submit_player(actor, command).kind,
        SubmissionKind::Applied,
        "the displayed payment advances the real evolution"
    );
    Some(())
}

fn waiting_equal(first: &Game, second: &Game, actor: Viewer, references: &BattleReferences) {
    let viewer = opposite(actor);
    assert!(
        matches!(first.input(viewer), Input::Waiting(_)),
        "the other seat really waits"
    );
    assert_eq!(
        first.input(viewer),
        second.input(viewer),
        "the complete Waiting summary hides materials"
    );
    assert_eq!(
        first.battle_observation(viewer, references).unwrap(),
        second.battle_observation(viewer, references).unwrap(),
        "the complete waiting board hides material membership and selection"
    );
    for game in [first, second] {
        assert!(
            game.battle_question(
                viewer,
                &QuestionId::new("waiting-material".into()),
                references
            )
            .unwrap()
            .is_none(),
            "the waiting seat has no answerable question"
        );
    }
}

fn assert_material_domain(
    projection: &QuestionProjection,
    native: &SelectionQuestion,
    actor: Viewer,
) {
    assert_eq!(
        projection.question().usage,
        wire::QuestionUse::EvolutionMaterial,
        "materials retain their own use"
    );
    assert_eq!(
        projection.question().purpose,
        wire::QuestionPurpose::PlayParameter,
        "intrinsic parameters are unpaid"
    );
    assert_eq!(
        projection.question().processing.stage,
        Some(ProcessingStage::Declare),
        "choosing materials is not a payment"
    );
    assert_eq!(
        projection.observation().processing,
        vec![projection.question().processing.clone()],
        "question and board share the same context"
    );
    let wire_selection = selection(projection).unwrap();
    assert_eq!(
        wire_selection.candidates.len(),
        native.members.len(),
        "all frozen members are projected"
    );
    assert_eq!(
        wire_selection.bounds.min.get(),
        native.cardinality.minimum.0,
        "the native minimum remains effective"
    );
    assert_eq!(
        wire_selection.bounds.max.map(sve_battle_contract::Nat::get),
        native.cardinality.maximum.map(|max| max.0),
        "the native maximum remains effective"
    );
    let zone = &projection
        .observation()
        .players
        .iter()
        .find(|player| player.seat == seat(actor))
        .unwrap()
        .zones
        .cemetery;
    for (candidate, member) in wire_selection.candidates.iter().zip(&native.members) {
        let handle = member.object.as_ref().unwrap();
        let located = zone
            .cards
            .iter()
            .find(|card| card.object.as_str() == handle.as_str())
            .unwrap();
        let expected = &projection.observation().objects[&located.object];
        assert_eq!(
            candidate.subject,
            Some(Subject::Card {
                object: Some(located.object.clone()),
                card: expected.effective_face.clone()
            }),
            "a material comes from this authorized object table"
        );
        let origin = candidate.origin.unwrap();
        assert_eq!(
            origin.zone,
            Zone::Cemetery,
            "the material's actual zone is preserved"
        );
        assert_eq!(
            origin.player,
            seat(actor),
            "the actual zone owner is preserved"
        );
        assert_eq!(
            origin.position, located.position,
            "the material's actual position is preserved"
        );
    }
}

fn assert_rejected_answers(projection: &QuestionProjection, actor: Viewer) {
    let answer = chosen(projection, &[1]);
    assert!(
        projection
            .answer(opposite(actor), &projection.question().id, &answer)
            .is_err(),
        "a foreign seat cannot decode"
    );
    assert!(
        projection
            .answer(actor, &QuestionId::new("foreign".into()), &answer)
            .is_err(),
        "a foreign question cannot decode"
    );
    for bad in [
        chosen(projection, &[]),
        chosen(projection, &[0, 0]),
        chosen(projection, &[0, 1]),
        WireAnswer::Select {
            selected: vec![OptionId::new("foreign".into())],
            groups: None,
            roles: vec![],
        },
    ] {
        assert!(
            projection
                .answer(actor, &projection.question().id, &bad)
                .is_err(),
            "missing, repeated, excessive and foreign materials must not decode"
        );
    }
}

#[test]
fn wire_materials_choose_the_later_cemetery_twin_and_pay_only_after_discard() {
    let (catalog, references) = battle_evolution::fixture(&["unit-follower"]).unwrap();
    let base = player_evolution::material_program();
    for actor in [Viewer::P1, Viewer::P2] {
        let mut game = make(
            &catalog,
            &base,
            actor,
            &["unit-spell"],
            &["unit-spell", "unit-spell"],
            "unit-other",
        );
        let native = domain(&game, actor).unwrap();
        assert_eq!(native.members.len(), 2);
        game.battle_observation(actor, &references).unwrap();
        let before = exact_authority(&game);
        let projection = projected(&game, actor, &references, "materials");
        assert!(exact_authority(&game) == before, "projection is read-only");
        assert_material_domain(&projection, &native, actor);
        assert_rejected_answers(&projection, actor);
        let answer = chosen(&projection, &[1]);
        let command = projection
            .answer(actor, &projection.question().id, &answer)
            .unwrap();
        assert!(
            matches!(&command, Command::Evolution { answer: EvolutionAnswer::Materials(handles), .. } if handles == &vec![native.members[1].handle])
        );
        assert_eq!(
            game.submit_player(actor, command).kind,
            SubmissionKind::Applied
        );
        let after = exact_authority(&game);
        let stale = projection
            .answer(actor, &projection.question().id, &answer)
            .unwrap();
        assert_eq!(
            game.submit_player(actor, stale).kind,
            SubmissionKind::Rejected
        );
        assert!(
            exact_authority(&game) == after,
            "a stale material answer does not alter the next stop"
        );
        assert!(matches!(
            question(&game, actor).shape,
            QuestionShape::Evolution(EvolutionQuestion::Discard(_))
        ));
        let own = if actor == Viewer::P1 { "P1" } else { "P2" };
        let unpaid = game.projection(View::Referee).unwrap();
        assert_eq!(unpaid[own]["cemetery"], json!(["mat-0", "mat-1"]));
        assert_eq!(unpaid[own]["hand"], json!(["fee-0"]));
        let discard = projected(&game, actor, &references, "discard");
        let command = discard
            .answer(actor, &discard.question().id, &chosen(&discard, &[0]))
            .unwrap();
        assert_eq!(
            game.submit_player(actor, command).kind,
            SubmissionKind::Applied
        );
        assert_eq!(
            game.projection(View::Referee).unwrap()[own]["hand"],
            json!(["fee-0"])
        );
        pay(&mut game, actor, &references).unwrap();
        let paid = game.projection(View::Referee).unwrap();
        assert_eq!(paid[own]["banish"], json!(["mat-1"]));
        assert_eq!(paid[own]["cemetery"], json!(["mat-0", "fee-0"]));
    }
}

#[test]
fn material_stops_preserve_both_seats_unrelated_hand_secrets() {
    let (catalog, references) = battle_evolution::fixture(&["unit-follower"]).unwrap();
    let base = material_program(false);
    for actor in [Viewer::P1, Viewer::P2] {
        let first = make(
            &catalog,
            &base,
            actor,
            &["unit-spell"],
            &["unit-spell", "unit-amulet"],
            "unit-other",
        );
        let foreign_secret = make(
            &catalog,
            &base,
            actor,
            &["unit-spell"],
            &["unit-spell", "unit-amulet"],
            "unit-spell",
        );
        let own_secret = make(
            &catalog,
            &base,
            actor,
            &["unit-amulet"],
            &["unit-spell", "unit-amulet"],
            "unit-other",
        );
        assert_eq!(domain(&first, actor).unwrap().members.len(), 2);
        assert_eq!(domain(&own_secret, actor).unwrap().members.len(), 2);
        let before = exact_authority(&first);
        let a = projected(&first, actor, &references, "materials");
        let b = projected(&foreign_secret, actor, &references, "materials");
        assert_eq!(selection(&a).unwrap().candidates.len(), 2);
        assert_eq!(a.question(), b.question());
        assert_eq!(a.observation(), b.observation());
        waiting_equal(&first, &own_secret, actor, &references);
        assert!(
            exact_authority(&first) == before,
            "reading complete projections preserves authority"
        );
    }
}

#[test]
fn different_materials_stay_unmoved_until_the_payment() {
    let (catalog, references) = battle_evolution::fixture(&["unit-follower"]).unwrap();
    let base = material_program(false);
    for actor in [Viewer::P1, Viewer::P2] {
        let mut first = make(
            &catalog,
            &base,
            actor,
            &[],
            &["unit-spell", "unit-amulet"],
            "unit-other",
        );
        let mut second = make(
            &catalog,
            &base,
            actor,
            &[],
            &["unit-spell", "unit-amulet"],
            "unit-other",
        );
        for (game, index) in [(&mut first, 0), (&mut second, 1)] {
            assert_eq!(domain(game, actor).unwrap().members.len(), 2);
            let projection = projected(game, actor, &references, "materials");
            let command = projection
                .answer(
                    actor,
                    &projection.question().id,
                    &chosen(&projection, &[index]),
                )
                .unwrap();
            assert_eq!(
                game.submit_player(actor, command).kind,
                SubmissionKind::Applied
            );
            assert!(matches!(
                question(game, actor).shape,
                QuestionShape::Evolution(EvolutionQuestion::Payment(_))
            ));
        }
        waiting_equal(&first, &second, actor, &references);
        let own = if actor == Viewer::P1 { "P1" } else { "P2" };
        for (game, selected) in [(&mut first, "mat-0"), (&mut second, "mat-1")] {
            assert_eq!(
                game.projection(View::Referee).unwrap()[own]["cemetery"],
                json!(["mat-0", "mat-1"])
            );
            pay(game, actor, &references).unwrap();
            assert_eq!(
                game.projection(View::Referee).unwrap()[own]["banish"],
                json!([selected])
            );
        }
    }
}

#[test]
fn optional_material_cost_with_no_candidates_accepts_the_native_empty_set() {
    let (catalog, references) = battle_evolution::fixture(&["unit-follower"]).unwrap();
    let base = material_program(true);
    for actor in [Viewer::P1, Viewer::P2] {
        let mut game = make(&catalog, &base, actor, &[], &[], "unit-other");
        let native = domain(&game, actor).unwrap();
        assert_eq!(native.members.len(), 0);
        assert_eq!(native.cardinality.minimum.0, 0);
        let projection = projected(&game, actor, &references, "empty-materials");
        assert_eq!(selection(&projection).unwrap().candidates.len(), 0);
        let command = projection
            .answer(actor, &projection.question().id, &chosen(&projection, &[]))
            .unwrap();
        assert_eq!(
            game.submit_player(actor, command).kind,
            SubmissionKind::Applied
        );
        assert!(matches!(
            question(&game, actor).shape,
            QuestionShape::Evolution(EvolutionQuestion::Payment(_))
        ));
        pay(&mut game, actor, &references).unwrap();
    }
}
