#![expect(
    clippy::panic,
    reason = "Unexpected printed decisions invalidate these effect-evolution scenarios."
)]
use super::*;
use sve_engine::game::internal::DecisionView;

fn initial(base: &str, form: &str, zone: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    initial["players"]["P1"]["zones"][zone] = json!([{"id":"source","card":base}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"first","card":"BP01-037"},{"id":"second","card":"BP01-037"},{"id":"third","card":"BP01-037"}
    ]);
    initial
}

fn enter(game: &mut Game) {
    let CardPlay::Entered(entry) = game
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("real follower entry");
    };
    if entry.resume == ResumeStage::CheckTiming {
        game.resume_internal_card().unwrap();
    }
}

fn pending(game: &mut Game, printing: &str) -> CardPlay {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("pending");
    };
    assert_eq!(options.len(), 1, "only this printed ability is pending");
    assert_eq!(
        options[0].card, printing,
        "the actual base or evolved printing supplies the source"
    );
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::Pending(options[0].handle),
    )
    .unwrap()
}

fn question(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(question) = play else {
        panic!("expected a printed choice");
    };
    question
}

fn evolve(game: &mut Game, play: CardPlay, form: &str) {
    let question = question(play);
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("evolution form");
    };
    let form_handle = domain
        .members
        .iter()
        .find(|member| member.card.as_deref() == Some(form))
        .map(|member| member.handle)
        .unwrap();
    assert!(
        game.internal_decision(View::P2).is_none(),
        "the opponent cannot inspect the private evolution deck"
    );
    let before = format!("{game:?}");
    game.answer_internal_card(
        PlayerId { seat: 2 },
        question.token,
        Answer::Targets(vec![form_handle]),
    )
    .unwrap_err();
    assert_eq!(
        format!("{game:?}"),
        before,
        "foreign answers preserve complete authority"
    );
    let activation = resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Targets(vec![form_handle]),
        )
        .unwrap(),
    );
    assert_eq!(
        activation
            .progress
            .facts
            .iter()
            .filter(|fact| matches!(fact, Fact::Evolved(_)))
            .count(),
        1,
        "the effect evolves exactly once"
    );
    assert!(
        !activation
            .progress
            .facts
            .iter()
            .any(|fact| matches!(fact, Fact::CardPlayed { .. })),
        "effect evolution does not play another card"
    );
    game.resume_internal_card().unwrap();
}

fn discard_first(game: &mut Game, play: CardPlay, count: usize) {
    let question = question(play);
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("printed discard");
    };
    assert_eq!(
        domain.members.len(),
        count,
        "the cards were drawn before discarding"
    );
    resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Targets(vec![domain.members[0].handle]),
        )
        .unwrap(),
    );
    game.resume_internal_card().unwrap();
}

#[test]
fn bp22_100_family_full_field_evolves_without_four_pp_then_real_101_draws_and_discards() {
    for (base, form) in [("BP22-100", "BP22-101"), ("BP22-P55", "BP22-P56")] {
        let mut initial = initial(base, form, "hand");
        initial["players"]["P1"]["zones"]["field"] = json!([
            {"id":"amulet","card":"BP01-016"},{"id":"ally1","card":"BP01-037"},
            {"id":"ally2","card":"BP01-037"},{"id":"ally3","card":"BP22-100"}
        ]);
        initial["players"]["P1"]["zones"]["evolve_deck"].as_array_mut().unwrap().push(
            json!({"id":"spare-form","card":if form == "BP22-101" { "BP22-P56" } else { "BP22-101" }})
        );
        let mut game = game(&initial);
        enter(&mut game);
        let play = pending(&mut game, base);
        evolve(&mut game, play, form);
        let evolved = game.projection(View::Referee).unwrap();
        assert_eq!(evolved["objects"]["source"]["power"], 4_u8);
        assert_eq!(evolved["objects"]["source"]["hp"], 4_u8);
        assert_eq!(evolved["P1"]["pp"]["current"], 8_u8);
        let evolved_play = pending(&mut game, form);
        discard_first(&mut game, evolved_play, 2);
        let after = game.projection(View::Referee).unwrap();
        assert_eq!(after["P1"]["hand"], json!(["second"]));
        assert_eq!(after["P1"]["cemetery"], json!(["first"]));
        assert_eq!(after["P1"]["deck"], json!(["third"]));
        assert_eq!(after["P1"]["pp"]["current"], 8_u8);
        assert_eq!(game.internal_rule_action(&json!({"do":"evolve","source":"ally3","evolve_card":"spare-form","pay":{"pp":4_u8}})).unwrap().outcome, "resolved");
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
            4_u8,
            "effect evolution leaves the normal evolution quota available"
        );
    }
}

