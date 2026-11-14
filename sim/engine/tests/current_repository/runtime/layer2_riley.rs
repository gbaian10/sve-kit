#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate the real Riley scenarios."
)]
use super::*;

const PLAYER: PlayerId = PlayerId { seat: 1 };

fn position(zone: &str, stacks: u8, enemy: bool) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"][zone] = json!([{"id":"riley","card":"BP07-038"}]);
    if stacks > 0 {
        initial["players"]["P1"]["zones"]["field"] = json!([
            {"id":"soil","card":"BP01-075","state":{"counters":{"stack_counter":stacks}}}
        ]);
    }
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"draw1","card":"BP01-037"},{"id":"draw2","card":"BP01-037"},
        {"id":"spare","card":"BP01-037"}
    ]);
    if enemy {
        initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-097"}]);
    }
    initial
}

fn finish(game: &mut Game, mut play: CardPlay, modes: &[u64], pay: bool, expected_count: u64) {
    for _ in 0_u8..32 {
        if let CardPlay::Awaiting(question) = play {
            let answer = match &question.domain {
                InputDomain::Modes { count, .. } => {
                    assert_eq!(*count, Nat(expected_count));
                    if expected_count == 1 {
                        game.answer_internal_card(
                            question.actor,
                            question.token,
                            Answer::Modes(vec![ModeId(1), ModeId(2), ModeId(3)]),
                        )
                        .unwrap_err();
                    }
                    Answer::Modes(modes.iter().copied().map(ModeId).collect())
                }
                InputDomain::Targets(domain) => Answer::Targets(vec![domain.members[0].handle]),
                InputDomain::OptionalCost => Answer::OptionalCost(pay),
                InputDomain::CostMaterials(domain) => {
                    Answer::CostMaterials(vec![domain.members[0].handle])
                }
                InputDomain::WardEntry { .. } => Answer::WardEntry(false),
                _ => panic!("unexpected Riley decision: {:?}", question.domain),
            };
            play = game
                .answer_internal_card(question.actor, question.token, answer)
                .unwrap();
            continue;
        }
        if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
        let Some(question) = game.internal_pending_choice().unwrap() else {
            return;
        };
        let InputDomain::Pending(options) = &question.domain else {
            panic!("pending Riley ability");
        };
        play = game
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap();
    }
    panic!("Riley scenario did not settle");
}

fn enter(game: &mut Game, zone: &str) -> CardPlay {
    if zone == "cemetery" {
        let ability = game.internal_activated_sources("riley").unwrap()[0].definition;
        game.activate_internal_card("riley", ability, PLAYER)
            .unwrap()
    } else {
        game.play_internal_card("riley", PLAYER).unwrap()
    }
}

#[test]
fn bp07_038_hand_entry_selects_one_mode_and_pays_one_earth_rite() {
    let mut game = game(&position("hand", 2, true));
    let play = enter(&mut game, "hand");
    finish(&mut game, play, &[1], true, 1);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["damage"], 5_u8);
    assert_eq!(state["objects"]["riley"]["power"], 3_u8);
    assert_eq!(
        state["objects"]["soil"]["counters"]["スタックカウンター"],
        1_u8
    );
    assert_eq!(state["P1"]["hand"], json!([]));
    assert_eq!(state["P1"]["pp"]["current"], 7_u8);
}

#[test]
fn bp07_038_cemetery_activation_chooses_all_three_and_grants_storm_and_lastword() {
    let mut game = game(&position("cemetery", 2, true));
    let play = enter(&mut game, "cemetery");
    finish(&mut game, play, &[1, 2, 3], true, 3);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["damage"], 5_u8);
    assert_eq!(state["objects"]["riley"]["power"], 6_u8);
    assert_eq!(state["P1"]["hand"], json!(["draw1", "draw2"]));
    assert_eq!(
        state["objects"]["soil"]["counters"]["スタックカウンター"],
        1_u8
    );
    assert_eq!(state["P1"]["pp"]["current"], 4_u8);
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"riley","target":"P2.leader"}))
            .unwrap()
            .outcome,
        "resolved"
    );
}

#[test]
fn bp07_038_without_enemy_can_choose_buff_and_draw_from_cemetery() {
    let mut game = game(&position("cemetery", 2, false));
    let play = enter(&mut game, "cemetery");
    finish(&mut game, play, &[2, 3], true, 3);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["riley"]["power"], 6_u8);
    assert_eq!(state["P1"]["hand"], json!(["draw1", "draw2"]));
}

#[test]
fn bp07_038_declining_earth_rite_keeps_stack_and_resolves_no_mode() {
    let mut game = game(&position("hand", 2, false));
    let play = enter(&mut game, "hand");
    finish(&mut game, play, &[3], false, 1);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        state["objects"]["soil"]["counters"]["スタックカウンター"],
        2_u8
    );
    assert_eq!(state["objects"]["riley"]["power"], 3_u8);
    assert_eq!(state["P1"]["hand"], json!([]));
}

#[test]
fn bp07_038_granted_lastword_banishes_the_cemetery_generation_after_combat() {
    let mut initial = position("cemetery", 2, true);
    initial["players"]["P2"]["zones"]["field"][0]["state"]["acted"] = json!(true);
    let mut game = game(&initial);
    let play = enter(&mut game, "cemetery");
    finish(&mut game, play, &[2, 3], true, 3);
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"riley","target":"enemy"}))
            .unwrap()
            .outcome,
        "resolved"
    );
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
    let quick = game.internal_quick_choice().unwrap().unwrap();
    game.answer_internal_card(quick.actor, quick.token, Answer::Quick(None))
        .unwrap();
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("Riley lastword");
    };
    assert_eq!(options.len(), 1);
    let play = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap();
    finish(&mut game, play, &[], true, 3);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["banish"], json!(["riley"]));
    assert_eq!(state["P1"]["cemetery"], json!([]));
    assert_eq!(state["objects"]["riley"]["generation"], 3_u8);
}
