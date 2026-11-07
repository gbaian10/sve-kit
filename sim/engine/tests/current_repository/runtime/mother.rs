#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate the printed Mother resurrection scenarios."
)]
use super::*;

fn position(number: &str, foreign: bool, full: bool, form: bool) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"blaze","card":"BP01-089"}]);
    initial["players"]["P2"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{
        "id":"source", "card":number, "owner":if foreign { "P2" } else { "P1" },
        "state":{"entered_this_turn":true}
    }]);
    initial["players"]["P2"]["zones"]["field"] = json!([{
        "id":"enemy", "card":"BP01-171", "state":{"power":4_u8,"hp":10_u8,"max_hp":10_u8}
    }]);
    if full {
        initial["players"]["P2"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .extend((0..4_u8).map(|n| json!({"id":format!("filler-{n}"),"card":"BP01-171","state":{"hp":10_u8,"max_hp":10_u8}})));
    }
    initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"tree","card":"BP07-T03"}]);
    let owner = if foreign { "P2" } else { "P1" };
    if form {
        initial["players"][owner]["zones"]["evolve_deck"] = json!([{
            "id":"form", "card":if number == "BP07-104" { "BP07-105" } else { "BP07-SL21" }
        }]);
    }
    initial
}

fn checks(board: &mut Game) {
    while board.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        board.resume_internal_card().unwrap();
    }
}

fn finish(board: &mut Game, mut play: CardPlay, decline: bool) {
    while let CardPlay::Awaiting(question) = play {
        let answer = match &question.domain {
            InputDomain::OptionalCost => Answer::OptionalCost(true),
            InputDomain::CostMaterials(domain) => {
                Answer::CostMaterials(vec![domain.members[0].handle])
            }
            InputDomain::Targets(domain) => {
                let is_form = domain
                    .members
                    .iter()
                    .any(|member| matches!(member.card.as_deref(), Some("BP07-105" | "BP07-SL21")));
                if is_form {
                    assert_eq!(
                        domain.cardinality.minimum,
                        Nat(0),
                        "private evolution resources may be treated as absent (CR 4.1.2.2)"
                    );
                    assert!(
                        board.internal_decision(View::P2).is_none(),
                        "only the actor sees evolution choices"
                    );
                    let before = format!("{board:?}");
                    board
                        .answer_internal_card(
                            question.actor,
                            question.token,
                            Answer::Targets(vec![u64::MAX]),
                        )
                        .unwrap_err();
                    assert_eq!(
                        format!("{board:?}"),
                        before,
                        "an out-of-domain answer changes nothing"
                    );
                    assert_eq!(
                        board.projection(View::Referee).unwrap()["objects"]["source"]["acted"],
                        true,
                        "the source is already acted before the evolution choice"
                    );
                }
                Answer::Targets(if decline && is_form {
                    vec![]
                } else {
                    domain
                        .members
                        .first()
                        .map_or_else(Vec::new, |member| vec![member.handle])
                })
            }
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            other => panic!("unexpected Mother decision {other:?}"),
        };
        play = board
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    checks(board);
}

fn settle(board: &mut Game, decline: bool) {
    loop {
        checks(board);
        if let Some(question) = board.internal_pending_choice().unwrap() {
            let InputDomain::Pending(options) = &question.domain else {
                panic!("Mother Lastword ordering");
            };
            let play = board
                .answer_internal_card(
                    question.actor,
                    question.token,
                    Answer::Pending(options[0].handle),
                )
                .unwrap();
            finish(board, play, decline);
            continue;
        }
        let Some(question) = board.internal_quick_choice().unwrap() else {
            return;
        };
        let play = board
            .answer_internal_card(question.actor, question.token, Answer::Quick(None))
            .unwrap();
        finish(board, play, decline);
    }
}

fn destroy_source(board: &mut Game, decline: bool) {
    let spell = board
        .play_internal_card("blaze", PlayerId { seat: 1 })
        .unwrap();
    finish(board, spell, decline);
    settle(board, decline);
}

#[test]
fn bp07_104_all_prints_return_acted_and_evolve_after_the_ex_tree_payment() {
    // Both printings reuse the full public selection and its admitted evolution faces.
    for number in ["BP07-104", "BP07-SL20"] {
        let mut board = game(&position(number, false, false, true));
        destroy_source(&mut board, false);
        let state = board.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["field"], json!(["source"]));
        assert_eq!(state["P1"]["ex"], json!([]));
        assert_eq!(state["P1"]["cemetery"], json!(["blaze"]));
        assert_eq!(state["P1"]["pp"]["current"], 5_u8);
        assert_eq!(state["objects"]["source"]["generation"], 2_u8);
        assert_eq!(state["objects"]["source"]["acted"], true);
        assert_eq!(state["objects"]["source"]["power"], 4_u8);
        assert_eq!(state["objects"]["source"]["hp"], 4_u8);
    }
}