#[test]
fn bp22_042_family_banishes_two_witch_cards_then_real_043_evolves_and_draws() {
    for (base, form) in [("BP22-042", "BP22-043"), ("BP22-P20", "BP22-P21")] {
        let mut initial = initial(base, form, "hand");
        initial["players"]["P1"]["zones"]["cemetery"] = json!([
            {"id":"wizard1","card":"BP02-046"},{"id":"wizard2","card":"BP08-042"},
            {"id":"other-class","card":"BP01-037"}
        ]);
        let mut game = game(&initial);
        enter(&mut game);
        let offered = question(pending(&mut game, base));
        assert_eq!(offered.domain, InputDomain::OptionalCost);
        let materials = question(
            game.answer_internal_card(offered.actor, offered.token, Answer::OptionalCost(true))
                .unwrap(),
        );
        let InputDomain::CostMaterials(domain) = &materials.domain else {
            panic!("graveyard materials");
        };
        assert_eq!(domain.members.len(), 2);
        let handles = domain
            .members
            .iter()
            .map(|member| member.handle)
            .collect::<Vec<_>>();
        let before = format!("{game:?}");
        game.answer_internal_card(
            materials.actor,
            materials.token,
            Answer::CostMaterials(vec![handles[0]]),
        )
        .unwrap_err();
        assert_eq!(format!("{game:?}"), before);
        let play = game
            .answer_internal_card(
                materials.actor,
                materials.token,
                Answer::CostMaterials(handles),
            )
            .unwrap();
        let paid = game.projection(View::Referee).unwrap();
        assert_eq!(paid["P1"]["banish"].as_array().unwrap().len(), 2);
        assert_eq!(paid["P1"]["cemetery"], json!(["other-class"]));
        evolve(&mut game, play, form);
        resolved(pending(&mut game, form));
        game.resume_internal_card().unwrap();
        let after = game.projection(View::Referee).unwrap();
        assert_eq!(after["objects"]["source"]["power"], 3_u8);
        assert_eq!(after["objects"]["source"]["hp"], 2_u8);
        assert_eq!(after["P1"]["hand"], json!(["first"]));
        assert_eq!(after["P1"]["pp"]["current"], 8_u8);
    }
}

#[test]
fn bp20_046_family_uses_actual_entry_zone_and_real_047_target_damage_draw_discard() {
    for (base, form, zone) in [
        ("BP20-046", "BP20-047", "hand"),
        ("BP20-P22", "BP20-P23", "ex"),
    ] {
        let mut initial = initial(base, form, zone);
        initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
        let mut game = game(&initial);
        enter(&mut game);
        let play = pending(&mut game, base);
        if zone == "hand" {
            resolved(play);
            game.resume_internal_card().unwrap();
            assert!(game.internal_pending_choice().unwrap().is_none());
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["objects"]["source"]["power"], 3_u8);
            assert_eq!(state["P1"]["evolve_deck"], json!(["form"]));
            continue;
        }
        evolve(&mut game, play, form);
        let target = question(pending(&mut game, form));
        let InputDomain::Targets(domain) = &target.domain else {
            panic!("real evolved target");
        };
        assert_eq!(domain.members.len(), 1);
        let target_play = game
            .answer_internal_card(
                target.actor,
                target.token,
                Answer::Targets(vec![domain.members[0].handle]),
            )
            .unwrap();
        discard_first(&mut game, target_play, 1);
        let after = game.projection(View::Referee).unwrap();
        assert_eq!(after["objects"]["enemy"]["damage"], 3_u8);
        assert_eq!(after["objects"]["source"]["power"], 4_u8);
        assert_eq!(after["P1"]["hand"], json!([]));
        assert_eq!(after["P1"]["cemetery"], json!(["first"]));
        assert_eq!(after["P1"]["pp"]["current"], 7_u8);
    }
}
