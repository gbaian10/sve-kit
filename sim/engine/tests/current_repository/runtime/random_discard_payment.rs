use super::*;

#[test]
fn demonic_piper_pays_a_random_hand_card_before_gaining_attack_and_drain() {
    let mut initial = position_with_source("BP03-088", "hand");
    initial["players"]["P1"]["zones"]["hand"]
        .as_array_mut()
        .unwrap()
        .extend([
            json!({"id":"one","card":"BP01-171"}),
            json!({"id":"two","card":"BP18-005"}),
        ]);
    let mut game = game(&initial);
    assert!(matches!(
        enter_with_fanfare(&mut game, "BP03-088"),
        CardPlay::Resolved(_)
    ));
    checks(&mut game);

    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["P1"]["hand"].as_array().unwrap().len(), 1);
    assert_eq!(after["P1"]["cemetery"].as_array().unwrap().len(), 1);
    assert_eq!(after["objects"]["source"]["power"], 4);
    assert!(has_keyword(&after, "source", "ドレイン"));
}
