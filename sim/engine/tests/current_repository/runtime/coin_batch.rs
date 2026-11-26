use super::*;
use sve_engine::game::player::{
    Command, Input, MainActionAnswer, MainIntentKind, QuestionShape, SubmissionKind, Viewer,
};

fn position() -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"top","card":"BP01-037"},{"id":"next","card":"BP01-037"}
    ]);
    initial
}

fn pending_card(board: &mut Game, number: &str) -> CardPlay {
    let question = board.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("pending trigger");
    };
    assert_eq!(
        options.len(),
        1,
        "the card must be the only pending trigger"
    );
    assert_eq!(
        options[0].card, number,
        "the expected card must own the trigger"
    );
    board
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap()
}

fn assert_gold(board: &Game, expected: usize) {
    let state = board.projection(View::Referee).unwrap();
    let cards = state["P1"]["ex"].as_array().unwrap();
    assert_eq!(cards.len(), expected, "EX must contain the created coins");
    for id in cards {
        assert_eq!(
            state["objects"][id.as_str().unwrap()]["card"],
            "BP14-T02",
            "each generated object must use the complete coin template"
        );
    }
}

#[test]
fn shining_gold_coin_can_be_played_as_a_zero_pp_spell_without_activating_it() {
    let mut initial = position();
    initial["players"]["P1"]["pp"]["current"] = json!(0_u8);
    initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"coin","card":"BP14-T02"}]);
    let mut board = game(&initial);
    board.advance_player_input();
    let Input::Question(question) = board.input(Viewer::P1) else {
        panic!("main phase question");
    };
    let QuestionShape::ActionWindow(actions) = question.shape else {
        panic!("main action window");
    };
    let play = actions
        .intents
        .into_iter()
        .find(|intent| matches!(&intent.kind, MainIntentKind::Play { card } if card.properties.card == "BP14-T02"))
        .expect("a single coin can be played at zero PP");
    assert_eq!(
        board.submit_player(
            Viewer::P1,
            Command::MainAction { token: question.token, intent: play.id, answer: MainActionAnswer::None }
        ).kind,
        SubmissionKind::Applied
    );
    let state = board.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["pp"]["current"], 0_u8);
    assert_eq!(state["P1"]["ex"], json!([]));
    assert_eq!(state["P1"]["cemetery"], json!([]));
    assert!(state["objects"].get("coin").is_none());
    assert_eq!(state["P1"]["leader"]["life"], 10_u8);
    assert_eq!(state["P1"]["hand"], json!([]));
    assert_eq!(state["P1"]["deck"], json!(["top", "next"]));
}

#[test]
fn shining_gold_coin_banishes_itself_and_two_other_own_coins_before_healing_and_drawing() {
    let mut initial = position();
    initial["players"]["P1"]["zones"]["ex"] = json!([
        {"id":"source","card":"BP14-T02"},{"id":"a","card":"BP14-T02"},
        {"id":"b","card":"BP14-T02"},{"id":"unrelated","card":"BP01-037"}
    ]);
    initial["players"]["P2"]["zones"]["ex"] = json!([{"id":"foreign","card":"BP14-T02"}]);
    let mut board = game(&initial);
    let CardPlay::Awaiting(question) = board
        .activate_internal_card("source", AbilityId(1), actor())
        .unwrap()
    else {
        panic!("materials");
    };
    let InputDomain::CostMaterials(domain) = &question.domain else {
        panic!("materials");
    };
    assert_eq!(domain.members.len(), 2);
    assert_eq!(domain.cardinality.minimum, Nat(2));
    let before = board.projection(View::Referee).unwrap();
    assert_eq!(before["P1"]["pp"]["current"], 10_u8);
    assert_eq!(before["P1"]["ex"].as_array().unwrap().len(), 4);
    resolved(
        board
            .answer_internal_card(
                actor(),
                question.token,
                Answer::CostMaterials(domain.members.iter().map(|m| m.handle).collect()),
            )
            .unwrap(),
    );
    let state = board.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["pp"]["current"], 9_u8);
    assert_eq!(state["P1"]["leader"]["life"], 11_u8);
    assert_eq!(state["P1"]["hand"], json!(["top"]));
    assert_eq!(state["P1"]["ex"], json!(["unrelated"]));
    assert_eq!(state["P2"]["ex"], json!(["foreign"]));
}

