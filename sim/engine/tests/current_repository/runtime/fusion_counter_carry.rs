#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate the printed Fusion batch."
)]
use super::*;

const PLAYER: PlayerId = PlayerId { seat: 1 };

fn line(number: &str, printed: u32) -> AbilityId {
    selection().current[number]
        .program
        .card()
        .abilities
        .iter()
        .find(|ability| ability.source.line == printed)
        .unwrap()
        .node
        .id
}

fn finish(board: &mut Game, mut play: CardPlay) {
    while let CardPlay::Awaiting(question) = play {
        let answer = match &question.domain {
            InputDomain::OptionalCost => Answer::OptionalCost(true),
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            InputDomain::Targets(domain) => Answer::Targets(vec![domain.members[0].handle]),
            other => panic!("unexpected Fusion batch choice {other:?}"),
        };
        play = board
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    while board.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        board.resume_internal_card().unwrap();
    }
}

fn drain(board: &mut Game) {
    loop {
        while board.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            board.resume_internal_card().unwrap();
        }
        let Some(question) = board.internal_pending_choice().unwrap() else {
            return;
        };
        let InputDomain::Pending(options) = &question.domain else {
            panic!("printed trigger")
        };
        let play = board
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap();
        finish(board, play);
    }
}

fn fuse(
    board: &mut Game,
    source: &str,
    number: &str,
    printed: u32,
    material: &str,
    candidates: usize,
) {
    let CardPlay::Awaiting(question) = board
        .activate_internal_card(source, line(number, printed), PLAYER)
        .unwrap()
    else {
        panic!("Fusion material")
    };
    let InputDomain::CostMaterials(domain) = &question.domain else {
        panic!("single Fusion cost")
    };
    assert_eq!(
        domain.members.len(),
        candidates,
        "only the non-Token Follower can pay Fusion"
    );
    assert!(
        board.internal_decision(View::P2).is_none(),
        "the opponent cannot see the material choice"
    );
    let handle = domain
        .members
        .iter()
        .find(|member| member.card.as_deref() == Some(material))
        .unwrap()
        .handle;
    let play = board
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::CostMaterials(vec![handle]),
        )
        .unwrap();
    finish(board, play);
}

fn researcher(numbers: &[&str], form: &str, remaining_pp: u8) {
    for &number in numbers {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["hand"] = json!([
            {"id":"source","card":number}, {"id":"material","card":"BP01-171"}
        ]);
        initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"token","card":"BP19-P34"}]);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
        let mut board = game(&initial);
        fuse(&mut board, "source", number, 3, "BP01-171", 1);
        let paid = board.projection(View::Referee).unwrap();
        assert_eq!(
            paid["objects"]["source"]["counters"]["融合カウンター"],
            1_u8,
            "{number} gains one Fusion counter in EX"
        );
        assert_eq!(
            paid["P1"]["cemetery"],
            json!(["material"]),
            "the selected hand material pays the Fusion"
        );
        assert_eq!(
            paid["P1"]["ex"],
            json!(["token", "source"]),
            "Follower Tokens cannot pay this Fusion"
        );
        let play = board.play_internal_card("source", PLAYER).unwrap();
        finish(&mut board, play);
        drain(&mut board);
        let entered = board.projection(View::Referee).unwrap();
        assert_eq!(
            entered["objects"]["source"]["counters"]["融合カウンター"],
            0_u8,
            "{number} pays the carried Fusion counter on entry"
        );
        assert_eq!(
            entered["P1"]["field"].as_array().unwrap().len(),
            2,
            "the paid Fanfare produces one follower"
        );
        let experiment = entered["P1"]["field"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(Value::as_str)
            .find(|id| *id != "source")
            .unwrap();
        assert_eq!(
            entered["objects"][experiment]["name"],
            "多頭の実験体",
            "the new follower is the named Experiment"
        );
        assert_eq!(
            board
                .internal_rule_action(
                    &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
                )
                .unwrap()
                .outcome,
            "resolved",
            "{number} can evolve after playing from EX"
        );
        let evolved = board.projection(View::Referee).unwrap();
        assert_eq!(
            evolved["objects"]["source"]["evolved"],
            true,
            "{number} installs its evolved form"
        );
        assert_eq!(
            evolved["P1"]["pp"]["current"],
            remaining_pp,
            "play and evolution charge their printed costs"
        );
    }
}

#[test]
fn bp19_042_all_prints_fuse_a_non_token_follower_then_spend_the_counter_and_evolve() {
    researcher(&["BP19-042", "BP19-P11"], "BP19-043", 6);
}

#[test]
fn bp19_046_all_prints_fuse_a_non_token_follower_then_spend_the_counter_and_evolve() {
    researcher(&["BP19-046", "BP19-P14"], "BP19-047", 7);
}
