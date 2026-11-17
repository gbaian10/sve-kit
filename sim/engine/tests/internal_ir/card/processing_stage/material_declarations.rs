use super::recipient_declarations::{append_distribution, shares};
use super::*;
use sve_engine::game::internal::{CostPaymentPurpose, InputDomain, PlayDeclaration};

#[derive(Clone, Copy)]
enum MaterialKind {
    Field,
    Hand,
}

fn material_program(kind: MaterialKind, target_maximum: u64) -> WellFormedCardProgram {
    let material = match kind {
        MaterialKind::Field => {
            json!({"act":{"sel":{"who":"self","zone":"field","kind":"amulet"},"n":1}})
        }
        MaterialKind::Hand => {
            json!({"reveal":{"sel":{"who":"self","zone":"hand","kind":"spell"},"n":1}})
        }
    };
    let mut body = json!({"cost":{"steps":[{"pp":1},{"act_self":true},material]},
        "do":[{"optional":{"do":[{"life":{"who":"self","delta":1}}]}}]});
    if target_maximum != 0 {
        let mut target =
            json!({"bind":"enemy","from":{"who":"opp","zone":"field","kind":"follower"},"n":1});
        if target_maximum != 1 {
            target.as_object_mut().unwrap().remove("n");
            target["upto"] = json!(target_maximum);
        }
        body["targets"] = json!([target]);
    }
    let mut raw = program("activate", body).card().clone();
    // The existing non-mode path permits head-cost materials before formal targets.
    raw.abilities[0]
        .node
        .play
        .as_mut()
        .unwrap()
        .parameters
        .sort_by_key(|decision| {
            !matches!(
                decision.kind,
                DecisionKind::PlayParameter {
                    slot: PlaySlot::CostMaterials,
                    ..
                }
            )
        });
    WellFormedCardProgram::check(raw).unwrap()
}

fn start(program: &WellFormedCardProgram, actor: Viewer, eligible_hand: usize) -> Game {
    let me = if actor == Viewer::P1 { "P1" } else { "P2" };
    let them = if actor == Viewer::P1 { "P2" } else { "P1" };
    let mut game = fixture_with(program, actor, |initial| {
        initial["players"][me]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .extend([
                json!({"id":"ally0","card":"unit-amulet"}),
                json!({"id":"ally1","card":"unit-amulet"}),
            ]);
        initial["players"][me]["zones"]["hand"] = json!((0..2).map(|i| json!({"id":format!("h{i}"),"card":if i < eligible_hand { "unit-spell" } else { "unit-follower" }})).collect::<Vec<_>>());
        initial["players"][them]["zones"]["field"] = json!(
            (0..4)
                .map(|i| json!({"id":format!("f{i}"),"card":"unit-follower"}))
                .collect::<Vec<_>>()
        );
    });
    game.activate_internal_card(
        "source",
        program.card().abilities[0].node.id,
        actor.player(),
    )
    .unwrap();
    game
}

fn apply(game: &mut Game, actor: Viewer, answer: Answer) {
    let q = question(game, actor);
    assert_eq!(
        submit(game, actor, &q, answer),
        SubmissionKind::Applied,
        "the fixture answers its current question legally"
    );
}

fn declarations(game: &Game) -> Vec<PlayDeclaration> {
    let before = exact_authority(game);
    let result = game.internal_play_declarations().unwrap().to_vec();
    for viewer in [Viewer::P1, Viewer::P2] {
        game.observe(viewer).unwrap();
    }
    assert_eq!(
        game.internal_play_declarations().unwrap(),
        result.as_slice(),
        "observation does not publish material declarations"
    );
    assert!(
        exact_authority(game) == before,
        "material declaration reads are pure"
    );
    result
}

fn selected(game: &Game) -> Vec<ObjectRef> {
    let recorded = declarations(game);
    assert_eq!(
        recorded.len(),
        1,
        "the fixture has exactly one accepted material group"
    );
    let objects = if let PlayDeclaration::Materials { objects, .. } = &recorded[0] {
        Some(objects.clone())
    } else {
        None
    };
    assert!(
        objects.is_some(),
        "the accepted parameter was a material group"
    );
    objects.unwrap()
}

