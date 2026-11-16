use super::*;
use sve_battle_contract::observation::BattleObservation;

fn waiting_payment(
    branch: &Game,
    actor: Viewer,
    references: &BattleReferences,
    original: &QuestionProjection,
) -> Option<(BattleObservation, Input)> {
    let payment = branch
        .battle_question(
            actor,
            &QuestionId::new("host-private-discard-payment".into()),
            references,
        )
        .ok()??;
    assert!(
        matches!(
            payment.question().shape,
            wire::QuestionShape::EvolutionPayment { .. }
        ),
        "the selected cost advances to a genuine Payment question"
    );
    let own = payment
        .observation()
        .players
        .iter()
        .find(|player| player.seat == seat(actor))?;
    let before = original
        .observation()
        .players
        .iter()
        .find(|player| player.seat == seat(actor))?;
    assert_eq!(
        own.zones.hand, before.zones.hand,
        "selecting a cost leaves every hand member in place"
    );
    assert_eq!(
        (own.pp, own.ep, own.sep),
        (before.pp, before.ep, before.sep),
        "no resource is spent before Payment"
    );
    let waiter = opposite(actor);
    let input = branch.input(waiter);
    assert!(
        matches!(&input, Input::Waiting(summary) if summary.actor == actor),
        "the opponent is waiting for the same actor"
    );
    assert!(
        branch
            .battle_question(
                waiter,
                &QuestionId::new("host-waiting-discard".into()),
                references
            )
            .ok()?
            .is_none(),
        "the waiting seat receives no private payment question"
    );
    let board = branch.battle_observation(waiter, references).ok()?;
    assert!(
        board
            .objects
            .values()
            .all(|object| object.card.printing_id != "printing-spell"
                && object.effective_face.printing_id != "printing-spell"),
        "a selected hand cost has not published any Spell object"
    );
    assert!(
        board
            .remembered
            .iter()
            .all(|memory| memory.card.printing_id != "printing-spell"),
        "a selected hand cost has not published Spell face memory"
    );
    Some((board, input))
}

#[test]
fn wire_evolution_discard_selected_cost_keeps_the_waiting_payment_board_identical() {
    let (catalog, references) = battle_evolution::fixture(&["unit-follower"]).unwrap();
    let base = program();
    for actor in [Viewer::P1, Viewer::P2] {
        let game = make(
            &catalog,
            &base,
            actor,
            &["unit-spell", "unit-spell", "unit-amulet"],
            "unit-other",
        );
        let domain = discard_domain(&game, actor).unwrap();
        assert_eq!(domain.members.len(), 2);
        assert_ne!(domain.members[0].handle, domain.members[1].handle);
        assert_ne!(domain.members[0].object, domain.members[1].object);
        let before = exact_authority(&game);
        let projection = projected(&game, actor, &references);
        assert_domain(&projection, &domain, actor).unwrap();
        assert!(
            exact_authority(&game) == before,
            "observing choices cannot select costs"
        );
        let mut boards = Vec::new();
        let mut inputs = Vec::new();
        for (choice, member) in selection(&projection)
            .unwrap()
            .candidates
            .iter()
            .zip(&domain.members)
        {
            let mut branch = game.clone();
            let command = projection
                .answer(
                    actor,
                    &projection.question().id,
                    &select(vec![choice.id.clone()]),
                )
                .unwrap();
            assert_eq!(
                command,
                Command::Evolution {
                    token: question(&game, actor).token,
                    answer: EvolutionAnswer::Discard(vec![member.handle])
                }
            );
            assert_eq!(
                branch.submit_player(actor, command).kind,
                SubmissionKind::Applied
            );
            let native = question(&branch, actor);
            assert!(matches!(
                native.shape,
                QuestionShape::Evolution(EvolutionQuestion::Payment(_))
            ));
            let after = exact_authority(&branch);
            let (board, input) = waiting_payment(&branch, actor, &references, &projection).unwrap();
            inputs.push(input);
            boards.push(board);
            assert!(
                exact_authority(&branch) == after,
                "Payment projections cannot publish or pay the selected discard"
            );
        }
        assert_eq!(boards.len(), 2);
        assert!(inputs.windows(2).all(|pair| pair.first() == pair.last()));
        assert!(
            boards.windows(2).all(|pair| pair.first() == pair.last()),
            "different selected discards cannot change any waiting-seat Payment board component"
        );
    }
}
