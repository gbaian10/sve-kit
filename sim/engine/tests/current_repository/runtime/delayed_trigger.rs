#![expect(
    clippy::panic,
    reason = "Unexpected questions invalidate delayed event scenarios."
)]
use super::event_cemetery::{finish_checks, move_named, prepared, prepared_program};
use super::event_combat_life::life;
use super::event_subject_capture::production;
use super::*;

fn position(source: &str) -> Value {
    let mut value = draw_position();
    value["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":source}]);
    value["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"draw1","card":"movement-unit"}, {"id":"draw2","card":"movement-unit"},
        {"id":"draw3","card":"movement-unit"}, {"id":"draw4","card":"movement-unit"}
    ]);
    value["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"movement-enemy"}]);
    value
}
fn target(board: &mut Game, play: CardPlay) {
    let CardPlay::Awaiting(question) = play else {
        panic!("formal target");
    };
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("target domain");
    };
    let handle = domain
        .members
        .iter()
        .find(|member| member.card.as_deref() == Some("movement-enemy"))
        .unwrap()
        .handle;
    resolved(
        board
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Targets(vec![handle]),
            )
            .unwrap(),
    );
    finish_checks(board);
}
fn pending_count(board: &mut Game, count: usize) {
    for left in (1..=count).rev() {
        let question = board.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = &question.domain else {
            panic!("installed pending");
        };
        assert_eq!(
            options.len(),
            left,
            "one installed occurrence is consumed per callback"
        );
        resolved(
            board
                .answer_internal_card(
                    question.actor,
                    question.token,
                    Answer::Pending(options[0].handle),
                )
                .unwrap(),
        );
        finish_checks(board);
    }
    assert!(
        board.internal_pending_choice().unwrap().is_none(),
        "no additional installed occurrence remains"
    );
}

#[test]
fn bp15_001_installed_token_creation_survives_its_source_leaving() {
    for alias in ["BP15-001", "BP15-SL01", "BP15-U01"] {
        assert!(selection().current.contains_key(alias));
    }
    let mut board = prepared_program(
        &position("BP15-001"),
        &production(vec![
            move_named(
                ZoneKind::Field,
                OwnerZone::Cemetery,
                "絶命の不殺・エズディア",
            ),
            move_named(ZoneKind::Field, OwnerZone::Cemetery, "movement enemy"),
        ]),
    );
    let activated = selection().current["BP15-001"]
        .program
        .card()
        .abilities
        .iter()
        .find(|ability| ability.node.kind == AbilityKind::Activated)
        .unwrap()
        .node
        .id;
    let play = board
        .activate_internal_card("source", activated, PlayerId { seat: 1 })
        .unwrap();
    target(&mut board, play);
    activate(&mut board, "P1");
    finish_checks(&mut board);
    pending_count(&mut board, 1);
    let state = board.projection(View::Referee).unwrap();
    assert!(
        state["P1"]["cemetery"]
            .as_array()
            .unwrap()
            .contains(&json!("source"))
    );
    assert_eq!(state["P1"]["ex"].as_array().unwrap().len(), 1);
    let token = state["P1"]["ex"][0].as_str().unwrap();
    assert_eq!(state["objects"][token]["name"], "絶命の痛撃");
}

#[test]
fn bp15_008_two_spell_installations_both_draw_after_the_spells_leave_resolution() {
    for alias in ["BP15-008", "BP15-P01"] {
        assert!(selection().current.contains_key(alias));
    }
    let mut initial = position("BP15-001");
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"spell1","card":"BP15-008"}, {"id":"spell2","card":"BP15-P01"}
    ]);
    initial["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"printed_observer","card":"BP09-026"}));
    let mut board = prepared(
        &initial,
        move_named(ZoneKind::Field, OwnerZone::Cemetery, "movement enemy"),
    );
    for id in ["spell1", "spell2"] {
        let play = board.play_internal_card(id, PlayerId { seat: 1 }).unwrap();
        target(&mut board, play);
    }
    assert_eq!(
        board.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        10_u8,
        "the printed Ezudia discount applies to both spells"
    );
    activate(&mut board, "P1");
    finish_checks(&mut board);
    pending_count(&mut board, 3);
    assert_eq!(
        board.projection(View::Referee).unwrap()["P1"]["hand"],
        json!(["draw1", "draw2"])
    );
}