#[test]
fn bp07_104_all_prints_banish_the_returned_generation_without_a_successful_evolution() {
    for number in ["BP07-104", "BP07-SL20"] {
        let declines = number == "BP07-SL20";
        let mut board = game(&position(number, false, false, declines));
        destroy_source(&mut board, declines);
        let state = board.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["field"], json!([]));
        assert_eq!(state["P1"]["banish"], json!(["source"]));
        assert_eq!(state["P1"]["cemetery"], json!(["blaze"]));
        assert_eq!(state["P1"]["ex"], json!([]));
        assert_eq!(state["objects"]["source"]["generation"], 3_u8);
    }
}

#[test]
fn bp07_104_all_prints_return_to_the_owner_and_cannot_use_the_owners_private_form() {
    for number in ["BP07-104", "BP07-SL20"] {
        let mut board = game(&position(number, true, false, true));
        destroy_source(&mut board, false);
        let state = board.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["field"], json!([]));
        assert_eq!(state["P1"]["ex"], json!([]));
        assert_eq!(state["P2"]["field"], json!(["enemy"]));
        assert_eq!(state["P2"]["banish"], json!(["source"]));
        assert_eq!(state["P2"]["evolve_deck"], json!(["form"]));
        assert_eq!(state["objects"]["source"]["generation"], 3_u8);
    }
}

#[test]
fn bp07_104_all_prints_banish_the_cemetery_generation_if_the_owners_field_is_full() {
    for number in ["BP07-104", "BP07-SL20"] {
        let mut board = game(&position(number, true, true, true));
        destroy_source(&mut board, false);
        let state = board.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["ex"], json!([]));
        assert_eq!(state["P2"]["field"].as_array().unwrap().len(), 5);
        assert_eq!(state["P2"]["cemetery"], json!([]));
        assert_eq!(state["P2"]["banish"], json!(["source"]));
        assert_eq!(state["objects"]["source"]["generation"], 2_u8);
        assert_eq!(state["P2"]["evolve_deck"], json!(["form"]));
    }
}

#[test]
fn bp10_057_all_prints_return_the_named_cemetery_follower_acted_after_departure() {
    for number in ["BP10-057", "BP10-SL15"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["field"] = json!([{
            "id":"source","card":number,"state":{"hp":1_u8,"max_hp":1_u8}
        }]);
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"blaze","card":"BP01-089"}]);
        initial["players"]["P1"]["zones"]["cemetery"] = json!([{"id":"base","card":"BP03-056"}]);
        let mut board = game(&initial);
        destroy_source(&mut board, false);
        let state = board.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["field"], json!(["base"]));
        // CR 9.2.2 returns a departed Advance card to its owner's evolve deck.
        assert_eq!(state["P1"]["ex"], json!([]));
        assert_eq!(state["P1"]["evolve_deck"], json!(["source"]));
        assert_eq!(state["P1"]["cemetery"], json!(["blaze"]));
        assert_eq!(state["objects"]["base"]["acted"], true);
        assert_eq!(state["objects"]["base"]["generation"], 1_u8);
        assert_eq!(state["P1"]["pp"]["current"], 5_u8);
    }
}
