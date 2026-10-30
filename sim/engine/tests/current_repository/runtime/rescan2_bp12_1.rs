#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate these game scenarios."
)]
use super::*;
use sve_engine::game::internal::{DamageShare, SearchAnswer};

const fn actor() -> PlayerId {
    PlayerId { seat: 1 }
}

fn position(card: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"first","card":"BP01-171"},
        {"id":"second","card":"BP01-171"},
        {"id":"third","card":"BP01-171"},
        {"id":"fourth","card":"BP01-171"}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    initial
}

fn finish(game: &mut Game, mut play: CardPlay) {
    while let CardPlay::Awaiting(question) = play {
        let answer = if matches!(&question.domain, InputDomain::WardEntry { .. }) {
            Answer::WardEntry(false)
        } else if matches!(&question.domain, InputDomain::OptionalCost) {
            Answer::OptionalCost(true)
        } else if matches!(&question.domain, InputDomain::Modes { .. }) {
            Answer::Modes(vec![ModeId(1)])
        } else if let InputDomain::Distribution(domain) = &question.domain {
            assert_eq!(
                domain.members.len(),
                1,
                "this scenario allocates all damage to its single selected follower"
            );
            Answer::Distribution(vec![DamageShare {
                handle: domain.members[0].handle,
                amount: domain.total,
            }])
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
    finish(game, step);
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
        finish(game, step);
        game.resume_internal_card().unwrap();
    }
}

fn evolve(game: &mut Game, form: &str, cost: u8) {
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":form,"pay":{"pp":cost}})
        )
        .unwrap()
        .outcome,
        "resolved",
        "the printed evolution cost permits the real form"
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

fn pass_quick(game: &mut Game) {
    let q = game.internal_quick_choice().unwrap().unwrap();
    let step = game
        .answer_internal_card(q.actor, q.token, Answer::Quick(None))
        .unwrap();
    finish(game, step);
}

#[test]
fn bp12_031_fanfare_replaces_one_damage_when_named_duke_is_present() {
    for duke in [false, true] {
        let mut initial = position("BP12-031");
        if duke {
            initial["players"]["P1"]["zones"]["field"] = json!([{"id":"duke","card":"BP08-025"}]);
        }
        let mut game = game(&initial);
        play(&mut game);
        triggers(&mut game);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P2"]["field"],
            if duke { json!([]) } else { json!(["enemy"]) }
        );
        if !duke {
            assert_eq!(state["objects"]["enemy"]["damage"], 1_u8);
        }
    }
}

#[test]
fn bp12_013_real_evolution_attack_destroys_follower_and_hits_leader() {
    let mut game = game(&evolved_position("BP12-012", "BP12-013"));
    evolve(&mut game, "form", 4);
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"P2.leader"}))
            .unwrap()
            .outcome,
        "resolved",
        "a follower present since the start can attack after evolution"
    );
    triggers(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["cemetery"], json!(["enemy"]));
    assert_eq!(state["P2"]["leader"]["life"], 17_u8);
    assert_eq!(state["P1"]["pp"]["current"], 6_u8);
}

#[test]
fn bp12_098_damage_is_own_amulet_count_plus_one() {
    for amulets in [0_usize, 2] {
        let mut initial = evolved_position("BP12-097", "BP12-098");
        for n in 0..amulets {
            initial["players"]["P1"]["zones"]["field"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":format!("amulet{n}"),"card":"BP01-016"}));
        }
        let mut game = game(&initial);
        evolve(&mut game, "form", 1);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["objects"]["enemy"]["damage"],
            amulets.checked_add(1).unwrap()
        );
    }
}

#[test]
fn bp12_051_heals_only_when_two_wizard_followers_are_present() {
    for two in [false, true] {
        let mut initial = position("BP12-051");
        initial["players"]["P1"]["zones"]["field"] = json!([
            {"id":"wizard","card":"BP02-046"},{"id":"other","card":if two {"BP08-042"}else{"BP01-171"}}
        ]);
        let mut game = game(&initial);
        play(&mut game);
        game.resume_internal_card().unwrap();
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["enemy"]["damage"], 4_u8);
        assert_eq!(
            state["P1"]["leader"]["life"],
            if two { 22_u8 } else { 20_u8 }
        );
    }
}

