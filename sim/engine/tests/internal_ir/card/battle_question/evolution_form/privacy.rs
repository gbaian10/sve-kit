use super::*;
use alloc::collections::BTreeSet;
use core::error::Error;

fn fixture() -> Result<(Arc<Catalog>, BattleReferences), Box<dyn Error>> {
    let (input, registry, document) =
        evolution::documents_for_forms(&["unit-follower", "unit-follower"]);
    let mut rows = Vec::new();
    for line in input.lines() {
        let mut row: Value = serde_json::from_str(line)?;
        if row["number"] == "unit-other" {
            // A real Evolve card with a different name isolates pairing legality from zone size.
            let face = row["faces"]
                .as_array_mut()
                .and_then(|faces| faces.first_mut())
                .ok_or("incompatible evolution face")?;
            face["card_type"] = json!("フォロワー・エボルヴ");
            face["cost"] = json!("-");
            face["power"] = json!("4");
            face["hp"] = json!("5");
        }
        rows.push(row.to_string());
    }
    battle_evolution::fixture_from_documents((rows.join("\n"), registry, document), &[1, 0])
}

#[test]
fn wire_evolution_form_waiting_board_ignores_hidden_deck_composition() {
    let (catalog, references) = fixture().unwrap();
    for actor in [Viewer::P1, Viewer::P2] {
        let waiter = opposite(actor);
        let mut boards = Vec::new();
        let mut waiting_inputs = Vec::new();
        let mut legal_objects = Vec::new();
        let mut actor_questions = Vec::new();
        for (cards, count) in [
            (["unit-form", "unit-form"], 4_usize),
            (["unit-form", "unit-other"], 2),
            (["unit-other", "unit-form"], 2),
        ] {
            let game = evolution_payment::at_form(
                &catalog,
                actor,
                (2, 0, 1),
                "unit-spell",
                json!([
                    {"id":"physical-form-0","card":cards[0]},
                    {"id":"physical-form-1","card":cards[1]}
                ]),
            );
            let before = exact_authority(&game);
            let native = player_pipeline::question(&game, actor);
            let QuestionShape::Evolution(EvolutionQuestion::Form(forms)) = native.shape else {
                panic!("a genuine Form stop is required in every composition")
            };
            assert_eq!(forms.len(), count);
            legal_objects.push(
                forms
                    .iter()
                    .map(|form| form.card.handle.clone())
                    .collect::<BTreeSet<_>>(),
            );
            let projection = projected(&game, actor, &references, "host-private-form");
            assert_eq!(options(&projection).unwrap().len(), count);
            actor_questions.push(projection.question().clone());
            let waiting = game.input(waiter);
            assert!(matches!(&waiting, Input::Waiting(summary) if summary.actor == actor));
            assert!(
                game.battle_question(waiter, &QuestionId::new("host-waiting".into()), &references)
                    .unwrap()
                    .is_none()
            );
            waiting_inputs.push(waiting);
            let board = game.battle_observation(waiter, &references).unwrap();
            let hidden = &board
                .players
                .iter()
                .find(|player| player.seat == seat(actor))
                .unwrap()
                .zones
                .evolve_deck;
            assert_eq!(hidden.count, Nat::from(2_u32));
            assert!(
                hidden.cards.is_empty(),
                "the waiter only knows the physical zone size"
            );
            for printing in ["printing-form", "printing-other"] {
                assert!(board.objects.values().all(|object| {
                    object.card.printing_id != printing
                        && object.effective_face.printing_id != printing
                }));
                assert!(
                    board
                        .remembered
                        .iter()
                        .all(|memory| { memory.card.printing_id != printing })
                );
            }
            boards.push(board);
            assert!(
                exact_authority(&game) == before,
                "both seats' projection reads preserve the Form authority"
            );
        }
        assert_ne!(legal_objects[0], legal_objects[1]);
        assert_ne!(legal_objects[1], legal_objects[2]);
        assert_ne!(actor_questions[0], actor_questions[1]);
        assert_ne!(actor_questions[1], actor_questions[2]);
        assert!(
            waiting_inputs
                .windows(2)
                .all(|pair| pair.first() == pair.last())
        );
        assert!(
            boards.windows(2).all(|pair| pair.first() == pair.last()),
            "hidden pairing counts and identities cannot change any waiting-seat board component"
        );
    }
}
