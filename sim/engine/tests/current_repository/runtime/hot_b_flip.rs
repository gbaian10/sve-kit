#![expect(
    clippy::panic,
    reason = "The scenario must expose the printed target or mode question."
)]
use super::event_cemetery::finish_checks;
use super::*;
use sve_engine::game::internal::DecisionView;

const CHEVAL: &[&str] = &[
    "BP22-R21",
    "BP22-SL33",
    "ECP01-019",
    "ECP01-SL09",
    "ECP01-SP09",
    "ECP01-SSP09",
];
const GUIDE: &[&str] = &["ECP01-055", "ECP01-SL25"];

fn initial() -> Value {
    json!({"history":"explicit","turn":{"active":"P1","first_player":"P1","phase":"main","elapsed_turns":{"P1":3,"P2":3}},"players":{
        "P1":{"leader":{"class":"ニュートラル","life":20},"pp":{"current":10,"max":10},"ep":0,"sep":0,"zones":{
            "evolve_deck":[{"id":"carrot-a","card":"CP01-085","face_up":true},{"id":"carrot-b","card":"CP01-085","face_up":true},{"id":"down","card":"CP01-085","face_up":false}],
            "deck":[{"id":"draw","card":"BP01-171"},{"id":"padding","card":"BP01-171"}]}},
        "P2":{"leader":{"class":"ニュートラル","life":20},"pp":{"current":10,"max":10},"ep":0,"sep":0,"zones":{}}
    }})
}
fn guide_ability(number: &str) -> AbilityId {
    selection_with_rules().current[number]
        .program
        .card()
        .abilities
        .iter()
        .find(|ability| ability.node.kind == AbilityKind::Activated && ability.source.line == 2)
        .unwrap()
        .node
        .id
}

fn board(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "real-face-down",
    )
    .unwrap()
}
fn waiting(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(question) = play else {
        panic!("printed choice")
    };
    question
}
fn choose(board: &mut Game, question: &DecisionView, handles: Vec<u64>) -> CardPlay {
    board
        .answer_internal_card(question.actor, question.token, Answer::Targets(handles))
        .unwrap()
}
fn pending_source(board: &mut Game, number: &str) -> CardPlay {
    finish_checks(board);
    let question = board.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("pending fanfare")
    };
    let option = options.iter().find(|option| option.card == number).unwrap();
    board
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(option.handle),
        )
        .unwrap()
}

#[test]
fn real_cheval_all_printings_select_the_carrot_before_testing_the_ten_uma_condition() {
    for number in CHEVAL {
        for graves in [9_u8, 10] {
            let mut initial = initial();
            initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":number}]);
            initial["players"]["P1"]["zones"]["cemetery"] = json!(
                (0..graves)
                    .map(|i| json!({"id":format!("grave-{i}"),"card":"ECP01-028"}))
                    .collect::<Vec<_>>()
            );
            let mut board = board(&initial);
            assert!(matches!(
                board
                    .play_internal_card("source", PlayerId { seat: 1 })
                    .unwrap(),
                CardPlay::Entered(_)
            ));
            let question = waiting(pending_source(&mut board, number));
            let InputDomain::Targets(domain) = &question.domain else {
                panic!("face-up carrots")
            };
            assert_eq!(domain.members.len(), 2);
            assert_eq!(domain.cardinality.minimum, Nat(1));
            resolved(choose(
                &mut board,
                &question,
                vec![domain.members[0].handle],
            ));
            let state = board.projection(View::Referee).unwrap();
            assert_eq!(state["objects"]["carrot-a"]["face_up"], graves < 10);
            assert_eq!(state["objects"]["carrot-b"]["face_up"], true);
            assert_eq!(state["P1"]["pp"]["current"], 8_u8);
        }
    }
}

#[test]
fn real_mass_production_flips_the_chosen_form_and_draws_but_cannot_play_without_a_face_up_target() {
    for face_up in [false, true] {
        let mut initial = initial();
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP06-117"}]);
        initial["players"]["P1"]["zones"]["evolve_deck"] =
            json!([{"id":"form","card":"BP03-021","face_up":face_up}]);
        let mut board = board(&initial);
        let before = format!("{board:?}");
        if !face_up {
            board
                .play_internal_card("source", PlayerId { seat: 1 })
                .unwrap_err();
            assert!(
                format!("{board:?}") == before,
                "Q875: no payment or draw without a legal target"
            );
            continue;
        }
        let question = waiting(
            board
                .play_internal_card("source", PlayerId { seat: 1 })
                .unwrap(),
        );
        resolved(choose(&mut board, &question, vec![0]));
        let state = board.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["form"]["face_up"], false);
        assert_eq!(state["P1"]["hand"], json!(["draw"]));
        assert_eq!(state["P1"]["pp"]["current"], 9_u8);
        assert!(
            board.projection(View::P2).unwrap()["objects"]
                .get("form")
                .is_none()
        );
    }
}

