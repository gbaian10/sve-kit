#![expect(
    clippy::panic,
    reason = "Unexpected choices invalidate these complete printed discard scenarios."
)]
use super::event_cemetery::finish_checks;
use super::*;

#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "These printed cards ask only one material or target, Ward entry, or May."
)]
fn finish(game: &mut Game, mut play: CardPlay, preferred: Option<&str>, accept: bool) {
    while let CardPlay::Awaiting(question) = play {
        let answer = match &question.domain {
            InputDomain::Integer(bounds) => {
                assert_eq!(bounds.minimum, Nat(0), "May permits declining");
                assert_eq!(bounds.maximum, Some(Nat(1)), "May permits one acceptance");
                Answer::Integer(Nat(u64::from(accept)))
            }
            InputDomain::WardEntry { .. } => Answer::WardEntry(true),
            InputDomain::Targets(domain) | InputDomain::CostMaterials(domain) => {
                assert_eq!(domain.cardinality.minimum, Nat(1), "one printed member is required");
                assert_eq!(domain.cardinality.maximum, Some(Nat(1)), "only one printed member is accepted");
                let member = domain.members.iter().find(|member| {
                    preferred.is_none_or(|number| member.card.as_deref() == Some(number))
                }).unwrap();
                if matches!(question.domain, InputDomain::Targets(_)) {
                    Answer::Targets(vec![member.handle])
                } else {
                    Answer::CostMaterials(vec![member.handle])
                }
            }
            _ => panic!("unexpected printed choice: {:?}", question.domain),
        };
        play = game.answer_internal_card(question.actor, question.token, answer).unwrap();
    }
    assert!(matches!(play, CardPlay::Entered(_) | CardPlay::Resolved(_)), "the complete printed action enters or resolves");
    finish_checks(game);
}

fn pending(game: &mut Game, number: &str, line: u64) -> CardPlay {
    finish_checks(game);
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("a printed pending occurrence");
    };
    let option = options.iter().find(|option| option.card == number && option.line == line).unwrap();
    game.answer_internal_card(question.actor, question.token, Answer::Pending(option.handle)).unwrap()
}

fn returned(number: &str, accept: bool, current: u8, maximum: u8) -> Game {
    let mut initial = draw_position();
    initial["players"]["P1"]["leader"]["class"] = json!(if number == "BP12-021" { "ロイヤル" } else { "ドラゴン" });
    initial["players"]["P1"]["leader"]["life"] = json!(15_u8);
    initial["players"]["P1"]["pp"] = json!({"current":current,"max":maximum});
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"producer","card":"BP08-045"}, {"id":"source","card":number}
    ]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"draw0","card":"BP01-171"}, {"id":"draw1","card":"BP01-171"},
        {"id":"top0","card":"BP01-171"}, {"id":"top1","card":"BP01-171"},
        {"id":"tail","card":"BP01-171"}
    ]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = match number {
        "BP12-021" => json!([{"id":"form","card":"BP12-022"}]),
        "BP12-063" => json!([{"id":"form","card":"BP12-064"}]),
        _ => json!([]),
    };
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP20-111"}]);
    let mut game = Game::from_selected_effects(
        selection_with_rules().clone(), &initial, &Value::Null, &Value::Null, "discard-zero-light",
    ).unwrap();
    let produced = game.play_internal_card("producer", PlayerId { seat: 1 }).unwrap();
    finish(&mut game, produced, Some(number), true);
    let before = game.projection(View::Referee).unwrap();
    assert_eq!(before["P1"]["pp"]["current"], current.checked_sub(2).unwrap(), "the real discovery spell pays two PP");
    assert_eq!(before["P1"]["hand"], json!(["draw0", "draw1"]), "the real spell draws two then discards the chosen printed card");
    assert!(before["P1"]["cemetery"].as_array().unwrap().contains(&json!("source")), "the source really reaches the cemetery before its May");
    let discarded = pending(&mut game, number, 1);
    finish(&mut game, discarded, None, accept);
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["P1"]["ex"], if accept { json!(["source"]) } else { json!([]) }, "May follows the actual discarded generation only when accepted");
    assert!(game.internal_pending_choice().unwrap().is_none(), "the complete discarded occurrence finishes");
    game
}

#[test]
fn divine_tiger_may_stay_discarded_or_replay_and_deal_three_to_a_formal_target() {
    for accept in [false, true] {
        let mut game = returned("BP04-072", accept, 10, 10);
        if accept {
            let replayed = game.play_internal_card("source", PlayerId { seat: 1 }).unwrap();
            finish(&mut game, replayed, Some("BP20-111"), true);
            let view = game.projection(View::Referee).unwrap();
            assert_eq!(view["P1"]["pp"]["current"], 6_u8);
            assert_eq!(view["P2"]["cemetery"], json!(["enemy"]), "the real three-health follower takes the printed three damage");
            assert!(view["P1"]["cemetery"].as_array().unwrap().contains(&json!("source")));
        } else {
            assert!(game.projection(View::Referee).unwrap()["P1"]["cemetery"].as_array().unwrap().contains(&json!("source")));
        }
        assert!(game.internal_pending_choice().unwrap().is_none());
    }
}

