#![expect(
    clippy::panic,
    reason = "Unexpected questions invalidate the target-role fixture."
)]
use super::*;
use sve_engine::game::internal::{DecisionView, InputDomain, TargetRoleAssignment};

fn expanded() -> Value {
    let mut tree = fanfare::expanded::tree().clone();
    let mut entry = tree["cards"]["BP08-038"]["abilities"][0].clone();
    entry["fanfare"] = json!({
        "targets":[{"bind":"enemies","from":{"who":"opp","zone":"field","kind":"follower"},
            "upto":3,"roles":{"primary":{"if_nonempty":1},"rest":"remaining"}}],
        "do":[{"damage":{"to":{"role":{"of":"enemies","name":"primary"}},"n":5}},
              {"damage":{"to":{"role":{"of":"enemies","name":"rest"}},"n":2}}]
    });
    let mut card = tree["cards"]["BP08-038"].clone();
    card["abilities"] = json!([entry]);
    tree["cards"] = json!({"unit-follower":card});
    tree
}

fn position() -> (Game, DecisionView) {
    position_with(&expanded())
}

fn position_with(tree: &Value) -> (Game, DecisionView) {
    let typed = lowerer()
        .lower(tree)
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"s","card":"unit-follower"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"a","card":"unit-follower"}, {"id":"b","card":"unit-follower"},
        {"id":"c","card":"unit-follower"}, {"id":"amulet","card":"unit-amulet"}
    ]);
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "role-targets",
    )
    .unwrap();
    game.load_internal_card("unit-follower", &typed).unwrap();
    game.load_internal_card(
        "unit-amulet",
        &WellFormedCardProgram::check(CardProgram {
            abilities: vec![],
            card_play_costs: vec![],
        })
        .unwrap(),
    )
    .unwrap();
    game.play_internal_card("s", PlayerId { seat: 1 }).unwrap();
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let CardPlay::Awaiting(question) = game
        .answer_internal_card(pending.actor, pending.token, Answer::Pending(1))
        .unwrap()
    else {
        panic!("role target question")
    };
    (game, question)
}

fn answer(selected: &[u64], primary: &[u64], rest: &[u64]) -> Answer {
    Answer::TargetsWithRoles {
        targets: selected.to_vec(),
        roles: vec![
            TargetRoleAssignment {
                role: Nat(0),
                targets: primary.to_vec(),
            },
            TargetRoleAssignment {
                role: Nat(1),
                targets: rest.to_vec(),
            },
        ],
    }
}

#[test]
fn target_roles_partition_zero_to_three_targets_without_using_member_order() {
    for count in 0..=3_u64 {
        let (mut game, question) = position();
        let InputDomain::Targets(domain) = &question.domain else {
            panic!("targets")
        };
        assert_eq!(domain.roles.len(), 2);
        assert_eq!(domain.members.len(), 3);
        let packet = game.projection(View::P1).unwrap();
        assert_eq!(
            packet["awaiting"]["domain"]["roles"][0]["cardinality"],
            json!({"if_nonempty":1})
        );
        assert!(game.projection(View::P2).unwrap()["awaiting"].is_null());
        let selected = (0..count).collect::<Vec<_>>();
        let primary = selected.last().copied().into_iter().collect::<Vec<_>>();
        let rest = selected
            .iter()
            .copied()
            .filter(|handle| !primary.contains(handle))
            .collect::<Vec<_>>();
        let play = game
            .answer_internal_card(
                question.actor,
                question.token,
                answer(&selected, &primary, &rest),
            )
            .unwrap();
        assert!(!matches!(play, CardPlay::Awaiting(_)));
        game.resume_internal_card().unwrap();
        let state = game.projection(View::Referee).unwrap();
        for (index, id) in ["a", "b", "c"].into_iter().enumerate() {
            let handle = u64::try_from(index).unwrap();
            if primary.contains(&handle) {
                assert!(
                    state["P2"]["cemetery"]
                        .as_array()
                        .unwrap()
                        .contains(&json!(id))
                );
            } else {
                assert_eq!(
                    state["objects"][id]["hp"],
                    if rest.contains(&handle) { 1 } else { 3 }
                );
            }
        }
    }
}