#[test]
fn real_guide_printings_flip_zero_or_two_carrots_and_always_gain_one_ep() {
    for number in GUIDE {
        for count in [0, 2] {
            let mut initial = initial();
            initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":number}]);
            let mut board = board(&initial);
            let mode = waiting(
                board
                    .activate_internal_card("source", guide_ability(number), PlayerId { seat: 1 })
                    .unwrap(),
            );
            let targets = waiting(
                board
                    .answer_internal_card(mode.actor, mode.token, Answer::Modes(vec![ModeId(2)]))
                    .unwrap(),
            );
            let InputDomain::Targets(domain) = &targets.domain else {
                panic!("up to two carrots")
            };
            assert_eq!(domain.members.len(), 2);
            let handles = domain
                .members
                .iter()
                .take(count)
                .map(|member| member.handle)
                .collect();
            resolved(choose(&mut board, &targets, handles));
            let state = board.projection(View::Referee).unwrap();
            assert_eq!(
                state["P1"]["ep"], 1_u8,
                "Q1863: also available to the first player"
            );
            assert_eq!(state["objects"]["source"]["acted"], true);
            assert_eq!(state["objects"]["carrot-a"]["face_up"], count == 0);
            assert_eq!(state["objects"]["carrot-b"]["face_up"], count == 0);
        }
    }
}

#[test]
fn real_guide_other_modes_recover_the_target_or_buff_only_other_uma_followers() {
    for mode in [1, 3] {
        let mut initial = initial();
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"ECP01-055"},{"id":"ally","card":"ECP01-028"},{"id":"other","card":"BP01-171"}]);
        initial["players"]["P1"]["zones"]["cemetery"] =
            json!([{"id":"recover","card":"ECP01-028"}]);
        let mut board = board(&initial);
        let before = board.projection(View::Referee).unwrap();
        let question = waiting(
            board
                .activate_internal_card("source", guide_ability("ECP01-055"), PlayerId { seat: 1 })
                .unwrap(),
        );
        let play = board
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Modes(vec![ModeId(mode)]),
            )
            .unwrap();
        if mode == 1 {
            let target = waiting(play);
            resolved(choose(&mut board, &target, vec![0]));
        } else {
            resolved(play);
        }
        let state = board.projection(View::Referee).unwrap();
        assert_eq!(
            state["objects"]["source"]["power"],
            before["objects"]["source"]["power"]
        );
        assert_eq!(
            state["objects"]["other"]["power"],
            before["objects"]["other"]["power"]
        );
        if mode == 1 {
            assert_eq!(state["P1"]["hand"], json!(["recover"]));
        } else {
            assert_eq!(
                state["objects"]["ally"]["power"].as_i64().unwrap(),
                before["objects"]["ally"]["power"].as_i64().unwrap() + 1
            );
            assert_eq!(
                state["objects"]["ally"]["hp"].as_i64().unwrap(),
                before["objects"]["ally"]["hp"].as_i64().unwrap() + 1
            );
        }
    }
}

#[test]
fn real_guide_fanfare_discards_an_uma_card_and_recovers_four_pp() {
    let mut initial = initial();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"ECP01-055"},{"id":"payment","card":"ECP01-028"},{"id":"unrelated","card":"BP01-171"}]);
    let mut board = board(&initial);
    assert!(matches!(
        board
            .play_internal_card("source", PlayerId { seat: 1 })
            .unwrap(),
        CardPlay::Entered(_)
    ));
    let mut play = pending_source(&mut board, "ECP01-055");
    let mut selected = false;
    while let CardPlay::Awaiting(question) = play {
        let answer = match &question.domain {
            InputDomain::OptionalCost => Answer::OptionalCost(true),
            InputDomain::CostMaterials(domain) | InputDomain::Targets(domain) => {
                assert_eq!(domain.members.len(), 1);
                assert_eq!(domain.members[0].card.as_deref(), Some("ECP01-028"));
                assert!(board.internal_decision(View::P2).is_none());
                selected = true;
                let handles = vec![domain.members[0].handle];
                if matches!(question.domain, InputDomain::Targets(_)) {
                    Answer::Targets(handles)
                } else {
                    Answer::CostMaterials(handles)
                }
            }
            InputDomain::Abilities(_)
            | InputDomain::ResourceEntryPayment { .. }
            | InputDomain::WardEntry { .. }
            | InputDomain::Quick { .. }
            | InputDomain::BeforeDraw
            | InputDomain::Pending(_)
            | InputDomain::TriggerQuota(_)
            | InputDomain::Integer(_)
            | InputDomain::Order(_)
            | InputDomain::DamageReplacements(_)
            | InputDomain::DriveTrigger { .. }
            | InputDomain::Search(_)
            | InputDomain::SearchPartition(_)
            | InputDomain::Distribution(_)
            | InputDomain::SearchReplacement
            | InputDomain::TokenCapacity { .. }
            | InputDomain::Modes { .. } => panic!("Uma discard payment"),
        };
        play = board
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    resolved(play);
    assert!(selected, "the controller must select the paid Uma card");
    let state = board.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["pp"]["current"], 8_u8);
    assert_eq!(state["P1"]["hand"], json!(["unrelated"]));
    assert_eq!(state["P1"]["cemetery"], json!(["payment"]));
}

#[test]
fn real_guide_cannot_choose_the_recovery_mode_without_a_legal_cemetery_target() {
    let mut initial = initial();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"ECP01-055"}]);
    let mut board = board(&initial);
    let question = waiting(
        board
            .activate_internal_card("source", guide_ability("ECP01-055"), PlayerId { seat: 1 })
            .unwrap(),
    );
    let before = format!("{board:?}");
    board
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Modes(vec![ModeId(1)]),
        )
        .unwrap_err();
    assert!(
        format!("{board:?}") == before,
        "Q1864: an impossible mandatory target cannot pay the activation cost"
    );
    assert_eq!(
        board.projection(View::Referee).unwrap()["objects"]["source"]["acted"],
        false
    );
}
