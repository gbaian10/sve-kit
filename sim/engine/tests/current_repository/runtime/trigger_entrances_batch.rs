#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Only the explicitly expected public decisions belong to these scenarios."
)]
use super::*;
use sve_engine::game::internal::SearchAnswer;

fn board(initial: &Value) -> Game {
    event_cemetery::prepared(
        initial,
        event_combat_life::life(PlayerRef::Controller {}, -1),
    )
}

fn checks(board: &mut Game) {
    while board.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        board.resume_internal_card().unwrap();
    }
}

fn finish(board: &mut Game, mut play: CardPlay, mode: Option<u64>) {
    while let CardPlay::Awaiting(question) = play {
        let answer = match &question.domain {
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            InputDomain::OptionalCost => Answer::OptionalCost(true),
            InputDomain::Targets(domain) => Answer::Targets(
                domain
                    .members
                    .iter()
                    .take(usize::try_from(domain.cardinality.minimum.0).unwrap())
                    .map(|m| m.handle)
                    .collect(),
            ),
            InputDomain::CostMaterials(domain) => Answer::CostMaterials(
                domain
                    .members
                    .iter()
                    .take(usize::try_from(domain.cardinality.minimum.0).unwrap())
                    .map(|m| m.handle)
                    .collect(),
            ),
            InputDomain::Search(domain) => Answer::Search(SearchAnswer::Ungrouped(
                domain
                    .cards
                    .first()
                    .map(|c| vec![c.handle])
                    .unwrap_or_default(),
            )),
            InputDomain::Modes { modes, .. } => {
                let chosen = ModeId(mode.unwrap());
                assert!(modes.contains(&chosen), "mode {chosen:?} is still offered");
                Answer::Modes(vec![chosen])
            }
            _ => panic!("unexpected batch decision"),
        };
        play = board
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    assert!(
        !matches!(play, CardPlay::Cancelled),
        "the expected decisions complete the play"
    );
    checks(board);
}

fn pending(board: &mut Game, number: &str, mode: Option<u64>) {
    checks(board);
    let question = board.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("pending");
    };
    assert_eq!(options.len(), 1, "one pending occurrence at a time");
    assert_eq!(
        options[0].card, number,
        "the batch card is the pending source"
    );
    let play = board
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap();
    finish(board, play, mode);
}

fn definition(number: &str) -> AbilityId {
    selection().current[number]
        .program
        .card()
        .abilities
        .iter()
        .find(|a| a.node.kind == AbilityKind::Activated)
        .unwrap()
        .node
        .id
}

#[test]
fn bp04_105_fanfare_searches_an_amulet_after_paying_its_printed_cost() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP04-105"}]);
    initial["players"]["P1"]["zones"]["deck"] =
        json!([{"id":"amulet","card":"BP04-104"},{"id":"follower","card":"BP01-037"}]);
    let mut board = board(&initial);
    let play = board
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap();
    finish(&mut board, play, None);
    pending(&mut board, "BP04-105", None);
    let state = board.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["amulet"]));
    assert_eq!(state["P1"]["pp"]["current"], 8_u8);
}

#[test]
fn cp02_092_quick_declares_its_cemetery_target_before_sacrificing_the_source() {
    let mut initial = draw_position();
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"lily","card":"CP02-092"}]);
    initial["players"]["P2"]["zones"]["cemetery"] = json!([{"id":"friend","card":"CP02-001"}]);
    let mut board = board(&initial);
    board
        .internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    let question = board.internal_quick_choice().unwrap().unwrap();
    let InputDomain::Quick { options, .. } = &question.domain else {
        panic!("Quick");
    };
    assert_eq!(options.len(), 1);
    let CardPlay::Awaiting(target) = board
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Quick(Some(options[0].handle)),
        )
        .unwrap()
    else {
        panic!("formal target before costs");
    };
    assert!(matches!(target.domain, InputDomain::Targets(_)));
    let before = board.projection(View::Referee).unwrap();
    assert_eq!(before["P2"]["pp"]["current"], 10_u8);
    assert_eq!(before["P2"]["field"], json!(["lily"]));
    finish(&mut board, CardPlay::Awaiting(target), None);
    let state = board.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["hand"], json!(["friend"]));
    assert_eq!(state["P2"]["cemetery"], json!(["lily"]));
    assert_eq!(state["P2"]["pp"]["current"], 8_u8);
}

#[test]
fn bp10_047_ex_end_phase_draws_then_discards_the_preexisting_hand_card() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"formula","card":"BP10-047"}]);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"old","card":"BP01-037"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"drawn","card":"BP01-037"}]);
    let mut board = board(&initial);
    board
        .internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    pending(&mut board, "BP10-047", None);
    let state = board.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["drawn"]));
    assert_eq!(state["P1"]["cemetery"], json!(["old"]));
    assert_eq!(state["P1"]["ex"], json!(["formula"]));
}

#[test]
fn bp10_110_ex_end_phase_damages_the_opposing_leader_and_followers_together() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"world","card":"BP10-110"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"movement-enemy"}]);
    let mut board = board(&initial);
    board
        .internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    pending(&mut board, "BP10-110", None);
    let state = board.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["leader"]["life"], 16_u8);
    assert_eq!(state["objects"]["enemy"]["hp"], 16_u8);
    assert_eq!(state["P1"]["ex"], json!(["world"]));
}