#[test]
fn target_roles_reject_incomplete_foreign_and_overlapping_answers_atomically() {
    let (mut game, question) = position();
    let before = game.projection(View::Referee).unwrap();
    let mut duplicate_role = answer(&[0, 1], &[0], &[1]);
    let Answer::TargetsWithRoles { roles, .. } = &mut duplicate_role else {
        panic!("role answer fixture")
    };
    roles[1].role = Nat(0);
    for bad in [
        Answer::Targets(vec![0, 1]),
        answer(&[0, 1], &[], &[0, 1]),
        answer(&[0, 1], &[0, 1], &[]),
        answer(&[0, 1], &[0], &[0, 1]),
        answer(&[0, 1], &[0], &[]),
        answer(&[0, 1], &[0], &[2]),
        answer(&[0, 1], &[0], &[99]),
        answer(&[0, 0], &[0], &[0]),
        answer(&[], &[0], &[]),
        duplicate_role,
        Answer::TargetsWithRoles {
            targets: vec![0],
            roles: vec![],
        },
    ] {
        game.answer_internal_card(question.actor, question.token, bad)
            .unwrap_err();
        assert_eq!(game.projection(View::Referee).unwrap(), before);
    }
    game.answer_internal_card(question.actor, question.token, answer(&[0, 1], &[1], &[0]))
        .unwrap();
    game.answer_internal_card(question.actor, question.token, answer(&[], &[], &[]))
        .unwrap_err();
}

#[test]
fn target_roles_reject_unsupported_cardinalities_and_unknown_references() {
    for roles in [
        json!({"primary":{"if_nonempty":2},"rest":"remaining"}),
        json!({"primary":{"if_nonempty":1}}),
        json!({"primary":{"n":1},"rest":"remaining"}),
        json!({"primary":"remaining","rest":"remaining"}),
    ] {
        let mut tree = expanded();
        tree["cards"]["unit-follower"]["abilities"][0]["fanfare"]["targets"][0]["roles"] = roles;
        lowerer().lower(&tree).unwrap_err();
    }
    for from in [
        json!({"who":"opp","zone":"cemetery","kind":"follower"}),
        json!({"leader":"opp"}),
    ] {
        let mut tree = expanded();
        tree["cards"]["unit-follower"]["abilities"][0]["fanfare"]["targets"][0]["from"] = from;
        lowerer().lower(&tree).unwrap_err();
    }
    for key in ["of", "name"] {
        let mut tree = expanded();
        tree["cards"]["unit-follower"]["abilities"][0]["fanfare"]["do"][0]["damage"]["to"]["role"]
            [key] = json!("unknown");
        lowerer().lower(&tree).unwrap_err();
    }
}

#[test]
fn target_roles_are_not_accepted_for_an_ordinary_target_question() {
    let mut tree = expanded();
    let body = &mut tree["cards"]["unit-follower"]["abilities"][0]["fanfare"];
    body["targets"][0].as_object_mut().unwrap().remove("roles");
    body["do"] = json!([{"damage":{"to":"enemies","n":1}}]);
    let (mut game, question) = position_with(&tree);
    game.answer_internal_card(question.actor, question.token, answer(&[0], &[0], &[]))
        .unwrap_err();
    game.answer_internal_card(question.actor, question.token, Answer::Targets(vec![0]))
        .unwrap();
}

#[test]
fn target_roles_runtime_rejects_nonfield_formal_target_domains() {
    let typed = lowerer()
        .lower(&expanded())
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    for destination in [ZoneKind::Cemetery, ZoneKind::Ex] {
        let mut card = typed.card().clone();
        let domain = &mut card.abilities[0].node.play.as_mut().unwrap().parameters[0].domain;
        let ChoiceDomain::Objects { constraint, .. } = domain else {
            panic!("objects")
        };
        let Selector::Zone { zone, .. } = &mut constraint.groups[0].selector else {
            panic!("zone")
        };
        *zone = destination;
        let program = WellFormedCardProgram::check(card).unwrap();
        let mut game = Game::new(
            catalog(),
            &setup(1, json!([])),
            &Value::Null,
            &Value::Null,
            "role-loader",
        )
        .unwrap();
        game.load_internal_card("unit-follower", &program)
            .unwrap_err();
    }
}

#[test]
fn player_question_roles_accept_one_complete_partition_without_authority_reads() {
    use crate::player_pipeline::{choose_selection, question, selected_answer};
    use sve_engine::game::player::{Command, DecisionShape, QuestionShape, SubmissionKind, Viewer};
    let (mut game, _) = position();
    let public = question(&game, Viewer::P1);
    let QuestionShape::Decision(DecisionShape::Targets(targets)) = &public.shape else {
        panic!("public roles");
    };
    assert_eq!(targets.roles.len(), 2);
    assert!(matches!(
        targets.roles[0].cardinality,
        RoleCardinality::IfNonempty { count: Nat(1) }
    ));
    assert!(matches!(
        targets.roles[1].cardinality,
        RoleCardinality::Remainder {}
    ));
    let mut selected = choose_selection(targets);
    if selected.is_empty() {
        selected.push(targets.members[0].handle);
    }
    let complete = selected_answer(&public.shape, &selected, true);
    assert_eq!(
        game.submit_player(
            Viewer::P1,
            Command::Answer {
                token: public.token,
                answer: complete
            }
        )
        .kind,
        SubmissionKind::Applied
    );
}
