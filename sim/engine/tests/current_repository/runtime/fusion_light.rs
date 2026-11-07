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
            InputDomain::TokenCapacity { choices } => {
                let chosen = choices
                    .iter()
                    .position(|names| matches!(names.as_slice(), [name] if name == "黄金の靴"))
                    .unwrap();
                Answer::TokenCapacity(u64::try_from(chosen).unwrap())
            }
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

fn drain(board: &mut Game) -> usize {
    let mut count = 0_usize;
    loop {
        while board.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            board.resume_internal_card().unwrap();
        }
        let Some(question) = board.internal_pending_choice().unwrap() else {
            return count;
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
        count = count.checked_add(1).unwrap();
    }
}

fn finish_quick_windows(board: &mut Game) -> usize {
    let mut triggers = 0_usize;
    loop {
        triggers = triggers.checked_add(drain(board)).unwrap();
        let Some(question) = board.internal_quick_choice().unwrap() else {
            return triggers;
        };
        let continuation = board
            .answer_internal_card(question.actor, question.token, Answer::Quick(None))
            .unwrap();
        finish(board, continuation);
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
        "Fusion excludes its source and applies the printed material filter"
    );
    assert!(
        board.internal_decision(View::P2).is_none(),
        "the private Fusion payment is visible only to its controller"
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

fn named_ex(board: &Game, name: &str) -> usize {
    let state = board.projection(View::Referee).unwrap();
    state["P1"]["ex"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|id| state["objects"][id.as_str().unwrap()]["name"] == name)
        .count()
}

fn treasure_position(number: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"source","card":number}, {"id":"fuser","card":"BP20-026"},
        {"id":"material","card":"BP20-026"}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"enemy","card":"BP01-171","state":{"acted":false,"power":3_u8,"hp":10_u8,"max_hp":10_u8}}
    ]);
    initial
}

fn play_treasure(board: &mut Game, holder: &str) -> usize {
    let spell = board.play_internal_card(holder, PLAYER).unwrap();
    finish(board, spell);
    drain(board)
}

#[test]
fn bp20_029_all_prints_create_a_cup_and_share_the_heal_quota_between_fusion_and_play() {
    // Both printings share the real selection; constructing that fixture is expensive.
    for (played_first, number) in [(false, "BP20-029"), (true, "BP20-P16")] {
        let mut board = game(&treasure_position(number));
        let entry = board.play_internal_card("source", PLAYER).unwrap();
        finish(&mut board, entry);
        assert_eq!(drain(&mut board), 1);
        assert_eq!(named_ex(&board, "黄金の杯"), 1);
        let first = if played_first {
            play_treasure(&mut board, "material")
        } else {
            fuse(&mut board, "fuser", "BP20-026", 2, "BP20-026", 2);
            drain(&mut board)
        };
        assert_eq!(first, 1, "either event can consume the first trigger");
        assert_eq!(
            board.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
            11_u8
        );
        let second = if played_first {
            fuse(&mut board, "fuser", "BP20-026", 2, "BP15-P30", 1);
            drain(&mut board)
        } else {
            play_treasure(&mut board, "fuser")
        };
        assert_eq!(second, 0, "one quota belongs to the combined subscription");
        assert_eq!(
            board.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
            11_u8
        );
    }
}

#[test]
fn bp20_033_all_prints_gain_assail_from_fusion_and_offer_both_lastword_treasures_together() {
    for (played_first, number) in [(false, "BP20-033"), (true, "BP20-P19")] {
        let mut initial = treasure_position(number);
        initial["players"]["P1"]["zones"]["ex"] = json!([
            {"id":"a","card":"BP01-171"}, {"id":"b","card":"BP01-171"},
            {"id":"c","card":"BP01-171"}
        ]);
        if played_first {
            initial["players"]["P1"]["zones"]["ex"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":"d","card":"BP01-171"}));
        }
        let mut board = game(&initial);
        let play = board.play_internal_card("source", PLAYER).unwrap();
        finish(&mut board, play);
        let granted = if played_first {
            play_treasure(&mut board, "material")
        } else {
            fuse(&mut board, "fuser", "BP20-026", 2, "BP20-026", 1);
            drain(&mut board)
        };
        assert_eq!(granted, 1, "either event grants Assail");
        assert_eq!(
            board
                .internal_rule_action(&json!({"do":"attack","attacker":"source","target":"enemy"}))
                .unwrap()
                .outcome,
            "resolved",
            "Rush and granted Assail permit attacking this standing follower after entry"
        );
        assert_eq!(finish_quick_windows(&mut board), 1);
        let state = board.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["cemetery"], json!(["material", "source"]));
        assert_eq!(state["P1"]["ex"].as_array().unwrap().len(), 5);
        assert_eq!(named_ex(&board, "黄金の靴"), 1);
        assert_eq!(
            named_ex(&board, "黄金の杯"),
            0,
            "one shared capacity choice can keep the second printed token"
        );
    }
}
