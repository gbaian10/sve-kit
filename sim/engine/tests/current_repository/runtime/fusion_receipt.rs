#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate the printed Sephie scenarios."
)]
use super::*;

const PLAYER: PlayerId = PlayerId { seat: 1 };

fn finish(board: &mut Game, mut play: CardPlay, modes: &[ModeId]) {
    while let CardPlay::Awaiting(question) = play {
        let answer = match &question.domain {
            InputDomain::Modes {
                modes: available,
                count,
                ..
            } => {
                if modes == [ModeId(2)] {
                    assert_eq!(
                        available.as_slice(),
                        modes,
                        "Q2572 excludes the damage mode when there is no legal target"
                    );
                }
                assert!(
                    modes.iter().all(|mode| available.contains(mode)),
                    "printed modes must be legal in the current position"
                );
                assert_eq!(
                    *count,
                    Nat(u64::try_from(modes.len()).unwrap()),
                    "the cemetery threshold determines whether two modes are allowed"
                );
                Answer::Modes(modes.to_vec())
            }
            InputDomain::Targets(domain) => {
                Answer::Targets(domain.members.iter().map(|member| member.handle).collect())
            }
            InputDomain::OptionalCost => Answer::OptionalCost(true),
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            other => panic!("unexpected Sephie decision {other:?}"),
        };
        play = board
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    while board.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        board.resume_internal_card().unwrap();
    }
}

fn settle(board: &mut Game, modes: &[ModeId]) {
    while let Some(question) = board.internal_pending_choice().unwrap() {
        let InputDomain::Pending(options) = &question.domain else {
            panic!("printed pending ability")
        };
        let selected = options.iter().min_by_key(|option| option.line).unwrap();
        let play = board
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(selected.handle),
            )
            .unwrap();
        finish(board, play, modes);
    }
}

#[test]
fn sephie_base_all_prints_draw_only_for_the_actual_eight_prison_material() {
    for (number, selected, draws) in [
        ("BP19-038", "BP19-040", true),
        ("BP19-SL10", "BP01-171", false),
        ("PR-557", "BP19-040", true),
    ] {
        let mut initial = draw_position();
        let eight_prison_zone = if number == "PR-557" { "ex" } else { "hand" };
        let other_zone = if eight_prison_zone == "hand" {
            "ex"
        } else {
            "hand"
        };
        initial["players"]["P1"]["zones"][eight_prison_zone] =
            json!([{"id":"eight","card":"BP19-040"}]);
        initial["players"]["P1"]["zones"][other_zone] = json!([{"id":"other","card":"BP01-171"}]);
        initial["players"]["P1"]["zones"]["hand"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"source","card":number}));
        initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"drawn","card":"BP01-171"}]);
        let mut board = game(&initial);
        let ability = selection().current[number]
            .program
            .card()
            .abilities
            .iter()
            .find(|ability| ability.node.qualification == AbilityQualification::Fusion)
            .unwrap()
            .node
            .id;
        let CardPlay::Awaiting(question) = board
            .activate_internal_card("source", ability, PLAYER)
            .unwrap()
        else {
            panic!("Fusion materials")
        };
        let InputDomain::CostMaterials(domain) = &question.domain else {
            panic!("material choice")
        };
        assert_eq!(
            domain.members.len(),
            2,
            "both follower materials are eligible"
        );
        assert!(
            board.internal_decision(View::P2).is_none(),
            "the opponent cannot see the Fusion candidates"
        );
        let handle = domain
            .members
            .iter()
            .find(|member| member.card.as_deref() == Some(selected))
            .unwrap()
            .handle;
        let play = board
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::CostMaterials(vec![handle]),
            )
            .unwrap();
        finish(&mut board, play, &[]);
        let fused = board.projection(View::Referee).unwrap();
        assert_eq!(
            fused["P1"]["deck"],
            if draws { json!([]) } else { json!(["drawn"]) },
            "{number} draws only for a paid Eight-Prison follower"
        );
        assert_eq!(
            fused["P1"]["cemetery"],
            if draws {
                json!(["eight"])
            } else {
                json!(["other"])
            },
            "only the chosen material is paid"
        );
        assert_eq!(
            fused["objects"]["source"]["counters"]["融合カウンター"], 1_u8,
            "both qualifying and nonqualifying materials add a Fusion counter"
        );
        let from_ex = board.play_internal_card("source", PLAYER).unwrap();
        finish(&mut board, from_ex, &[]);
        settle(&mut board, &[]);
        let entered = board.projection(View::Referee).unwrap();
        assert_eq!(
            entered["objects"]["source"]["counters"]["融合カウンター"], 0_u8,
            "the Fanfare pays the carried Fusion counter"
        );
        assert_eq!(
            entered["P1"]["field"].as_array().unwrap().len(),
            2,
            "the paid Fanfare creates one experiment"
        );
    }
}

#[test]
fn sephie_evolved_all_prints_use_the_printed_mode_limit_and_super_evolution() {
    for (form, enemy, grave, super_evolution, modes) in [
        ("BP19-039", false, 0_usize, false, vec![ModeId(2)]),
        ("BP19-SL11", true, 10, false, vec![ModeId(1), ModeId(2)]),
        ("BP19-U03", true, 9, true, vec![ModeId(1)]),
    ] {
        let mut initial = draw_position();
        initial["turn"]["elapsed_turns"] = json!({"P1":7_u8,"P2":6_u8});
        initial["players"]["P1"]["sep"] = json!(1_u8);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP19-038"}]);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
        initial["players"]["P1"]["zones"]["cemetery"] = json!(
            (0..grave)
                .map(|index| json!({"id":format!("grave-{index}"),"card":"BP19-040"}))
                .collect::<Vec<_>>()
        );
        if enemy {
            initial["players"]["P2"]["zones"]["field"] =
                json!([{"id":"enemy","card":"BP01-171","state":{"hp":20_u8,"max_hp":20_u8}}]);
        }
        if super_evolution {
            initial["players"]["P1"]["zones"]["field"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":"experiment","card":"BP19-P34"}));
        }
        let mut board = game(&initial);
        let pay = if super_evolution {
            json!({"pp":1_u8,"sep":1_u8})
        } else {
            json!({"pp":1_u8})
        };
        assert_eq!(
            board
                .internal_rule_action(
                    &json!({"do":"evolve","source":"source","evolve_card":"form","pay":pay})
                )
                .unwrap()
                .outcome,
            "resolved",
            "{form} can evolve through the printed base card",
        );
        settle(&mut board, &modes);
        let after = board.projection(View::Referee).unwrap();
        assert_eq!(
            after["objects"]["source"]["evolved"], true,
            "{form} is installed"
        );
        assert_eq!(
            after["P1"]["field"].as_array().unwrap().len(),
            2,
            "the board has exactly one created or preexisting experiment"
        );
        if enemy {
            assert_eq!(
                after["objects"]["enemy"]["damage"], 5_u8,
                "a selected damage mode deals five"
            );
        }
        if super_evolution {
            assert!(
                after["objects"]["experiment"]["keywords"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("疾走")),
                "super-evolution grants the existing experiment Storm"
            );
        }
    }
}
