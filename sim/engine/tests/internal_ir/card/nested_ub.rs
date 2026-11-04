#![expect(
    clippy::panic,
    reason = "Unexpected choices invalidate the effect-play UB scenarios."
)]
use super::*;
use sve_engine::EngineFailure;
use sve_engine::game::internal::{DecisionView, InputDomain};

#[derive(Clone, Copy)]
enum Case {
    Printed,
    BodyPayment,
    HeaderPayment,
    Granted,
    Silenced,
    Limited,
    History,
    Observer,
}

fn life(n: u8) -> Value {
    json!({"life":{"who":"self","delta":n}})
}

fn tree(case: Case) -> Value {
    let mut tree = fanfare::expanded::tree().clone();
    let prototype = tree["cards"]["BP01-041"].clone();
    let mut parent = prototype["abilities"][0].clone();
    let mut child = parent.clone();
    let mut steps = vec![life(1)];
    steps.extend([json!({"play_ability":{"of":"friend","ability":{"ub":true,"n":1},"cost_policy":"waive_original"}}), life(10)]);
    parent["activate"] = json!({"cost":{"pp":1},"targets":[{"bind":"friend","from":{"who":"self","zone":"field","kind":"follower","other":true},"n":1}],"do":steps});
    if matches!(case, Case::History | Case::Observer) {
        parent["activate"]["ub"] = json!(true);
    }
    child["line"] = json!(5);
    child["activate"] =
        json!({"ub":true,"cost":{"steps":[{"pp":9},{"act_self":true}]},"do":[life(2)]});
    match case {
        Case::BodyPayment => {
            child.as_object_mut().unwrap().remove("activate");
            child["auto"] = json!({"on":{"phase":{"whose":"self","which":"end"}},"ub":true,"cost":{"pp":9},"do":[{"pay":{"cost":{"act_self":true},"do":[life(2)]}}]});
        }
        Case::HeaderPayment => {
            child.as_object_mut().unwrap().remove("activate");
            child["fanfare"] = json!({"ub":true,"optional_cost":{"act":{"sel":{"who":"self","zone":"field","kind":"follower","orig_cost_eq":99},"n":1},"result":"payment"},"do":[{"if":{"cond":{"paid":{"receipt":"payment"}},"then":[life(2)]}}]});
        }
        Case::Limited => {
            child["activate"]["per_turn"] = json!(1);
            child["activate"]["cost"] = json!({"steps":[{"pp":1},{"act_self":true}]});
        }
        Case::History | Case::Observer => {
            child["activate"]["cost"] = json!({"pp":0});
            child["activate"]["do"] = json!([{"if":{"cond":{"cmp":{"left":{"history_count":{"window":"this_turn","event":"ub_activated","who":"self","exclude":"this_execution"}},"op":"ge","right":3}},"then":[life(2)]}}]);
        }
        Case::Printed | Case::Granted | Case::Silenced => {}
    }
    let mut children = vec![child];
    if matches!(case, Case::Observer) {
        let mut observer = parent.clone();
        observer["line"] = json!(6);
        observer.as_object_mut().unwrap().remove("activate");
        observer["auto"] = json!({"on":{"ub_activated":{"who":"self","holder":{"who":"self","zone":"field","kind":"follower","other":true}}},"do":[life(1)]});
        children.push(observer);
    }
    let mut outer = prototype.clone();
    outer["abilities"] = json!([parent]);
    let mut inner = prototype;
    inner["abilities"] = json!(children);
    tree["cards"] = json!({"unit-other-follower":outer,"unit-follower":inner});
    if matches!(case, Case::Granted | Case::Silenced) {
        let mut provider = tree["cards"]["unit-other-follower"].clone();
        let mut aura = provider["abilities"][0].clone();
        aura.as_object_mut().unwrap().remove("activate");
        aura["static"] = json!({"zone":"equipment","effects":[{"grant":{"on":"host","abilities":[{"activate":{"ub":true,"cost":{"steps":[{"pp":9},{"act_self":true}]},"do":[life(4)]}}]}}]});
        provider["abilities"] = json!([aura]);
        tree["cards"]["unit-equipment"] = provider;
    }

    tree
}