#[test]
fn bp12_028_draw_requires_both_commander_and_soldier_followers() {
    for (commander, soldier) in [(false, false), (true, false), (false, true), (true, true)] {
        let mut initial = position("BP12-028");
        let mut field = vec![];
        if commander {
            field.push(json!({"id":"commander","card":"BP01-037"}));
        }
        if soldier {
            field.push(json!({"id":"soldier","card":"BP01-038"}));
        }
        initial["players"]["P1"]["zones"]["field"] = json!(field);
        let mut game = game(&initial);
        play(&mut game);
        game.resume_internal_card().unwrap();
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["hand"],
            if commander && soldier {
                json!(["first", "second"])
            } else {
                json!([])
            }
        );
    }
}

#[test]
fn bp12_077_crimson_gates_both_healing_and_draw() {
    for crimson in [false, true] {
        let mut initial = evolved_position("BP12-076", "BP12-077");
        if crimson {
            initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"payer","card":"BP01-118"}]);
        }
        let mut game = game(&initial);
        if crimson {
            let step = game.play_internal_card("payer", actor()).unwrap();
            finish(&mut game, step);
            triggers(&mut game);
        }
        evolve(&mut game, "form", 1);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P1"]["hand"],
            if crimson { json!(["first"]) } else { json!([]) }
        );
        assert_eq!(
            state["P1"]["leader"]["life"],
            if crimson { 21_u8 } else { 20_u8 }
        );
    }
}

#[test]
fn bp12_003_combo_three_replaces_formal_distribution_total_two_with_four() {
    for played in [2_usize, 3] {
        let mut initial = evolved_position("BP12-002", "BP12-003");
        for n in 0..played {
            initial["players"]["P1"]["zones"]["hand"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":format!("prior{n}"),"card":"BP01-171"}));
        }
        let mut game = game(&initial);
        for n in 0..played {
            let step = game
                .play_internal_card(&format!("prior{n}"), actor())
                .unwrap();
            finish(&mut game, step);
            game.resume_internal_card().unwrap();
        }
        evolve(&mut game, "form", 1);
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
            if played == 3 { 4_u8 } else { 2_u8 }
        );
    }
}

fn effect_entry() -> Action {
    Action::Move {
        objects: Selector::Zone {
            players: PlayerSet {
                relation: PlayerRelation::SelfPlayer,
                relative_to: PlayerRef::Controller {},
                text_quantifier: TextQuantifier::All,
                cardinality: Cardinality {
                    minimum: Nat(1),
                    maximum: Some(Nat(1)),
                },
            },
            zone: ZoneKind::Ex,
            view: SelectionView::Current {},
            binding: Binding::new(50, ScopeId(1)),
            predicate: Box::new(Expr::Literal(Bool(true))),
        },
        movement: MoveSpec {
            to: MoveDestination::Shared {
                zone: ZoneRef {
                    player: PlayerRef::Controller {},
                    kind: ZoneKind::Field,
                },
            },
            initial_posture: None,
            entry_effects: vec![],
            position: None,
            order: None,
        },
        reason: MoveReason::Put,
    }
}

