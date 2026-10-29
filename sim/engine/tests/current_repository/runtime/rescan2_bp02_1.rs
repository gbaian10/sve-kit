#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate these game scenarios."
)]
use super::*;

const fn actor() -> PlayerId {
    PlayerId { seat: 1 }
}

fn position(card: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-037","state":{"acted":true}}]);
    initial
}

fn finish(game: &mut Game, mut play: CardPlay) {
    while let CardPlay::Awaiting(question) = play {
        let answer = if matches!(&question.domain, InputDomain::WardEntry { .. }) {
            Answer::WardEntry(false)
        } else if matches!(&question.domain, InputDomain::OptionalCost) {
            Answer::OptionalCost(true)
        } else if let InputDomain::Targets(domain) = &question.domain {
            Answer::Targets(
                domain
                    .members
                    .iter()
                    .take(usize::try_from(domain.cardinality.maximum.unwrap().0).unwrap())
                    .map(|member| member.handle)
                    .collect(),
            )
        } else if let InputDomain::CostMaterials(domain) = &question.domain {
            Answer::CostMaterials(vec![domain.members[0].handle])
        } else {
            panic!("unexpected decision")
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
}

fn play(game: &mut Game, id: &str) {
    let step = game.play_internal_card(id, actor()).unwrap();
    finish(game, step);
}

fn trigger(game: &mut Game) {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("pending ability")
    };
    assert_eq!(
        options.len(),
        1,
        "only the intended printed ability is pending"
    );
    let step = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap();
    finish(game, step);
    game.resume_internal_card().unwrap();
}

fn activate(game: &mut Game, card: &str) {
    let ability = selection().current[card]
        .program
        .card()
        .abilities
        .iter()
        .find(|a| a.node.kind == AbilityKind::Activated)
        .unwrap()
        .node
        .id;
    let step = game
        .activate_internal_card("source", ability, actor())
        .unwrap();
    finish(game, step);
    game.resume_internal_card().unwrap();
}

fn evolve(game: &mut Game, cost: u8) {
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":cost}})
        )
        .unwrap()
        .outcome,
        "resolved",
        "normal evolution pays the printed cost"
    );
}

fn evolution(base: &str, form: &str) -> Value {
    let mut initial = position(base);
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":base}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
    initial
}

fn quick_pass(game: &mut Game) {
    let q = game.internal_quick_choice().unwrap().unwrap();
    let step = game
        .answer_internal_card(q.actor, q.token, Answer::Quick(None))
        .unwrap();
    finish(game, step);
}

#[test]
fn bp02_011_damages_up_to_two_with_combo_replacement() {
    for prior in [0_usize, 2] {
        let mut initial = position("BP02-011");
        initial["players"]["P2"]["zones"]["field"] =
            json!([{"id":"enemy","card":"BP01-037"},{"id":"second","card":"BP01-037"}]);
        for n in 0..prior {
            initial["players"]["P1"]["zones"]["hand"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":format!("prior{n}"),"card":"BP01-171"}));
        }
        let mut game = game(&initial);
        for n in 0..prior {
            play(&mut game, &format!("prior{n}"));
            game.resume_internal_card().unwrap();
        }
        play(&mut game, "source");
        let s = game.projection(View::Referee).unwrap();
        for id in ["enemy", "second"] {
            assert_eq!(
                s["objects"][id]["damage"],
                if prior == 0 { 1_u8 } else { 2_u8 }
            );
        }
        assert_eq!(s["P2"]["leader"]["life"], 20_u8);
    }
    let mut initial = position("BP02-011");
    initial["players"]["P2"]["zones"]["field"] = json!([]);
    let mut game = game(&initial);
    play(&mut game, "source");
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        9_u8
    );
}

#[test]
fn bp02_120_damages_the_selected_follower_then_its_controller() {
    let mut game = game(&position("BP02-120"));
    play(&mut game, "source");
    game.resume_internal_card().unwrap();
    let s = game.projection(View::Referee).unwrap();
    assert_eq!(s["P2"]["cemetery"], json!(["enemy"]));
    assert_eq!(s["P2"]["leader"]["life"], 18_u8);
    assert_eq!(s["P1"]["leader"]["life"], 20_u8);
}

#[test]
fn bp02_104_pays_fanfare_pp_before_dealing_four() {
    let mut game = game(&position("BP02-104"));
    play(&mut game, "source");
    trigger(&mut game);
    let s = game.projection(View::Referee).unwrap();
    assert_eq!(s["P1"]["pp"]["current"], 5_u8);
    assert_eq!(s["objects"]["enemy"]["damage"], 4_u8);
}

#[test]
fn bp02_002_evolution_buffs_eve_and_grants_entry_turn_rush() {
    let mut initial = evolution("BP02-001", "BP02-002");
    initial["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"eve","card":"BP02-T01","state":{"entered_this_turn":true}}));
    let mut game = game(&initial);
    evolve(&mut game, 1);
    trigger(&mut game);
    let s = game.projection(View::Referee).unwrap();
    assert_eq!(s["objects"]["eve"]["power"], 5_u8);
    assert_eq!(s["objects"]["eve"]["hp"], 5_u8);
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"eve","target":"P2.leader"}))
            .unwrap()
            .outcome,
        "cannot-attack"
    );
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"eve","target":"enemy"}))
            .unwrap()
            .outcome,
        "resolved"
    );
}

