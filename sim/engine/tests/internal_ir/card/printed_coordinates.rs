use super::*;

#[test]
fn printed_coordinates_distinguish_scopes_and_reject_duplicate_scope_atomically() {
    for distinct in [false, true] {
        let first = ability(1, vec![]);
        let mut second = ability(2, vec![]);
        second.source.line = 1;
        second.node.source.origin = first.node.source.origin.clone();
        if distinct {
            second.source.scope = "second".into();
        }
        let mut game = Game::new(
            catalog(),
            &setup(1, json!([])),
            &Value::Null,
            &Value::Null,
            "coordinate",
        )
        .unwrap();
        let before = exact_authority(&game);
        let result = game.load_internal_card("unit-follower", &card(vec![first, second]));
        if distinct {
            result.unwrap();
        } else {
            assert!(
                matches!(result, Err(sve_engine::EngineFailure::Invalid(reason))
                if reason == "invalid or duplicate printed ability coordinate")
            );
            assert_eq!(before, exact_authority(&game));
        }
    }
}
