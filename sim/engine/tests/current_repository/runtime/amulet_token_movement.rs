use super::*;

fn tree_game(initial: &Value) -> (Game, AbilityId) {
    let (snapshot, keywords, input) = inputs();
    let selected = Catalog::from_current_selection(&snapshot, &keywords, &input).unwrap();
    let ability = selected.current["BP07-T03"]
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
        "amulet-token-movement",
    )
    .unwrap();
    (game, ability)
}

#[test]
fn bp07_t03_puts_itself_in_cemetery_and_its_leave_trigger_survives_token_erasure() {
    for seat in ["P1", "P2"] {
        let actor = PlayerId {
            seat: seat_number(seat),
        };
        let mut initial = draw_position();
        initial["turn"]["active"] = json!(seat);
        initial["players"][seat]["zones"]["ex"] = json!([{"id":"tree","card":"BP07-T03"}]);
        initial["players"][seat]["zones"]["hand"] = json!([]);
        initial["players"][seat]["zones"]["deck"] = json!([
            {"id":"drawn","card":"BP01-037"},{"id":"remaining","card":"BP01-037"}
        ]);
        let (mut game, ability) = tree_game(&initial);
        assert!(matches!(
            game.play_internal_card("tree", actor).unwrap(),
            CardPlay::Entered(_)
        ));
        game.resume_internal_card().unwrap();
        assert!(game.internal_pending_choice().unwrap().is_none());
        let before = format!("{game:?}");
        game.activate_internal_card(
            "tree",
            ability,
            PlayerId {
                seat: if seat == "P1" { 2 } else { 1 },
            },
        )
        .unwrap_err();
        assert!(
            format!("{game:?}") == before,
            "another actor cannot pay or move the token"
        );
        let moved = resolved(game.activate_internal_card("tree", ability, actor).unwrap());
        assert!(moved.progress.facts.iter().any(|fact| matches!(fact, Fact::Movement(m) if m.method == MoveMethod::Put && m.from.kind == ZoneKind::Field && m.movement.actual_to.as_ref().is_some_and(|zone| zone.kind == ZoneKind::Cemetery))));
        let erased = game.projection(View::Referee).unwrap();
        assert_eq!(erased[seat]["field"], json!([]));
        assert_eq!(erased[seat]["cemetery"], json!([]));
        assert!(erased["objects"].get("tree").is_none());
        assert_eq!(erased[seat]["pp"]["current"], 8_u8);
        game.resume_internal_card().unwrap();
        resolve_departure(&mut game, seat);
    }
}

#[expect(clippy::panic, reason = "Unexpected decisions fail the scenario.")]
fn resolve_departure(game: &mut Game, seat: &str) {
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("leave trigger")
    };
    assert_eq!(options.len(), 1, "only the tree's leave trigger is pending");
    assert_eq!(
        options[0].card, "BP07-T03",
        "the erased token remains the source"
    );
    assert!(
        options[0].event.contains("LeaveField"),
        "the trigger remembers the departure"
    );
    let CardPlay::Awaiting(discard) = game
        .answer_internal_card(
            pending.actor,
            pending.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap()
    else {
        panic!("draw then discard")
    };
    let InputDomain::Targets(domain) = &discard.domain else {
        panic!("discard choice")
    };
    assert_eq!(
        domain.members.len(),
        1,
        "the drawn card is the only hand card"
    );
    assert_eq!(
        domain.members[0].card.as_deref(),
        Some("BP01-037"),
        "the chooser can identify the drawn card"
    );
    let opponent = if seat == "P1" { View::P2 } else { View::P1 };
    assert!(
        game.internal_decision(opponent).is_none(),
        "the opponent cannot see the private choice"
    );
    let hidden = game.projection(opponent).unwrap();
    assert!(
        hidden["objects"].get("drawn").is_none(),
        "the drawn hand slot stays hidden"
    );
    let result = resolved(
        game.answer_internal_card(
            discard.actor,
            discard.token,
            Answer::Targets(vec![domain.members[0].handle]),
        )
        .unwrap(),
    );
    assert_eq!(
        result
            .progress
            .facts
            .iter()
            .filter(|fact| matches!(fact, Fact::Draw(_)))
            .count(),
        1,
        "the leave trigger draws once"
    );
    assert_eq!(
        result
            .progress
            .facts
            .iter()
            .filter(|fact| matches!(fact, Fact::Discard(_)))
            .count(),
        1,
        "the leave trigger discards once"
    );
    game.resume_internal_card().unwrap();
    assert!(
        game.internal_pending_choice().unwrap().is_none(),
        "erasure does not duplicate the leave trigger"
    );
    let finished = game.projection(View::Referee).unwrap();
    assert_eq!(
        finished[seat]["hand"],
        json!([]),
        "the drawn card was discarded"
    );
    assert_eq!(
        finished[seat]["cemetery"],
        json!(["drawn"]),
        "only the discard remains in cemetery"
    );
    assert_eq!(
        finished[seat]["deck"],
        json!(["remaining"]),
        "only one card was drawn"
    );
}
