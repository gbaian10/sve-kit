use super::event_cemetery::finish_checks;
use super::union_burst_batch1::{finish, pending, question, ub};
use super::*;
use sve_engine::game::internal::{InstructionOutcome, SearchAnswer};

#[test]
fn croche_caps_battery_at_seven_and_uses_one_x_for_pp_counters_and_search() {
    let mut initial = draw_position();
    initial["players"]["P1"]["construction"] = json!("title");
    initial["players"]["P1"]["title_code"] = json!("princess_connect_re_dive");
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"CP04-115"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"partner","card":"CP04-112"},{"id":"same","card":"CP04-P87"},{"id":"other","card":"BP01-037"}
    ]);
    let mut game = Game::from_selected_effects(
        selection().clone(),
        &initial,
        &Value::Null,
        &Value::Null,
        "croche",
    )
    .unwrap();
    assert!(matches!(
        game.play_internal_card("source", PlayerId { seat: 1 })
            .unwrap(),
        CardPlay::Entered(_)
    ));
    ub(&resolved(pending(&mut game, "CP04-115")));
    finish_checks(&mut game);
    let before = game.projection(View::Referee).unwrap();
    assert_eq!(
        before["objects"]["source"]["counters"]["バッテリーカウンター"],
        7_u8
    );
    let ability = selection().current["CP04-115"]
        .program
        .card()
        .abilities
        .iter()
        .find(|ability| ability.node.kind == AbilityKind::Activated)
        .unwrap()
        .node
        .id;
    let q = question(
        game.activate_internal_card("source", ability, PlayerId { seat: 1 })
            .unwrap(),
    );
    assert_eq!(
        q.domain,
        InputDomain::Integer(Cardinality {
            minimum: Nat(0),
            maximum: Some(Nat(7))
        })
    );
    game.answer_internal_card(q.actor, q.token, Answer::Integer(Nat(8)))
        .unwrap_err();
    let search = question(
        game.answer_internal_card(q.actor, q.token, Answer::Integer(Nat(7)))
            .unwrap(),
    );
    let InputDomain::Search(domain) = &search.domain else {
        panic!("X search");
    };
    assert_eq!(domain.cards.len(), 1);
    assert_eq!(domain.cards[0].card, "CP04-112");
    resolved(
        game.answer_internal_card(
            search.actor,
            search.token,
            Answer::Search(SearchAnswer::Ungrouped(vec![domain.cards[0].handle])),
        )
        .unwrap(),
    );
    finish_checks(&mut game);
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["P1"]["pp"]["current"], 2_u8);
    assert_eq!(
        after["objects"]["source"]["counters"]["バッテリーカウンター"],
        0_u8
    );
    assert_eq!(after["objects"]["source"]["acted"], true);
    let partner = pending(&mut game, "CP04-112");
    ub(&resolved(finish(&mut game, partner)));
}

#[test]
fn beauty_and_beast_keeps_paid_counter_x_and_gets_storm_only_outside_hand() {
    for from in ["hand", "ex"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["leader"]["class"] = json!("エルフ");
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"][from] = json!([{"id":"source","card":"BP03-001","state":if from=="ex" {json!({"counters":{"童話カウンター":2_u8}})} else {json!({})}}]);
        let mut game = Game::from_selected_effects(
            selection().clone(),
            &initial,
            &Value::Null,
            &Value::Null,
            "beauty-counter",
        )
        .unwrap();
        assert!(matches!(
            game.play_internal_card("source", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ));
        resolved(pending(&mut game, "BP03-001"));
        finish_checks(&mut game);
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(
            view["objects"]["source"]["keywords"]
                .as_array()
                .unwrap()
                .contains(&json!("疾走")),
            from == "ex"
        );
        let count = if from == "ex" { 3_u8 } else { 1_u8 };
        assert_eq!(
            view["objects"]["source"]["counters"]["童話カウンター"],
            count
        );
        let ability = selection().current["BP03-001"]
            .program
            .card()
            .abilities
            .iter()
            .find(|ability| ability.node.kind == AbilityKind::Activated)
            .unwrap()
            .node
            .id;
        let q = question(
            game.activate_internal_card("source", ability, PlayerId { seat: 1 })
                .unwrap(),
        );
        assert_eq!(
            q.domain,
            InputDomain::Integer(Cardinality {
                minimum: Nat(0),
                maximum: Some(Nat(u64::from(count)))
            })
        );
        resolved(
            game.answer_internal_card(q.actor, q.token, Answer::Integer(Nat(u64::from(count))))
                .unwrap(),
        );
        let after = game.projection(View::Referee).unwrap();
        assert_eq!(after["objects"]["source"]["power"], 5_u8 + count);
        assert_eq!(after["objects"]["source"]["hp"], 7_u8 + count);
        assert_eq!(
            after["objects"]["source"]["counters"]["童話カウンター"],
            0_u8
        );
        assert_eq!(after["P1"]["pp"]["current"], 3_u8);
    }
}

#[test]
fn a_full_battery_still_records_the_union_burst_when_no_counter_is_added() {
    let mut initial = draw_position();
    initial["players"]["P1"]["construction"] = json!("title");
    initial["players"]["P1"]["title_code"] = json!("princess_connect_re_dive");
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["ex"] = json!([
        {"id":"source","card":"CP04-115","state":{"counters":{"バッテリーカウンター":7_u8}}}
    ]);
    let mut game = Game::from_selected_effects(
        selection().clone(),
        &initial,
        &Value::Null,
        &Value::Null,
        "croche-full",
    )
    .unwrap();
    assert!(matches!(
        game.play_internal_card("source", PlayerId { seat: 1 })
            .unwrap(),
        CardPlay::Entered(_)
    ));
    let result = resolved(pending(&mut game, "CP04-115"));
    ub(&result);
    assert!(result.progress.receipts.values().any(|receipt|matches!(receipt, InstructionOutcome::Counter(change) if change.before==Nat(7)&&change.after==Nat(7))));
    finish_checks(&mut game);
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(
        after["objects"]["source"]["counters"]["バッテリーカウンター"],
        7_u8
    );
}
