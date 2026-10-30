#![expect(
    clippy::panic,
    reason = "Unexpected questions invalidate the printed event scenarios."
)]
use super::*;

fn selected() -> &'static SelectedEffects {
    static SELECTED: OnceLock<SelectedEffects> = OnceLock::new();
    SELECTED.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = producer_inputs();
        snapshot = snapshot.lines().map(|line| {
            let mut row: Value = serde_json::from_str(line).unwrap();
            if row["number"] == "producer" { row["faces"][0]["name"] = json!("movement producer"); }
            row.to_string()
        }).collect::<Vec<_>>().join("\n");
        snapshot.push('\n');
        let face = snapshot.lines().map(|line| serde_json::from_str::<Value>(line).unwrap())
            .find(|entry| entry["number"] == "BP01-037").unwrap()["faces"][0].clone();
        for (number, kind, traits) in [("movement-unit", "フォロワー", json!(["乗物"])),
            ("movement-other", "フォロワー", json!(["反例"])),
            ("movement-token", "フォロワー・トークン", json!(["乗物"])),
            ("movement-enemy", "フォロワー", json!(["反例"])), ("movement-amulet", "アミュレット", json!(["乗物"]))] {
            let mut material = face.clone();
            material["name"] = json!(if number == "movement-enemy" {"movement enemy"} else {"movement material"});
            material["text"] = Value::Null;
            material["sections"] = json!([]);
            material["card_type"] = json!(kind);
            material["traits"] = traits;
            material["power"] = json!(if kind.contains("フォロワー") {"10"} else {"-"});
            material["hp"] = json!(if kind.contains("フォロワー") {"20"} else {"-"});
            snapshot.push_str(&json!({"number":number,"faces":[material]}).to_string());
            snapshot.push('\n');
            input["documents"].as_array_mut().unwrap().push(json!([
                format!("{number}.yaml"),format!("format: 1\nkind: effect_set\ncards:\n  {number}:\n    status: complete\n    abilities: []\n")
            ]));
        }
        Catalog::from_current_selection(&snapshot, &keywords, &input).unwrap()
    })
}

pub(super) fn move_material(from: ZoneKind, to: OwnerZone) -> Action {
    move_named(from, to, "movement material")
}

pub(super) fn move_named(from: ZoneKind, to: OwnerZone, name: &str) -> Action {
    let mut action = movement(to);
    let Action::Move { objects, .. } = &mut action else {
        panic!("movement producer")
    };
    *objects = subjects_in(from);
    let Selector::Zone {
        binding, predicate, ..
    } = objects
    else {
        panic!("material selector")
    };
    **predicate = Expr::Compute(Box::new(Predicate::NamesIntersect {
        left: Expr::Live(Box::new(NamesRead::Names {
            object: ObjectView::Current {
                object: Box::new(ObjectOperand::Bound { binding: *binding }),
            },
        })),
        right: Expr::Literal(vec![CardName(name.into())]),
    }));
    action
}

pub(super) fn position(card: &str, active: &str, material: &str, side: &str, zone: &str) -> Value {
    let mut value = draw_position();
    value["turn"]["active"] = json!(active);
    value["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":card}]);
    value["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"movement-enemy"}]);
    let entry = json!({"id":"material","card":material});
    if value["players"][side]["zones"][zone].is_null() {
        value["players"][side]["zones"][zone] = json!([]);
    }
    value["players"][side]["zones"][zone]
        .as_array_mut()
        .unwrap()
        .push(entry);
    value
}

pub(super) fn prepared(value: &Value, action: Action) -> Game {
    let mut game = with_selected_producer(value, action, selected());
    let empty = WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![],
    })
    .unwrap();
    for number in [
        "movement-unit",
        "movement-other",
        "movement-token",
        "movement-enemy",
        "movement-amulet",
    ] {
        game.load_internal_card(number, &empty).unwrap();
    }
    game
}

pub(super) fn finish_checks(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}

