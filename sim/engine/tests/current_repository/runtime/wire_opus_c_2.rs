#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate the printed scenarios."
)]
use super::*;

const fn actor() -> PlayerId {
    PlayerId { seat: 1 }
}

fn finish_checks(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}

fn activated(number: &str) -> AbilityId {
    selection().current[number]
        .program
        .card()
        .abilities
        .iter()
        .find(|ability| ability.node.kind == AbilityKind::Activated)
        .unwrap()
        .node
        .id
}

/// What an activation offered: the mode count and the cost-material cards.
#[derive(Default)]
struct Offered {
    modes: usize,
    materials: Vec<String>,
}

/// Activates `source`, picks mode `mode`, targets the first member and pays with `material`.
fn activate(game: &mut Game, number: &str, mode: usize, material: &str) -> Offered {
    let mut offered = Offered::default();
    let mut step = game
        .activate_internal_card("source", activated(number), actor())
        .unwrap();
    while let CardPlay::Awaiting(q) = step {
        let reply = match &q.domain {
            InputDomain::Modes { modes, .. } => {
                offered.modes = modes.len();
                Answer::Modes(vec![modes[mode]])
            }
            InputDomain::Targets(d) => Answer::Targets(vec![d.members[0].handle]),
            InputDomain::CostMaterials(d) => {
                offered.materials = d.members.iter().filter_map(|m| m.card.clone()).collect();
                let member = d
                    .members
                    .iter()
                    .find(|m| m.card.as_deref() == Some(material))
                    .unwrap();
                Answer::CostMaterials(vec![member.handle])
            }
            other => panic!("unexpected printed decision: {other:?}"),
        };
        step = game.answer_internal_card(q.actor, q.token, reply).unwrap();
    }
    finish_checks(game);
    offered
}

fn names(game: &Game, zone: &str, name: &str) -> usize {
    let view = game.projection(View::Referee).unwrap();
    view["P1"][zone]
        .as_array()
        .unwrap()
        .iter()
        .filter(|id| view["objects"][id.as_str().unwrap()]["name"] == name)
        .count()
}

fn arcus(printing: &str) -> Game {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"source","card":printing},
        {"id":"fodder","card":"BP01-037"}
    ]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"top0","card":"BP01-037"},
        {"id":"top1","card":"BP01-037"},
        {"id":"tail","card":"BP01-037"}
    ]);
    game(&initial)
}

#[test]
fn bp09_071_all_printings_send_another_follower_to_choose_one_of_three_modes_once_per_turn() {
    for printing in ["BP09-071", "BP09-SL15", "BP09-U05", "BP22-R10"] {
        for mode in 0..3 {
            let mut game = arcus(printing);
            let offered = activate(&mut game, "BP09-071", mode, "BP01-037");
            assert_eq!(offered.modes, 3, "{printing}");
            // The source itself is never a material for 場の他のフォロワー.
            assert_eq!(offered.materials, ["BP01-037"], "{printing}");
            let view = game.projection(View::Referee).unwrap();
            assert!(
                view["P1"]["cemetery"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("fodder")),
                "{printing}"
            );
            match mode {
                0 => assert_eq!(view["P1"]["hand"], json!(["top0"]), "{printing}"),
                1 => assert_eq!(names(&game, "field", "ゴースト"), 2, "{printing}"),
                _ => assert_eq!(view["P1"]["deck"], json!(["tail"]), "{printing}"),
            }
            game.activate_internal_card("source", activated("BP09-071"), actor())
                .unwrap_err();
        }
    }
}

#[test]
fn bp09_071_cannot_activate_without_another_follower() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP09-071"}]);
    let mut game = game(&initial);
    game.activate_internal_card("source", activated("BP09-071"), actor())
        .unwrap_err();
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["field"], json!(["source"]));
}

fn vania(printing: &str, bat: bool) -> Game {
    let mut initial = draw_position();
    let mut field = vec![
        json!({"id":"source","card":printing}),
        json!({"id":"ally","card":"BP01-037"}),
    ];
    if bat {
        field.push(json!({"id":"bat","card":"BP01-T15"}));
    }
    initial["players"]["P1"]["zones"]["field"] = Value::Array(field);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"top","card":"BP01-037"}]);
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    game(&initial)
}

#[test]
fn bp09_078_all_printings_act_and_send_a_forest_bat_for_either_mode() {
    for printing in ["BP09-078", "BP09-P23"] {
        let mut game = vania(printing, true);
        let offered = activate(&mut game, "BP09-078", 0, "BP01-T15");
        assert_eq!(offered.modes, 2, "{printing}");
        assert_eq!(offered.materials, ["BP01-T15"], "{printing}: only the named bat");
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["objects"]["enemy"]["damage"], 4_u8, "{printing}");
        assert_eq!(view["P1"]["hand"], json!(["top"]), "{printing}");
        assert_eq!(view["objects"]["source"]["acted"], true, "{printing}");
        assert!(!view["P1"]["field"].as_array().unwrap().contains(&json!("bat")), "{printing}");

        let mut leaders = vania(printing, true);
        activate(&mut leaders, "BP09-078", 1, "BP01-T15");
        let drained = leaders.projection(View::Referee).unwrap();
        assert_eq!(drained["P2"]["leader"]["life"], 18_u8, "{printing}");
        assert_eq!(drained["P1"]["leader"]["life"], 12_u8, "{printing}");
        assert_eq!(drained["P1"]["hand"], json!(["top"]), "{printing}");
        assert_eq!(drained["objects"]["enemy"]["damage"], 0_u8, "{printing}");
    }
}

