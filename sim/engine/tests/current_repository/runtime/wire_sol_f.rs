#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate these printed card scenarios."
)]
use super::*;
use alloc::collections::BTreeSet;
use sve_engine::game::internal::SearchAnswer;

const PLAYER: PlayerId = PlayerId { seat: 1 };

fn position(card: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"enemy","card":"BP01-037","state":{"power":3_u8,"hp":10_u8,"max_hp":10_u8}}
    ]);
    for seat in ["P1", "P2"] {
        initial["players"][seat]["zones"]["deck"] = json!([
            {"id":format!("{seat}-top"),"card":"BP01-171"},
            {"id":format!("{seat}-tail"),"card":"BP01-171"}
        ]);
    }
    initial
}

fn finish(game: &mut Game, mut play: CardPlay, modes: &[ModeId]) {
    while let CardPlay::Awaiting(question) = play {
        let answer = if let InputDomain::Targets(domain) = &question.domain {
            let limit = domain
                .cardinality
                .maximum
                .unwrap_or(domain.cardinality.minimum);
            let mut names = BTreeSet::new();
            Answer::Targets(
                domain
                    .members
                    .iter()
                    .filter(|member| !domain.distinct_names || names.insert(member.name.clone()))
                    .take(usize::try_from(limit.0).unwrap())
                    .map(|m| m.handle)
                    .collect(),
            )
        } else if let InputDomain::CostMaterials(domain) = &question.domain {
            Answer::CostMaterials(
                domain
                    .members
                    .iter()
                    .take(usize::try_from(domain.cardinality.minimum.0).unwrap())
                    .map(|m| m.handle)
                    .collect(),
            )
        } else if let InputDomain::Search(domain) = &question.domain {
            Answer::Search(SearchAnswer::Ungrouped(
                domain
                    .cards
                    .iter()
                    .take(usize::try_from(domain.cardinality.maximum.unwrap_or(Nat(1)).0).unwrap())
                    .map(|m| m.handle)
                    .collect(),
            ))
        } else if let InputDomain::Order(cards) = &question.domain {
            Answer::Order(cards.iter().map(|m| m.handle).collect())
        } else if matches!(&question.domain, InputDomain::Modes { .. }) {
            Answer::Modes(modes.to_vec())
        } else if matches!(&question.domain, InputDomain::OptionalCost) {
            Answer::OptionalCost(true)
        } else if matches!(&question.domain, InputDomain::WardEntry { .. }) {
            Answer::WardEntry(false)
        } else if matches!(&question.domain, InputDomain::TokenCapacity { .. }) {
            Answer::TokenCapacity(0)
        } else {
            panic!("unexpected decision: {:?}", question.domain)
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
}

fn settle(game: &mut Game) {
    loop {
        while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
        let Some(question) = game.internal_pending_choice().unwrap() else {
            break;
        };
        let InputDomain::Pending(options) = &question.domain else {
            panic!("pending trigger")
        };
        let play = game
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap();
        finish(game, play, &[ModeId(1)]);
    }
}

fn play(game: &mut Game, id: &str, modes: &[ModeId]) {
    let step = game.play_internal_card(id, PLAYER).unwrap();
    finish(game, step, modes);
    settle(game);
}

fn activate(game: &mut Game, card: &str, modes: &[ModeId]) {
    let ability = selection().current[card]
        .program
        .card()
        .abilities
        .iter()
        .find(|a| {
            a.node.kind == AbilityKind::Activated
                && matches!(a.node.source.origin, AbilityOrigin::Printed { .. })
        })
        .unwrap()
        .node
        .id;
    let step = game
        .activate_internal_card("source", ability, PLAYER)
        .unwrap();
    finish(game, step, modes);
    settle(game);
}

fn evolved_position(base: &str, form: &str) -> Value {
    let mut initial = position(base);
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":base}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
    initial
}

fn evolve(game: &mut Game, pp: u8) {
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":pp}})
        )
        .unwrap()
        .outcome,
        "resolved",
        "the printed evolution cost permits this form"
    );
    settle(game);
}

