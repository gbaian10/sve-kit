#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate these printed Look scenarios."
)]
use super::*;
use sve_engine::game::internal::DecisionView;

fn question(card: &str, deck: Value) -> (Game, DecisionView) {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    initial["players"]["P1"]["zones"]["deck"] = deck;
    question_from(&initial, card)
}

fn question_from(initial: &Value, card: &str) -> (Game, DecisionView) {
    let mut board = game(initial);
    assert!(
        matches!(
            board
                .play_internal_card("source", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ),
        "the follower enters before its printed fanfare is offered"
    );
    while board.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        board.resume_internal_card().unwrap();
    }
    let pending = board.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("fanfare pending")
    };
    let handle = options
        .iter()
        .find(|option| option.card == card)
        .unwrap()
        .handle;
    let CardPlay::Awaiting(choice) = board
        .answer_internal_card(pending.actor, pending.token, Answer::Pending(handle))
        .unwrap()
    else {
        panic!("grouped Look")
    };
    assert!(
        board.internal_decision(View::P2).is_none(),
        "only the controller can inspect the Look choice"
    );
    (board, choice)
}

fn handles(choice: &DecisionView, card: &str) -> Vec<u64> {
    let InputDomain::Targets(domain) = &choice.domain else {
        panic!("private targets")
    };
    domain
        .members
        .iter()
        .filter(|member| member.card.as_deref() == Some(card))
        .map(|member| member.handle)
        .collect()
}

fn settle(board: &mut Game) {
    while board.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        board.resume_internal_card().unwrap();
    }
}

fn finish(board: &mut Game, choice: &DecisionView, picked: Vec<u64>) {
    let play = board
        .answer_internal_card(choice.actor, choice.token, Answer::Targets(picked))
        .unwrap();
    match play {
        CardPlay::Awaiting(order) => {
            let InputDomain::Order(cards) = &order.domain else {
                panic!("bottom order")
            };
            let reverse = cards.iter().rev().map(|card| card.handle).collect();
            resolved(
                board
                    .answer_internal_card(order.actor, order.token, Answer::Order(reverse))
                    .unwrap(),
            );
        }
        CardPlay::Resolved(_) => (),
        CardPlay::Entered(_)
        | CardPlay::Cancelled
        | CardPlay::Continued(_)
        | CardPlay::GameEnded(_) => panic!("Look must finish after ordering its remainder"),
    }
    settle(board);
}

#[test]
fn bp11_086_keeps_one_arid_and_one_cheap_follower_and_rejects_two_only_cheap_cards() {
    for alias in ["BP11-086", "BP11-SL16", "BP11-U06"] {
        assert!(selection().current.contains_key(alias));
    }
    let (mut board, choice) = question(
        "BP11-086",
        json!([
            {"id":"arid","card":"BP11-086"},
            {"id":"cheap_a","card":"BP01-034"},
            {"id":"cheap_b","card":"BP01-034"},
            {"id":"wrong","card":"BP01-037"},
            {"id":"tail","card":"BP01-006"}
        ]),
    );
    assert_eq!(handles(&choice, "BP01-037"), Vec::<u64>::new());
    let cheap = handles(&choice, "BP01-034");
    board
        .answer_internal_card(choice.actor, choice.token, Answer::Targets(cheap.clone()))
        .unwrap_err();
    assert!(board.internal_decision(View::P2).is_none());
    finish(
        &mut board,
        &choice,
        vec![handles(&choice, "BP11-086")[0], cheap[0]],
    );
    let state = board.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["ex"], json!(["arid", "cheap_a"]));
    assert_eq!(state["P1"]["deck"], json!(["tail", "wrong", "cheap_b"]));
}