#[test]
fn bp02_030_activation_grants_storm_and_bane() {
    let mut initial = position("BP02-030");
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"source","card":"BP02-030","state":{"entered_this_turn":true}}]);
    let mut game = game(&initial);
    activate(&mut game, "BP02-030");
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        7_u8
    );
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"P2.leader"}))
            .unwrap()
            .outcome,
        "resolved"
    );
    quick_pass(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
        17_u8
    );
    let mut duel = super::game(&initial);
    activate(&mut duel, "BP02-030");
    assert_eq!(
        duel.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"enemy"}))
            .unwrap()
            .outcome,
        "resolved"
    );
    quick_pass(&mut duel);
    assert_eq!(
        duel.projection(View::Referee).unwrap()["P2"]["cemetery"],
        json!(["enemy"])
    );
}

#[test]
fn bp02_038_ann_discount_is_two_even_with_multiple_greas() {
    for copies in [0_usize, 1, 2] {
        let mut initial = position("BP02-038");
        initial["players"]["P1"]["zones"]["field"] = json!(
            (0..copies)
                .map(|n| json!({"id":format!("grea{n}"),"card":"BP02-040"}))
                .collect::<Vec<_>>()
        );
        let mut game = game(&initial);
        play(&mut game, "source");
        let s = game.projection(View::Referee).unwrap();
        assert_eq!(
            s["P1"]["pp"]["current"],
            if copies == 0 { 6_u8 } else { 8_u8 }
        );
        assert!(
            s["P1"]["field"]
                .as_array()
                .unwrap()
                .contains(&json!("source"))
        );
    }
}

#[test]
fn bp02_040_grea_discount_and_activation_depend_on_ann() {
    for copies in [0_usize, 1, 2] {
        let mut initial = position("BP02-040");
        initial["players"]["P1"]["zones"]["field"] = json!(
            (0..copies)
                .map(|n| json!({"id":format!("ann{n}"),"card":"BP02-038"}))
                .collect::<Vec<_>>()
        );
        let mut game = game(&initial);
        play(&mut game, "source");
        game.resume_internal_card().unwrap();
        activate(&mut game, "BP02-040");
        let s = game.projection(View::Referee).unwrap();
        assert_eq!(
            s["P1"]["pp"]["current"],
            if copies == 0 { 6_u8 } else { 8_u8 }
        );
        assert_eq!(s["objects"]["source"]["acted"], true);
        if copies == 0 {
            assert_eq!(s["objects"]["enemy"]["damage"], 3_u8);
        } else {
            assert_eq!(s["P2"]["cemetery"], json!(["enemy"]));
        }
    }
}

#[test]
fn bp02_072_evolution_destroys_enemy_and_heals_its_power() {
    let mut game = game(&evolution("BP02-071", "BP02-072"));
    evolve(&mut game, 2);
    trigger(&mut game);
    let s = game.projection(View::Referee).unwrap();
    assert_eq!(s["P2"]["cemetery"], json!(["enemy"]));
    assert_eq!(s["P1"]["leader"]["life"], 25_u8);
}

#[test]
fn bp02_073_lastword_destroys_an_enemy_and_heals_its_power() {
    let mut initial = position("BP02-073");
    initial["turn"]["active"] = json!("P2");
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"source","card":"BP02-073","state":{"acted":true}}]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"attacker","card":"BP01-037"},{"id":"enemy","card":"BP01-037"}]);
    let mut game = game(&initial);
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"attacker","target":"source"}))
            .unwrap()
            .outcome,
        "resolved"
    );
    quick_pass(&mut game);
    trigger(&mut game);
    let s = game.projection(View::Referee).unwrap();
    assert_eq!(s["P1"]["cemetery"], json!(["source"]));
    assert_eq!(s["P2"]["field"], json!([]));
    assert_eq!(s["P1"]["leader"]["life"], 25_u8);
}

fn end_loot(evolved: bool) {
    for crimson in [false, true] {
        let mut initial = evolution("BP02-085", "BP02-086");
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"payer","card":"BP01-118"}]);
        initial["players"]["P1"]["zones"]["deck"] =
            json!([{"id":"drawn","card":"BP01-171"},{"id":"next","card":"BP01-171"}]);
        let mut game = game(&initial);
        if crimson {
            play(&mut game, "payer");
            trigger(&mut game);
        }
        if evolved {
            evolve(&mut game, 1);
        }
        assert_eq!(
            game.internal_rule_action(&json!({"do":"end-phase"}))
                .unwrap()
                .outcome,
            "resolved",
            "ending the main phase triggers the printed End ability"
        );
        trigger(&mut game);
        let s = game.projection(View::Referee).unwrap();
        assert_eq!(
            s["P1"]["deck"],
            if crimson {
                json!(["next"])
            } else {
                json!(["drawn", "next"])
            },
            "only Crimson draws the top card"
        );
        assert_eq!(
            s["P1"]["hand"],
            if crimson { json!([]) } else { json!(["payer"]) },
            "Crimson draws then discards; a false condition preserves the hand"
        );
        if crimson {
            assert!(
                s["P1"]["cemetery"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("drawn")),
                "the drawn card can pay the resolution discard"
            );
        }
    }
}

#[test]
fn bp02_085_end_loots_only_after_own_life_was_lost_this_turn() {
    end_loot(false);
}

#[test]
fn bp02_086_attacks_damage_both_leaders_and_end_loots_in_crimson() {
    let mut game = game(&evolution("BP02-085", "BP02-086"));
    evolve(&mut game, 1);
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"P2.leader"}))
            .unwrap()
            .outcome,
        "resolved"
    );
    trigger(&mut game);
    quick_pass(&mut game);
    let s = game.projection(View::Referee).unwrap();
    assert_eq!(s["P1"]["leader"]["life"], 19_u8);
    assert_eq!(s["P2"]["leader"]["life"], 16_u8);
    end_loot(true);
}