fn unpaid(game: &Game, actor: Viewer) {
    let me = if actor == Viewer::P1 { "P1" } else { "P2" };
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        state[me]["pp"]["current"], 3,
        "selection is not a PP payment"
    );
    for id in ["source", "ally0", "ally1"] {
        assert_eq!(
            state["objects"][id]["acted"], false,
            "selection does not act {id}"
        );
    }
    assert!(
        game.internal_cost_spec_payments().unwrap().is_empty(),
        "no CostSpec invocation has completed"
    );
}

fn payment_matches(game: &Game, objects: &[ObjectRef]) {
    let original = game
        .internal_cost_spec_payments()
        .unwrap()
        .iter()
        .find(|cost| cost.purpose == CostPaymentPurpose::Original)
        .unwrap();
    assert_eq!(
        original.steps.len(),
        3,
        "PP, source Act and selected material all paid"
    );
    assert_eq!(
        original.steps[2].subjects, objects,
        "the payment writer used the declared physical generations"
    );
    assert_eq!(
        original.steps[2].quantity,
        Quantity {
            amount: Int(1),
            unit: QuantityUnit::Cards
        },
        "one material was actually paid"
    );
}

#[test]
fn choosing_a_material_is_retained_before_any_payment() {
    for kind in [MaterialKind::Field, MaterialKind::Hand] {
        let program = material_program(kind, 1);
        for actor in [Viewer::P1, Viewer::P2] {
            let mut game = start(&program, actor, 2);
            assert!(
                declarations(&game).is_empty(),
                "probes do not accept material declarations"
            );
            let native = game.internal_decision(View::Referee).unwrap();
            let q = question(&game, actor);
            let before = exact_authority(&game);
            let wrong = if actor == Viewer::P1 {
                Viewer::P2
            } else {
                Viewer::P1
            };
            assert_eq!(
                submit(&mut game, wrong, &q, Answer::CostMaterials(vec![0])),
                SubmissionKind::Rejected
            );
            for answer in [
                Answer::CostMaterials(vec![99]),
                Answer::CostMaterials(vec![0, 0]),
                Answer::Targets(vec![0]),
            ] {
                assert_eq!(
                    submit(&mut game, actor, &q, answer),
                    SubmissionKind::Rejected
                );
            }
            assert!(
                exact_authority(&game) == before,
                "invalid materials do not leave declarations"
            );
            apply(&mut game, actor, Answer::CostMaterials(vec![0]));
            let saved = declarations(&game);
            let objects = selected(&game);
            assert_eq!(objects.len(), 1);
            assert_eq!(
                saved,
                vec![PlayDeclaration::Materials {
                    player: actor.player(),
                    decision: native.token.decision,
                    objects: objects.clone()
                }]
            );
            unpaid(&game, actor);
            assert!(matches!(
                question(&game, actor).shape,
                QuestionShape::Decision(player::DecisionShape::Targets(_))
            ));
            apply(&mut game, actor, Answer::Targets(vec![0]));
            assert_eq!(declarations(&game), saved);
            payment_matches(&game, &objects);
            closed_wire(&game, actor);
            finish_may(&mut game);
            assert_eq!(game.internal_play_declarations(), None);
        }
    }
}

fn hand_candidates(game: &Game) -> usize {
    let q = game.internal_decision(View::Referee).unwrap();
    let domain = if let InputDomain::CostMaterials(domain) = q.domain {
        Some(domain)
    } else {
        None
    };
    assert!(
        domain.is_some(),
        "the fixture requires a private cost-material domain"
    );
    domain.unwrap().members.len()
}

