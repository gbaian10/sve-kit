use super::event_cemetery::finish_checks;
use super::event_subject_capture::prepared_program;
use super::union_burst_batch1::{finish, pending, question};
use super::*;
use sve_engine::game::player::{DecisionShape, Input, QuestionShape, Viewer};

fn target_program(maximum: u8, recipients: bool, zone: &str, aura: bool) -> WellFormedCardProgram {
    let mut tree = expanded::tree().clone();
    let mut card = tree["cards"]["BP01-041"].clone();
    let mut ability = card["abilities"][0].clone();
    let from = if recipients {
        json!({"who":"opp","kind":"follower","leader_or":true})
    } else {
        json!({"who":"opp","zone":zone,"kind":"follower"})
    };
    ability["activate"] = json!({"cost":{"pp":1_u8},"targets":[{"bind":"victims","from":from,"upto":maximum}],"do":[{"damage":{"to":"victims","n":1_u8}}]});
    if zone == "ex" {
        ability["activate"]["do"] = json!([{"banish":"victims"}]);
    }
    card["abilities"] = json!([ability]);
    if aura {
        let mut passive = card["abilities"][0].clone();
        passive["line"] = json!(2_u8);
        passive.as_object_mut().unwrap().remove("activate");
        passive["static"] = json!({"effects":[{"grant":{"on":{"who":"opp","zone":"field","kind":"follower"},"keywords":["aura"]}}]});
        card["abilities"].as_array_mut().unwrap().push(passive);
    }
    tree["cards"] = json!({"producer":card});
    lowerer().lower(&tree).unwrap().remove("producer").unwrap()
}

fn candidates(forced: usize, number: &str, zone: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    initial["players"]["P2"]["zones"][zone] = Value::Array(
        (0..forced)
            .map(|index| json!({"id":format!("x{index}"),"card":number}))
            .chain((0_usize..2).map(|index| json!({"id":format!("y{index}"),"card":"BP01-171"})))
            .collect(),
    );
    initial
}

#[test]
fn q511_and_q2460_maximize_requirements_in_all_five_optional_target_examples() {
    for number in ["BP03-091", "CP04-087"] {
        for (forced, maximum) in [(1, 1), (2, 1), (1, 2), (2, 2), (3, 2)] {
            let initial = candidates(forced, number, "field");
            let mut base =
                prepared_program(&initial, &target_program(maximum, false, "field", false));
            let q = question(
                base.activate_internal_card("producer", AbilityId(1), PlayerId { seat: 1 })
                    .unwrap(),
            );
            let InputDomain::Targets(domain) = &q.domain else {
                panic!("target question")
            };
            let Input::Question(player_question) = base.input(Viewer::P1) else {
                panic!("authorized player target question")
            };
            let QuestionShape::Decision(DecisionShape::Targets(selection)) = player_question.shape
            else {
                panic!("player target constraints")
            };
            let count = Nat(u64::try_from(forced.min(usize::from(maximum))).unwrap());
            assert_eq!(selection.groups.len(), 2);
            assert_eq!(selection.groups[0].cardinality.minimum, count);
            assert_eq!(selection.groups[0].cardinality.maximum, Some(count));
            assert!(selection.cardinality.minimum.0 >= count.0);
            for mask in 0..(1_usize << domain.members.len()) {
                let chosen = domain
                    .members
                    .iter()
                    .enumerate()
                    .filter(|(index, _)| mask & (1 << index) != 0)
                    .map(|(_, member)| member)
                    .collect::<Vec<_>>();
                let chosen_forced = chosen
                    .iter()
                    .filter(|member| member.card.as_deref() == Some(number))
                    .count();
                let expected = chosen.len() <= usize::from(maximum)
                    && chosen_forced == forced.min(usize::from(maximum));
                let mut attempt = base.clone();
                let result = attempt.answer_internal_card(
                    q.actor,
                    q.token,
                    Answer::Targets(chosen.iter().map(|member| member.handle).collect()),
                );
                assert_eq!(
                    result.is_ok(),
                    expected,
                    "forced {forced}, maximum {maximum}, mask {mask}, card {number}"
                );
            }
        }
    }
}

#[test]
fn recipient_targets_cannot_choose_a_leader_or_zero_instead_of_a_required_follower() {
    let mut game = prepared_program(
        &candidates(1, "BP03-091", "field"),
        &target_program(1, true, "field", false),
    );
    let q = question(
        game.activate_internal_card("producer", AbilityId(1), PlayerId { seat: 1 })
            .unwrap(),
    );
    let InputDomain::Targets(domain) = &q.domain else {
        panic!("recipient targets")
    };
    game.answer_internal_card(q.actor, q.token, Answer::Targets(vec![]))
        .unwrap_err();
    let leader = domain
        .members
        .iter()
        .find(|member| member.card.is_none())
        .unwrap()
        .handle;
    game.answer_internal_card(q.actor, q.token, Answer::Targets(vec![leader]))
        .unwrap_err();
    let required = domain
        .members
        .iter()
        .find(|member| member.card.as_deref() == Some("BP03-091"))
        .unwrap()
        .handle;
    let play = game
        .answer_internal_card(q.actor, q.token, Answer::Targets(vec![required]))
        .unwrap();
    resolved(finish(&mut game, play));
}

