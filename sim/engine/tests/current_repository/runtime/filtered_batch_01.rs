#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate printed card scenarios."
)]
use super::*;

fn finish(game: &mut Game, mut play: CardPlay) {
    while let CardPlay::Awaiting(question) = play {
        let answer = if matches!(&question.domain, InputDomain::WardEntry { .. }) {
            Answer::WardEntry(false)
        } else if matches!(&question.domain, InputDomain::OptionalCost) {
            Answer::OptionalCost(true)
        } else if let InputDomain::CostMaterials(domain) = &question.domain {
            Answer::CostMaterials(domain.members.iter().take(1).map(|m| m.handle).collect())
        } else {
            let InputDomain::Targets(domain) = &question.domain else {
                panic!("unexpected printed choice: {:?}", question.domain);
            };
            Answer::Targets(domain.members.iter().take(1).map(|m| m.handle).collect())
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    assert!(
        !matches!(play, CardPlay::Cancelled),
        "the printed play has legal targets and payment"
    );
}

fn trigger(game: &mut Game) {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("pending trigger")
    };
    assert_eq!(options.len(), 1, "one printed Fanfare is pending");
    let play = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap();
    finish(game, play);
}

fn position(card: &str, material: &str, count: usize, payment: bool) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    if payment {
        initial["players"]["P1"]["zones"]["hand"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"payment","card":material}));
    }
    initial["players"]["P1"]["zones"]["cemetery"] = json!(
        (0..count)
            .map(|i| json!({"id":format!("grave-{i}"),"card":material}))
            .collect::<Vec<_>>()
    );
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"top","card":"BP01-037"},{"id":"next","card":"BP01-037"},{"id":"last","card":"BP01-037"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP04-117"}]);
    initial
}

#[test]
fn filtered_batch_01_bp05_011_checks_printed_effect() {
    for card in ["BP05-011", "BP05-P06"] {
        for count in [2, 3] {
            let mut game = game(&position(card, "BP01-006", count, false));
            let play = game
                .play_internal_card("source", PlayerId { seat: 1 })
                .unwrap();
            finish(&mut game, play);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["P1"]["hand"].as_array().unwrap().len(),
                usize::from(count >= 3)
            );
            assert_eq!(state["objects"]["enemy"]["damage"], 7_u8);
        }
    }
}

#[test]
fn filtered_batch_01_bp06_012_checks_printed_effect() {
    for card in ["BP06-012", "PR-130"] {
        for count in [2, 3] {
            let mut game = game(&position(card, "BP01-006", count, false));
            let play = game
                .play_internal_card("source", PlayerId { seat: 1 })
                .unwrap();
            finish(&mut game, play);
            trigger(&mut game);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["objects"]["source"]["hp"],
                if count >= 3 { 3_u8 } else { 2_u8 }
            );
        }
    }
}

#[test]
fn filtered_batch_01_bp08_015_checks_printed_effect() {
    for card in ["BP08-015"] {
        for count in [2, 3] {
            let mut game = game(&position(card, "BP01-006", count, false));
            let play = game
                .play_internal_card("source", PlayerId { seat: 1 })
                .unwrap();
            finish(&mut game, play);
            trigger(&mut game);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["objects"]["enemy"]["power"],
                if count >= 3 { 4_i64 } else { 8_i64 }
            );
        }
    }
}

#[test]
fn filtered_batch_01_bp08_029_checks_printed_effect() {
    for card in ["BP08-029"] {
        for count in [1, 2] {
            let mut game = game(&position(card, "BP01-029", count, false));
            let play = game
                .play_internal_card("source", PlayerId { seat: 1 })
                .unwrap();
            finish(&mut game, play);
            trigger(&mut game);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["objects"]["source"]["keywords"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("必殺")),
                count >= 2
            );
            assert_eq!(
                state["objects"]["source"]["power"],
                if count >= 2 { 3_u8 } else { 2_u8 }
            );
            assert_eq!(
                state["objects"]["source"]["hp"],
                if count >= 2 { 3_u8 } else { 2_u8 }
            );
        }
    }
}

#[test]
fn filtered_batch_01_bp08_047_checks_printed_effect() {
    for card in ["BP08-047"] {
        for count in [4, 5] {
            let mut game = game(&position(card, "BP08-047", count, false));
            let play = game
                .play_internal_card("source", PlayerId { seat: 1 })
                .unwrap();
            finish(&mut game, play);
            trigger(&mut game);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["objects"]["source"]["power"],
                if count >= 5 { 3_u8 } else { 2_u8 }
            );
            assert_eq!(
                state["objects"]["source"]["hp"],
                if count >= 5 { 5_u8 } else { 4_u8 }
            );
        }
    }
}

#[test]
fn filtered_batch_01_bp09_044_checks_printed_effect() {
    for card in ["BP09-044", "DSD01a-012", "PR-227"] {
        for count in [4, 5] {
            let mut game = game(&position(card, "BP09-044", count, false));
            let play = game
                .play_internal_card("source", PlayerId { seat: 1 })
                .unwrap();
            finish(&mut game, play);
            trigger(&mut game);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["P1"]["pp"]["current"],
                if count >= 5 { 8_u8 } else { 7_u8 }
            );
            assert_eq!(state["P1"]["hand"], json!(["top"]));
        }
    }
}