#[test]
fn bp09_078_cannot_activate_without_a_forest_bat() {
    let mut game = vania("BP09-078", false);
    game.activate_internal_card("source", activated("BP09-078"), actor())
        .unwrap_err();
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["objects"]["source"]["acted"], false);
}

#[test]
fn private_material_choices_still_cannot_sit_beside_activated_modes() {
    let mut raw = selection().current["BP09-078"].program.card().clone();
    material_parameter(&mut raw).visibility = Visibility::Only {
        players: PlayerSet {
            relation: PlayerRelation::SelfPlayer,
            relative_to: PlayerRef::Controller {},
            cardinality: Cardinality {
                minimum: Nat(1),
                maximum: Some(Nat(1)),
            },
            text_quantifier: TextQuantifier::Implicit,
        },
    };
    rejected_at_header(raw);
}

/// Ends the opponent's turn so the controller's main phase starts with Arcus on the field.
fn arcus_main_phase(cemetery: usize) -> Game {
    let mut initial = draw_position();
    initial["turn"]["active"] = json!("P2");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP09-071"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"draw","card":"BP01-037"},
        {"id":"tail","card":"BP01-037"}
    ]);
    initial["players"]["P1"]["zones"]["cemetery"] = Value::Array(
        (0..cemetery)
            .map(|i| json!({"id":format!("grave{i}"),"card":"BP01-037"}))
            .collect(),
    );
    initial["players"]["P2"]["zones"]["deck"] = json!([{"id":"enemy_draw","card":"BP01-037"}]);
    let mut game = game(&initial);
    assert_eq!(
        game.internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap()
            .outcome,
        "resolved",
        "the opponent turn ends"
    );
    loop {
        finish_checks(&mut game);
        if let Some(q) = game.internal_pending_choice().unwrap() {
            let InputDomain::Pending(options) = &q.domain else {
                panic!("main-phase trigger");
            };
            let step = game
                .answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
                .unwrap();
            assert!(matches!(step, CardPlay::Resolved(_)), "no printed decision");
            continue;
        }
        let Some(quick) = game.internal_quick_choice().unwrap() else {
            break;
        };
        game.answer_internal_card(quick.actor, quick.token, Answer::Quick(None))
            .unwrap();
    }
    game
}

#[test]
fn bp09_071_main_phase_makes_a_ghost_only_at_necrocharge_ten() {
    for (cemetery, ghosts) in [(10, 1), (9, 0)] {
        let game = arcus_main_phase(cemetery);
        assert_eq!(names(&game, "field", "ゴースト"), ghosts, "{cemetery}");
    }
}

fn material_parameter(raw: &mut CardProgram) -> &mut Decision {
    raw.abilities[0]
        .node
        .play
        .as_mut()
        .unwrap()
        .parameters
        .iter_mut()
        .find(|decision| {
            matches!(
                decision.kind,
                DecisionKind::PlayParameter {
                    slot: PlaySlot::CostMaterials,
                    ..
                }
            )
        })
        .unwrap()
}

fn rejected_at_header(raw: CardProgram) {
    let error = WellFormedCardProgram::check(raw).unwrap_err();
    assert_eq!(
        error.reason,
        WellFormedReason::UnsupportedShape,
        "the header rejects the shape"
    );
    assert!(
        error.ir_path.ends_with("/abilities/0"),
        "rejected at the ability header: {}",
        error.ir_path
    );
}

#[test]
fn variable_count_materials_still_cannot_sit_beside_activated_modes() {
    let mut raw = selection().current["BP09-078"].program.card().clone();
    let ChoiceDomain::Objects { constraint, .. } = &mut material_parameter(&mut raw).domain else {
        panic!("material domain");
    };
    let any = Cardinality {
        minimum: Nat(0),
        maximum: None,
    };
    constraint.cardinality = any;
    constraint.groups[0].cardinality = any;
    rejected_at_header(raw);
}

#[test]
fn modes_with_their_own_fee_still_cannot_follow_head_materials() {
    let mut raw = selection().current["BP09-078"].program.card().clone();
    let ability = &mut raw.abilities[0].node;
    let CostSpec::Ordered { steps } = &ability.play.as_ref().unwrap().costs else {
        panic!("ordered head costs");
    };
    let fee = steps[0].clone();
    let body = ability.body.as_mut().unwrap();
    let mode = body
        .steps
        .iter_mut()
        .find_map(|step| match &mut step.node {
            Node::Control {
                control: Control::Mode { modes, .. },
            } => modes.first_mut(),
            _ => None,
        })
        .unwrap();
    mode.costs = CostSpec::Ordered { steps: vec![fee] };
    rejected_at_header(raw);
}