fn named(state: &Value, zone: &str, name: &str) -> usize {
    state["P1"][zone]
        .as_array()
        .unwrap()
        .iter()
        .filter(|id| state["objects"][id.as_str().unwrap()]["name"] == name)
        .count()
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
    finish(game, step);
    game.resume_internal_card().unwrap();
    triggers(game);
}
fn finish_mode(game: &mut Game, initial_play: CardPlay, mode: ModeId) {
    let CardPlay::Awaiting(question) = initial_play else {
        panic!("mode declaration")
    };
    assert!(
        matches!(&question.domain, InputDomain::Modes { .. }),
        "the evolved follower offers its printed modes"
    );
    let step = game
        .answer_internal_card(question.actor, question.token, Answer::Modes(vec![mode]))
        .unwrap();
    finish(game, step);
    game.resume_internal_card().unwrap();
}
#[test]
fn bp12_p24_ex_spell_can_bounce_a_follower_heal_or_draw() {
    for mode in [ModeId(1), ModeId(2), ModeId(3)] {
        let mut initial = position("BP12-P24");
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"source","card":"BP12-P24"}]);
        let mut game = game(&initial);
        let step = game.play_internal_card("source", actor()).unwrap();
        finish_mode(&mut game, step, mode);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P2"]["hand"],
            if mode == ModeId(1) {
                json!(["enemy"])
            } else {
                json!([])
            }
        );
        assert_eq!(
            state["P1"]["leader"]["life"],
            if mode == ModeId(2) { 24_u8 } else { 20_u8 }
        );
        assert_eq!(
            state["P1"]["hand"],
            if mode == ModeId(3) {
                json!(["first", "second"])
            } else {
                json!([])
            }
        );
        assert_eq!(state["P1"]["cemetery"], json!([]));
    }
}
#[test]
fn bp12_032_activation_spends_four_pp_for_buff_and_entry_turn_storm() {
    let mut game = game(&position("BP12-032"));
    play(&mut game);
    game.resume_internal_card().unwrap();
    activate(&mut game, "BP12-032");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["pp"]["current"], 4_u8);
    assert_eq!(state["objects"]["source"]["power"], 4_u8);
    assert_eq!(state["objects"]["source"]["hp"], 5_u8);
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"P2.leader"}))
            .unwrap()
            .outcome,
        "resolved",
        "the paid grant permits an entry-turn leader attack"
    );
}
#[test]
fn bp12_033_fanfare_adds_two_power_only_to_selected_own_soldier() {
    let mut initial = position("BP12-033");
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"soldier","card":"BP01-038"},{"id":"wrong","card":"BP01-037"}]);
    let mut game = game(&initial);
    play(&mut game);
    triggers(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["soldier"]["power"], 4_u8);
    assert_eq!(state["objects"]["soldier"]["hp"], 1_u8);
    assert_eq!(state["objects"]["wrong"]["power"], 5_u8);
}
#[test]
fn bp12_086_creates_repair_mode_and_only_effect_entry_evolves_it() {
    for effect in [false, true] {
        let mut initial = position("BP12-086");
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP12-087"}]);
        if effect {
            initial["players"]["P1"]["zones"]["hand"] = json!([]);
            initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"source","card":"BP12-086"}]);
        }
        let mut game = if effect {
            with_producer(&initial, effect_entry())
        } else {
            game(&initial)
        };
        if effect {
            let step = game
                .activate_internal_card("producer", AbilityId(1), actor())
                .unwrap();
            finish(&mut game, step);
            game.resume_internal_card().unwrap();
        } else {
            play(&mut game);
        }
        triggers(&mut game);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(named(&state, "ex", "リペアモード"), 1);
        assert_eq!(
            state["P1"]["evolution"],
            if effect { json!(["form"]) } else { json!([]) }
        );
        assert_eq!(
            state["P1"]["pp"]["current"],
            if effect { 10_u8 } else { 8_u8 }
        );
    }
}
#[test]
fn bp12_087_real_evolution_modes_damage_by_machine_count_or_grant_storm() {
    for mode in [ModeId(1), ModeId(2)] {
        let mut initial = position("BP12-086");
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"machine","card":"BP07-035"}]);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP12-087"}]);
        let mut game = game(&initial);
        play(&mut game);
        triggers(&mut game);
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":2_u8}})
            )
            .unwrap()
            .outcome,
            "resolved",
            "the real base evolves for two PP"
        );
        let pending = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = &pending.domain else {
            panic!("evolution ability")
        };
        let step = game
            .answer_internal_card(
                pending.actor,
                pending.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap();
        finish_mode(&mut game, step, mode);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["objects"]["enemy"]["damage"],
            if mode == ModeId(1) { 2_u8 } else { 0_u8 }
        );
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"attack","attacker":"source","target":"P2.leader"})
            )
            .unwrap()
            .outcome,
            if mode == ModeId(2) {
                "resolved"
            } else {
                "cannot-attack"
            }
        );
    }
}
#[test]
fn bp12_085_damages_enemy_and_creates_one_snake_in_each_public_zone() {
    let mut game = game(&position("BP12-085"));
    play(&mut game);
    game.resume_internal_card().unwrap();
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["cemetery"], json!(["enemy"]));
    assert_eq!(named(&state, "field", "毒蛇"), 1);
    assert_eq!(named(&state, "ex", "毒蛇"), 1);
}
#[test]
fn bp12_014_fanfare_and_leaving_field_each_create_natera_in_ex() {
    let mut entered = game(&position("BP12-014"));
    play(&mut entered);
    triggers(&mut entered);
    assert_eq!(
        named(
            &entered.projection(View::Referee).unwrap(),
            "ex",
            "ナテラの大樹"
        ),
        1
    );
    let mut initial = position("BP12-014");
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP12-014"}]);
    initial["players"]["P2"]["zones"]["field"][0]["state"] = json!({"acted":true});
    let mut combat = game(&initial);
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
    assert_eq!(
        named(
            &combat.projection(View::Referee).unwrap(),
            "ex",
            "ナテラの大樹"
        ),
        1
    );
}
#[test]
fn bp12_004_fanfare_creates_the_same_batch_carbuncle_spell_token() {
    let mut game = game(&position("BP12-004"));
    play(&mut game);
    triggers(&mut game);
    assert_eq!(
        named(
            &game.projection(View::Referee).unwrap(),
            "ex",
            "カーバンクルの煌めき"
        ),
        1
    );
}
#[test]
fn bp12_102_creates_tiger_in_ex_then_acts_to_grant_its_field_storm() {
    let initial = position("BP12-102");
    let action = Action::Token {
        tokens: vec![TokenSpec {
            template: TokenTemplateId("ホーリータイガー".into()),
            name: Expr::Literal(CardName("ホーリータイガー".into())),
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
    finish(&mut game, step);
    game.resume_internal_card().unwrap();
    play(&mut game);
    triggers(&mut game);
    let before = game.projection(View::Referee).unwrap();
    assert_eq!(named(&before, "ex", "ホーリータイガー"), 1);
    let tiger = before["P1"]["field"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .find(|id| before["objects"][id]["name"] == "ホーリータイガー")
        .unwrap();
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":tiger,"target":"P2.leader"}))
            .unwrap()
            .outcome,
        "cannot-attack"
    );
    activate(&mut game, "BP12-102");
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":tiger,"target":"P2.leader"}))
            .unwrap()
            .outcome,
        "resolved",
        "the amulet grants Storm to the Tiger created this turn"
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["source"]["acted"],
        true
    );
}
#[test]
fn bp12_055_combat_lastword_creates_natera_on_field() {
    let mut initial = position("BP12-055");
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP12-055"}]);
    initial["players"]["P2"]["zones"]["field"][0]["state"] = json!({"acted":true});
    let mut game = game(&initial);
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"enemy"}))
            .unwrap()
            .outcome,
        "resolved",
        "the follower present since the start attacks the rested enemy"
    );
    pass_quick(&mut game);
    triggers(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["cemetery"], json!(["source"]));
    assert_eq!(named(&state, "field", "ナテラの大樹"), 1);
}
#[test]
fn bp12_101_paid_activation_heals_draws_and_lastword_creates_falcon() {
    let mut game = game(&position("BP12-101"));
    play(&mut game);
    game.resume_internal_card().unwrap();
    activate(&mut game, "BP12-101");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["pp"]["current"], 5_u8);
    assert_eq!(state["P1"]["leader"]["life"], 22_u8);
    assert_eq!(state["P1"]["hand"], json!(["first", "second"]));
    assert_eq!(named(&state, "field", "ホーリーファルコン"), 1);
}
#[test]
fn bp12_100_combat_lastword_pays_one_pp_for_falcon() {
    let mut initial = position("BP12-100");
    initial["players"]["P2"]["zones"]["field"][0]["state"] = json!({"acted":true});
    let mut game = game(&initial);
    play(&mut game);
    game.resume_internal_card().unwrap();
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"enemy"}))
            .unwrap()
            .outcome,
        "resolved",
        "printed Rush permits the entry-turn attack"
    );
    pass_quick(&mut game);
    triggers(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(named(&state, "field", "ホーリーファルコン"), 1);
    assert_eq!(state["P1"]["pp"]["current"], 6_u8);
}