fn build(case: Case, title: bool) -> (Game, AbilityId, AbilityId) {
    let programs = lowerer().lower(&tree(case)).unwrap();
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["pp"] = json!({"current":10,"max":10});
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"parent","card":"unit-other-follower"},{"id":"child","card":"unit-follower"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([]);
    if matches!(case, Case::Granted | Case::Silenced) {
        initial["players"]["P1"]["zones"]["equipment"] =
            json!([{"id":"equipment","card":"unit-equipment","state":{"equipped_to":"child"}}]);
    }
    if matches!(case, Case::Silenced) {
        initial["players"]["P1"]["zones"]["field"][1]["state"] = json!({"silenced":true});
    }
    if title {
        initial["players"]["P1"]["construction"] = json!("title");
        initial["players"]["P1"]["title_code"] = json!("ub_test");
    }
    let mut game = Game::new(
        event_ability::title_catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "nested-ub",
    )
    .unwrap();
    for (number, program) in &programs {
        game.load_internal_card(number, program).unwrap();
    }
    let parent = programs["unit-other-follower"].card().abilities[0].node.id;
    let child = programs["unit-follower"].card().abilities[0].node.id;
    (game, parent, child)
}

fn awaiting(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(question) = play else {
        panic!("expected a question");
    };
    question
}

fn target(game: &mut Game, parent: AbilityId) -> CardPlay {
    let question = awaiting(
        game.activate_internal_card("parent", parent, PlayerId { seat: 1 })
            .unwrap(),
    );
    let InputDomain::Targets(options) = &question.domain else {
        panic!("formal follower target");
    };
    assert_eq!(
        options.members.len(),
        1,
        "only the other own follower is a formal target"
    );
    game.answer_internal_card(question.actor, question.token, Answer::Targets(vec![0]))
        .unwrap()
}

fn select(game: &mut Game, parent: AbilityId, granted: bool) -> CardPlay {
    let question = awaiting(target(game, parent));
    let InputDomain::Abilities(options) = &question.domain else {
        panic!("effective UB abilities");
    };
    assert_eq!(
        options.len(),
        if granted { 2 } else { 1 },
        "effective choices include the printed UB and any installed UB"
    );
    let option = options
        .iter()
        .find(|option| (option.source_card == "unit-equipment") == granted)
        .unwrap();
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::Ability(option.handle),
    )
    .unwrap()
}

fn state(game: &Game, pp: u8, life: u8, ub: u8) {
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        state["P1"]["pp"]["current"], pp,
        "only costs authorized by this play are paid"
    );
    assert_eq!(
        state["P1"]["leader"]["life"], life,
        "both nested and resumed bodies have their expected effects"
    );
    assert_eq!(
        state["semantic_state"]["counters_this_turn"]["P1"]["ub_activated"], ub,
        "only committed UB plays contribute to this turn's history"
    );
}

