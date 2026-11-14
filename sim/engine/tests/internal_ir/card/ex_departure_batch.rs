use super::*;
use sve_engine::game::internal::{InputDomain, ResumeStage};
fn finish(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}
fn observer() -> WellFormedCardProgram {
    lowerer().lower(&equipment_played::tree(&json!({"auto":{
        "on":{"moved":{"from":{"who":"self","zone":"ex"},"during":{"turn":"self"}}},
        "aggregation":"batch","per_turn":1,"do":[{"life":{"who":"self","delta":{"event":"amount"}}}]
    }}),"unit-amulet")).unwrap().remove("unit-amulet").unwrap()
}
#[test]
fn ex_departure_batch_counts_only_successful_own_members_and_preserves_a_departed_observer() {
    let observer = observer();
    for (own_turn, destination) in [
        (false, OwnerZone::Cemetery),
        (true, OwnerZone::Cemetery),
        (true, OwnerZone::Ex),
    ] {
        let producer = card(vec![ability(
            1,
            vec![movement::put(
                1,
                Selector::Union {
                    sets: vec![zone(ZoneKind::Ex, 31), zone(ZoneKind::Field, 32)],
                },
                destination,
            )],
        )]);

        let (snapshot, registry, document) = catalog_documents(&json!("1"));
        let snapshot = snapshot.replace("スペル", "スペル・トークン");
        let catalog = Arc::new(
            Catalog::from_documents(&snapshot, &registry, &[("unit.yaml".into(), document)])
                .unwrap(),
        );
        let mut initial = setup(1, json!([]));
        initial["turn"]["active"] = json!("P1");
        initial["players"]["P1"]["zones"]["field"] =
            json!([{"id":"producer","card":"unit-follower"}]);
        let observer_player = if own_turn { "P1" } else { "P2" };
        initial["players"][observer_player]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"observer","card":"unit-amulet","owner":"P2"}));
        initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"own-a","card":"unit-spell"},{"id":"own-b","card":"unit-follower","owner":if destination==OwnerZone::Cemetery {"P2"} else {"P1"}}]);
        initial["players"]["P2"]["zones"]["ex"] = json!([{"id":"foreign","card":"unit-follower"}]);
        let mut game =
            Game::new(catalog, &initial, &Value::Null, &Value::Null, "ex-batch").unwrap();
        game.load_internal_card("unit-amulet", &observer).unwrap();
        game.load_internal_card("unit-follower", &producer).unwrap();
        game.load_internal_card("unit-spell", &card(vec![]))
            .unwrap();
        resolved(
            game.activate_internal_card("producer", AbilityId(1), PlayerId { seat: 1 })
                .unwrap(),
        );
        finish(&mut game);
        let question = game.internal_pending_choice().unwrap();
        assert_eq!(
            question.is_some(),
            own_turn && destination == OwnerZone::Cemetery,
            "the departure is restricted to the observer controller's turn"
        );
        if destination == OwnerZone::Cemetery {
            assert!(
                game.projection(View::Referee).unwrap()["P1"]["ex"]
                    .as_array()
                    .unwrap()
                    .is_empty(),
                "EX is already empty before resolving its captured amount"
            );
        }
        if let Some(question) = question {
            let InputDomain::Pending(options) = &question.domain else {
                panic!("EX batch trigger");
            };
            assert_eq!(
                options.len(),
                1,
                "the batch produces one occurrence even with two successful members"
            );
            resolved(
                game.answer_internal_card(
                    question.actor,
                    question.token,
                    Answer::Pending(options[0].handle),
                )
                .unwrap(),
            );
            finish(&mut game);
            assert_eq!(
                game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
                22_u8,
                "two own departures include token erasure and exclude foreign EX and field departures"
            );
            assert!(
                game.internal_pending_choice().unwrap().is_none(),
                "a departed observer resolves its single captured occurrence"
            );
        }
    }
}

#[test]
fn ex_departure_batch_includes_a_formal_play_from_ex() {
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"observer","card":"unit-amulet"}]);
    initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"played","card":"unit-follower"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([]);
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "ex-play-batch",
    )
    .unwrap();
    game.load_internal_card("unit-amulet", &observer()).unwrap();
    game.load_internal_card("unit-follower", &card(vec![]))
        .unwrap();
    assert!(
        matches!(
            game.play_internal_card("played", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ),
        "the ordinary EX play commits its movement"
    );
    finish(&mut game);
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("played EX departure");
    };
    assert_eq!(
        options.len(),
        1,
        "one formal play is one successful EX batch member"
    );
    resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap(),
    );
    finish(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        21_u8,
        "the frozen departure count is one"
    );
}
