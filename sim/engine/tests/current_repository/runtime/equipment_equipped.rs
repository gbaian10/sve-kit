#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate the printed equipment scenarios."
)]
use super::event_cemetery::finish_checks;
use super::union_burst_batch1::{finish, pending};
use super::*;
use sve_engine::ir::TokenTemplateId;

#[test]
fn all_shefi_printings_equip_ice_dragon_sword_and_resolve_its_own_trigger() {
    for printing in ["CP04-055", "CP04-SL13", "PCS01-022", "PR-511"] {
        let initial = shefi_position(printing, 7, false);
        let mut board = Game::from_selected_effects(
            selection_with_rules().clone(),
            &initial,
            &Value::Null,
            &Value::Null,
            "equipment-event",
        )
        .unwrap();
        assert!(matches!(
            board
                .play_internal_card("shefi", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ));
        finish_checks(&mut board);
        let play = pending(&mut board, printing);
        resolved(finish(&mut board, play));
        finish_checks(&mut board);
        let equipped = board.projection(View::Referee).unwrap();
        assert_eq!(equipped["P1"]["pp"]["current"], 5_u8);
        assert_eq!(equipped["objects"]["shefi"]["power"], 2_u8);
        let equipment = equipped["P1"]["equipment"][0].as_str().unwrap();
        assert_eq!(equipped["objects"][equipment]["equipped_to"], "shefi");
        let trigger = pending(&mut board, "CP04-T08");
        resolved(finish(&mut board, trigger));
        finish_checks(&mut board);
        let after = board.projection(View::Referee).unwrap();
        assert_eq!(after["objects"]["enemy"]["acted"], true);
        assert_eq!(after["objects"]["shefi"]["power"], 4_u8);
        assert_eq!(after["objects"]["shefi"]["max_hp"], 4_u8);
        assert!(board.internal_pending_choice().unwrap().is_none());
    }
}

#[test]
fn initial_equipment_associations_do_not_create_an_equipped_event_for_any_printing() {
    for printing in ["CP04-T08", "CP04-PR22", "CP04-PR34"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["leader"]["class"] = json!("ドラゴン");
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"shefi","card":"CP04-055"}]);
        initial["players"]["P1"]["zones"]["equipment"] =
            json!([{"id":"sword","card":printing,"state":{"equipped_to":"shefi"}}]);
        initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
        let mut board = Game::from_selected_effects(
            selection_with_rules().clone(),
            &initial,
            &Value::Null,
            &Value::Null,
            "initial-equipment-event",
        )
        .unwrap();
        finish_checks(&mut board);
        let after = board.projection(View::Referee).unwrap();
        assert_eq!(after["objects"]["sword"]["equipped_to"], "shefi");
        assert_eq!(after["objects"]["shefi"]["power"], 2_u8);
        assert_eq!(after["objects"]["shefi"]["max_hp"], 2_u8);
        assert_eq!(after["objects"]["enemy"]["acted"], false);
        assert!(board.internal_pending_choice().unwrap().is_none());
    }
}

fn equip_step() -> Action {
    Action::Equip {
        holder: ObjectOperand::SourceHolder {},
        template: TokenTemplateId("氷竜剣".into()),
        count: Nat(1),
    }
}

fn return_host() -> Action {
    Action::Move {
        objects: Selector::Objects {
            objects: vec![ObjectOperand::SourceHolder {}],
        },
        movement: MoveSpec {
            to: MoveDestination::Owners {
                zone: OwnerZone::Hand,
            },
            initial_posture: None,
            entry_effects: vec![],
            position: None,
            order: None,
        },
        reason: MoveReason::Put,
    }
}

fn instruction_board(actions: Vec<Action>) -> Game {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"producer","card":"producer"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    let mut board = Game::from_selected_effects(
        producer_selection().clone(),
        &initial,
        &Value::Null,
        &Value::Null,
        "equipped-instructions",
    )
    .unwrap();
    board
        .load_internal_card("producer", &producer_steps(actions))
        .unwrap();
    board
}

