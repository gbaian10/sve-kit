use super::*;

#[test]
fn ecp02_053_lesson_behavior_and_all_printings() {
    for number in ["ECP02-053", "ECP02-P26"] {
        let (game, before) = lesson::play_with_lesson(number, false, json!([]));
        let view = game.projection(View::Referee).unwrap();
        paid(&view, &before, 1, 0);
        assert_eq!(view["objects"]["enemy"]["damage"], 1_u8);
        assert_eq!(view["P1"]["leader"]["life"], 11_u8);
    }
}

#[test]
fn ecp02_060_lesson_behavior_and_all_printings() {
    for number in ["ECP02-060", "ECP02-SL28"] {
        let (game, before) = lesson::play_with_lesson(number, false, json!([]));
        let view = game.projection(View::Referee).unwrap();
        paid(&view, &before, 1, 0);
        assert_eq!(view["objects"]["enemy"]["power"], 2_u8);
        assert_eq!(view["objects"]["enemy"]["hp"], 9_u8);
    }
}

#[test]
fn ecp02_067_lesson_behavior_and_all_printings() {
    for number in ["ECP02-067", "ECP02-P35"] {
        let (game, before) = lesson::play_with_lesson(number, false, json!([]));
        let view = game.projection(View::Referee).unwrap();
        paid(&view, &before, 2, 0);
        assert_eq!(view["P2"]["field"], json!(["other"]));
        assert_eq!(view["P2"]["banish"], json!(["enemy"]));
    }
}

#[test]
fn ecp02_sp03a_lesson_behavior_and_all_printings() {
    for number in ["ECP02-SP03a", "ECP02-SP03b", "ECP02-U03a", "ECP02-U03b"] {
        let (game, before) = lesson::play_with_lesson(number, false, json!([]));
        let view = game.projection(View::Referee).unwrap();
        paid(&view, &before, 2, 0);
        assert_eq!(
            view["objects"]["enemy"]["damage"], 1_u8,
            "the source is the one Cool follower"
        );
    }
}

#[test]
fn ecp02_sp09_lesson_behavior_and_all_printings() {
    for number in ["ECP02-SP09", "ECP02-U09"] {
        let (game, before) = lesson::play_with_lesson(number, true, json!([]));
        let view = game.projection(View::Referee).unwrap();
        paid(&view, &before, 1, 0);
        assert_eq!(view["P1"]["hand"], json!(["drawn"]));
    }
}

#[test]
fn pr_436_lesson_behavior_and_all_printings() {
    let number = "PR-436";
    let (game, before) = lesson::play_with_lesson(number, true, json!([]));
    let view = game.projection(View::Referee).unwrap();
    paid(&view, &before, 1, 1);
    assert!(
        view["objects"]["source"]["keywords"]
            .as_array()
            .unwrap()
            .contains(&json!("疾走"))
    );
}
