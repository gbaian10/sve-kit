#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate these complete-card scenarios."
)]
use super::*;

const fn actor() -> PlayerId {
    PlayerId { seat: 1 }
}

fn rules_game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "damage-prevent-cards",
    )
    .unwrap()
}

fn finish(game: &mut Game, mut play: CardPlay, mode: usize, optional: bool) -> CardPlay {
    while let CardPlay::Awaiting(question) = play {
        let answer = match &question.domain {
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            InputDomain::OptionalCost => Answer::OptionalCost(optional),
            InputDomain::TokenCapacity { choices } => {
                assert_eq!(
                    choices.len(),
                    1,
                    "the printed single token has one complete allocation"
                );
                Answer::TokenCapacity(0)
            }
            InputDomain::Modes { modes, .. } => Answer::Modes(vec![modes[mode]]),
            InputDomain::Targets(domain) => Answer::Targets(vec![domain.members[0].handle]),
            InputDomain::CostMaterials(domain) => {
                Answer::CostMaterials(vec![domain.members[0].handle])
            }
            InputDomain::ResourceEntryPayment { .. }
            | InputDomain::Quick { .. }
            | InputDomain::BeforeDraw
            | InputDomain::Pending(_)
            | InputDomain::TriggerQuota(_)
            | InputDomain::Integer(_)
            | InputDomain::Order(_)
            | InputDomain::DriveTrigger { .. }
            | InputDomain::Search(_)
            | InputDomain::SearchPartition(_)
            | InputDomain::Distribution(_)
            | InputDomain::SearchReplacement
            | InputDomain::DamageReplacements(_) => panic!("unexpected card decision"),
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    play
}

fn pending_with_options(game: &mut Game, number: &str, mode: usize, optional: bool) -> CardPlay {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = question.domain else {
        panic!("printed trigger");
    };
    let option = options.iter().find(|option| option.card == number).unwrap();
    let play = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(option.handle),
        )
        .unwrap();
    let play = finish(game, play, mode, optional);
    if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
    play
}

fn pending(game: &mut Game, number: &str) {
    resolved(pending_with_options(game, number, 0, false));
}

fn pass_quick(game: &mut Game) {
    for _ in 0_u8..4 {
        let Some(question) = game.internal_quick_choice().unwrap() else {
            return;
        };
        game.answer_internal_card(question.actor, question.token, Answer::Quick(None))
            .unwrap();
    }
    panic!("Quick window did not close");
}

fn damage_field(n: i64) -> Action {
    Action::Damage {
        recipients: RecipientSet::Objects {
            objects: Selector::Difference {
                input: Box::new(subjects()),
                except: Box::new(Selector::Objects {
                    objects: vec![ObjectOperand::SourceHolder {}],
                }),
            },
        },
        amount: DamageAmount::Uniform {
            amount: Expr::Literal(Int(n)),
        },
        flags: DamageFlags::Ability,
        source: ObjectView::SourceInformation {},
        batch: BatchPolicy::Instruction {},
    }
}

fn hit(game: &mut Game) {
    resolved(
        game.activate_internal_card("producer", AbilityId(1), actor())
            .unwrap(),
    );
    game.resume_internal_card().unwrap();
}

fn immunity(number: &str) {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"immune","card":number},{"id":"ordinary","card":"BP01-037"}]);
    let mut game = with_producer(&initial, damage_field(1));
    hit(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        state["objects"]["immune"]["damage"], 0_u8,
        "the printed follower prevents its own ability damage"
    );
    assert_eq!(
        state["objects"]["ordinary"]["damage"], 1_u8,
        "ordinary followers still take the same hit"
    );
}

#[test]
fn bp14_084_both_printings_prevent_ability_damage_only_on_themselves() {
    for number in ["BP14-084", "PR-534"] {
        immunity(number);
    }
}

#[test]
fn bp15_042_both_advance_printings_prevent_ability_damage() {
    for number in ["BP15-042", "BP15-SL14"] {
        immunity(number);
    }
}

#[test]
fn bp18_075_ability_immunity_does_not_shield_other_followers() {
    immunity("BP18-075");
}