#[test]
fn ex_and_silenced_cards_do_not_impose_target_requirements() {
    for zone in ["field", "ex"] {
        let mut initial = candidates(1, "BP03-091", zone);
        if zone == "field" {
            initial["players"]["P2"]["zones"][zone][0]["state"] = json!({"silenced":true});
        }
        let mut game = prepared_program(&initial, &target_program(1, false, zone, false));
        let q = question(
            game.activate_internal_card("producer", AbilityId(1), PlayerId { seat: 1 })
                .unwrap(),
        );
        let play = game
            .answer_internal_card(q.actor, q.token, Answer::Targets(vec![]))
            .unwrap();
        resolved(finish(&mut game, play));
    }
}

#[test]
fn diamond_master_plays_chooses_each_keyword_and_normally_evolves_to_revive_a_ward() {
    for mode in [1, 2] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP03-091"}]);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP03-092"}]);
        initial["players"]["P1"]["zones"]["cemetery"] = json!([{"id":"ward","card":"BP01-046"}]);
        let mut game = Game::from_selected_effects(
            selection_with_rules().clone(),
            &initial,
            &Value::Null,
            &Value::Null,
            "diamond-master",
        )
        .unwrap();
        let entry = game
            .play_internal_card("source", PlayerId { seat: 1 })
            .unwrap();
        finish(&mut game, entry);
        let q = question(pending(&mut game, "BP03-091"));
        let play = game
            .answer_internal_card(q.actor, q.token, Answer::Modes(vec![ModeId(mode)]))
            .unwrap();
        resolved(finish(&mut game, play));
        finish_checks(&mut game);
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":3_u8}})
            )
            .unwrap()
            .outcome,
            "resolved"
        );
        let trigger = pending(&mut game, "BP03-092");
        resolved(finish(&mut game, trigger));
        assert!(
            game.projection(View::Referee).unwrap()["P1"]["field"]
                .as_array()
                .unwrap()
                .contains(&json!("ward"))
        );
    }
}

#[test]
fn kuuka_keeps_its_full_union_burst_activation_with_required_targeting() {
    let mut initial = draw_position();
    initial["players"]["P1"]["construction"] = json!("title");
    initial["players"]["P1"]["title_code"] = json!("princess_connect_re_dive");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"CP04-087"}]);
    let mut game = Game::from_selected_effects(
        selection_with_rules().clone(),
        &initial,
        &Value::Null,
        &Value::Null,
        "kuuka",
    )
    .unwrap();
    let id = selection_with_rules().current["CP04-087"]
        .program
        .card()
        .abilities
        .iter()
        .find(|a| a.node.kind == AbilityKind::Activated)
        .unwrap()
        .node
        .id;
    let play = game
        .activate_internal_card("source", id, PlayerId { seat: 1 })
        .unwrap();
    resolved(finish(&mut game, play));
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["source"]["acted"], true);
    assert!(
        state["objects"]["source"]["keywords"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v == "守護")
    );
}

#[test]
fn aura_prevents_the_requirement_instead_of_forcing_an_illegal_target() {
    let mut game = prepared_program(
        &candidates(1, "BP03-091", "field"),
        &target_program(1, true, "field", true),
    );
    let q = question(
        game.activate_internal_card("producer", AbilityId(1), PlayerId { seat: 1 })
            .unwrap(),
    );
    let InputDomain::Targets(domain) = &q.domain else {
        panic!("aura recipients")
    };
    assert_eq!(domain.members.len(), 1);
    assert!(domain.members[0].card.is_none());
    let play = game
        .answer_internal_card(
            q.actor,
            q.token,
            Answer::Targets(vec![domain.members[0].handle]),
        )
        .unwrap();
    resolved(finish(&mut game, play));
}

#[test]
fn priority_is_maximized_within_cost_and_distinct_name_constraints() {
    for (cost_cap, names) in [(Some(5), false), (None, true), (Some(5), true)] {
        let mut card = target_program(2, false, "field", false).card().clone();
        let specification = card.abilities[0].node.play.as_mut().unwrap();
        let ChoiceDomain::Objects { constraint, .. } = &mut specification.parameters[0].domain
        else {
            panic!("objects")
        };
        if names {
            constraint.distinct_by = Some(ObjectProperty::Names);
        }
        if let Some(limit) = cost_cap {
            constraint.aggregate_bounds.push(AggregateBound {
                property: NumericProperty::OriginalCost,
                comparison: Comparison::LessEqual,
                value: Expr::Literal(Int(limit)),
            });
        }
        let program = WellFormedCardProgram::check(card).unwrap();
        let mut game = prepared_program(&candidates(2, "BP03-091", "field"), &program);
        let q = question(
            game.activate_internal_card("producer", AbilityId(1), PlayerId { seat: 1 })
                .unwrap(),
        );
        let InputDomain::Targets(domain) = &q.domain else {
            panic!("targets")
        };
        let required = domain
            .members
            .iter()
            .filter(|m| m.card.as_deref() == Some("BP03-091"))
            .map(|m| m.handle)
            .collect::<Vec<_>>();
        let other = domain
            .members
            .iter()
            .find(|m| m.card.as_deref() == Some("BP01-171"))
            .unwrap()
            .handle;
        game.answer_internal_card(q.actor, q.token, Answer::Targets(vec![other]))
            .unwrap_err();
        game.answer_internal_card(q.actor, q.token, Answer::Targets(required.clone()))
            .unwrap_err();
        let play = game
            .answer_internal_card(q.actor, q.token, Answer::Targets(vec![required[0], other]))
            .unwrap();
        resolved(finish(&mut game, play));
    }
}
