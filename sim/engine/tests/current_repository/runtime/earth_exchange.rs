use super::*;
use sve_engine::game::player::{
    Command, DecisionShape, Input, MainActionAnswer, MainIntentId, MainIntentKind, Question,
    QuestionShape, QuestionToken, SubmissionKind, Viewer,
};

fn player_question(game: &Game) -> Question {
    let Input::Question(question) = game.input(Viewer::P1) else {
        panic!("Predator Golem player question");
    };
    *question
}

fn predator_intent(game: &Game) -> Option<(QuestionToken, MainIntentId)> {
    let question = player_question(game);
    let QuestionShape::ActionWindow(actions) = question.shape else {
        panic!("Predator Golem main action");
    };
    actions
        .intents
        .into_iter()
        .find(|intent| matches!(&intent.kind, MainIntentKind::Play { card } if card.properties.card == "BP22-039"))
        .map(|intent| (question.token, intent.id))
}

fn finish_player_play(game: &mut Game, earth_rite: bool) {
    let (token, intent) = predator_intent(game).unwrap();
    assert_eq!(
        game.submit_player(Viewer::P1, Command::MainAction { token, intent, answer: MainActionAnswer::None }).kind,
        SubmissionKind::Applied,
        "the offered play starts a formal declaration"
    );
    loop {
        let question = player_question(game);
        let answer = match question.shape {
            QuestionShape::Decision(DecisionShape::OptionalCost) => Answer::OptionalCost(earth_rite),
            QuestionShape::Decision(DecisionShape::CostMaterials(domain)) => {
                assert_eq!(domain.members.len(), 1, "the sole field Stack pays the exchange");
                Answer::CostMaterials(vec![domain.members[0].handle])
            }
            QuestionShape::Decision(DecisionShape::WardEntry { .. }) => Answer::WardEntry(false),
            QuestionShape::Decision(DecisionShape::Pending(options)) => Answer::Pending(options[0].handle),
            QuestionShape::Decision(DecisionShape::Targets(domain)) => {
                Answer::Targets(domain.members.iter().map(|member| member.handle).collect())
            }
            QuestionShape::ActionWindow(_) => return,
            _ => panic!("unexpected Predator Golem player question"),
        };
        assert_eq!(
            game.submit_player(Viewer::P1, Command::Answer { token: question.token, answer }).kind,
            SubmissionKind::Applied,
            "a legal choice must advance Predator Golem's play and Fanfare"
        );
    }
}

#[test]
fn bp22_039_player_main_play_at_one_pp_requires_and_spends_nine_stack() {
    let mut initial = draw_position();
    initial["players"]["P1"]["pp"]["current"] = json!(1_u8);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP22-039"}]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-037"}]);
    for stack in [9_u8, 8] {
        initial["players"]["P1"]["zones"]["field"] = json!([
            {"id":"soil","card":"BP03-T02","state":{"counters":{"stack_counter":stack}}}
        ]);
        let mut game = game(&initial);
        game.advance_player_input();
        if stack == 8 {
            assert!(predator_intent(&game).is_none());
            continue;
        }
        finish_player_play(&mut game, true);
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["P1"]["pp"]["current"], 0_u8);
        assert_eq!(view["P1"]["field"], json!(["source"]));
    }
}

#[test]
fn bp22_039_player_fanfare_damages_and_heals_with_zero_targets() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP22-039"}]);
    let mut game = game(&initial);
    game.advance_player_input();
    finish_player_play(&mut game, false);
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P2"]["leader"]["life"], 17_u8);
    assert_eq!(view["P1"]["leader"]["life"], 23_u8);
}

#[test]
fn bp22_039_whole_card_pays_nine_stack_sets_cost_one_then_destroys_and_heals() {
    let mut initial = draw_position();
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    initial["players"]["P1"]["pp"]["current"] = json!(1_u8);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP22-039"}]);
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"soil","card":"BP01-075","state":{"counters":{"stack_counter":9_u8}}}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!(
        (0..3_u8)
            .map(|i| json!({"id":format!("enemy-{i}"),"card":"BP01-037"}))
            .collect::<Vec<_>>()
    );
    let mut game = game(&initial);
    let mut play = game
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap();
    loop {
        while let CardPlay::Awaiting(question) = play {
            let answer = match &question.domain {
                InputDomain::OptionalCost => Answer::OptionalCost(true),
                InputDomain::CostMaterials(domain) => {
                    assert_eq!(
                        domain.members.len(),
                        1,
                        "only the Stack amulet pays Earth Rite"
                    );
                    Answer::CostMaterials(vec![domain.members[0].handle])
                }
                InputDomain::Targets(domain) => {
                    Answer::Targets(domain.members.iter().map(|member| member.handle).collect())
                }
                InputDomain::WardEntry { .. } => Answer::WardEntry(false),
                _ => panic!("printed exchange or Fanfare question"),
            };
            play = game
                .answer_internal_card(question.actor, question.token, answer)
                .unwrap();
        }
        while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
        let Some(question) = game.internal_pending_choice().unwrap() else {
            break;
        };
        let InputDomain::Pending(options) = &question.domain else {
            panic!("printed Fanfare");
        };
        assert_eq!(
            options.len(),
            1,
            "only Predator Golem contributes a Fanfare"
        );
        play = game
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap();
    }
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(
        view["P1"]["pp"]["current"], 0_u8,
        "the single quotation costs one PP"
    );
    assert_eq!(
        view["P1"]["cemetery"],
        json!(["soil"]),
        "zero remaining Stack leaves the field"
    );
    assert_eq!(
        view["P2"]["cemetery"].as_array().unwrap().len(),
        3,
        "all three selected followers are destroyed"
    );
    assert_eq!(
        view["P2"]["leader"]["life"], 17_u8,
        "the Fanfare damages every opposing leader"
    );
    assert_eq!(
        view["P1"]["leader"]["life"], 13_u8,
        "the same Fanfare heals the original controller"
    );
    assert!(
        view["objects"]["source"]["keywords"]
            .as_array()
            .unwrap()
            .iter()
            .any(|keyword| keyword == "守護"),
        "the native Ward remains active after the payment"
    );
}
