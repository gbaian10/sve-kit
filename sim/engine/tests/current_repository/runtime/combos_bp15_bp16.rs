#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate the complete printed card scenarios."
)]
use super::admission_fixture::{position, settle};
use super::*;

fn finish(game: &mut Game, mut play: CardPlay, modes: &[u64], accept: bool) -> Vec<&'static str> {
    let mut trace = Vec::new();
    for _ in 0_u8..32_u8 {
        let CardPlay::Awaiting(question) = play else {
            assert!(
                !matches!(play, CardPlay::Cancelled),
                "the complete printed ability resolves"
            );
            return trace;
        };
        let answer = match &question.domain {
            InputDomain::Modes { modes: offered, .. } => {
                trace.push("modes");
                let selected = modes.iter().copied().map(ModeId).collect::<Vec<_>>();
                assert!(
                    selected.iter().all(|mode| offered.contains(mode)),
                    "the requested printed modes are available"
                );
                Answer::Modes(selected)
            }
            InputDomain::Targets(domain) => {
                trace.push("targets");
                assert_eq!(
                    domain.members.len(),
                    1,
                    "the scenario leaves exactly one legal candidate"
                );
                Answer::Targets(vec![domain.members[0].handle])
            }
            InputDomain::OptionalCost => {
                trace.push("optional_cost");
                Answer::OptionalCost(accept)
            }
            InputDomain::CostMaterials(domain) => {
                trace.push("materials");
                assert!(accept, "declined payment never requests materials");
                assert_eq!(
                    domain.members.len(),
                    1,
                    "the scenario leaves exactly one legal candidate"
                );
                Answer::CostMaterials(vec![domain.members[0].handle])
            }
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            InputDomain::Quick { .. } => Answer::Quick(None),
            _ => panic!("unexpected printed choice: {:?}", question.domain),
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    panic!("unbounded printed choice loop");
}

fn enter(game: &mut Game) {
    let play = game
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap();
    finish(game, play, &[], false);
}

fn trigger(game: &mut Game, number: &str) -> CardPlay {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("printed trigger must be pending");
    };
    let option = options.iter().find(|option| option.card == number).unwrap();
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::Pending(option.handle),
    )
    .unwrap()
}

#[test]
fn bp15_078_conditions_destroy_and_leader_damage_on_three_named_followers() {
    for count in [2_i32, 3_i32] {
        let mut initial = position("BP15-078");
        initial["players"]["P1"]["zones"]["field"] = json!(
            (0_i32..count)
                .map(|index| json!({"id":format!("echo{index}"),"card":"BP15-P32"}))
                .collect::<Vec<_>>()
        );
        let mut game = game(&initial);
        enter(&mut game);
        let fanfare = trigger(&mut game, "BP15-078");
        finish(&mut game, fanfare, &[], false);
        settle(&mut game);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P2"]["leader"]["life"],
            if count == 3_i32 { 17_i32 } else { 20_i32 }
        );
        assert_eq!(state["P1"]["pp"]["current"], 8_i32);
        assert_eq!(
            state["P2"]["cemetery"],
            if count == 3_i32 {
                json!(["enemy"])
            } else {
                json!([])
            }
        );
        assert_eq!(
            state["P2"]["field"],
            if count == 3_i32 {
                json!([])
            } else {
                json!(["enemy"])
            }
        );
        for keyword in ["突進", "指定攻撃"] {
            assert!(
                state["objects"]["source"]["keywords"]
                    .as_array()
                    .unwrap()
                    .contains(&json!(keyword))
            );
        }
    }
}

#[test]
fn bp15_078_lastword_creates_its_admitted_token_and_mills_after_real_combat() {
    let mut game = game(&position("BP15-078"));
    enter(&mut game);
    let fanfare = trigger(&mut game, "BP15-078");
    finish(&mut game, fanfare, &[], false);
    settle(&mut game);
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"enemy"}))
            .unwrap()
            .outcome,
        "resolved"
    );
    let quick = game.internal_quick_choice().unwrap().unwrap();
    game.answer_internal_card(quick.actor, quick.token, Answer::Quick(None))
        .unwrap();
    let lastword = trigger(&mut game, "BP15-078");
    finish(&mut game, lastword, &[], false);
    settle(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["cemetery"], json!(["source", "draw1"]));
    assert_eq!(state["P1"]["deck"], json!(["draw2", "draw3"]));
    let created = state["P1"]["ex"].as_array().unwrap();
    assert_eq!(created.len(), 1);
    assert_eq!(
        state["objects"][created[0].as_str().unwrap()]["name"],
        "絶叫の拡散"
    );
}

#[test]
fn bp15_117_selects_its_amulet_before_drawing_and_rejects_duplicate_or_three_modes() {
    let mut initial = position("BP15-117");
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"amulet","card":"BP01-075","state":{"counters":{"stack_counter":1_i32}}}]);
    let mut game = game(&initial);
    enter(&mut game);
    let fanfare = trigger(&mut game, "BP15-117");
    let CardPlay::Awaiting(mode) = &fanfare else {
        panic!("mode choice");
    };
    assert!(
        matches!(&mode.domain, InputDomain::Modes { modes, count, .. } if modes.len() == 4 && count.0 == 2),
        "actual mode domain: {:?}",
        mode.domain
    );
    for invalid in [
        vec![ModeId(4), ModeId(4)],
        vec![ModeId(1), ModeId(2), ModeId(4)],
    ] {
        let before = game.projection(View::Referee).unwrap();
        game.answer_internal_card(mode.actor, mode.token, Answer::Modes(invalid))
            .unwrap_err();
        assert_eq!(game.projection(View::Referee).unwrap(), before);
    }
    let targeting = game
        .answer_internal_card(
            mode.actor,
            mode.token,
            Answer::Modes(vec![ModeId(1), ModeId(4)]),
        )
        .unwrap();
    assert!(
        matches!(&targeting, CardPlay::Awaiting(question) if matches!(question.domain, InputDomain::Targets(_)))
    );
    let before_target = game.projection(View::Referee).unwrap();
    assert_eq!(before_target["P1"]["hand"], json!([]));
    assert_eq!(before_target["P2"]["field"], json!(["amulet"]));
    finish(&mut game, targeting, &[], false);
    settle(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["cemetery"], json!(["amulet"]));
    assert_eq!(state["P1"]["hand"], json!(["draw1", "draw2"]));
    assert_eq!(state["P1"]["deck"], json!(["draw3"]));
    assert_eq!(state["P1"]["pp"]["current"], 3_i32);
}

