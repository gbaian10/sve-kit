#![expect(
    clippy::panic,
    reason = "Unexpected choices invalidate the printed Stack scenarios."
)]
use super::*;

fn initial(card: &str) -> Value {
    let mut value = draw_position();
    value["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    value["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"drawn","card":"BP01-037"},{"id":"remaining","card":"BP01-037"}
    ]);
    value["players"]["P2"]["zones"]["field"] = json!([
        {"id":"enemy","card":"BP01-037"}
    ]);
    value
}

fn enter_and_trigger(game: &mut Game, card: &str) {
    let CardPlay::Entered(entry) = game
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("printed amulet entry");
    };
    let entered = game.projection(View::Referee).unwrap();
    assert_eq!(
        entered["objects"]["source"]["counters"]["スタックカウンター"], 1_u8,
        "Stack enters with one counter"
    );
    if entry.resume == ResumeStage::CheckTiming {
        game.resume_internal_card().unwrap();
    }
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("Fanfare pending choice");
    };
    assert_eq!(options.len(), 1, "only the printed Fanfare is pending");
    assert_eq!(
        options[0].card, card,
        "the actual printing supplies its Fanfare"
    );
    let mut play = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap();
    if let CardPlay::Awaiting(target) = play {
        let InputDomain::Targets(domain) = &target.domain else {
            panic!("Fanfare damage target");
        };
        assert_eq!(
            domain.members.len(),
            1,
            "only one opposing follower is legal"
        );
        play = game
            .answer_internal_card(
                target.actor,
                target.token,
                Answer::Targets(vec![domain.members[0].handle]),
            )
            .unwrap();
    }
    resolved(play);
    game.resume_internal_card().unwrap();
}

#[test]
fn bp01_075_stack_enters_with_one_counter_and_draws_without_a_reminder_trigger() {
    let mut game = game(&initial("BP01-075"));
    enter_and_trigger(&mut game, "BP01-075");
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["hand"], json!(["drawn"]));
    assert_eq!(view["P1"]["field"], json!(["source"]));
    assert!(game.internal_pending_choice().unwrap().is_none());
}

#[test]
fn bp01_074_and_its_reprint_enter_stack_and_create_the_printed_rush_golem() {
    for card in ["BP01-074", "PR-007"] {
        let mut game = game(&initial(card));
        enter_and_trigger(&mut game, card);
        let view = game.projection(View::Referee).unwrap();
        let field = view["P1"]["field"].as_array().unwrap();
        assert_eq!(field.len(), 2);
        let id = field
            .iter()
            .find(|id| id.as_str() != Some("source"))
            .unwrap()
            .as_str()
            .unwrap();
        assert_eq!(view["objects"][id]["name"], "攻撃型ゴーレム");
        assert_eq!(view["objects"][id]["power"], 3_u8);
        assert_eq!(view["objects"][id]["hp"], 2_u8);
        assert_eq!(
            view["objects"][id]["counters"]["スタックカウンター"],
            Value::Null
        );
    }
}

#[test]
fn bp02_051_and_its_reprint_enter_stack_then_damage_the_declared_follower() {
    for card in ["BP02-051", "PR-022"] {
        let mut game = game(&initial(card));
        enter_and_trigger(&mut game, card);
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["objects"]["enemy"]["damage"], 3_u8);
        assert_eq!(
            view["objects"]["source"]["counters"]["スタックカウンター"],
            1_u8
        );
    }
}

#[test]
fn stack_departure_replaces_the_move_and_zero_cleanup_is_not_destruction() {
    let mut value = initial("BP01-075");
    value["players"]["P1"]["zones"]["hand"] = json!([]);
    value["players"]["P1"]["zones"]["field"] = json!([
        {"id":"source","card":"BP01-075","state":{"counters":{"stack_counter":2_u8}}}
    ]);
    value["players"]["P2"]["zones"]["field"] = json!([]);
    let mut game = with_producer(
        &value,
        Action::Destroy {
            objects: subjects(),
        },
    );
    let generation =
        game.projection(View::Referee).unwrap()["objects"]["source"]["generation"].clone();
    let first = activate(&mut game, "P1");
    assert!(!first.progress.facts.iter().any(|fact| matches!(fact, Fact::Movement(m) if m.method == MoveMethod::Destroy && m.movement.before != first.source.holder)));
    game.resume_internal_card().unwrap();
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["objects"]["source"]["generation"], generation);
    assert_eq!(
        view["objects"]["source"]["counters"]["スタックカウンター"],
        1_u8
    );
    assert_eq!(view["P1"]["field"], json!(["source"]));
    let mut bounced = with_producer(&value, movement(OwnerZone::Hand));
    activate(&mut bounced, "P1");
    bounced.resume_internal_card().unwrap();
    let opponent = bounced.projection(View::P2).unwrap();
    assert_eq!(opponent["objects"]["source"]["name"], "雄大なる教え");
    assert_eq!(
        opponent["objects"]["source"]["counters"]["スタックカウンター"],
        1_u8
    );
    let mut full_ex = value.clone();
    full_ex["players"]["P1"]["zones"]["ex"] = json!([
        {"id":"e0","card":"BP01-037"},{"id":"e1","card":"BP01-037"},
        {"id":"e2","card":"BP01-037"},{"id":"e3","card":"BP01-037"},
        {"id":"e4","card":"BP01-037"}
    ]);
    let mut full_ex = with_producer(&full_ex, movement(OwnerZone::Ex));
    activate(&mut full_ex, "P1");
    full_ex.resume_internal_card().unwrap();
    let ex_view = full_ex.projection(View::Referee).unwrap();
    assert_eq!(
        ex_view["objects"]["source"]["counters"]["スタックカウンター"],
        1_u8
    );
    assert_eq!(ex_view["P1"]["ex"].as_array().unwrap().len(), 5);
    assert_eq!(ex_view["P1"]["field"], json!(["source", "producer"]));
    // A second independent effect consumes the last counter, then CR 11.7 moves it.
    let mut value = value.clone();
    value["players"]["P1"]["zones"]["field"][0]["state"]["counters"]["stack_counter"] = json!(1_u8);
    let mut zeroed = with_producer(&value, movement(OwnerZone::Hand));
    let second = activate(&mut zeroed, "P1");
    assert!(!second.progress.facts.iter().any(
        |fact| matches!(fact, Fact::Movement(m) if m.movement.requested_to.kind == ZoneKind::Hand && m.movement.before != second.source.holder)
    ));
    zeroed.resume_internal_card().unwrap();
    let zero_view = zeroed.projection(View::Referee).unwrap();
    assert_eq!(zero_view["P1"]["cemetery"], json!(["source"]));
    assert_eq!(zero_view["P1"]["hand"], json!(["producer"]));
    assert!(zeroed.internal_pending_choice().unwrap().is_none());
}

#[test]
fn ordinary_amulet_has_no_stack_counter_or_stack_departure_replacement() {
    let mut value = initial("BP01-041");
    value["players"]["P1"]["zones"]["hand"] = json!([]);
    value["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP01-041"}]);
    value["players"]["P2"]["zones"]["field"] = json!([]);
    let mut game = with_producer(&value, movement(OwnerZone::Hand));
    activate(&mut game, "P1");
    game.resume_internal_card().unwrap();
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["hand"], json!(["source", "producer"]));
    assert_eq!(
        view["objects"]["source"]["counters"]["スタックカウンター"],
        Value::Null
    );
}
