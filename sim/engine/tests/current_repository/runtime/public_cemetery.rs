use super::*;

fn bell_game(initial: &Value) -> (Game, AbilityId) {
    let selected = selection().clone();
    let ability = selected.current["BP18-115"]
        .program
        .card()
        .abilities
        .iter()
        .find(|entry| entry.node.kind == AbilityKind::Activated)
        .unwrap()
        .node
        .id;
    let game = Game::from_selected_effects(
        selected,
        initial,
        &Value::Null,
        &Value::Null,
        "public-cemetery",
    )
    .unwrap();
    (game, ability)
}

#[expect(
    clippy::panic,
    reason = "An unexpected pending domain fails the scenario."
)]
fn resolve_bell(game: &mut Game) -> Activation {
    let question = game.internal_pending_choice().unwrap().unwrap();
    if let InputDomain::Pending(options) = &question.domain {
        assert_eq!(options.len(), 1, "only the bell trigger is pending");
        assert_eq!(options[0].card, "BP18-115", "the bell remains the source");
        resolved(
            game.answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap(),
        )
    } else {
        panic!("expected bell trigger");
    }
}

#[test]
fn bp18_115_puts_itself_in_cemetery_as_an_effect_and_triggers_lastword() {
    for seat in ["P1", "P2"] {
        let mut initial = draw_position();
        initial["turn"]["active"] = json!(seat);
        initial["players"][seat]["zones"]["hand"] = json!([{"id":"bell","card":"BP18-115"}]);
        initial["players"][seat]["zones"]["deck"] = json!([{"id":"drawn","card":"BP01-037"}]);
        let (mut game, ability) = bell_game(&initial);
        assert!(matches!(
            game.play_internal_card(
                "bell",
                PlayerId {
                    seat: seat_number(seat)
                }
            )
            .unwrap(),
            CardPlay::Entered(_)
        ));
        resolve_bell(&mut game);
        game.resume_internal_card().unwrap();
        let before = format!("{game:?}");
        game.activate_internal_card(
            "bell",
            ability,
            PlayerId {
                seat: if seat == "P1" { 2 } else { 1 },
            },
        )
        .unwrap_err();
        assert!(
            format!("{game:?}") == before,
            "another actor cannot pay or move the card"
        );
        let result = resolved(
            game.activate_internal_card(
                "bell",
                ability,
                PlayerId {
                    seat: seat_number(seat),
                },
            )
            .unwrap(),
        );
        assert!(
            result
                .progress
                .facts
                .iter()
                .any(|fact| matches!(fact, Fact::Movement(m) if m.method == MoveMethod::Put))
        );
        assert!(
            !result
                .progress
                .facts
                .iter()
                .any(|fact| matches!(fact, Fact::Movement(m) if m.method == MoveMethod::Destroy))
        );
        game.resume_internal_card().unwrap();
        resolve_bell(&mut game);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state[seat]["field"], json!([]));
        assert_eq!(state[seat]["cemetery"], json!(["bell"]));
        assert_eq!(state[seat]["hand"], json!(["drawn"]));
        assert_eq!(state[seat]["leader"]["life"], 21_u8);
        assert_eq!(state[seat]["pp"]["current"], 7_u8);
    }
}
