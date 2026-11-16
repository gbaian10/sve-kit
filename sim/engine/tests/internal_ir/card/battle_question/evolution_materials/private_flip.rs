use super::*;
use sve_engine::game::player::observation::ObservedZone;

fn program() -> WellFormedCardProgram {
    let mut tree = fanfare::expanded::tree().clone();
    let mut base = tree["cards"]["synthetic-evolution"].clone();
    base["abilities"][0]["evolve"]["cost"] = json!({"flip_up":{
        "sel":{"who":"self","zone":"evolve_deck","kind":"evolve_follower","face_up":false,"name":"eligible-fee"},"n":1
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
    program: &WellFormedCardProgram,
    actor: Viewer,
    last_face: u64,
) -> Option<Game> {
    let own = if actor == Viewer::P1 { "P1" } else { "P2" };
    let mut zones = json!({});
    zones[own]["evolve_deck"] = json!([
        {"id":"e","card":"unit-form","state":{"face":0},"face_up":false},
        {"id":"flip-a","card":"unit-form","state":{"face":1},"face_up":false},
        {"id":"flip-b","card":"unit-form","state":{"face":last_face},"face_up":false}
    ]);
    let mut game = evolution::position_with_catalog(
        Arc::clone(catalog),
        actor.player().seat,
        program,
        &card(vec![]),
        false,
        &zones,
    );
    player_evolution::start(&mut game, actor);
    let first = question(&game, actor);
    let QuestionShape::Evolution(EvolutionQuestion::Form(forms)) = first.shape else {
        return None;
    };
    let observed = game.observe(actor).ok()?;
    let intended = observed
        .players
        .iter()
        .find(|player| player.player == actor)?
        .zones
        .iter()
        .find(|zone| zone.kind == ObservedZone::EvolveDeck)?
        .cards
        .iter()
        .find(|card| card.position == Some(0))?;
    let form = forms
        .iter()
        .find(|form| form.card.handle == intended.card.handle && form.face == 0)?;
    assert_eq!(
        game.submit_player(
            actor,
            Command::Evolution {
                token: first.token,
                answer: EvolutionAnswer::Form(form.id)
            }
        )
        .kind,
        SubmissionKind::Applied,
        "the genuine selected form opens its cost material stop"
    );
    Some(game)
}

#[test]
fn private_flip_materials_hide_eligible_faces_and_the_unpaid_selection() {
    let (catalog, references) =
        battle_evolution::fixture(&["unit-follower", "eligible-fee"]).unwrap();
    let base = program();
    for actor in [Viewer::P1, Viewer::P2] {
        let mut two = make(&catalog, &base, actor, 1).unwrap();
        let mut one = make(&catalog, &base, actor, 0).unwrap();
        assert_eq!(domain(&two, actor).unwrap().members.len(), 2);
        assert_eq!(domain(&one, actor).unwrap().members.len(), 1);
        let before = exact_authority(&two);
        let a = projected(&two, actor, &references, "private-flip");
        let b = projected(&one, actor, &references, "private-flip");
        assert_eq!(selection(&a).unwrap().candidates.len(), 2);
        assert_eq!(selection(&b).unwrap().candidates.len(), 1);
        for projection in [&a, &b] {
            for candidate in &selection(projection).unwrap().candidates {
                assert_eq!(candidate.origin.unwrap().zone, Zone::EvolveDeck);
                let Some(Subject::Card { card, .. }) = &candidate.subject else {
                    panic!("authorized flip material face");
                };
                assert_eq!(card.face.get(), 1);
            }
        }
        waiting_equal(&two, &one, actor, &references);
        assert!(
            exact_authority(&two) == before,
            "complete reads leave authority unchanged"
        );
        for (game, projection, index) in [(&mut two, &a, 1), (&mut one, &b, 0)] {
            let answer = chosen(projection, &[index]);
            let command = projection
                .answer(actor, &projection.question().id, &answer)
                .unwrap();
            assert_eq!(
                game.submit_player(actor, command).kind,
                SubmissionKind::Applied
            );
            assert!(matches!(
                question(game, actor).shape,
                QuestionShape::Evolution(EvolutionQuestion::Payment(_))
            ));
            let board = game
                .battle_observation(opposite(actor), &references)
                .unwrap();
            let owner = board
                .players
                .iter()
                .find(|player| player.seat == seat(actor))
                .unwrap();
            assert_eq!(
                owner.zones.evolve_deck.cards.len(),
                0,
                "no selected fee is published before payment"
            );
        }
        waiting_equal(&two, &one, actor, &references);
        for game in [&mut two, &mut one] {
            pay(game, actor, &references).unwrap();
            let board = game
                .battle_observation(opposite(actor), &references)
                .unwrap();
            let owner = board
                .players
                .iter()
                .find(|player| player.seat == seat(actor))
                .unwrap();
            assert_eq!(owner.zones.evolve_deck.count.get(), 2);
            assert_eq!(owner.zones.evolve_deck.cards.len(), 1);
            let revealed = &owner.zones.evolve_deck.cards[0];
            assert_eq!(
                board.objects[&revealed.object].effective_face.face.get(),
                1,
                "payment publishes the selected current face"
            );
        }
    }
}