#[test]
fn bp12_105_counts_both_zones_and_dual_traits_once() {
    for form in ["BP12-105", "BP12-P21", "PR-328"] {
        for enough in [false, true] {
            let mut initial = evolved_position("BP12-104", form);
            initial["players"]["P1"]["zones"]["field"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":"tree","card":"BP12-T12"}));
            let mut ex = vec![
                json!({"id":"dual","card":"BP12-104"}),
                json!({"id":"repair","card":"BP07-T02"}),
            ];
            if enough {
                ex.push(json!({"id":"extra","card":"BP12-T12"}));
            }
            initial["players"]["P1"]["zones"]["ex"] = json!(ex);
            let mut game = game(&initial);
            evolve(&mut game, 1);
            assert_eq!(
                game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
                if enough { 2_u8 } else { 0_u8 }
            );
        }
    }
}

#[test]
fn bp12_035_hand_activation_frees_ex_before_moving_itself() {
    for card in ["BP12-035", "BP12-SL09"] {
        let mut initial = position(card);
        initial["players"]["P1"]["zones"]["ex"] = json!([
            {"id":"fuel1","card":"BP07-T02"},{"id":"fuel2","card":"BP07-T02"},
            {"id":"keep1","card":"BP07-T02"},{"id":"keep2","card":"BP07-T02"},{"id":"keep3","card":"BP07-T02"}
        ]);
        let mut game = game(&initial);
        activate(&mut game, card, &[ModeId(1)]);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["pp"]["current"], 7_u8);
        assert!(
            state["P1"]["ex"]
                .as_array()
                .unwrap()
                .contains(&json!("source"))
        );
        assert_eq!(state["P1"]["ex"].as_array().unwrap().len(), 4);
        assert_eq!(state["P1"]["field"].as_array().unwrap().len(), 1);
    }
}

#[test]
fn bp12_056_reads_actual_discard_history_for_evolution_damage() {
    for form in ["BP12-056", "BP12-P11"] {
        for discard in [false, true] {
            let mut initial = evolved_position("BP12-055", form);
            initial["players"]["P1"]["zones"]["hand"] = json!([
                {"id":"ritual","card":"BP02-062"},{"id":"fuel","card":"BP01-171"}
            ]);
            let mut game = game(&initial);
            if discard {
                play(&mut game, "ritual", &[ModeId(1)]);
            }
            evolve(&mut game, 1);
            assert_eq!(
                game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
                if discard { 4_u8 } else { 2_u8 }
            );
        }
    }
}

#[test]
fn ecp02_059_summons_a_searched_follower_and_grants_drain() {
    for card in ["ECP02-059", "ECP02-P32"] {
        let mut initial = position(card);
        initial["players"]["P1"]["zones"]["deck"] =
            json!([{"id":"sachiko","card":"CP02-071"},{"id":"tail","card":"BP01-171"}]);
        initial["players"]["P1"]["zones"]["cemetery"] = json!(
            (0_u8..5_u8)
                .map(|i| json!({"id":format!("cute-{i}"),"card":"ECP02-059"}))
                .collect::<Vec<_>>()
        );
        let mut game = game(&initial);
        play(&mut game, "source", &[ModeId(1), ModeId(2)]);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["enemy"]["damage"], 4_u8);
        assert!(
            state["P1"]["field"]
                .as_array()
                .unwrap()
                .contains(&json!("sachiko"))
        );
        assert!(
            state["objects"]["sachiko"]["keywords"]
                .as_array()
                .unwrap()
                .contains(&json!("ドレイン"))
        );
    }
}

