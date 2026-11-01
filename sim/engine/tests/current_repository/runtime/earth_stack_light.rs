#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected choices invalidate the printed Stack scenarios."
)]
use super::*;
use sve_engine::game::internal::InstructionOutcome;

fn position(card: &str) -> Value {
    let mut initial = draw_position();
    let zone = if matches!(card, "BP21-T04" | "BP21-T05") {
        "ex"
    } else {
        "hand"
    };
    initial["players"]["P1"]["zones"][zone] = json!([{"id":"source","card":card}]);
    for seat in ["P1", "P2"] {
        initial["players"][seat]["zones"]["deck"] = json!([
            {"id":format!("{seat}-draw0"),"card":"BP01-037"},
            {"id":format!("{seat}-draw1"),"card":"BP01-037"},
            {"id":format!("{seat}-draw2"),"card":"BP01-037"}
        ]);
    }
    initial
}

fn enter(game: &mut Game) {
    let CardPlay::Entered(entry) = game
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("printed amulet entry");
    };
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["source"]["counters"]["スタックカウンター"],
        1_u8,
        "Stack enters with one counter"
    );
    if entry.resume == ResumeStage::CheckTiming {
        game.resume_internal_card().unwrap();
    }
}

fn fanfare(game: &mut Game, card: &str, mode: usize) -> Activation {
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("printed Fanfare");
    };
    let option = options.iter().find(|option| option.card == card).unwrap();
    let mut play = game
        .answer_internal_card(pending.actor, pending.token, Answer::Pending(option.handle))
        .unwrap();
    while let CardPlay::Awaiting(question) = play {
        let answer = match &question.domain {
            InputDomain::Targets(domain) => {
                assert_eq!(
                    domain.members.len(),
                    1,
                    "the printed filters leave one legal target"
                );
                Answer::Targets(vec![domain.members[0].handle])
            }
            InputDomain::Modes { modes, .. } => Answer::Modes(vec![modes[mode]]),
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            _ => panic!("unexpected Fanfare decision"),
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    let result = resolved(play);
    game.resume_internal_card().unwrap();
    result
}

fn named(view: &Value, zone: &str, name: &str) -> usize {
    view["P1"][zone]
        .as_array()
        .unwrap()
        .iter()
        .filter(|id| view["objects"][id.as_str().unwrap()]["name"] == name)
        .count()
}

#[test]
fn bp01_066_banishes_only_an_opposing_follower_with_at_most_four_current_life() {
    for card in ["BP01-066", "BP01-P14"] {
        let mut initial = position(card);
        initial["players"]["P2"]["zones"]["field"] = json!([
            {"id":"eligible","card":"BP01-037","state":{"damage":1_u8}},
            {"id":"healthy","card":"BP01-037"}
        ]);
        initial["players"]["P1"]["zones"]["field"] =
            json!([{"id":"ally","card":"BP01-037","state":{"damage":1_u8}}]);
        let mut game = game(&initial);
        enter(&mut game);
        fanfare(&mut game, card, 0);
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["P2"]["banish"], json!(["eligible"]));
        assert_eq!(view["P2"]["field"], json!(["healthy"]));
        assert_eq!(view["P1"]["field"], json!(["ally", "source"]));
    }
}

#[test]
fn bp03_048_heals_its_controller_while_retaining_the_stack_amulet() {
    for card in ["BP03-048", "PR-121"] {
        let mut initial = position(card);
        initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
        initial["players"]["P2"]["leader"]["life"] = json!(9_u8);
        let mut game = game(&initial);
        enter(&mut game);
        fanfare(&mut game, card, 0);
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["P1"]["leader"]["life"], 13_u8);
        assert_eq!(view["P2"]["leader"]["life"], 9_u8);
        assert_eq!(view["P1"]["field"], json!(["source"]));
    }
}