#[test]
fn filtered_batch_01_bp09_086_checks_printed_effect() {
    for card in [
        "BP09-086",
        "BP09-SL16",
        "SP01-040",
        "SP01-SL40",
        "SP01-SP26",
        "SP01-U24",
    ] {
        for count in [1, 2] {
            let mut game = game(&position(card, "BP01-127", count, false));
            let play = game
                .play_internal_card("source", PlayerId { seat: 1 })
                .unwrap();
            finish(&mut game, play);
            trigger(&mut game);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["objects"]["source"]["keywords"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("疾走")),
                count >= 2
            );
            assert_eq!(
                state["objects"]["source"]["power"],
                if count >= 2 { 3_u8 } else { 2_u8 }
            );
            assert_eq!(
                state["objects"]["source"]["hp"],
                if count >= 2 { 4_u8 } else { 3_u8 }
            );
        }
    }
}

#[test]
fn filtered_batch_01_bp12_080_checks_printed_effect() {
    for card in ["BP12-080"] {
        for count in [0, 2] {
            let mut game = game(&position(card, "BP12-080", count, false));
            let play = game
                .play_internal_card("source", PlayerId { seat: 1 })
                .unwrap();
            finish(&mut game, play);
            trigger(&mut game);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["objects"]["source"]["power"],
                5_u64 + u64::try_from(count).unwrap()
            );
            assert_eq!(
                state["objects"]["source"]["hp"],
                4_u64 + u64::try_from(count).unwrap()
            );
        }
    }
}

#[test]
fn filtered_batch_01_bp13_030_checks_printed_effect() {
    for card in ["BP13-030", "SD07-017"] {
        for count in [4, 5] {
            let mut game = game(&position(card, "BP02-018", count, false));
            let play = game
                .play_internal_card("source", PlayerId { seat: 1 })
                .unwrap();
            finish(&mut game, play);
            trigger(&mut game);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["objects"]["enemy"]["damage"],
                if count >= 5 { 3_u8 } else { 0_u8 }
            );
        }
    }
}

#[test]
fn filtered_batch_01_bp14_015_checks_printed_effect() {
    for card in ["BP14-015"] {
        for count in [2, 3] {
            let mut game = game(&position(card, "BP01-006", count, false));
            let play = game
                .play_internal_card("source", PlayerId { seat: 1 })
                .unwrap();
            finish(&mut game, play);
            trigger(&mut game);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["objects"]["source"]["keywords"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("指定攻撃")),
                count >= 3
            );
            assert_eq!(
                state["objects"]["source"]["power"],
                if count >= 3 { 4_u8 } else { 2_u8 }
            );
        }
    }
}

#[test]
fn filtered_batch_01_bp14_108_checks_printed_effect() {
    for card in ["BP14-108"] {
        for count in [4, 5] {
            let mut game = game(&position(card, "BP14-108", count, false));
            let play = game
                .play_internal_card("source", PlayerId { seat: 1 })
                .unwrap();
            finish(&mut game, play);
            trigger(&mut game);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["P1"]["leader"]["life"],
                if count >= 5 { 12_u8 } else { 10_u8 }
            );
        }
    }
}

#[test]
fn filtered_batch_01_bp15_014_checks_printed_effect() {
    for card in ["BP15-014", "BP15-P03"] {
        for count in [2, 3] {
            let mut game = game(&position(card, "BP01-006", count, false));
            let play = game
                .play_internal_card("source", PlayerId { seat: 1 })
                .unwrap();
            finish(&mut game, play);
            trigger(&mut game);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["objects"]["enemy"]["hp"],
                if count >= 3 { 1_u8 } else { 8_u8 }
            );
        }
    }
}

#[test]
fn filtered_batch_01_bp18_076_checks_printed_effect() {
    for card in ["BP18-076", "BP18-P31"] {
        for count in [4, 5] {
            let mut game = game(&position(card, "BP18-076", count, false));
            let play = game
                .play_internal_card("source", PlayerId { seat: 1 })
                .unwrap();
            finish(&mut game, play);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["objects"]["enemy"]["damage"],
                if count >= 5 { 4_u8 } else { 2_u8 }
            );
        }
    }
}

#[test]
fn filtered_batch_01_bp20_008_checks_printed_effect() {
    for card in ["BP20-008", "BP20-P03"] {
        for count in [5, 6] {
            let mut game = game(&position(card, "BP01-006", count, false));
            let play = game
                .play_internal_card("source", PlayerId { seat: 1 })
                .unwrap();
            finish(&mut game, play);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["objects"]["enemy"]["damage"],
                if count >= 6 { 3_u8 } else { 2_u8 }
            );
            assert_eq!(
                state["P2"]["leader"]["life"],
                if count >= 6 { 18_u8 } else { 20_u8 }
            );
        }
    }
}

#[test]
fn filtered_batch_01_bp22_089_checks_printed_effect() {
    for card in ["BP22-089", "BP22-P50"] {
        for count in [1, 2] {
            let mut game = game(&position(card, "BP22-089", count, true));
            let play = game
                .play_internal_card("source", PlayerId { seat: 1 })
                .unwrap();
            finish(&mut game, play);
            trigger(&mut game);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["objects"]["source"]["power"],
                if count + 1 >= 3 { 3_u8 } else { 2_u8 }
            );
        }
    }
}