#[test]
fn ironfist_replays_with_ward_evolves_and_uses_the_true_forms_next_ex_discount_once() {
    let mut game = returned("BP12-021", true, 10, 10);
    let entered = game.play_internal_card("source", PlayerId { seat: 1 }).unwrap();
    finish(&mut game, entered, None, true);
    let fanfare = pending(&mut game, "BP12-021", 4);
    finish(&mut game, fanfare, None, true);
    let normal = game.projection(View::Referee).unwrap();
    assert_eq!(normal["P1"]["pp"]["current"], 3_u8);
    assert_eq!(normal["P1"]["ex"], json!(["top0"]));
    assert_eq!(normal["objects"]["source"]["acted"], true, "the real Ward entry is chosen");
    assert!(normal["objects"]["source"]["keywords"].as_array().unwrap().contains(&json!("守護")));
    assert_eq!(game.internal_rule_action(&json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":3_u8}})).unwrap().outcome, "resolved");
    let evolution = pending(&mut game, "BP12-022", 2);
    finish(&mut game, evolution, None, true);
    let evolved = game.projection(View::Referee).unwrap();
    assert_eq!(evolved["P1"]["pp"]["current"], 0_u8, "the printed three-PP evolution is paid");
    assert_eq!(evolved["P1"]["evolution"], json!(["form"]));
    assert_eq!(evolved["P1"]["ex"], json!(["top0", "top1"]));
    let free = game.play_internal_card("top0", PlayerId { seat: 1 }).unwrap();
    finish(&mut game, free, None, true);
    let played = game.projection(View::Referee).unwrap();
    assert_eq!(played["P1"]["pp"]["current"], 0_u8);
    assert!(played["P1"]["field"].as_array().unwrap().contains(&json!("top0")), "the actual one-cost Goblin is free only because of the true evolved form");
    assert!(game.play_internal_card("top1", PlayerId { seat: 1 }).is_err(), "the next-only discount has been consumed");
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["P1"]["ex"], json!(["top1"]));
    assert_eq!(after["P1"]["deck"], json!(["tail"]));
    assert!(game.internal_pending_choice().unwrap().is_none());
}

#[test]
fn healing_dragoon_replays_then_heals_at_own_end_in_both_complete_faces() {
    for evolve in [false, true] {
        let mut game = returned("BP12-063", true, 10, 10);
        let entered = game.play_internal_card("source", PlayerId { seat: 1 }).unwrap();
        finish(&mut game, entered, None, true);
        if evolve {
            assert_eq!(game.internal_rule_action(&json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":2_u8}})).unwrap().outcome, "resolved");
        }
        let before = game.projection(View::Referee).unwrap();
        assert_eq!(before["P1"]["pp"]["current"], if evolve { 5_u8 } else { 7_u8 });
        assert_eq!(before["P1"]["leader"]["life"], 15_u8);
        game.internal_rule_action(&json!({"do":"end-phase"})).unwrap();
        let end = pending(&mut game, if evolve { "BP12-064" } else { "BP12-063" }, if evolve { 1 } else { 3 });
        finish(&mut game, end, None, true);
        assert_eq!(game.projection(View::Referee).unwrap()["P1"]["leader"]["life"], if evolve { 17_u8 } else { 16_u8 });
        assert!(game.internal_pending_choice().unwrap().is_none());
    }
}

#[test]
fn both_kit_prints_keep_rush_and_gain_power_and_assail_only_when_awakened() {
    for number in ["BP16-068", "BP16-P15"] {
        for awake in [false, true] {
            let mut game = returned(number, true, 6, if awake { 10 } else { 6 });
            let entered = game.play_internal_card("source", PlayerId { seat: 1 }).unwrap();
            finish(&mut game, entered, None, true);
            let fanfare = pending(&mut game, number, 3);
            finish(&mut game, fanfare, None, true);
            let view = game.projection(View::Referee).unwrap();
            assert_eq!(view["P1"]["pp"]["current"], 3_u8);
            assert_eq!(view["objects"]["source"]["power"], if awake { 3_u8 } else { 2_u8 });
            assert_eq!(view["objects"]["source"]["hp"], 1_u8);
            let keywords = view["objects"]["source"]["keywords"].as_array().unwrap();
            assert!(keywords.contains(&json!("突進")));
            assert_eq!(keywords.contains(&json!("指定攻撃")), awake);
            assert!(game.internal_pending_choice().unwrap().is_none());
        }
    }
}

#[test]
fn shared_feelings_always_creates_the_true_tree_but_heals_and_recovers_pp_only_when_awakened() {
    for awake in [false, true] {
        let mut game = returned("BP17-072", true, 6, if awake { 10 } else { 6 });
        let replayed = game.play_internal_card("source", PlayerId { seat: 1 }).unwrap();
        finish(&mut game, replayed, None, true);
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["P1"]["pp"]["current"], if awake { 3_u8 } else { 2_u8 });
        assert_eq!(view["P1"]["leader"]["life"], if awake { 16_u8 } else { 15_u8 });
        let field = view["P1"]["field"].as_array().unwrap();
        assert_eq!(field.len(), 1);
        assert_eq!(view["objects"][field[0].as_str().unwrap()]["card"], "BP07-T03", "both branches create the complete already-admitted Naterran Great Tree");
        assert_eq!(view["P1"]["hand"], json!(["draw0", "draw1"]));
        assert_eq!(view["P1"]["deck"], json!(["top0", "top1", "tail"]));
        assert!(view["P1"]["cemetery"].as_array().unwrap().contains(&json!("source")));
        assert!(game.internal_pending_choice().unwrap().is_none());
    }
}