#[test]
fn private_candidate_counts_do_not_change_the_waiting_seats_whole_native_observation() {
    let program = material_program(MaterialKind::Hand, 1);
    for actor in [Viewer::P1, Viewer::P2] {
        let waiting = if actor == Viewer::P1 {
            Viewer::P2
        } else {
            Viewer::P1
        };
        let one = start(&program, actor, 1);
        let two = start(&program, actor, 2);
        assert_eq!(hand_candidates(&one), 1);
        assert_eq!(hand_candidates(&two), 2);
        assert_eq!(one.observe(waiting).unwrap(), two.observe(waiting).unwrap());
        assert_eq!(one.input(waiting), two.input(waiting));
        closed_wire(&one, actor);
        closed_wire(&two, actor);
    }
}

#[test]
fn different_private_materials_before_payment_leave_the_waiting_observation_identical() {
    let program = material_program(MaterialKind::Hand, 1);
    for actor in [Viewer::P1, Viewer::P2] {
        let waiting = if actor == Viewer::P1 {
            Viewer::P2
        } else {
            Viewer::P1
        };
        let mut first = start(&program, actor, 2);
        let mut second = first.clone();
        apply(&mut first, actor, Answer::CostMaterials(vec![0]));
        apply(&mut second, actor, Answer::CostMaterials(vec![1]));
        assert_ne!(selected(&first), selected(&second));
        unpaid(&first, actor);
        unpaid(&second, actor);
        assert_eq!(
            first.observe(waiting).unwrap(),
            second.observe(waiting).unwrap()
        );
        assert_eq!(first.input(waiting), second.input(waiting));
        closed_wire(&first, actor);
        closed_wire(&second, actor);
    }
}

#[test]
fn a_selection_retry_discards_the_old_material_and_only_pays_the_new_one() {
    let program = append_distribution(&material_program(MaterialKind::Field, 4));
    for actor in [Viewer::P1, Viewer::P2] {
        let mut game = start(&program, actor, 2);
        apply(&mut game, actor, Answer::CostMaterials(vec![0]));
        let first = selected(&game);
        unpaid(&game, actor);
        apply(&mut game, actor, Answer::Targets(vec![0, 1, 2, 3]));
        assert!(
            declarations(&game).is_empty(),
            "the rejected round no longer declares a material"
        );
        unpaid(&game, actor);
        apply(&mut game, actor, Answer::CostMaterials(vec![1]));
        let second = selected(&game);
        assert_ne!(first, second);
        apply(&mut game, actor, Answer::Targets(vec![0, 1]));
        apply(&mut game, actor, shares());
        let complete = declarations(&game);
        assert_eq!(complete.len(), 2);
        assert!(matches!(complete[0], PlayDeclaration::Materials { .. }));
        assert!(matches!(complete[1], PlayDeclaration::Distribution { .. }));
        payment_matches(&game, &second);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["ally0"]["acted"], false);
        assert_eq!(state["objects"]["ally1"]["acted"], true);
        finish_may(&mut game);
        assert_eq!(game.internal_play_declarations(), None);
    }
}

#[test]
fn a_body_fault_discards_the_material_declaration_and_its_payment() {
    for kind in [MaterialKind::Field, MaterialKind::Hand] {
        let program = lifecycle::with_overflow(&material_program(kind, 0));
        for actor in [Viewer::P1, Viewer::P2] {
            let mut game = start(&program, actor, 2);
            apply(&mut game, actor, Answer::CostMaterials(vec![0]));
            let objects = selected(&game);
            assert_eq!(objects.len(), 1);
            payment_matches(&game, &objects);
            let may = question(&game, actor);
            assert_eq!(
                submit(&mut game, actor, &may, Answer::Integer(Nat(1))),
                SubmissionKind::EngineFault
            );
            assert_eq!(game.internal_play_declarations(), None);
            let me = if actor == Viewer::P1 { "P1" } else { "P2" };
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state[me]["pp"]["current"], 3);
            assert_eq!(state["objects"]["source"]["acted"], false);
            assert_eq!(state["objects"]["ally0"]["acted"], false);
        }
    }
}