#[test]
fn bp15_008_installs_before_its_own_lethal_damage_reaches_the_check_timing() {
    let mut initial = position("BP15-001");
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"spell","card":"BP15-008"}]);
    initial["players"]["P2"]["zones"]["field"][0]["state"]["damage"] = json!(19_u8);
    let mut board = prepared(&initial, life(PlayerRef::Controller {}, 0));
    let play = board
        .play_internal_card("spell", PlayerId { seat: 1 })
        .unwrap();
    target(&mut board, play);
    pending_count(&mut board, 1);
    assert_eq!(
        board.projection(View::Referee).unwrap()["P1"]["hand"],
        json!(["draw1"])
    );
}

#[test]
fn bp15_008_expires_at_the_actual_turn_end_before_the_target_enters_cemetery() {
    let mut initial = position("BP15-001");
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"spell","card":"BP15-008"}]);
    initial["players"]["P2"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"next_producer","card":"producer"}));
    initial["players"]["P2"]["zones"]["deck"] = json!([
        {"id":"next_draw","card":"movement-unit"}, {"id":"next_tail","card":"movement-unit"}
    ]);
    let mut board = prepared(
        &initial,
        move_named(ZoneKind::Field, OwnerZone::Cemetery, "movement enemy"),
    );
    let play = board
        .play_internal_card("spell", PlayerId { seat: 1 })
        .unwrap();
    target(&mut board, play);
    board
        .internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    finish_checks(&mut board);
    if board.projection(View::Referee).unwrap()["turn"]["active"] == "P1" {
        if let Some(question) = board.internal_quick_choice().unwrap() {
            board
                .answer_internal_card(question.actor, question.token, Answer::Quick(None))
                .unwrap();
            finish_checks(&mut board);
        }
        if board.projection(View::Referee).unwrap()["turn"]["active"] == "P1" {
            board
                .internal_rule_action(&json!({"do":"end-discard","select":[]}))
                .unwrap();
            finish_checks(&mut board);
        }
    }
    assert_eq!(
        board.projection(View::Referee).unwrap()["turn"]["active"],
        "P2"
    );
    resolved(
        board
            .activate_internal_card("next_producer", AbilityId(1), PlayerId { seat: 2 })
            .unwrap(),
    );
    finish_checks(&mut board);
    pending_count(&mut board, 0);
    let state = board.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!([]));
    assert!(
        state["P2"]["cemetery"]
            .as_array()
            .unwrap()
            .contains(&json!("enemy"))
    );
}

#[test]
fn bp15_012_installs_even_without_three_hunters_and_heals_after_its_source_leaves() {
    for alias in ["BP15-012", "BP15-P02"] {
        assert!(selection().current.contains_key(alias));
    }
    for hunters in [false, true] {
        let mut initial = position("BP15-001");
        initial["players"]["P1"]["zones"]["field"] = json!([]);
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP15-012"}]);
        initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
        if hunters {
            initial["players"]["P1"]["zones"]["cemetery"] = json!([
                {"id":"h1","card":"BP15-008"}, {"id":"h2","card":"BP15-008"}, {"id":"h3","card":"BP15-008"}
            ]);
        }
        let mut board = prepared_program(
            &initial,
            &production(vec![
                move_named(ZoneKind::Field, OwnerZone::Cemetery, "絶命の崇拝者"),
                move_named(ZoneKind::Field, OwnerZone::Cemetery, "movement enemy"),
            ]),
        );
        assert!(matches!(
            board
                .play_internal_card("source", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ));
        finish_checks(&mut board);
        let question = board.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = &question.domain else {
            panic!("fanfare");
        };
        assert_eq!(options.len(), 1);
        let play = board
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap();
        target(&mut board, play);
        assert_eq!(
            board.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
            if hunters { 2_u8 } else { 0_u8 }
        );
        activate(&mut board, "P1");
        finish_checks(&mut board);
        pending_count(&mut board, 1);
        assert_eq!(
            board.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
            12_u8
        );
    }
}