#[test]
fn bp21_048_takes_only_an_academy_follower_and_spell_into_ex() {
    for alias in ["BP21-048", "BP21-P24"] {
        assert!(selection().current.contains_key(alias));
    }
    let (mut board, choice) = question(
        "BP21-048",
        json!([
            {"id":"follower","card":"BP21-048"},
            {"id":"spell","card":"BP21-012"},
            {"id":"wrong_follower","card":"BP01-034"},
            {"id":"wrong_spell","card":"BP01-006"},
            {"id":"tail","card":"BP01-034"}
        ]),
    );
    assert_eq!(handles(&choice, "BP01-034"), Vec::<u64>::new());
    assert_eq!(handles(&choice, "BP01-006"), Vec::<u64>::new());
    finish(
        &mut board,
        &choice,
        vec![
            handles(&choice, "BP21-048")[0],
            handles(&choice, "BP21-012")[0],
        ],
    );
    let state = board.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["ex"], json!(["follower", "spell"]));
    assert_eq!(
        state["P1"]["deck"],
        json!(["tail", "wrong_spell", "wrong_follower"])
    );
}

#[test]
fn ecp02_034_assigns_an_overlapping_trait_card_once_and_reveals_only_taken_cards() {
    for alias in ["ECP02-034", "ECP02-P18"] {
        assert!(selection().current.contains_key(alias));
    }
    let (mut board, choice) = question(
        "ECP02-034",
        json!([
            {"id":"all_traits","card":"ECP02-072"},
            {"id":"cool","card":"ECP02-003"},
            {"id":"cute","card":"ECP02-034"},
            {"id":"tail","card":"BP01-034"}
        ]),
    );
    let all = handles(&choice, "ECP02-072")[0];
    board
        .answer_internal_card(choice.actor, choice.token, Answer::Targets(vec![all, all]))
        .unwrap_err();
    let public_before = board.projection(View::P2).unwrap();
    assert!(
        !public_before["known_cards"]
            .as_array()
            .unwrap()
            .iter()
            .any(|card| card["id"] == "all_traits")
    );
    finish(
        &mut board,
        &choice,
        vec![
            all,
            handles(&choice, "ECP02-003")[0],
            handles(&choice, "ECP02-034")[0],
        ],
    );
    let state = board.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["all_traits", "cool", "cute"]));
    assert_eq!(state["P1"]["deck"], json!(["tail"]));
    let public = board.projection(View::P2).unwrap();
    let known = public["known_cards"].as_array().unwrap();
    for id in ["all_traits", "cool", "cute"] {
        assert!(known.iter().any(|card| card["id"] == id));
    }
    assert!(!known.iter().any(|card| card["id"] == "tail"));
}

#[test]
fn bp12_037_reveals_wizard_groups_then_moves_a_hand_spell_to_ex_with_a_turn_discount() {
    for alias in ["BP12-037", "BP12-SL11", "BP12-U03", "EBD02-002"] {
        assert!(selection().current.contains_key(alias));
    }
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP12-037"}]);
    initial["players"]["P1"]["zones"]["cemetery"] = json!(
        (0_u8..5)
            .map(|n| json!({"id":format!("grave{n}"),"card":"BP01-062"}))
            .collect::<Vec<_>>()
    );
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"wizard","card":"BP01-062"},
        {"id":"spell","card":"BP01-065"},
        {"id":"wrong","card":"BP01-034"},
        {"id":"tail","card":"BP01-006"}
    ]);
    let (mut board, choice) = question_from(&initial, "BP12-037");
    assert_eq!(handles(&choice, "BP01-034"), Vec::<u64>::new());
    finish(
        &mut board,
        &choice,
        vec![
            handles(&choice, "BP01-062")[0],
            handles(&choice, "BP01-065")[0],
        ],
    );
    let activation = selection().current["BP12-037"]
        .program
        .card()
        .abilities
        .iter()
        .find(|entry| entry.node.kind == AbilityKind::Activated)
        .unwrap()
        .node
        .id;
    let CardPlay::Awaiting(hand) = board
        .activate_internal_card("source", activation, PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("hand spell choice")
    };
    assert!(board.internal_decision(View::P2).is_none());
    let picked = handles(&hand, "BP01-065");
    assert_eq!(picked.len(), 1);
    resolved(
        board
            .answer_internal_card(hand.actor, hand.token, Answer::Targets(picked))
            .unwrap(),
    );
    settle(&mut board);
    let before = board.projection(View::Referee).unwrap();
    assert_eq!(before["P1"]["ex"], json!(["spell"]));
    assert_eq!(before["objects"]["source"]["acted"], true);
    assert_eq!(before["P1"]["pp"]["current"], 7_u8);
    resolved(
        board
            .play_internal_card("spell", PlayerId { seat: 1 })
            .unwrap(),
    );
    let after = board.projection(View::Referee).unwrap();
    assert_eq!(
        after["P1"]["pp"]["current"], 7_u8,
        "the cost-three spell costs zero this turn"
    );
    assert_eq!(after["P1"]["hand"], json!(["wizard", "tail", "wrong"]));
}