#[test]
fn bp19_085_consumes_four_life_loss_triggers_and_ignores_the_fifth() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"source","card":"BP19-085"},{"id":"producer","card":"producer"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"movement-enemy"}]);
    let mut board = board(&initial);
    for n in 0_u8..5 {
        let play = board
            .activate_internal_card("producer", AbilityId(1), PlayerId { seat: 1 })
            .unwrap();
        finish(&mut board, play, None);
        if n < 4 {
            pending(&mut board, "BP19-085", None);
        } else {
            assert!(board.internal_pending_choice().unwrap().is_none());
        }
    }
    let state = board.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["leader"]["life"], 15_u8);
    assert_eq!(state["objects"]["enemy"]["hp"], 16_u8);
}

#[test]
fn ecp02_022_cute_requirement_and_once_limit_preserve_payment_and_discount_the_next_spell() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"ECP02-021","state":{"evolved":true,"evolved_with":"form","power":3_u8,"hp":3_u8,"max_hp":3_u8}}]);
    initial["players"]["P1"]["zones"]["evolution"] = json!([{"id":"form","card":"ECP02-022"}]);
    initial["players"]["P1"]["zones"]["ex"] =
        json!([{"id":"lesson1","card":"CP02-T01"},{"id":"lesson2","card":"CP02-T01"}]);
    initial["players"]["P1"]["zones"]["cemetery"] = json!(
        (0_u8..5)
            .map(|n| json!({"id":format!("cute{n}"),"card":"CP02-034"}))
            .collect::<Vec<_>>()
    );
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"spell","card":"CP02-034"}]);
    let mut board = board(&initial);
    let activation = board
        .activate_internal_card("source", definition("ECP02-022"), PlayerId { seat: 1 })
        .unwrap();
    finish(&mut board, activation, None);
    let before = board.projection(View::Referee).unwrap();
    assert_eq!(before["P1"]["pp"]["current"], 9_u8);
    assert_eq!(before["P1"]["ex"].as_array().unwrap().len(), 1);
    let error = board
        .activate_internal_card("source", definition("ECP02-022"), PlayerId { seat: 1 })
        .unwrap_err();
    assert!(matches!(error, EngineFailure::IllegalPlay(message) if message.contains("quota")));
    assert_eq!(board.projection(View::Referee).unwrap(), before);
    let play = board
        .play_internal_card("spell", PlayerId { seat: 1 })
        .unwrap();
    finish(&mut board, play, None);
    let state = board.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["pp"]["current"], 9_u8);
    assert_eq!(state["objects"]["source"]["power"], 4_u8);
}

#[test]
fn bp22_092_uses_turn_ordinal_for_payment_and_resolves_fanfare_before_end_phase() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"curse","card":"BP22-092"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"movement-enemy"}]);
    let mut board = board(&initial);
    let play = board
        .play_internal_card("curse", PlayerId { seat: 1 })
        .unwrap();
    finish(&mut board, play, None);
    pending(&mut board, "BP22-092", None);
    let played = board.projection(View::Referee).unwrap();
    assert_eq!(played["P1"]["pp"]["current"], 4_u8);
    assert_eq!(played["objects"]["enemy"]["power"], 5_u8);
    assert_eq!(played["objects"]["enemy"]["hp"], 15_u8);
    board
        .internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    pending(&mut board, "BP22-092", None);
    let state = board.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["leader"]["life"], 22_u8);
    assert_eq!(state["P2"]["leader"]["life"], 18_u8);
}

#[test]
fn bp19_020_keeps_turn_local_modes_separate_while_playing_three_treasures() {
    let mut initial = draw_position();
    initial["players"]["P1"]["pp"]["current"] = json!(6_u8);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP19-019","state":{"evolved":true,"evolved_with":"form","power":4_u8,"hp":4_u8,"max_hp":4_u8}}]);
    initial["players"]["P1"]["zones"]["evolution"] = json!([{"id":"form","card":"BP19-020"}]);
    initial["players"]["P1"]["zones"]["ex"] = json!(
        (1_u8..=3)
            .map(|n| json!({"id":format!("flag{n}"),"card":"BP19-P33"}))
            .collect::<Vec<_>>()
    );
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"movement-enemy"}]);
    initial["players"]["P2"]["zones"]["deck"] =
        json!([{"id":"milled1","card":"BP01-037"},{"id":"milled2","card":"BP01-037"}]);
    let mut board = board(&initial);
    for mode in 1..=3 {
        let play = board
            .play_internal_card(&format!("flag{mode}"), PlayerId { seat: 1 })
            .unwrap();
        finish(&mut board, play, None);
        pending(&mut board, "BP19-020", Some(mode));
    }
    let state = board.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["pp"]["current"], 8_u8);
    assert_eq!(
        state["P2"]["cemetery"],
        json!(["enemy", "milled1", "milled2"])
    );
    assert_eq!(state["objects"]["source"]["power"], 7_u8);
}
