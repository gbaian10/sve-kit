#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate these game scenarios."
)]
use super::*;
use sve_engine::game::internal::SearchAnswer;

const fn actor() -> PlayerId {
    PlayerId { seat: 1 }
}

fn position(card: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"first","card":"BP01-171"},
        {"id":"second","card":"BP01-171"},
        {"id":"third","card":"BP01-171"},
        {"id":"fourth","card":"BP01-171"}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    initial
}

fn finish(game: &mut Game, mut play: CardPlay, paid: bool) {
    while let CardPlay::Awaiting(question) = play {
        let answer = if matches!(&question.domain, InputDomain::WardEntry { .. }) {
            Answer::WardEntry(false)
        } else if matches!(&question.domain, InputDomain::OptionalCost) {
            Answer::OptionalCost(paid)
        } else if let InputDomain::Search(domain) = &question.domain {
            Answer::Search(SearchAnswer::Ungrouped(
                domain
                    .cards
                    .iter()
                    .take(usize::try_from(domain.cardinality.maximum.unwrap().0).unwrap())
                    .map(|card| card.handle)
                    .collect(),
            ))
        } else if let InputDomain::Order(cards) = &question.domain {
            Answer::Order(cards.iter().rev().map(|card| card.handle).collect())
        } else if let InputDomain::CostMaterials(domain) = &question.domain {
            Answer::CostMaterials(
                domain
                    .members
                    .iter()
                    .take(usize::try_from(domain.cardinality.maximum.unwrap().0).unwrap())
                    .map(|member| member.handle)
                    .collect(),
            )
        } else if let InputDomain::Targets(domain) = &question.domain {
            Answer::Targets(
                domain
                    .members
                    .iter()
                    .take(usize::try_from(domain.cardinality.maximum.unwrap().0).unwrap())
                    .map(|member| member.handle)
                    .collect(),
            )
        } else {
            panic!("unexpected decision")
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
}

fn play(game: &mut Game) {
    let step = game.play_internal_card("source", actor()).unwrap();
    finish(game, step, true);
}

fn triggers(game: &mut Game) {
    while let Some(question) = game.internal_pending_choice().unwrap() {
        let InputDomain::Pending(options) = &question.domain else {
            panic!("pending ability expected")
        };
        let step = game
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap();
        finish(game, step, true);
        game.resume_internal_card().unwrap();
    }
}

fn pass_quick(game: &mut Game) {
    let q = game.internal_quick_choice().unwrap().unwrap();
    let step = game
        .answer_internal_card(q.actor, q.token, Answer::Quick(None))
        .unwrap();
    finish(game, step, true);
}

fn evolve(game: &mut Game, form: &str, cost: u8) {
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":form,"pay":{"pp":cost}})
        )
        .unwrap()
        .outcome,
        "resolved",
        "the real base uses its printed evolution cost"
    );
    triggers(game);
}
fn evolved_position(base: &str, form: &str) -> Value {
    let mut initial = position(base);
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":base}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
    initial
}
fn end_turn(game: &mut Game) {
    let active = game.projection(View::Referee).unwrap()["turn"]["active"].clone();
    assert_eq!(
        game.internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap()
            .outcome,
        "resolved",
        "the active player ends its main phase"
    );
    loop {
        triggers(game);
        if game.projection(View::Referee).unwrap()["turn"]["active"] != active {
            break;
        }
        if game.internal_quick_choice().unwrap().is_some() {
            pass_quick(game);
        } else {
            assert_eq!(
                game.internal_rule_action(&json!({"do":"guard-act","select":[]}))
                    .unwrap()
                    .outcome,
                "resolved",
                "standing Ward followers may remain standing at the end-phase choice"
            );
        }
    }
}