#[test]
fn bp15_117_without_an_amulet_offers_the_other_modes_and_combines_damage_with_storm() {
    let mut initial = position("BP15-117");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"ally","card":"BP01-037"}]);
    let mut game = game(&initial);
    enter(&mut game);
    let fanfare = trigger(&mut game, "BP15-117");
    let CardPlay::Awaiting(question) = &fanfare else {
        panic!("mode choice");
    };
    assert!(
        matches!(&question.domain, InputDomain::Modes { modes, .. } if !modes.contains(&ModeId(1)))
    );
    finish(&mut game, fanfare, &[2, 3], false);
    settle(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["damage"], 3_i32);
    assert_eq!(state["objects"]["ally"]["damage"], 0_i32);
    assert!(
        state["objects"]["source"]["keywords"]
            .as_array()
            .unwrap()
            .contains(&json!("疾走"))
    );
    assert_eq!(state["P1"]["hand"], json!([]));
}

#[test]
fn bp16_052_can_create_a_stack_without_an_enemy_or_earth_rite_payment() {
    let mut initial = position("BP16-052");
    initial["players"]["P2"]["zones"]["field"] = json!([]);
    let mut game = game(&initial);
    enter(&mut game);
    let fanfare = trigger(&mut game, "BP16-052");
    let CardPlay::Awaiting(question) = &fanfare else {
        panic!("mode choice");
    };
    assert!(
        matches!(&question.domain, InputDomain::Modes { modes, .. } if modes == &vec![ModeId(1)])
    );
    let trace = finish(&mut game, fanfare, &[1], false);
    assert_eq!(trace, vec!["modes"]);
    settle(&mut game);
    let state = game.projection(View::Referee).unwrap();
    let field = state["P1"]["field"].as_array().unwrap();
    let soil = field
        .iter()
        .find(|id| state["objects"][id.as_str().unwrap()]["name"] == "大地の魔片")
        .unwrap();
    assert_eq!(field.len(), 2);
    assert_eq!(
        state["objects"][soil.as_str().unwrap()]["counters"]["スタックカウンター"],
        1_i32
    );
    assert_eq!(state["P1"]["pp"]["current"], 8_i32);
}

#[test]
fn bp16_052_mode_two_targets_before_its_optional_stack_payment() {
    for accept in [false, true] {
        let mut initial = position("BP16-052");
        initial["players"]["P1"]["zones"]["field"] =
            json!([{"id":"soil","card":"BP03-T02","state":{"counters":{"stack_counter":1_i32}}}]);
        let mut game = game(&initial);
        enter(&mut game);
        let fanfare = trigger(&mut game, "BP16-052");
        let mut play = fanfare;
        let mut target_seen = false;
        let mut materials_seen = false;
        while let CardPlay::Awaiting(question) = play {
            let answer = match &question.domain {
                InputDomain::Modes { .. } => Answer::Modes(vec![ModeId(2)]),
                InputDomain::OptionalCost => Answer::OptionalCost(accept),
                InputDomain::Targets(domain) => {
                    target_seen = true;
                    let before_target = game.projection(View::Referee).unwrap();
                    assert_eq!(
                        before_target["objects"]["soil"]["counters"]["スタックカウンター"], 1_i32,
                        "the formal target is selected before the Stack payment commits"
                    );
                    assert_eq!(
                        domain.members.len(),
                        1,
                        "the scenario leaves exactly one legal candidate"
                    );
                    Answer::Targets(vec![domain.members[0].handle])
                }
                InputDomain::CostMaterials(domain) => {
                    materials_seen = true;
                    assert!(accept, "declined payment never requests materials");
                    assert_eq!(
                        domain.members.len(),
                        1,
                        "the scenario leaves exactly one legal candidate"
                    );
                    assert_eq!(domain.members[0].card.as_deref(), Some("BP03-T02"));
                    Answer::CostMaterials(vec![domain.members[0].handle])
                }
                _ => panic!("unexpected mode payment question: {:?}", question.domain),
            };
            play = game
                .answer_internal_card(question.actor, question.token, answer)
                .unwrap();
        }
        assert!(
            !matches!(play, CardPlay::Cancelled),
            "the complete printed ability resolves"
        );
        assert!(
            target_seen,
            "declining payment does not remove the formal target requirement"
        );
        assert_eq!(materials_seen, accept);
        settle(&mut game);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["objects"]["enemy"]["damage"],
            if accept { 3_i32 } else { 0_i32 }
        );
        assert_eq!(
            state["P1"]["field"]
                .as_array()
                .unwrap()
                .contains(&json!("soil")),
            !accept
        );
        assert_eq!(state["P1"]["pp"]["current"], 8_i32);
        if !accept {
            assert_eq!(
                state["objects"]["soil"]["counters"]["スタックカウンター"],
                1_i32
            );
        }
    }
}