#[test]
fn bp04_056_privately_looks_at_one_card_without_drawing_or_reordering() {
    for card in ["BP04-056", "BP04-P15"] {
        let mut game = game(&position(card));
        enter(&mut game);
        let result = fanfare(&mut game, card, 0);
        assert!(result.progress.receipts.values().any(
            |receipt| matches!(receipt, InstructionOutcome::Look(look) if look.looked.len() == 1)
        ));
        let owner = game.projection(View::P1).unwrap();
        let opponent = game.projection(View::P2).unwrap();
        assert_eq!(owner["objects"]["P1-draw0"]["card"], "BP01-037");
        assert_eq!(opponent["objects"]["P1-draw0"], Value::Null);
        assert_eq!(owner["P1"]["hand"], json!([]));
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["deck"],
            json!(["P1-draw0", "P1-draw1", "P1-draw2"])
        );
    }
}

#[test]
fn bp06_053_revives_an_own_original_three_cost_alchemist() {
    let mut initial = position("BP06-053");
    initial["players"]["P1"]["zones"]["cemetery"] = json!([
        {"id":"revived","card":"BP02-042"},
        {"id":"wrong-trait","card":"BP01-037"},
        {"id":"expensive","card":"BP10-048"}
    ]);
    initial["players"]["P2"]["zones"]["cemetery"] = json!([{"id":"opposing","card":"BP02-042"}]);
    let mut game = game(&initial);
    let before = game.projection(View::Referee).unwrap()["objects"]["revived"]["generation"]
        .as_u64()
        .unwrap();
    enter(&mut game);
    fanfare(&mut game, "BP06-053", 0);
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["field"], json!(["source", "revived"]));
    assert_eq!(
        view["objects"]["revived"]["generation"],
        before.saturating_add(1)
    );
    assert_eq!(view["P1"]["cemetery"], json!(["wrong-trait", "expensive"]));
    assert_eq!(view["P2"]["cemetery"], json!(["opposing"]));
}

#[test]
fn bp14_052_damages_the_opposing_target_and_keeps_its_stack() {
    for card in ["BP14-052", "PR-398"] {
        let mut initial = position(card);
        initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
        let mut game = game(&initial);
        enter(&mut game);
        fanfare(&mut game, card, 0);
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["objects"]["enemy"]["damage"], 2_u8);
        assert_eq!(
            view["objects"]["source"]["counters"]["スタックカウンター"],
            1_u8
        );
    }
}

#[test]
fn bp21_044_creates_exactly_the_chosen_stack_token_in_ex_without_entering_it() {
    for card in ["BP21-044", "BP21-P21"] {
        for (mode, expected, other) in [
            (0_usize, "緊急召喚", "術式結界"),
            (1, "術式結界", "緊急召喚"),
        ] {
            let mut game = game(&position(card));
            enter(&mut game);
            fanfare(&mut game, card, mode);
            let view = game.projection(View::Referee).unwrap();
            assert_eq!(named(&view, "ex", expected), 1);
            assert_eq!(named(&view, "ex", other), 0);
            assert_eq!(view["P1"]["field"], json!(["source"]));
            assert_eq!(view["P1"]["hand"], json!([]));
            assert_eq!(view["P1"]["pp"]["current"], 8_u8);
        }
    }
}

fn end(game: &mut Game) {
    assert_eq!(
        game.internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap()
            .outcome,
        "resolved",
        "normal turn timing advances"
    );
    while let Some(quick) = game.internal_quick_choice().unwrap() {
        game.answer_internal_card(quick.actor, quick.token, Answer::Quick(None))
            .unwrap();
    }
}

#[test]
fn bp21_054_acts_the_target_and_blocks_only_its_next_start_stand() {
    for card in ["BP21-054", "BP21-P28"] {
        let mut initial = position(card);
        initial["players"]["P2"]["zones"]["field"] = json!([
            {"id":"enemy","card":"BP01-037"},
            {"id":"control","card":"BP01-097","state":{"acted":true}}
        ]);
        let mut game = game(&initial);
        enter(&mut game);
        let pending = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = &pending.domain else {
            panic!("printed Fanfare");
        };
        let CardPlay::Awaiting(target) = game
            .answer_internal_card(
                pending.actor,
                pending.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap()
        else {
            panic!("target before resting");
        };
        let InputDomain::Targets(domain) = &target.domain else {
            panic!("public follower targets");
        };
        assert_eq!(domain.members.len(), 2);
        resolved(
            game.answer_internal_card(
                target.actor,
                target.token,
                Answer::Targets(vec![
                    domain
                        .members
                        .iter()
                        .find(|member| member.card.as_deref() == Some("BP01-037"))
                        .unwrap()
                        .handle,
                ]),
            )
            .unwrap(),
        );
        game.resume_internal_card().unwrap();
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["enemy"]["acted"],
            true
        );
        end(&mut game);
        let first = game.projection(View::Referee).unwrap();
        assert_eq!(first["objects"]["enemy"]["acted"], true);
        assert_eq!(first["objects"]["control"]["acted"], false);
        end(&mut game);
        end(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["enemy"]["acted"],
            false
        );
    }
}