fn activate(game: &mut Game, card: &str) {
    let id = selection().current[card]
        .program
        .card()
        .abilities
        .iter()
        .find(|a| a.node.kind == AbilityKind::Activated)
        .unwrap()
        .node
        .id;
    let step = game.activate_internal_card("source", id, actor()).unwrap();
    finish(game, step, true);
    game.resume_internal_card().unwrap();
    triggers(game);
}
#[test]
fn bp13_047_two_wizards_replace_follower_damage_but_not_leader_damage() {
    for two in [false, true] {
        let mut initial = position("BP13-047");
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"wizard","card":"BP02-046"},{"id":"other","card":if two {"BP08-042"}else{"BP01-171"}}]);
        let mut game = game(&initial);
        play(&mut game);
        game.resume_internal_card().unwrap();
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["objects"]["enemy"]["damage"],
            if two { 4_u8 } else { 2_u8 }
        );
        assert_eq!(state["P2"]["leader"]["life"], 19_u8);
    }
}
#[test]
fn bp13_098_optional_amulet_payment_replaces_three_damage_with_five() {
    for paid in [false, true] {
        let mut initial = position("BP13-098");
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"amulet","card":"BP01-016"}]);
        let mut game = game(&initial);
        let mut step = game.play_internal_card("source", actor()).unwrap();
        while let CardPlay::Awaiting(question) = step {
            let answer = if matches!(&question.domain, InputDomain::OptionalCost) {
                Answer::OptionalCost(paid)
            } else if let InputDomain::CostMaterials(domain) = &question.domain {
                Answer::CostMaterials(vec![domain.members[0].handle])
            } else if let InputDomain::Targets(domain) = &question.domain {
                Answer::Targets(vec![domain.members[0].handle])
            } else {
                panic!("spell payment or target")
            };
            step = game
                .answer_internal_card(question.actor, question.token, answer)
                .unwrap();
        }
        game.resume_internal_card().unwrap();
        triggers(&mut game);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P2"]["field"],
            if paid { json!([]) } else { json!(["enemy"]) }
        );
        if !paid {
            assert_eq!(state["objects"]["enemy"]["damage"], 3_u8);
        }
        assert_eq!(
            state["P1"]["field"],
            if paid { json!([]) } else { json!(["amulet"]) }
        );
        assert_eq!(state["P1"]["pp"]["current"], 8_u8);
    }
}
#[test]
fn bp13_081_necrocharge_twenty_adds_four_leader_damage() {
    for grave in [19_usize, 20] {
        let mut initial = position("BP13-081");
        initial["players"]["P1"]["zones"]["cemetery"] = json!(
            (0..grave)
                .map(|n| json!({"id":format!("old{n}"),"card":"BP01-171"}))
                .collect::<Vec<_>>()
        );
        let mut game = game(&initial);
        play(&mut game);
        game.resume_internal_card().unwrap();
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["enemy"]["damage"], 4_u8);
        assert_eq!(
            state["P2"]["leader"]["life"],
            if grave == 20 { 16_u8 } else { 20_u8 }
        );
    }
}
#[test]
fn bp13_049_real_evolution_hits_follower_and_its_leader_for_one() {
    let mut game = game(&evolved_position("BP13-048", "BP13-049"));
    evolve(&mut game, "form", 1);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["damage"], 1_u8);
    assert_eq!(state["P2"]["leader"]["life"], 19_u8);
}
#[test]
fn bp13_085_fanfare_and_combat_lastword_each_damage_enemy_and_leader() {
    let mut initial = position("BP13-085");
    initial["players"]["P2"]["zones"]["field"][0]["state"] = json!({"acted":true});
    let mut entered_game = game(&initial);
    play(&mut entered_game);
    triggers(&mut entered_game);
    let entered = entered_game.projection(View::Referee).unwrap();
    assert_eq!(entered["objects"]["enemy"]["damage"], 4_u8);
    assert_eq!(entered["P2"]["leader"]["life"], 19_u8);
    let mut later = position("BP13-085");
    later["players"]["P1"]["zones"]["hand"] = json!([]);
    later["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP13-085"}]);
    later["players"]["P2"]["zones"]["field"][0]["state"] = json!({"acted":true});
    later["players"]["P2"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"survivor","card":"BP01-037"}));
    let mut combat = game(&later);
    assert_eq!(
        combat
            .internal_rule_action(&json!({"do":"attack","attacker":"source","target":"enemy"}))
            .unwrap()
            .outcome,
        "resolved",
        "the follower present since the start attacks the rested enemy"
    );
    pass_quick(&mut combat);
    triggers(&mut combat);
    let dead = combat.projection(View::Referee).unwrap();
    assert_eq!(dead["P1"]["cemetery"], json!(["source"]));
    assert_eq!(dead["P2"]["leader"]["life"], 19_u8);
    assert!(
        dead["P2"]["cemetery"]
            .as_array()
            .unwrap()
            .contains(&json!("enemy")),
        "the Lastword damage destroys the damaged enemy"
    );
}
#[test]
fn bp13_113_real_evolution_damage_tracks_hand_size() {
    for held in [1_usize, 3] {
        let mut initial = evolved_position("BP13-112", "BP13-113");
        initial["players"]["P1"]["zones"]["hand"] = json!(
            (0..held)
                .map(|n| json!({"id":format!("held{n}"),"card":"BP01-171"}))
                .collect::<Vec<_>>()
        );
        let mut game = game(&initial);
        evolve(&mut game, "form", 1);
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
            held
        );
    }
}
#[test]
fn bp13_p29_created_on_field_heals_then_banishes_at_next_own_main_phase() {
    let mut initial = position("BP13-P29");
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P2"]["zones"]["deck"] = json!([{"id":"oppdraw","card":"BP01-171"}]);
    let action = Action::Token {
        tokens: vec![TokenSpec {
            template: TokenTemplateId("アンの大英霊".into()),
            name: Expr::Literal(CardName("アンの大英霊".into())),
            count: Expr::Literal(Nat(1)),
        }],
        destination: TokenDestination::Single {
            zone: Expr::Literal(ZoneRef {
                player: PlayerRef::Controller {},
                kind: ZoneKind::Field,
            }),
        },
        output: Binding::new(50, ScopeId(1)),
        initial_state: None,
        initial_posture: Posture::Standing,
        entry_effects: vec![],
    };
    let mut game = with_producer(&initial, action);
    let step = game
        .activate_internal_card("producer", AbilityId(1), actor())
        .unwrap();
    finish(&mut game, step, true);
    game.resume_internal_card().unwrap();
    triggers(&mut game);
    let entered = game.projection(View::Referee).unwrap();
    assert_eq!(entered["P1"]["leader"]["life"], 22_u8);
    assert_eq!(entered["P1"]["field"].as_array().unwrap().len(), 2);
    end_turn(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["field"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    end_turn(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["field"], json!(["producer"]));
    assert_eq!(state["P1"]["banish"], json!([]));
}
#[test]
fn bp13_017_end_phase_acts_and_freezes_enemy_through_next_start() {
    let mut initial = position("BP13-017");
    initial["players"]["P2"]["zones"]["deck"] = json!([{"id":"oppdraw","card":"BP01-171"}]);
    let mut game = game(&initial);
    play(&mut game);
    game.resume_internal_card().unwrap();
    end_turn(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["turn"]["active"], "P2");
    assert_eq!(state["objects"]["enemy"]["acted"], true);
}
#[test]
fn bp13_104_activation_destroys_enemy_then_lastword_heals_and_draws() {
    let mut game = game(&position("BP13-104"));
    play(&mut game);
    game.resume_internal_card().unwrap();
    activate(&mut game, "BP13-104");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["pp"]["current"], 4_u8);
    assert_eq!(state["P1"]["cemetery"], json!(["source"]));
    assert_eq!(state["P2"]["cemetery"], json!(["enemy"]));
    assert_eq!(state["P1"]["leader"]["life"], 21_u8);
    assert_eq!(state["P1"]["hand"], json!(["first"]));
}
#[test]
fn bp13_103_fanfare_bonus_requires_entry_from_outside_hand() {
    for ex in [false, true] {
        let mut initial = position("BP13-103");
        if ex {
            initial["players"]["P1"]["zones"]["hand"] = json!([]);
            initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"source","card":"BP13-103"}]);
        }
        let mut game = game(&initial);
        play(&mut game);
        triggers(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["source"]["power"],
            if ex { 2_u8 } else { 0_u8 }
        );
    }
}
#[test]
fn bp13_019_fanfare_sacrifices_another_levin_to_destroy_enemy() {
    let mut initial = position("BP13-019");
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"levin","card":"BP13-030"},{"id":"wrong","card":"BP01-171"}]);
    let mut game = game(&initial);
    play(&mut game);
    triggers(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["cemetery"], json!(["levin"]));
    assert_eq!(state["P2"]["cemetery"], json!(["enemy"]));
    assert!(
        state["P1"]["field"]
            .as_array()
            .unwrap()
            .contains(&json!("source")),
        "the evolved source remains on the field"
    );
    assert!(
        state["P1"]["field"]
            .as_array()
            .unwrap()
            .contains(&json!("wrong")),
        "the unrelated follower remains on the field"
    );
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"P2.leader"}))
            .unwrap()
            .outcome,
        "resolved",
        "printed Storm permits the entry-turn leader attack"
    );
}
#[test]
fn bp13_069_fanfare_counts_other_sea_followers_without_counting_itself() {
    for sea in [false, true] {
        let mut initial = position("BP13-069");
        initial["players"]["P1"]["zones"]["field"] =
            json!([{"id":"ally","card":if sea {"BP04-069"}else{"BP01-171"}}]);
        let mut game = game(&initial);
        play(&mut game);
        triggers(&mut game);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["objects"]["source"]["power"],
            if sea { 4_u8 } else { 3_u8 }
        );
        assert_eq!(state["objects"]["source"]["hp"], 1_u8);
    }
}