#[test]
fn waived_printed_and_granted_ub_costs_do_not_mutate_the_later_normal_play() {
    for case in [Case::Printed, Case::Granted] {
        let (mut game, parent, _) = build(case, true);
        let granted = matches!(case, Case::Granted);
        let result = resolved(select(&mut game, parent, granted));
        state(&game, 9, if granted { 35 } else { 33 }, 1);
        let played = result
            .progress
            .facts
            .iter()
            .filter_map(|fact| {
                if let Fact::AbilityPlayed { source, .. } = fact {
                    Some(*source)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        assert_eq!(played.len(), 2);
        assert_ne!(played[0].holder, played[1].holder);
        assert_ne!(played[0].instance, played[1].instance);
        assert!(
            !result
                .progress
                .facts
                .iter()
                .any(|fact| matches!(fact, Fact::Posture(_)))
        );
        event_ability::finish(&mut game);
        let normal = resolved(
            game.activate_internal_card_instance("child", played[1].instance, PlayerId { seat: 1 })
                .unwrap(),
        );
        assert_eq!(normal.source, played[1]);
        assert_ne!(normal.execution, result.execution);
        state(&game, 0, if granted { 39 } else { 35 }, 2);
    }
}

#[test]
fn original_head_materials_are_absent_but_its_paid_branch_is_authorized() {
    let (mut game, parent, _) = build(Case::HeaderPayment, true);
    resolved(select(&mut game, parent, false));
    state(&game, 9, 33, 1);
}

#[test]
fn body_payment_is_still_offered_and_paid_or_declined_after_the_original_cost_is_waived() {
    for accepted in [false, true] {
        let (mut game, parent, _) = build(Case::BodyPayment, true);
        let question = awaiting(select(&mut game, parent, false));
        assert!(matches!(question.domain, InputDomain::OptionalCost));
        resolved(
            game.answer_internal_card(
                question.actor,
                question.token,
                Answer::OptionalCost(accepted),
            )
            .unwrap(),
        );
        state(&game, 9, if accepted { 33 } else { 31 }, 1);
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["child"]["acted"],
            accepted
        );
    }
}

#[test]
fn waived_play_still_requires_title_validity_and_an_effective_ability() {
    let (mut disabled, parent, _) = build(Case::Granted, false);
    resolved(target(&mut disabled, parent));
    state(&disabled, 9, 31, 0);
    let (mut silenced, parent, _) = build(Case::Silenced, true);
    resolved(target(&mut silenced, parent));
    state(&silenced, 9, 31, 0);
}

#[test]
fn authorized_ub_remains_eligible_after_normal_quota_use_and_records_every_effect_play() {
    let (mut game, parent, child) = build(Case::Limited, true);
    resolved(
        game.activate_internal_card("child", child, PlayerId { seat: 1 })
            .unwrap(),
    );
    state(&game, 9, 22, 1);
    event_ability::finish(&mut game);
    resolved(select(&mut game, parent, false));
    state(&game, 8, 35, 2);
    event_ability::finish(&mut game);
    resolved(select(&mut game, parent, false));
    state(&game, 7, 48, 3);
}

#[test]
fn an_effect_play_counts_for_normal_quota_without_limiting_its_effect_authorization() {
    let (mut game, parent, child) = build(Case::Limited, true);
    resolved(select(&mut game, parent, false));
    state(&game, 9, 33, 1);
    event_ability::finish(&mut game);
    let before = game.projection(View::Referee).unwrap();
    assert_eq!(
        before["objects"]["child"]["acted"], false,
        "waived original Act has not been paid"
    );
    let error = game
        .activate_internal_card("child", child, PlayerId { seat: 1 })
        .unwrap_err();
    assert!(
        matches!(error, EngineFailure::IllegalPlay(message) if message == "activated ability quota exhausted"),
        "normal play must be refused by the per-ability usage limit"
    );
    assert_eq!(
        game.projection(View::Referee).unwrap(),
        before,
        "normal refusal cannot consume resources or mutate the field"
    );
    resolved(select(&mut game, parent, false));
    state(&game, 8, 46, 2);
}

#[test]
fn nested_ub_history_retains_prior_same_definition_plays_and_the_committed_mother() {
    for case in [Case::History, Case::Observer] {
        let (mut game, parent, child) = build(case, true);
        for _ in 0..2 {
            resolved(
                game.activate_internal_card("child", child, PlayerId { seat: 1 })
                    .unwrap(),
            );
            event_ability::finish(&mut game);
            assert!(game.internal_pending_choice().unwrap().is_none());
        }
        state(&game, 10, 20, 2);
        resolved(select(&mut game, parent, false));
        state(&game, 9, 33, 4);
        event_ability::finish(&mut game);
        if matches!(case, Case::Observer) {
            let question = game.internal_pending_choice().unwrap().unwrap();
            let InputDomain::Pending(options) = &question.domain else {
                panic!("UB observer");
            };
            assert_eq!(
                options.len(),
                1,
                "the receiver observes the outer other holder only"
            );
            resolved(
                game.answer_internal_card(
                    question.actor,
                    question.token,
                    Answer::Pending(options[0].handle),
                )
                .unwrap(),
            );
            state(&game, 9, 34, 4);
            event_ability::finish(&mut game);
        }
        assert!(game.internal_pending_choice().unwrap().is_none());
    }
}
