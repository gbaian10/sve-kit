use super::*;

#[test]
fn normally_played_spell_draws_three_from_its_owners_deck() {
    // CR 5.10.1-.2: draw three performs three successive single-card draws.
    for seat in ["P1", "P2"] {
        let opponent = if seat == "P1" { "P2" } else { "P1" };
        let mut initial = draw_position();
        initial["turn"]["active"] = json!(seat);
        for owner in ["P1", "P2"] {
            initial["players"][owner]["zones"]["deck"] = json!([
                {"id":format!("{owner}-d1"),"card":"BP10-122"},
                {"id":format!("{owner}-d2"),"card":"BP10-122"},
                {"id":format!("{owner}-d3"),"card":"BP10-122"},
                {"id":format!("{owner}-d4"),"card":"BP10-122"}
            ]);
        }
        initial["players"][seat]["zones"]["hand"] = json!([{"id":"spell","card":"CP02-068"}]);
        let mut game = game(&initial);
        let result = resolved(
            game.play_internal_card(
                "spell",
                PlayerId {
                    seat: if seat == "P1" { 1 } else { 2 },
                },
            )
            .unwrap(),
        );
        assert!(
            result
                .progress
                .facts
                .iter()
                .any(|fact| matches!(fact, Fact::Draw(_)))
        );
        let referee = game.projection(View::Referee).unwrap();
        assert_eq!(referee[seat]["hand"].as_array().unwrap().len(), 3);
        assert_eq!(referee[seat]["deck"], json!([format!("{seat}-d4")]));
        assert_eq!(referee[seat]["cemetery"], json!(["spell"]));
        assert_eq!(referee[seat]["pp"]["current"], 6_u8);
        assert_eq!(projected_zone_count(&referee[opponent]["hand"]), 0_u64);
        assert_eq!(projected_zone_count(&referee[opponent]["deck"]), 4_u64);
        assert_eq!(game.internal_card_stage(), Some(ResumeStage::CheckTiming));
        game.resume_internal_card().unwrap();
        for view in [View::P1, View::P2, View::Referee] {
            let state = game.projection(view).unwrap();
            assert_eq!(projected_zone_count(&state[seat]["hand"]), 3_u64);
            assert_eq!(projected_zone_count(&state[seat]["deck"]), 1_u64);
            assert_eq!(projected_zone_count(&state[opponent]["hand"]), 0_u64);
            assert_eq!(projected_zone_count(&state[opponent]["deck"]), 4_u64);
            if view == View::Referee {
                assert_eq!(state[seat]["deck"], json!([format!("{seat}-d4")]));
            }
        }
    }
}

#[test]
fn drawing_past_the_last_card_checks_the_existing_deck_loss_rule() {
    // CR 1.2.1.2, 5.10.1.1-.2, 11.2.2: the third draw with an empty deck loses at check timing.
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"spell","card":"CP02-068"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"d1","card":"BP10-122"},
        {"id":"d2","card":"BP10-122"}
    ]);
    let mut game = game(&initial);
    resolved(
        game.play_internal_card("spell", PlayerId { seat: 1 })
            .unwrap(),
    );
    let referee = game.projection(View::Referee).unwrap();
    assert_eq!(referee["P1"]["hand"].as_array().unwrap().len(), 2);
    assert_eq!(referee["P1"]["deck"].as_array().unwrap().len(), 0);
    assert_eq!(game.internal_card_stage(), Some(ResumeStage::CheckTiming));
    assert_ne!(
        referee["game"]["ended"], true,
        "loss waits for check timing"
    );
    game.resume_internal_card().unwrap();
    let final_state = game.projection(View::Referee).unwrap();
    assert_eq!(final_state["game"]["ended"], true);
    assert_eq!(final_state["game"]["winner"], "P2");
    for view in [View::P1, View::P2, View::Referee] {
        assert_eq!(game.projection(view).unwrap()["game"]["winner"], "P2");
    }
}

#[test]
fn drawing_exactly_the_last_cards_does_not_lose() {
    // CR 5.10.1.1, 11.2.2: only an attempt on an empty deck loses; an empty deck after drawing does not.
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"spell","card":"CP02-068"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"d1","card":"BP10-122"},
        {"id":"d2","card":"BP10-122"},
        {"id":"d3","card":"BP10-122"}
    ]);
    let mut game = game(&initial);
    resolved(
        game.play_internal_card("spell", PlayerId { seat: 1 })
            .unwrap(),
    );
    game.resume_internal_card().unwrap();
    for view in [View::P1, View::P2, View::Referee] {
        let state = game.projection(view).unwrap();
        assert_eq!(projected_zone_count(&state["P1"]["hand"]), 3_u64);
        assert_eq!(projected_zone_count(&state["P1"]["deck"]), 0_u64);
        assert_ne!(state["game"]["ended"], true);
    }
}

#[test]
fn source_mutations_cannot_change_spell_head_actor_count_or_provenance() {
    const REJECTIONS: [&str; 4] = [
        "current_ability_closure_mismatch",
        "unsupported_construct",
        "current_ability_closure_mismatch",
        "invalid_current_source",
    ];
    // head moved, actor flipped (unsupported), count changed, provenance dropped.
    let (snapshot, keywords, accepted) = inputs();
    for (probe, rejection) in REJECTIONS.iter().enumerate() {
        let mut selection = accepted.clone();
        let author = selection["authors"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|author| author["input"]["card_no"] == "CP02-068")
            .unwrap();
        let candidate = &mut author["input"];
        match probe {
            0 => {
                let ability = &mut candidate["tree"]["cards"]["CP02-068"]["abilities"][0];
                let body = ability.as_object_mut().unwrap().remove("spell").unwrap();
                ability
                    .as_object_mut()
                    .unwrap()
                    .insert("attack_time".into(), body);
            }
            1 => {
                candidate["tree"]["cards"]["CP02-068"]["abilities"][0]["spell"]["do"][0]["draw"]
                    ["who"] = json!("opponent");
            }
            2 => {
                candidate["tree"]["cards"]["CP02-068"]["abilities"][0]["spell"]["do"][0]["draw"]
                    ["n"] = json!(2_u8);
            }
            // The last probe drops the source sidecar.
            _ => candidate["sidecar"] = json!([]),
        }
        assert!(
            failure(Catalog::from_current_selection(
                &snapshot, &keywords, &selection
            ))
            .contains(*rejection),
            "mutated input {probe} must be rejected for its own reason"
        );
    }
}
