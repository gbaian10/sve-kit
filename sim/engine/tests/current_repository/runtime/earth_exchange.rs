#![expect(
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate the printed Earth Rite exchange scenario."
)]
use super::*;

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