#[test]
fn bp03_040_returns_only_one_of_two_identically_named_chess_followers() {
    for form in ["BP03-040", "BP03-SL09"] {
        let mut initial = evolved_position("BP03-039", form);
        initial["players"]["P1"]["zones"]["cemetery"] = json!([
            {"id":"pawn1","card":"BP03-051"},{"id":"pawn2","card":"BP03-051"}
        ]);
        let mut game = game(&initial);
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":2_u8}})
            )
            .unwrap()
            .outcome,
            "resolved"
        );
        while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
        let pending = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = &pending.domain else {
            panic!("evolution trigger")
        };
        let triggered = game
            .answer_internal_card(
                pending.actor,
                pending.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap();
        let CardPlay::Awaiting(question) = triggered else {
            panic!("cemetery targets")
        };
        let InputDomain::Targets(domain) = &question.domain else {
            panic!("cemetery targets")
        };
        assert_eq!(domain.members.len(), 2);
        let before = game.projection(View::Referee).unwrap();
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Targets(domain.members.iter().map(|member| member.handle).collect()),
        )
        .unwrap_err();
        assert_eq!(game.projection(View::Referee).unwrap(), before);
        let step = game
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Targets(vec![domain.members[0].handle]),
            )
            .unwrap();
        finish(&mut game, step, &[ModeId(1)]);
        settle(&mut game);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["field"].as_array().unwrap().len(), 2);
        assert_eq!(state["P1"]["cemetery"].as_array().unwrap().len(), 1);
    }
}

#[test]
fn bp14_068_exchanges_an_ocean_rest_for_two_pp_discount() {
    let mut initial = position("BP14-068");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"ocean","card":"BP14-067"}]);
    let mut game = game(&initial);
    play(&mut game, "source", &[ModeId(1)]);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["pp"]["current"], 9_u8);
    assert_eq!(state["objects"]["ocean"]["acted"], true);
    assert_eq!(state["objects"]["enemy"]["acted"], true);
    assert_eq!(state["P1"]["hand"], json!(["P1-top"]));
}

#[test]
fn bp22_027_evolves_for_free_with_a_commander_thief() {
    for card in ["BP22-027", "BP22-P12"] {
        let mut initial = position(card);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"commander","card":"BP12-023"}]);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP22-028"}]);
        let mut game = game(&initial);
        play(&mut game, "source", &[ModeId(1)]);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["pp"]["current"], 7_u8);
        assert_eq!(state["objects"]["source"]["power"], 3_u8);
        assert_eq!(state["P1"]["evolve_deck"], json!([]));
    }
}

#[test]
fn ecp02_006_searches_then_pays_lesson_for_a_hand_amulet() {
    for card in ["ECP02-006", "ECP02-P01"] {
        let mut initial = position(card);
        initial["players"]["P1"]["zones"]["hand"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"amulet","card":"CP02-007"}));
        initial["players"]["P1"]["zones"]["deck"] =
            json!([{"id":"found","card":"CP02-007"},{"id":"tail","card":"BP01-171"}]);
        initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"lesson","card":"PR-155"}]);
        let mut game = game(&initial);
        play(&mut game, "source", &[ModeId(1)]);
        activate(&mut game, card, &[ModeId(1)]);
        let state = game.projection(View::Referee).unwrap();
        assert!(
            state["P1"]["field"]
                .as_array()
                .unwrap()
                .contains(&json!("found"))
        );
        assert!(
            state["P1"]["field"]
                .as_array()
                .unwrap()
                .contains(&json!("amulet"))
        );
        assert_eq!(state["P1"]["ex"], json!([]));
    }
}

#[test]
fn ecp02_007_returns_a_deremas_card_then_bottoms_an_enemy() {
    for form in ["ECP02-007", "ECP02-P02"] {
        let mut initial = evolved_position("ECP02-006", form);
        initial["players"]["P1"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"fuel","card":"CP02-007"}));
        let mut game = game(&initial);
        evolve(&mut game, 1);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["hand"].as_array().unwrap().len(), 1);
        assert_eq!(
            state["P2"]["deck"].as_array().unwrap().last(),
            Some(&json!("enemy"))
        );
    }
}

#[test]
fn bp06_011_buffs_fairy_tokens_in_either_zone_or_creates_two_in_ex() {
    for card in ["BP06-011", "BP06-P03"] {
        for zone in ["field", "ex"] {
            let mut initial = position(card);
            initial["players"]["P1"]["zones"][zone] = json!([{"id":"fairy","card":"BP06-T05"}]);
            let mut game = game(&initial);
            play(&mut game, "source", &[ModeId(1)]);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["objects"]["fairy"]["power"], 3_u8);
            assert_eq!(state["objects"]["fairy"]["hp"], 3_u8);
        }
        let mut game = game(&position(card));
        play(&mut game, "source", &[ModeId(2)]);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["ex"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
    }
}