#[test]
fn bp02_019_all_printings_really_evolve_and_prevent_retaliation() {
    for number in [
        "BP02-019",
        "BP02-SL05",
        "SD07-007",
        "SP01-015",
        "SP01-SL15",
        "SP01-SP10",
        "SP01-U09",
    ] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"albert","card":"BP02-018"}]);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":number}]);
        initial["players"]["P2"]["zones"]["field"] =
            json!([{"id":"enemy","card":"BP01-037","state":{"acted":true}}]);
        let mut game = rules_game(&initial);
        let evolution = game
            .internal_rule_action(
                &json!({"do":"evolve","source":"albert","evolve_card":"form","pay":{"pp":3_u8}}),
            )
            .unwrap();
        assert_eq!(evolution.outcome, "resolved");
        pending(&mut game, number);
        game.internal_rule_action(&json!({"do":"attack","attacker":"albert","target":"enemy"}))
            .unwrap();
        pending(&mut game, number);
        pass_quick(&mut game);
        let first = game.projection(View::Referee).unwrap();
        assert_eq!(
            first["objects"]["albert"]["damage"], 0_u8,
            "the five-power defender cannot retaliate"
        );
        assert_eq!(
            first["objects"]["albert"]["acted"], false,
            "the printed once-per-turn attack ability stands Albert"
        );
        assert_eq!(first["objects"]["enemy"]["damage"], 3_u8);
        game.internal_rule_action(&json!({"do":"attack","attacker":"albert","target":"P2.leader"}))
            .unwrap();
        assert!(
            game.internal_pending_choice().unwrap().is_none(),
            "the stand trigger is once per turn"
        );
        pass_quick(&mut game);
        let second = game.projection(View::Referee).unwrap();
        assert_eq!(second["P2"]["leader"]["life"], 17_u8);
        assert_eq!(second["objects"]["albert"]["acted"], true);
        assert_eq!(second["P1"]["pp"]["current"], 7_u8);
        assert_eq!(second["P1"]["evolve_deck"], json!([]));
    }
}

#[test]
fn bp10_056_all_printings_keep_both_choices_and_the_official_awakened_boundary() {
    for number in ["BP10-056", "BP10-SL14", "PR-344"] {
        for (maximum, mode, expected_maximum, expected_current, expected_life, expected_field) in [
            (6_u8, 0_usize, 7_u8, 3_u8, 10_u8, 1_usize),
            (6, 1, 6, 1, 10, 1),
            (7, 1, 7, 2, 12, 2),
        ] {
            let mut initial = draw_position();
            initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
            initial["players"]["P1"]["pp"] = json!({"max":maximum,"current":maximum});
            initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"aila","card":number}]);
            let mut game = rules_game(&initial);
            let play = game.play_internal_card("aila", actor()).unwrap();
            assert!(matches!(
                finish(&mut game, play, mode, false),
                CardPlay::Entered(_)
            ));
            if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
                game.resume_internal_card().unwrap();
            }
            resolved(pending_with_options(&mut game, number, mode, false));
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["P1"]["pp"]["max"], expected_maximum);
            assert_eq!(state["P1"]["pp"]["current"], expected_current);
            assert_eq!(
                state["P1"]["leader"]["life"], expected_life,
                "Q1308 puts both token and healing inside Awakened"
            );
            assert_eq!(
                state["P1"]["field"].as_array().unwrap().len(),
                expected_field
            );
        }
    }
}

#[test]
fn bp10_t02_both_printings_lose_ability_immunity_when_aila_leaves() {
    for number in ["BP10-T02", "PR-345"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["field"] =
            json!([{"id":"aila","card":"BP10-056"},{"id":"dragon","card":number}]);
        let mut game = with_producer(&initial, damage_field(2));
        hit(&mut game);
        let first = game.projection(View::Referee).unwrap();
        assert_eq!(first["P1"]["cemetery"], json!(["aila"]));
        assert_eq!(
            first["objects"]["dragon"]["damage"], 0_u8,
            "Aila's lethal damage waits for the checkpoint"
        );
        hit(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["dragon"]["damage"],
            2_u8,
            "the live named condition is now false"
        );
    }
}

#[test]
fn bp12_109_evolution_captures_only_the_opposing_followers() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"singer","card":"BP12-108"},{"id":"ally","card":"BP01-037"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP12-109"}]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-037","state":{"acted":true}}]);
    let mut game = rules_game(&initial);
    game.internal_rule_action(
        &json!({"do":"evolve","source":"singer","evolve_card":"form","pay":{"pp":1_u8}}),
    )
    .unwrap();
    pending(&mut game, "BP12-109");
    game.internal_rule_action(&json!({"do":"attack","attacker":"ally","target":"enemy"}))
        .unwrap();
    pass_quick(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        state["objects"]["ally"]["damage"], 0_u8,
        "the captured enemy cannot retaliate"
    );
    assert_eq!(
        state["P2"]["cemetery"],
        json!(["enemy"]),
        "the allied five damage was not prohibited"
    );
    assert_eq!(state["P1"]["pp"]["current"], 9_u8);
}