#[test]
fn a_queued_equipment_trigger_survives_its_erasure_and_keeps_the_old_host_generation() {
    let mut board = instruction_board(vec![equip_step(), return_host()]);
    let equip = activate(&mut board, "P1");
    let equipment = equip
        .progress
        .facts
        .iter()
        .find_map(|fact| {
            if let Fact::Equipped(event) = fact {
                Some(event.equipment)
            } else {
                None
            }
        })
        .unwrap();
    finish_checks(&mut board);
    let before = board.projection(View::Referee).unwrap();
    assert_eq!(before["P1"]["equipment"], json!([]));
    assert_eq!(before["P1"]["hand"], json!(["producer"]));
    let play = pending(&mut board, "CP04-T08");
    let resolved_trigger = resolved(finish(&mut board, play));
    assert_eq!(resolved_trigger.source.holder, equipment);
    finish_checks(&mut board);
    let after = board.projection(View::Referee).unwrap();
    assert_eq!(after["objects"]["enemy"]["acted"], true);
    assert_eq!(after["objects"]["producer"]["power"], 3_u8);
    assert_eq!(after["objects"]["producer"]["max_hp"], 3_u8);
    assert!(board.internal_pending_choice().unwrap().is_none());
}

#[test]
fn q2539_a_sword_without_a_legal_enemy_target_cannot_buff_its_host() {
    let mut initial = draw_position();
    initial["players"]["P1"]["leader"]["class"] = json!("ドラゴン");
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"shefi","card":"CP04-055"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([]);
    let mut board = Game::from_selected_effects(
        selection_with_rules().clone(),
        &initial,
        &Value::Null,
        &Value::Null,
        "equipment-no-target",
    )
    .unwrap();
    assert!(matches!(
        board
            .play_internal_card("shefi", PlayerId { seat: 1 })
            .unwrap(),
        CardPlay::Entered(_)
    ));
    let fanfare = pending(&mut board, "CP04-055");
    resolved(finish(&mut board, fanfare));
    finish_checks(&mut board);
    assert!(matches!(
        pending(&mut board, "CP04-T08"),
        CardPlay::Cancelled
    ));
    finish_checks(&mut board);
    let after = board.projection(View::Referee).unwrap();
    assert_eq!(after["P1"]["equipment"].as_array().unwrap().len(), 1);
    assert_eq!(after["objects"]["shefi"]["power"], 2_u8);
    assert_eq!(after["objects"]["shefi"]["max_hp"], 2_u8);
    assert!(board.internal_pending_choice().unwrap().is_none());
}

#[test]
fn two_new_equipment_instances_each_trigger_once_and_can_act_an_already_acted_target() {
    let mut board = instruction_board(vec![equip_step(), equip_step()]);
    let creating = activate(&mut board, "P1");
    assert_eq!(
        creating
            .progress
            .facts
            .iter()
            .filter(|fact| matches!(fact, Fact::Equipped(_)))
            .count(),
        2
    );
    finish_checks(&mut board);
    let first = pending(&mut board, "CP04-T08");
    let first = resolved(finish(&mut board, first));
    finish_checks(&mut board);
    let second = pending(&mut board, "CP04-T08");
    let second = resolved(finish(&mut board, second));
    assert_ne!(first.source.instance, second.source.instance);
    assert_ne!(first.source.holder, second.source.holder);
    finish_checks(&mut board);
    let after = board.projection(View::Referee).unwrap();
    assert_eq!(after["P1"]["equipment"].as_array().unwrap().len(), 2);
    assert_eq!(after["objects"]["producer"]["power"], 7_u8);
    assert_eq!(after["objects"]["producer"]["max_hp"], 7_u8);
    assert_eq!(after["objects"]["enemy"]["acted"], true);
    assert!(board.internal_pending_choice().unwrap().is_none());
}

#[test]
fn an_invalid_host_produces_neither_equipment_nor_an_equipped_trigger() {
    let mut board = instruction_board(vec![return_host(), equip_step()]);
    let result = activate(&mut board, "P1");
    assert_eq!(
        result
            .progress
            .facts
            .iter()
            .filter(|fact| matches!(fact, Fact::Equipped(_)))
            .count(),
        0
    );
    finish_checks(&mut board);
    let after = board.projection(View::Referee).unwrap();
    assert_eq!(after["P1"]["hand"], json!(["producer"]));
    assert_eq!(after["P1"]["equipment"], json!([]));
    assert_eq!(after["objects"]["enemy"]["acted"], false);
    assert!(board.internal_pending_choice().unwrap().is_none());
}

