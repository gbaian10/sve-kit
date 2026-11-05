#![expect(
    clippy::panic,
    reason = "Unexpected choices invalidate the printed discard chain."
)]
use super::event_cemetery::finish_checks;
use super::*;

fn board(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "discard-tail-light",
    )
    .unwrap()
}

#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "Only these choices belong to the printed discard chain."
)]
fn finish(board: &mut Game, mut play: CardPlay, preferred: Option<&str>) {
    while let CardPlay::Awaiting(question) = play {
        let answer = match &question.domain {
            InputDomain::OptionalCost => Answer::OptionalCost(true),
            InputDomain::WardEntry { .. } => Answer::WardEntry(true),
            InputDomain::Targets(domain) | InputDomain::CostMaterials(domain) => {
                let chosen = domain
                    .members
                    .iter()
                    .filter(|member| {
                        preferred.is_none_or(|number| member.card.as_deref() == Some(number))
                    })
                    .take(usize::try_from(domain.cardinality.maximum.unwrap_or(Nat(1)).0).unwrap())
                    .map(|member| member.handle)
                    .collect::<Vec<_>>();
                assert!(
                    chosen.len() >= usize::try_from(domain.cardinality.minimum.0).unwrap(),
                    "the printed cost or target has enough actual members"
                );
                if matches!(question.domain, InputDomain::Targets(_)) {
                    Answer::Targets(chosen)
                } else {
                    Answer::CostMaterials(chosen)
                }
            }
            _ => panic!("unexpected printed choice: {:?}", question.domain),
        };
        play = board
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    assert!(
        matches!(play, CardPlay::Entered(_) | CardPlay::Resolved(_)),
        "the printed play must enter or resolve completely"
    );
    finish_checks(board);
}

fn pending(board: &mut Game, number: &str) -> CardPlay {
    finish_checks(board);
    let question = board.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("a printed pending occurrence");
    };
    let option = options.iter().find(|option| option.card == number).unwrap();
    board
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(option.handle),
        )
        .unwrap()
}

#[test]
fn every_print_joins_the_real_filtered_discard_cost_ramp_and_opposing_discard_observer() {
    for (shield, ignis, attendant) in [
        ("BP22-066", "BP22-056", "BP05-082"),
        ("BP22-P36", "BP22-SL14", "BP05-P39"),
        ("BP22-066", "BP22-U04", "BP05-082"),
    ] {
        let mut initial = draw_position();
        initial["turn"]["active"] = json!("P2");
        initial["players"]["P1"]["leader"]["class"] = json!("ナイトメア");
        initial["players"]["P2"]["leader"]["class"] = json!("ドラゴン");
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"observer","card":attendant}]);
        initial["players"]["P1"]["zones"]["deck"] = json!([
            {"id":"mill0","card":"BP01-171"},
            {"id":"mill1","card":"BP01-171"},
            {"id":"tail","card":"BP01-171"}
        ]);
        initial["players"]["P2"]["pp"] = json!({"current":8_u8,"max":8_u8});
        initial["players"]["P2"]["zones"]["field"] = json!([]);
        initial["players"]["P2"]["zones"]["hand"] = json!([
            {"id":"shield","card":shield},{"id":"material","card":ignis}
        ]);
        let mut game = board(&initial);
        let entered = game
            .play_internal_card("shield", PlayerId { seat: 2 })
            .unwrap();
        finish(&mut game, entered, None);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P2"]["pp"]["current"],
            5_u8,
            "the actual three-PP Shield Dragon enters before its fanfare"
        );
        let fanfare = pending(&mut game, shield);
        finish(&mut game, fanfare, Some(ignis));
        let paid = game.projection(View::Referee).unwrap();
        assert_eq!(
            paid["P2"]["pp"]["current"], 7_u8,
            "the paid fanfare recovers two PP"
        );
        assert_eq!(
            paid["P2"]["cemetery"],
            json!(["material"]),
            "the printed seven-cost Dragon was discarded"
        );
        let ramp = pending(&mut game, ignis);
        finish(&mut game, ramp, None);
        let damage = pending(&mut game, shield);
        finish(&mut game, damage, Some(attendant));
        let opposing = pending(&mut game, attendant);
        finish(&mut game, opposing, None);
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(
            view["P2"]["pp"],
            json!({"current":5_u8,"max":9_u8}),
            "the discarded Ignis pays two PP and raises only the maximum"
        );
        assert_eq!(
            view["objects"]["observer"]["hp"], 1_u8,
            "Shield's own-turn discard occurrence deals two real damage"
        );
        assert_eq!(
            view["P2"]["leader"]["life"], 18_u8,
            "the other player's discard triggers the attendant"
        );
        assert_eq!(
            view["P1"]["leader"]["life"], 20_u8,
            "the observer uses its own controller"
        );
        assert_eq!(
            view["P1"]["cemetery"],
            json!(["mill0", "mill1"]),
            "the complete attendant body also mills its own two cards"
        );
        assert_eq!(
            view["P1"]["deck"],
            json!(["tail"]),
            "milling preserves the remaining deck order"
        );
        assert_eq!(
            view["objects"]["shield"]["acted"], true,
            "the true Ward entry was chosen"
        );
        assert!(
            view["objects"]["shield"]["keywords"]
                .as_array()
                .unwrap()
                .contains(&json!("守護")),
            "the complete printed Ward remains present"
        );
        assert!(
            game.internal_pending_choice().unwrap().is_none(),
            "all three complete printed occurrences resolved exactly once"
        );
    }
}

#[test]
fn every_ignis_print_evolves_free_with_a_small_hand_and_pays_one_with_a_large_hand() {
    for number in ["BP22-056", "BP22-SL14", "BP22-U04"] {
        for remaining in [3_u8, 4] {
            let mut initial = draw_position();
            initial["players"]["P1"]["leader"]["class"] = json!("ドラゴン");
            initial["players"]["P1"]["zones"]["field"] = json!([]);
            let mut hand = vec![json!({"id":"source","card":number})];
            hand.extend((0..remaining).map(|i| json!({"id":format!("held{i}"),"card":"BP01-171"})));
            initial["players"]["P1"]["zones"]["hand"] = json!(hand);
            initial["players"]["P1"]["zones"]["evolve_deck"] =
                json!([{"id":"form","card":"BP22-057"}]);
            let mut game = board(&initial);
            let entered = game
                .play_internal_card("source", PlayerId { seat: 1 })
                .unwrap();
            finish(&mut game, entered, None);
            let fanfare = pending(&mut game, number);
            finish(&mut game, fanfare, Some("BP22-057"));
            let before = game.projection(View::Referee).unwrap();
            assert_eq!(
                before["P1"]["pp"]["current"], 3_u8,
                "the normal face really pays seven PP"
            );
            if remaining == 4 {
                assert_eq!(
                    before["P1"]["evolution"],
                    json!([]),
                    "four remaining cards fail the fanfare threshold"
                );
                assert_eq!(game.internal_rule_action(&json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})).unwrap().outcome, "resolved", "the complete printed one-PP evolution remains usable");
            }
            let view = game.projection(View::Referee).unwrap();
            assert_eq!(
                view["P1"]["evolution"],
                json!(["form"]),
                "both legal paths use the real already-admitted evolved form"
            );
            assert_eq!(
                view["P1"]["pp"]["current"],
                if remaining == 4 { 2_u8 } else { 3_u8 },
                "effect evolution is free and ordinary evolution pays its printed cost"
            );
        }
    }
}