fn alchemist(card: &str, qualified: bool) -> Value {
    let mut initial = position(card);
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"alchemist","card":if qualified {"BP02-042"} else {"BP13-040"}}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"opposing","card":"BP02-042"}]);
    initial
}

#[test]
fn bp21_t04_creates_a_defender_and_adds_a_guardian_only_for_own_three_cost_alchemist() {
    for qualified in [false, true] {
        let mut game = game(&alchemist("BP21-T04", qualified));
        enter(&mut game);
        fanfare(&mut game, "BP21-T04", 0);
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(named(&view, "field", "防御型ゴーレム"), 1);
        assert_eq!(
            named(&view, "ex", "ガーディアンゴーレム"),
            usize::from(qualified)
        );
        assert_eq!(view["P1"]["pp"]["current"], 10_u8);
    }
}

#[test]
fn bp21_t05_draws_two_instead_of_one_only_for_an_own_qualifying_alchemist() {
    for qualified in [false, true] {
        let mut game = game(&alchemist("BP21-T05", qualified));
        enter(&mut game);
        fanfare(&mut game, "BP21-T05", 0);
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(
            view["P1"]["hand"],
            if qualified {
                json!(["P1-draw0", "P1-draw1"])
            } else {
                json!(["P1-draw0"])
            }
        );
        assert_eq!(
            view["objects"]["source"]["counters"]["スタックカウンター"],
            1_u8
        );
    }
}

fn earth_position(card: &str) -> Value {
    let mut value = position(card);
    value["players"]["P1"]["leader"]["life"] = json!(10_u8);
    value["players"]["P1"]["zones"]["field"] = json!([
        {"id":"soil","card":"BP01-075","state":{"counters":{"stack_counter":1_u8}}},
        {"id":"ally","card":"BP01-037"}
    ]);
    value["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-097"}]);
    value
}

fn earth_play(game: &mut Game, card: &str, accepted: bool) {
    let mut play = game
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap();
    loop {
        while let CardPlay::Awaiting(question) = play {
            let answer = match &question.domain {
                InputDomain::OptionalCost => Answer::OptionalCost(accepted),
                InputDomain::CostMaterials(domain) => {
                    assert!(accepted, "declining Earth Rite never selects materials");
                    assert_eq!(domain.members.len(), 1, "only the own field Stack can pay");
                    assert_eq!(
                        domain.members[0].card.as_deref(),
                        Some("BP01-075"),
                        "the selected material is the real Stack amulet"
                    );
                    Answer::CostMaterials(vec![domain.members[0].handle])
                }
                InputDomain::Targets(domain) => {
                    assert_eq!(
                        domain.members.len(),
                        1,
                        "only the printed opposing target is legal"
                    );
                    Answer::Targets(vec![domain.members[0].handle])
                }
                InputDomain::WardEntry { .. } => Answer::WardEntry(false),
                _ => panic!("unexpected printed Earth Rite decision"),
            };
            play = game
                .answer_internal_card(question.actor, question.token, answer)
                .unwrap();
        }
        if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
        let Some(pending) = game.internal_pending_choice().unwrap() else {
            break;
        };
        let InputDomain::Pending(options) = &pending.domain else {
            panic!("Fanfare pending");
        };
        let option = options.iter().find(|option| option.card == card).unwrap();
        play = game
            .answer_internal_card(pending.actor, pending.token, Answer::Pending(option.handle))
            .unwrap();
    }
}