pub(super) fn resolve_event(game: &mut Game, number: &str, count: usize) {
    let question = game.internal_pending_choice().unwrap();
    if count == 0 {
        assert!(question.is_none(), "the counterexample does not subscribe");
        return;
    }
    let question = question.unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("pending movement event")
    };
    assert_eq!(
        options.len(),
        count,
        "each successful movement contributes one occurrence"
    );
    assert!(
        options.iter().all(|option| option.card == number),
        "only the actual observer subscribes"
    );
    let mut play = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap();
    while let CardPlay::Awaiting(declaration) = play {
        let InputDomain::Targets(domain) = &declaration.domain else {
            panic!("formal enemy follower target")
        };
        let target = domain
            .members
            .iter()
            .find(|member| member.card.as_deref() == Some("movement-enemy"))
            .unwrap();
        play = game
            .answer_internal_card(
                declaration.actor,
                declaration.token,
                Answer::Targets(vec![target.handle]),
            )
            .unwrap();
    }
    assert!(
        !matches!(play, CardPlay::Cancelled),
        "the printed occurrence has a legal target"
    );
    finish_checks(game);
}

#[test]
fn bp05_076_observes_only_own_followers_buried_on_its_own_turn() {
    for (active, side, material, to, triggers) in [
        ("P1", "P1", "movement-unit", OwnerZone::Cemetery, true),
        ("P1", "P2", "movement-unit", OwnerZone::Cemetery, false),
        ("P2", "P1", "movement-unit", OwnerZone::Cemetery, false),
        ("P1", "P1", "movement-amulet", OwnerZone::Cemetery, false),
        ("P1", "P1", "movement-unit", OwnerZone::Hand, false),
    ] {
        let mut game = prepared(
            &position("BP05-076", active, material, side, "field"),
            move_material(ZoneKind::Field, to),
        );
        let before = game.projection(View::Referee).unwrap()["objects"]["source"]["power"]
            .as_u64()
            .unwrap();
        activate(&mut game, active);
        finish_checks(&mut game);
        resolve_event(&mut game, "BP05-076", usize::from(triggers));
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["source"]["power"],
            before + u64::from(triggers)
        );
    }
}

#[test]
fn bp09_026_observes_only_opposing_follower_cemetery_entries_on_its_turn() {
    for (active, side, expected) in [("P1", "P2", true), ("P1", "P1", false), ("P2", "P2", false)] {
        let mut game = prepared(
            &position("BP09-026", active, "movement-unit", side, "field"),
            move_material(ZoneKind::Field, OwnerZone::Cemetery),
        );
        let before = game.projection(View::Referee).unwrap();
        activate(&mut game, active);
        finish_checks(&mut game);
        resolve_event(&mut game, "BP09-026", usize::from(expected));
        let after = game.projection(View::Referee).unwrap();
        for stat in ["power", "hp"] {
            assert_eq!(
                after["objects"]["source"][stat],
                before["objects"]["source"][stat].as_u64().unwrap() + u64::from(expected)
            );
        }
    }
}

#[test]
fn bp11_008_printings_filter_vehicle_ex_entries_and_declare_damage_targets() {
    for card in ["BP11-008", "BP11-P03"] {
        for (active, side, material, expected) in [
            ("P1", "P1", "movement-unit", true),
            ("P1", "P1", "movement-other", false),
            ("P1", "P2", "movement-unit", false),
            ("P2", "P1", "movement-unit", false),
        ] {
            let mut game = prepared(
                &position(card, active, material, side, "field"),
                move_material(ZoneKind::Field, OwnerZone::Ex),
            );
            activate(&mut game, active);
            finish_checks(&mut game);
            resolve_event(&mut game, card, usize::from(expected));
            assert_eq!(
                game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
                if expected { 3_u8 } else { 0_u8 }
            );
        }
    }
}

#[test]
fn a_two_follower_batch_admits_two_occurrences_and_keeps_the_departure_controller() {
    let mut value = position("BP05-076", "P1", "movement-unit", "P1", "field");
    value["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"second","card":"movement-unit","owner":"P2"}));
    let mut game = prepared(&value, move_material(ZoneKind::Field, OwnerZone::Cemetery));
    let before = game.projection(View::Referee).unwrap()["objects"]["source"]["power"]
        .as_u64()
        .unwrap();
    activate(&mut game, "P1");
    finish_checks(&mut game);
    resolve_event(&mut game, "BP05-076", 2);
    resolve_event(&mut game, "BP05-076", 1);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["source"]["power"], before + 2);
    assert!(
        state["P2"]["cemetery"]
            .as_array()
            .unwrap()
            .contains(&json!("second")),
        "a stolen follower returns to its owner's cemetery while triggering its former controller's observer"
    );
}