#[test]
fn bp17_018_real_attack_trigger_prevents_the_defenders_retaliation() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"brave","card":"BP17-018"}]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-037","state":{"acted":true}}]);
    let mut game = rules_game(&initial);
    game.internal_rule_action(&json!({"do":"attack","attacker":"brave","target":"enemy"}))
        .unwrap();
    pending(&mut game, "BP17-018");
    pass_quick(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["brave"]["damage"], 0_u8);
    assert_eq!(
        state["objects"]["enemy"]["damage"], 4_u8,
        "the attack still deals its full printed power"
    );
    assert_eq!(state["objects"]["brave"]["acted"], true);
}

#[test]
fn bp18_116_all_printings_pay_discard_and_shield_both_real_named_sets() {
    for number in ["BP18-116", "BP18-SL26"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"chief","card":number},{"id":"clone","card":"BP18-T10"},{"id":"ordinary","card":"BP01-037"}]);
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"cost","card":"BP18-116"}]);
        let mut game = with_producer(&initial, damage_field(1));
        let ability = selection().current[number]
            .program
            .card()
            .abilities
            .iter()
            .find(|ability| ability.node.kind == AbilityKind::Activated)
            .unwrap()
            .node
            .id;
        let play = game
            .activate_internal_card("chief", ability, actor())
            .unwrap();
        resolved(finish(&mut game, play, 0, false));
        game.resume_internal_card().unwrap();
        hit(&mut game);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["cemetery"], json!(["cost"]));
        assert_eq!(state["objects"]["chief"]["damage"], 0_u8);
        assert_eq!(state["objects"]["clone"]["damage"], 0_u8);
        assert_eq!(state["objects"]["ordinary"]["damage"], 1_u8);
    }
}

#[test]
fn bp18_t10_both_real_token_printings_create_their_complete_named_clone_at_end_phase() {
    for number in ["BP18-T10", "BP18-P56"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"clone","card":number}]);
        let mut game = rules_game(&initial);
        game.internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap();
        pending(&mut game, number);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["field"].as_array().unwrap().len(), 2);
        let clone = state["P1"]["field"]
            .as_array()
            .unwrap()
            .iter()
            .find(|id| **id != json!("clone"))
            .unwrap()
            .as_str()
            .unwrap();
        assert_eq!(state["objects"][clone]["name"], "分身の術・煤木万裁");
        assert_eq!(state["objects"][clone]["power"], 5_u8);
        assert!(
            game.internal_pending_choice().unwrap().is_none(),
            "a token arriving during end phase does not replay that phase's start"
        );
    }
}

fn advance_entry() -> Action {
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
            zone: ZoneKind::Evolve,
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

#[test]
fn bp15_042_real_advance_entry_hits_every_opponent_and_keeps_own_side_safe() {
    for number in ["BP15-042", "BP15-SL14"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"advance","card":number}]);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"ally","card":"BP01-037"}]);
        initial["players"]["P2"]["zones"]["field"] = json!([
            {"id":"enemy-1","card":"BP01-037"},
            {"id":"enemy-2","card":"BP01-037"},
            {"id":"immune-enemy","card":"BP18-075"}
        ]);
        let mut game = with_producer(&initial, advance_entry());
        let play = game
            .activate_internal_card("producer", AbilityId(1), actor())
            .unwrap();
        resolved(finish(&mut game, play, 0, false));
        game.resume_internal_card().unwrap();
        pending(&mut game, number);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["leader"]["life"], 20_u8);
        assert_eq!(state["P2"]["leader"]["life"], 15_u8);
        assert_eq!(state["objects"]["ally"]["damage"], 0_u8);
        assert!(
            state["P1"]["field"]
                .as_array()
                .unwrap()
                .contains(&json!("advance"))
        );
        assert_eq!(state["P2"]["cemetery"], json!(["enemy-1", "enemy-2"]));
        assert_eq!(state["objects"]["immune-enemy"]["damage"], 0_u8);
    }
}

#[test]
fn bp17_018_fanfare_only_spends_and_buffs_when_the_optional_cost_is_accepted() {
    for (accepted, expected_pp, expected_stat) in [(false, 6_u8, 4_u8), (true, 4, 6)] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"brave","card":"BP17-018"}]);
        let mut game = rules_game(&initial);
        let play = game.play_internal_card("brave", actor()).unwrap();
        assert!(matches!(
            finish(&mut game, play, 0, accepted),
            CardPlay::Entered(_)
        ));
        if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
        let fanfare = pending_with_options(&mut game, "BP17-018", 0, accepted);
        assert!(matches!(
            fanfare,
            CardPlay::Resolved(_) | CardPlay::Cancelled
        ));
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["pp"]["current"], expected_pp);
        assert_eq!(state["objects"]["brave"]["power"], expected_stat);
        assert_eq!(state["objects"]["brave"]["hp"], expected_stat);
    }
}
