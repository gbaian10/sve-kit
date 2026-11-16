use super::*;
use crate::game::player::SubmissionKind;
use sve_battle_contract::{GroupChoice, Nat as WireNat};

fn grouped_answer(selection: &wire::Selection) -> Answer {
    let groups = selection
        .groups
        .iter()
        .map(|group| GroupChoice {
            group: group.group,
            selected: group.eligible.iter().take(1).cloned().collect(),
        })
        .collect::<Vec<_>>();
    Answer::Select {
        selected: groups
            .iter()
            .rev()
            .flat_map(|group| group.selected.iter().cloned())
            .collect(),
        groups: Some(groups.into_iter().rev().collect()),
        roles: vec![],
    }
}

#[test]
fn all_printed_search_groups_include_empty_assignments_and_keep_native_group_order() {
    for actor in [Viewer::P1, Viewer::P2] {
        let mut game = make(actor, &grouped(), &deck(), "unit-spell");
        let (native, selection, binding) = projected(&game, actor);
        assert_eq!(selection.group_assignment, wire::GroupAssignment::Explicit);
        assert_eq!(selection.groups.len(), 3);
        let empty = selection
            .groups
            .iter()
            .find(|group| group.group == WireNat::from(1_u32))
            .unwrap();
        assert_eq!(empty.eligible, []);
        assert_eq!(empty.bounds.min, WireNat::ZERO);
        assert_eq!(empty.bounds.max, Some(WireNat::ZERO));
        let answer = grouped_answer(&selection);
        assert_eq!(
            binding.decode(&answer),
            Ok(player::Answer::Search(player::SearchAnswer::Grouped(vec![
                vec![0],
                vec![],
                vec![1]
            ])))
        );
        submit_exact(&mut game, &native, &binding, &answer);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state[seats(actor).0]["hand"], json!(["deck-0", "deck-1"]));
        assert_eq!(state[seats(actor).0]["leader"]["life"], 21_u32);
    }
}

#[test]
fn missing_extra_duplicate_ineligible_or_inconsistent_group_assignments_are_rejected() {
    let game = make(Viewer::P1, &grouped(), &deck(), "unit-spell");
    let (_, selection, binding) = projected(&game, Viewer::P1);
    let good = grouped_answer(&selection);
    for mode in 0_u32..9 {
        let mut answer = good.clone();
        let Answer::Select {
            selected,
            groups,
            roles,
        } = &mut answer
        else {
            panic!("select");
        };
        match mode {
            0 => *groups = None,
            1 => {
                groups.as_mut().unwrap().pop();
            }
            2 => {
                let extra = groups.as_ref().unwrap().first().unwrap().clone();
                groups.as_mut().unwrap().push(extra);
            }
            3 => {
                groups.as_mut().unwrap().first_mut().unwrap().group = WireNat::from(99_u32);
            }
            4 => {
                groups
                    .as_mut()
                    .unwrap()
                    .iter_mut()
                    .find(|group| group.group == WireNat::from(1_u32))
                    .unwrap()
                    .selected = selected.clone();
            }
            5 => selected.clear(),
            6 => {
                let copy = selected.first().unwrap().clone();
                selected.push(copy);
            }
            7 => roles.push(sve_battle_contract::RoleChoice {
                role: WireNat::ZERO,
                selected: selected.clone(),
            }),
            8 => {
                let duplicate = groups.as_ref().unwrap().last().unwrap().group;
                groups.as_mut().unwrap().first_mut().unwrap().group = duplicate;
            }
            _ => panic!("nine malformed assignments"),
        }
        assert_eq!(binding.decode(&answer), Err(AnswerError));
    }
}

#[test]
fn overlapping_eligible_groups_require_a_disjoint_explicit_assignment() {
    let search = json!({"groups":[
        {"sel":{"who":"self","zone":"deck","kind":"follower"},"n":1_u32},
        {"sel":{"who":"self","zone":"deck","kind":"follower"},"n":1_u32}
    ],"to":"hand"});
    for actor in [Viewer::P1, Viewer::P2] {
        let mut game = make(actor, &search, &deck(), "unit-spell");
        let (native, selection, binding) = projected(&game, actor);
        let first = selection.candidates.first().unwrap().id.clone();
        let last = selection.candidates.last().unwrap().id.clone();
        let mut answer = Answer::Select {
            selected: vec![first.clone()],
            groups: Some(
                selection
                    .groups
                    .iter()
                    .map(|group| GroupChoice {
                        group: group.group,
                        selected: vec![first.clone()],
                    })
                    .collect(),
            ),
            roles: vec![],
        };
        assert_eq!(binding.decode(&answer), Err(AnswerError));
        let Answer::Select {
            selected,
            groups: Some(groups),
            ..
        } = &mut answer
        else {
            panic!("grouped search");
        };
        selected.push(last.clone());
        groups.last_mut().unwrap().selected = vec![last];
        submit_exact(&mut game, &native, &binding, &answer);
        assert_eq!(
            game.projection(View::Referee).unwrap()[seats(actor).0]["hand"],
            json!(["deck-0", "deck-2"])
        );
    }
}

#[test]
fn frozen_rules_names_and_total_original_cost_are_enforced_by_native_submission() {
    let constrained = json!({"sel":{"who":"self","zone":"deck"},"upto":3_u32,"distinct_by":"name","cost_sum_le":2_u32,"to":"hand"});
    for actor in [Viewer::P1, Viewer::P2] {
        for cards in [deck(), ["unit-follower", "unit-spell", "unit-amulet"]] {
            let mut game = make(actor, &constrained, &cards, "unit-spell");
            let (native, selection, binding) = projected(&game, actor);
            assert_eq!(selection.distinct, vec![wire::Distinct::RulesName]);
            assert_eq!(
                selection.original_cost_total.max,
                Some(sve_battle_contract::Int::from(2_i32))
            );
            assert!(selection.candidates.iter().all(|candidate| {
                candidate
                    .rules_names
                    .as_ref()
                    .is_some_and(|names| !names.is_empty())
                    && candidate.original_cost == Some(sve_battle_contract::Int::from(1_i32))
            }));
            let selected = selection
                .candidates
                .iter()
                .filter(|candidate| {
                    cards.last() != Some(&"unit-follower")
                        || candidate.rules_names
                            == selection.candidates.first().unwrap().rules_names
                })
                .map(|candidate| candidate.id.clone())
                .collect::<Vec<_>>();
            let bad = Answer::Select {
                selected: selected.clone(),
                groups: Some(vec![GroupChoice {
                    group: WireNat::ZERO,
                    selected,
                }]),
                roles: vec![],
            };
            let before = board_fixtures::exact_authority(&game);
            assert_eq!(
                game.submit_player(actor, binding.answer(native.token, &bad).unwrap())
                    .kind,
                SubmissionKind::Rejected
            );
            assert!(
                board_fixtures::exact_authority(&game) == before,
                "native constraint failures preserve the complete stop and authority"
            );
            let valid_selection = selection
                .candidates
                .iter()
                .take(2)
                .map(|candidate| candidate.id.clone())
                .collect::<Vec<_>>();
            let good = Answer::Select {
                selected: valid_selection.clone(),
                groups: Some(vec![GroupChoice {
                    group: WireNat::ZERO,
                    selected: valid_selection,
                }]),
                roles: vec![],
            };
            submit_exact(&mut game, &native, &binding, &good);
            assert_eq!(
                game.projection(View::Referee).unwrap()[seats(actor).0]["leader"]["life"],
                21_u32
            );
        }
    }
}