fn resolve_pris(board: &mut Game) {
    settle(board);
    let pending = board.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("played-card trigger")
    };
    assert_eq!(options.len(), 1, "only Pris observes the later Wizard play");
    let CardPlay::Awaiting(targets) = board
        .answer_internal_card(
            pending.actor,
            pending.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap()
    else {
        panic!("formal damage target")
    };
    let InputDomain::Targets(domain) = &targets.domain else {
        panic!("enemy follower")
    };
    assert_eq!(
        domain.members.len(),
        1,
        "the single opposing follower is the only legal target"
    );
    resolved(
        board
            .answer_internal_card(
                targets.actor,
                targets.token,
                Answer::Targets(vec![domain.members[0].handle]),
            )
            .unwrap(),
    );
    settle(board);
}

#[test]
fn ebd02_003_discounts_both_cost_limited_groups_and_observes_later_wizard_plays() {
    for alias in [
        "EBD02-003",
        "SP01-019",
        "SP01-SL19",
        "SP01-SP11",
        "SP01-SP12",
        "SP01-U10",
    ] {
        assert!(selection().current.contains_key(alias));
    }
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"EBD02-003"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"wizard","card":"BP01-062"},
        {"id":"spell","card":"BP01-065"},
        {"id":"expensive","card":"EBD02-003"},
        {"id":"wrong_follower","card":"BP01-034"},
        {"id":"wrong_spell","card":"BP01-006"},
        {"id":"tail_a","card":"BP01-006"},
        {"id":"tail_b","card":"BP01-034"}
    ]);
    let (mut board, choice) = question_from(&initial, "EBD02-003");
    for card in ["EBD02-003", "BP01-034", "BP01-006"] {
        assert_eq!(handles(&choice, card), Vec::<u64>::new());
    }
    finish(
        &mut board,
        &choice,
        vec![
            handles(&choice, "BP01-062")[0],
            handles(&choice, "BP01-065")[0],
        ],
    );
    assert!(
        board.internal_pending_choice().unwrap().is_none(),
        "Pris never observes its own entry play"
    );
    resolved(
        board
            .play_internal_card("spell", PlayerId { seat: 1 })
            .unwrap(),
    );
    resolve_pris(&mut board);
    assert!(matches!(
        board
            .play_internal_card("wizard", PlayerId { seat: 1 })
            .unwrap(),
        CardPlay::Entered(_)
    ));
    resolve_pris(&mut board);
    let state = board.projection(View::Referee).unwrap();
    assert_eq!(
        state["P1"]["pp"]["current"], 4_u8,
        "each successful EX move receives its own cost reduction"
    );
    assert_eq!(state["objects"]["enemy"]["hp"], 1_u8);
    assert_eq!(
        state["P1"]["deck"],
        json!(["wrong_spell", "wrong_follower", "expensive"])
    );
}