#[test]
fn bp13_096_real_evolution_freezes_two_rested_enemies_without_freezing_third() {
    let mut initial = evolved_position("BP13-095", "BP13-096");
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"enemy","card":"BP01-037","state":{"acted":true}},
        {"id":"enemy2","card":"BP01-037","state":{"acted":true}},
        {"id":"enemy3","card":"BP01-037","state":{"acted":true}}
    ]);
    initial["players"]["P2"]["zones"]["deck"] = json!([{"id":"oppdraw","card":"BP01-171"}]);
    let mut game = game(&initial);
    evolve(&mut game, "form", 2);
    end_turn(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["turn"]["active"], "P2");
    assert_eq!(state["objects"]["enemy"]["acted"], true);
    assert_eq!(state["objects"]["enemy2"]["acted"], true);
    assert_eq!(state["objects"]["enemy3"]["acted"], false);
}
#[test]
fn bp13_025_optional_two_pp_adds_albert_to_julius_search() {
    for paid in [false, true] {
        let mut initial = position("BP13-025");
        initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"julius","card":"BP02-027"},{"id":"albert","card":"BP02-018"},{"id":"wrong","card":"BP01-171"}]);
        let mut game = game(&initial);
        let step = game.play_internal_card("source", actor()).unwrap();
        finish(&mut game, step, paid);
        game.resume_internal_card().unwrap();
        triggers(&mut game);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["enemy"]["damage"], 3_u8);
        assert_eq!(state["P1"]["pp"]["current"], if paid { 5_u8 } else { 7_u8 });
        assert!(
            state["P1"]["field"]
                .as_array()
                .unwrap()
                .contains(&json!("julius")),
            "the named search puts Julius on the field"
        );
        assert_eq!(
            state["P1"]["field"]
                .as_array()
                .unwrap()
                .contains(&json!("albert")),
            paid
        );
    }
}
#[test]
fn bp13_100_real_evolution_selects_a_beast_and_orders_unselected_top_four() {
    let mut initial = evolved_position("BP13-099", "BP13-100");
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"wrong","card":"BP01-171"},{"id":"beast","card":"BP13-099"},{"id":"rest2","card":"BP01-037"},{"id":"rest3","card":"BP01-037"},{"id":"tail","card":"BP01-037"}]);
    let mut game = game(&initial);
    evolve(&mut game, "form", 1);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["beast"]));
    assert_eq!(
        state["P1"]["deck"],
        json!(["tail", "rest3", "rest2", "wrong"])
    );
}
#[test]
fn bp13_067_top_one_selection_and_awakened_healing_are_independent() {
    for awakened in [false, true] {
        for matching in [false, true] {
            let mut initial = position("BP13-067");
            initial["players"]["P1"]["pp"] = if awakened {
                json!({"current":10_u8,"max":10_u8})
            } else {
                json!({"current":6_u8,"max":6_u8})
            };
            initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"top","card":if matching {"BP13-067"}else{"BP01-171"}},{"id":"tail","card":"BP01-171"}]);
            let mut game = game(&initial);
            play(&mut game);
            triggers(&mut game);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["P1"]["hand"],
                if matching { json!(["top"]) } else { json!([]) }
            );
            assert_eq!(
                state["P1"]["deck"],
                if matching {
                    json!(["tail"])
                } else {
                    json!(["top", "tail"])
                }
            );
            assert_eq!(
                state["P1"]["leader"]["life"],
                if awakened { 22_u8 } else { 20_u8 }
            );
        }
    }
}
#[test]
fn bp13_031_real_evolution_selects_levin_and_orders_the_remainder() {
    let mut initial = evolved_position("BP13-030", "BP13-031");
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"wrong","card":"BP01-171"},{"id":"levin","card":"BP13-030"},{"id":"rest2","card":"BP01-037"},{"id":"rest3","card":"BP01-037"},{"id":"tail","card":"BP01-037"}]);
    let mut game = game(&initial);
    evolve(&mut game, "form", 1);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["levin"]));
    assert_eq!(
        state["P1"]["deck"],
        json!(["tail", "rest3", "rest2", "wrong"])
    );
}
#[test]
fn bp13_018_creates_fairy_before_searching_top_three_for_a_fairy_card() {
    let mut initial = position("BP13-018");
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"wrong","card":"BP01-171"},{"id":"fairy-card","card":"SD01-011"},{"id":"rest","card":"BP01-037"},{"id":"tail","card":"BP01-171"}]);
    let mut game = game(&initial);
    play(&mut game);
    triggers(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["fairy-card"]));
    assert_eq!(state["P1"]["deck"], json!(["tail", "rest", "wrong"]));
    assert_eq!(
        state["P1"]["field"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|id| state["objects"][id.as_str().unwrap()]["name"] == "フェアリー")
            .count(),
        1
    );
}
#[test]
fn bp13_102_searches_only_a_faith_follower_with_at_most_two_power() {
    let mut initial = position("BP13-102");
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"wrong-trait","card":"BP01-171"},{"id":"too-strong","card":"BP10-103"},{"id":"eligible","card":"BP13-099"}]);
    let mut game = game(&initial);
    play(&mut game);
    triggers(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["field"], json!(["source", "eligible"]));
    assert_eq!(state["P1"]["deck"], json!(["wrong-trait", "too-strong"]));
    assert_eq!(state["P1"]["pp"]["current"], 3_u8);
}