#[test]
fn bp01_054_and_pr08_put_two_defenders_in_ex_only_after_paying_earth_rite() {
    for card in ["BP01-054", "BP21-PR08"] {
        for pay in [false, true] {
            let mut game = game(&earth_position(card));
            earth_play(&mut game, card, pay);
            let view = game.projection(View::Referee).unwrap();
            assert_eq!(
                named(&view, "ex", "防御型ゴーレム"),
                if pay { 2 } else { 0 }
            );
            assert_eq!(
                view["P1"]["cemetery"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("soil")),
                pay
            );
        }
        let mut initial = earth_position(card);
        initial["players"]["P1"]["zones"]["ex"] = json!([
            {"id":"e0","card":"BP01-037"},{"id":"e1","card":"BP01-037"},
            {"id":"e2","card":"BP01-037"},{"id":"e3","card":"BP01-037"},
            {"id":"e4","card":"BP01-037"}
        ]);
        let mut game = game(&initial);
        earth_play(&mut game, card, true);
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["P1"]["ex"], json!(["e0", "e1", "e2", "e3", "e4"]));
        assert!(
            view["P1"]["cemetery"]
                .as_array()
                .unwrap()
                .contains(&json!("soil")),
            "an optional cost stays paid when the EX capacity prevents creation"
        );
    }
}

#[test]
fn bp02_044_always_selects_a_follower_and_banishes_it_only_when_earth_rite_is_paid() {
    for pay in [false, true] {
        let mut game = game(&earth_position("BP02-044"));
        earth_play(&mut game, "BP02-044", pay);
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(
            view["P2"]["banish"],
            if pay { json!(["enemy"]) } else { json!([]) }
        );
        assert!(
            view["P1"]["field"]
                .as_array()
                .unwrap()
                .contains(&json!("ally"))
        );
    }
}

fn earth_draw(card: &str) {
    for pay in [false, true] {
        let mut game = game(&earth_position(card));
        earth_play(&mut game, card, pay);
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(
            view["P1"]["leader"]["life"], 13_u8,
            "healing is unconditional"
        );
        assert_eq!(
            view["P1"]["hand"].as_array().unwrap().len(),
            if pay { 2 } else { 1 },
            "Earth Rite replaces one draw with two"
        );
        assert_eq!(
            view["P2"]["hand"].as_array().unwrap().len(),
            0,
            "only the controller draws"
        );
    }
}

#[test]
fn bp04_049_and_ebd02_014_heal_then_replace_one_draw_with_two_when_paid() {
    for card in ["BP04-049", "EBD02-014"] {
        earth_draw(card);
    }
}

#[test]
fn bp04_050_heals_then_replaces_one_draw_with_two_when_paid() {
    earth_draw("BP04-050");
}

#[test]
fn bp04_053_retains_intimidate_and_only_buffs_itself_after_payment() {
    for pay in [false, true] {
        let mut game = game(&earth_position("BP04-053"));
        earth_play(&mut game, "BP04-053", pay);
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(
            view["objects"]["source"]["power"],
            if pay { 5_u8 } else { 3_u8 }
        );
        assert_eq!(
            view["objects"]["source"]["hp"],
            if pay { 4_u8 } else { 3_u8 }
        );
        assert_eq!(view["objects"]["ally"]["power"], 5_u8);
        assert!(
            view["objects"]["source"]["keywords"]
                .as_array()
                .unwrap()
                .contains(&json!("威圧"))
        );
    }
}

#[test]
fn bp04_054_retains_ward_and_buffs_its_controller_followers_only_when_paid() {
    for pay in [false, true] {
        let mut game = game(&earth_position("BP04-054"));
        earth_play(&mut game, "BP04-054", pay);
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(
            view["objects"]["source"]["power"],
            if pay { 6_u8 } else { 5_u8 }
        );
        assert_eq!(
            view["objects"]["ally"]["power"],
            if pay { 6_u8 } else { 5_u8 }
        );
        assert_eq!(view["objects"]["enemy"]["power"], 7_u8);
        assert!(
            view["objects"]["source"]["keywords"]
                .as_array()
                .unwrap()
                .contains(&json!("守護"))
        );
    }
}