#[test]
fn a_full_ex_has_no_successful_entry_and_no_vehicle_occurrence() {
    let mut value = position("BP11-008", "P1", "movement-unit", "P1", "field");
    value["players"]["P1"]["zones"]["ex"] = json!(
        (0_usize..5)
            .map(|i| json!({"id":format!("old-{i}"),"card":"movement-other"}))
            .collect::<Vec<_>>()
    );
    let mut game = prepared(&value, move_material(ZoneKind::Field, OwnerZone::Ex));
    activate(&mut game, "P1");
    finish_checks(&mut game);
    resolve_event(&mut game, "BP11-008", 0);
    let state = game.projection(View::Referee).unwrap();
    assert!(
        state["P1"]["field"]
            .as_array()
            .unwrap()
            .contains(&json!("material")),
        "failed placement leaves the original generation on the field"
    );
    assert_eq!(state["objects"]["enemy"]["damage"], 0_u8);
}

#[test]
fn an_observer_leaving_in_the_same_batch_still_sees_every_own_follower_exit() {
    let value = position("BP05-076", "P1", "movement-unit", "P1", "field");
    let mut action = move_material(ZoneKind::Field, OwnerZone::Cemetery);
    let Action::Move {
        objects: Selector::Zone {
            binding, predicate, ..
        },
        ..
    } = &mut action
    else {
        panic!("field movement")
    };
    **predicate = Expr::Compute(Box::new(Predicate::Not {
        predicate: Expr::Compute(Box::new(Predicate::NamesIntersect {
            left: Expr::Live(Box::new(NamesRead::Names {
                object: ObjectView::Current {
                    object: Box::new(ObjectOperand::Bound { binding: *binding }),
                },
            })),
            right: Expr::Literal(vec![CardName("movement producer".into())]),
        })),
    }));
    let mut game = prepared(&value, action);
    activate(&mut game, "P1");
    finish_checks(&mut game);
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("sealed exit occurrences")
    };
    assert_eq!(
        options.len(),
        2,
        "the departing observer sees itself and its own other follower before the batch"
    );
    assert!(
        options
            .iter()
            .all(|option| option.card == "BP05-076" && option.line == 2),
        "the actual printed observer supplies both occurrences"
    );
    let state = game.projection(View::Referee).unwrap();
    assert!(
        state["P1"]["cemetery"]
            .as_array()
            .unwrap()
            .contains(&json!("source")),
        "the observer has already left when its occurrences are offered"
    );
}

#[test]
fn a_token_follower_exit_triggers_before_the_token_is_erased() {
    let value = position("BP05-076", "P1", "movement-token", "P1", "field");
    let mut game = prepared(&value, move_material(ZoneKind::Field, OwnerZone::Cemetery));
    let before = game.projection(View::Referee).unwrap()["objects"]["source"]["power"]
        .as_u64()
        .unwrap();
    activate(&mut game, "P1");
    finish_checks(&mut game);
    resolve_event(&mut game, "BP05-076", 1);
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["objects"]["source"]["power"], before + 1);
    assert!(
        !after["P1"]["cemetery"]
            .as_array()
            .unwrap()
            .contains(&json!("material"))
    );
    assert!(
        !after["P1"]["field"]
            .as_array()
            .unwrap()
            .contains(&json!("material"))
    );
}

#[test]
fn two_vehicle_ex_entries_admit_two_independent_damage_occurrences() {
    let mut value = position("BP11-008", "P1", "movement-unit", "P1", "field");
    value["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"second","card":"movement-unit"}));
    let mut game = prepared(&value, move_material(ZoneKind::Field, OwnerZone::Ex));
    activate(&mut game, "P1");
    finish_checks(&mut game);
    resolve_event(&mut game, "BP11-008", 2);
    resolve_event(&mut game, "BP11-008", 1);
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["objects"]["enemy"]["damage"], 6_u8);
    assert_eq!(after["P1"]["ex"], json!(["material", "second"]));
}