#[test]
fn bp14_026_fanfare_puts_a_complete_gold_coin_in_ex() {
    let mut initial = position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP14-026"}]);
    let mut board = game(&initial);
    assert!(matches!(
        board.play_internal_card("source", actor()).unwrap(),
        CardPlay::Entered(_)
    ));
    resolved(pending_card(&mut board, "BP14-026"));
    assert_gold(&board, 1);
    assert_eq!(
        board.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        10_u8
    );
}

#[test]
fn bp14_027_observes_a_coin_entering_own_ex_and_hits_the_targets_controller() {
    let mut initial = position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"observer","card":"BP14-026","state":{"evolved":true,"evolved_with":"observer-form","power":4_u8,"hp":4_u8,"max_hp":4_u8}}]);
    initial["players"]["P1"]["zones"]["evolution"] =
        json!([{"id":"observer-form","card":"BP14-027"}]);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"spell","card":"BP14-035"}]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-037","state":{"hp":10_u8,"max_hp":10_u8}}]);
    let mut board = game(&initial);
    resolved(board.play_internal_card("spell", actor()).unwrap());
    board.resume_internal_card().unwrap();
    let CardPlay::Awaiting(question) = pending_card(&mut board, "BP14-027") else {
        panic!("target");
    };
    resolved(
        board
            .answer_internal_card(actor(), question.token, Answer::Targets(vec![0]))
            .unwrap(),
    );
    let state = board.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["hp"], 7_u8);
    assert_eq!(state["P2"]["leader"]["life"], 19_u8);
    assert_gold(&board, 1);
}

#[test]
fn bp14_030_fanfare_creates_a_coin_without_evolving_when_jiemon_is_absent() {
    let mut initial = position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP14-030"}]);
    let mut board = game(&initial);
    assert!(matches!(
        board.play_internal_card("source", actor()).unwrap(),
        CardPlay::Entered(_)
    ));
    resolved(pending_card(&mut board, "BP14-030"));
    assert_gold(&board, 1);
    assert_eq!(
        board.projection(View::Referee).unwrap()["objects"]["source"]["hp"],
        2_u8
    );
}

#[test]
fn bp14_031_evolution_damages_and_draws_with_three_banquet_cards_in_ex() {
    let mut initial = position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP14-030"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP14-031"}]);
    initial["players"]["P1"]["zones"]["ex"] = json!([
        {"id":"a","card":"BP14-T02"},{"id":"b","card":"BP14-T02"},{"id":"c","card":"BP14-T02"}
    ]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-037","state":{"hp":10_u8,"max_hp":10_u8}}]);
    let mut board = game(&initial);
    board
        .internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}}),
        )
        .unwrap();
    let CardPlay::Awaiting(question) = pending_card(&mut board, "BP14-031") else {
        panic!("target");
    };
    resolved(
        board
            .answer_internal_card(actor(), question.token, Answer::Targets(vec![0]))
            .unwrap(),
    );
    let state = board.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["hp"], 8_u8);
    assert_eq!(state["P1"]["hand"], json!(["top"]));
}

#[test]
fn bp14_034_first_mode_damages_and_creates_a_coin() {
    let mut initial = position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP14-034"}]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-037","state":{"hp":10_u8,"max_hp":10_u8}}]);
    let mut board = game(&initial);
    let CardPlay::Awaiting(mode) = board.play_internal_card("source", actor()).unwrap() else {
        panic!("mode");
    };
    let InputDomain::Modes { modes, .. } = &mode.domain else {
        panic!("mode");
    };
    let CardPlay::Awaiting(target) = board
        .answer_internal_card(actor(), mode.token, Answer::Modes(vec![modes[0]]))
        .unwrap()
    else {
        panic!("target");
    };
    resolved(
        board
            .answer_internal_card(actor(), target.token, Answer::Targets(vec![0]))
            .unwrap(),
    );
    assert_eq!(
        board.projection(View::Referee).unwrap()["objects"]["enemy"]["hp"],
        8_u8
    );
    assert_gold(&board, 1);
}

#[test]
fn bp14_035_is_free_with_three_coins_and_draws_before_creating_another() {
    let mut initial = position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP14-035"}]);
    initial["players"]["P1"]["zones"]["ex"] = json!([
        {"id":"a","card":"BP14-T02"},{"id":"b","card":"BP14-T02"},{"id":"c","card":"BP14-T02"}
    ]);
    let mut board = game(&initial);
    resolved(board.play_internal_card("source", actor()).unwrap());
    let state = board.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["pp"]["current"], 10_u8);
    assert_eq!(state["P1"]["hand"], json!(["top"]));
    assert_gold(&board, 4);
}