#[test]
fn a_later_failure_rolls_back_the_equipment_its_association_and_its_queued_trigger() {
    let overflow = Action::Life {
        player: PlayerRef::Controller {},
        change: NumericChange::Add {
            amount: Expr::Compute(Box::new(IntOperation::Arithmetic {
                operation: Arithmetic::Add,
                arguments: vec![Expr::Literal(Int(i64::MAX)), Expr::Literal(Int(1))],
            })),
        },
    };
    let mut board = instruction_board(vec![equip_step(), overflow]);
    let before = board.projection(View::Referee).unwrap();
    let error = board
        .activate_internal_card("producer", AbilityId(1), PlayerId { seat: 1 })
        .unwrap_err();
    assert!(error.to_string().contains("overflow"));
    assert_eq!(board.projection(View::Referee).unwrap(), before);
    assert!(board.internal_pending_choice().unwrap().is_none());
}

fn shefi_position(printing: &str, maximum: u8, title: bool) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["leader"]["class"] = json!("ドラゴン");
    initial["players"]["P1"]["pp"] = json!({"current":maximum,"max":maximum});
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"shefi","card":printing}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    if title {
        initial["players"]["P1"]["construction"] = json!("title");
        initial["players"]["P1"]["title_code"] = json!("princess_connect_re_dive");
    }
    for player in ["P1", "P2"] {
        initial["players"][player]["zones"]["deck"] = json!(
            (0_u8..4)
                .map(|n| json!({"id":format!("{player}-draw{n}"),"card":"BP01-037"}))
                .collect::<Vec<_>>()
        );
    }
    initial
}

fn actual(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "shefi-complete",
    )
    .unwrap()
}

fn resolve_first(board: &mut Game, printing: &str) -> Activation {
    let play = pending(board, printing);
    let result = resolved(finish(board, play));
    finish_checks(board);
    result
}

fn resolve_shefi_line(board: &mut Game, line: u64) -> Activation {
    finish_checks(board);
    let question = board.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("printed Shefi triggers");
    };
    assert_eq!(
        options
            .iter()
            .filter(|option| option.card == "CP04-055")
            .count(),
        2,
        "both printed Fanfare abilities must be available before choosing their order"
    );
    let handle = options
        .iter()
        .find(|option| option.card == "CP04-055" && option.line == line)
        .unwrap()
        .handle;
    let play = board
        .answer_internal_card(question.actor, question.token, Answer::Pending(handle))
        .unwrap();
    let result = resolved(finish(board, play));
    finish_checks(board);
    result
}

#[test]
fn shefi_below_awakening_does_not_equip_or_create_an_attachment_trigger() {
    let mut board = actual(&shefi_position("CP04-055", 6, false));
    assert!(matches!(
        board
            .play_internal_card("shefi", PlayerId { seat: 1 })
            .unwrap(),
        CardPlay::Entered(_)
    ));
    resolve_first(&mut board, "CP04-055");
    let after = board.projection(View::Referee).unwrap();
    assert_eq!(after["P1"]["pp"]["current"], 4_u8);
    assert_eq!(after["P1"]["equipment"], json!([]));
    assert_eq!(after["objects"]["shefi"]["power"], 2_u8);
    assert_eq!(after["objects"]["enemy"]["acted"], false);
    assert!(board.internal_pending_choice().unwrap().is_none());
}

#[test]
fn q2527_both_fanfare_orders_preserve_shefis_ub_until_only_the_next_opposing_start() {
    for equipment_first in [false, true] {
        let mut board = actual(&shefi_position("CP04-055", 10, true));
        assert!(matches!(
            board
                .play_internal_card("shefi", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ));
        if equipment_first {
            resolve_shefi_line(&mut board, 2);
            resolve_first(&mut board, "CP04-T08");
            union_burst_batch1::ub(&resolve_first(&mut board, "CP04-055"));
        } else {
            union_burst_batch1::ub(&resolve_shefi_line(&mut board, 1));
            resolve_first(&mut board, "CP04-055");
            resolve_first(&mut board, "CP04-T08");
        }
        let after = board.projection(View::Referee).unwrap();
        assert_eq!(after["P1"]["pp"]["current"], 8_u8);
        assert_eq!(after["objects"]["shefi"]["power"], 4_u8);
        assert_eq!(after["objects"]["enemy"]["acted"], true);
        assert!(board.internal_pending_choice().unwrap().is_none());
        union_burst_batch2::end(&mut board);
        assert_eq!(
            board.projection(View::Referee).unwrap()["objects"]["enemy"]["acted"],
            true
        );
        union_burst_batch2::end(&mut board);
        union_burst_batch2::end(&mut board);
        let later = board.projection(View::Referee).unwrap();
        assert_eq!(later["objects"]["enemy"]["acted"], false);
        assert_eq!(later["objects"]["shefi"]["power"], 4_u8);
    }
}
