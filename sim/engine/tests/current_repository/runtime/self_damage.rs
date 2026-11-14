use super::*;
use std::fs::read_to_string;
use std::path::PathBuf;
use sve_scenario_runner::{ScenarioReport, Verdict};

#[test]
fn karen_pays_lesson_to_buff_the_field_then_damages_itself_without_another_cool_follower() {
    let mut initial = lesson::position();
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"source","card":"CSD02b-003"},
        {"id":"ally","card":"BP01-173"}
    ]);
    let mut game = game(&initial);
    let before = game.projection(View::Referee).unwrap();
    let ability = game.internal_activated_sources("source").unwrap()[0];
    let CardPlay::Awaiting(payment) = game
        .activate_internal_card_instance("source", ability.instance, PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("Lesson payment");
    };
    let InputDomain::CostMaterials(domain) = &payment.domain else {
        panic!("Lesson EX material");
    };
    assert_eq!(domain.members.len(), 2);
    let material = domain
        .members
        .iter()
        .find(|member| member.card.as_deref() == Some("PR-155"))
        .unwrap()
        .handle;
    resolved(
        game.answer_internal_card(
            payment.actor,
            payment.token,
            Answer::CostMaterials(vec![material]),
        )
        .unwrap(),
    );
    event_cemetery::finish_checks(&mut game);
    let buffed = game.projection(View::Referee).unwrap();
    assert_eq!(buffed["P1"]["pp"]["current"], 7_u8);
    assert_eq!(buffed["P1"]["banish"], json!(["first"]));
    for id in ["source", "ally"] {
        assert_eq!(
            buffed["objects"][id]["power"].as_i64().unwrap(),
            before["objects"][id]["power"]
                .as_i64()
                .unwrap()
                .checked_add(1)
                .unwrap()
        );
        assert_eq!(buffed["objects"][id]["hp"], before["objects"][id]["hp"]);
    }
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    event_cemetery::finish_checks(&mut game);
    while game.internal_pending_choice().unwrap().is_none() {
        let Some(window) = game.internal_quick_choice().unwrap() else {
            break;
        };
        game.answer_internal_card(window.actor, window.token, Answer::Quick(None))
            .unwrap();
        event_cemetery::finish_checks(&mut game);
    }
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("own End-phase ability");
    };
    assert_eq!(options.len(), 1);
    assert_eq!(options[0].card, "CSD02b-003");
    resolved(
        game.answer_internal_card(
            pending.actor,
            pending.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap(),
    );
    event_cemetery::finish_checks(&mut game);
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["objects"]["source"]["damage"], 1_u8);
    assert_eq!(after["objects"]["ally"]["damage"], 0_u8);
    assert_eq!(after["P1"]["leader"]["life"], 20_u8);
}

#[expect(
    clippy::panic,
    reason = "The printed race offers only its optional enemy target."
)]
fn resolve_machan_race(game: &mut Game) -> Activation {
    event_cemetery::finish_checks(game);
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("race trigger");
    };
    assert_eq!(
        options.len(),
        1,
        "only the printed race trigger should be pending"
    );
    let mut play = game
        .answer_internal_card(
            pending.actor,
            pending.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap();
    if let CardPlay::Awaiting(question) = play {
        let InputDomain::Targets(domain) = &question.domain else {
            panic!("optional enemy target");
        };
        let targets = domain
            .members
            .first()
            .map(|member| vec![member.handle])
            .unwrap_or_default();
        play = game
            .answer_internal_card(question.actor, question.token, Answer::Targets(targets))
            .unwrap();
    }
    resolved(play)
}

#[test]
fn aston_machan_race_buffs_then_self_damages_even_without_an_enemy_target() {
    for has_enemy in [false, true] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"ECP01-014"}]);
        initial["players"]["P1"]["zones"]["evolve_deck"] =
            json!([{"id":"carrot","card":"CSD01-032"}]);
        if has_enemy {
            initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
        }
        let mut game = Game::from_selected_effects(
            selection_with_rules().clone(),
            &initial,
            &Value::Null,
            &Value::Null,
            "aston-race",
        )
        .unwrap();
        meal_race::feed(&mut game, "ECP01-014", 0, false);
        let race = resolve_machan_race(&mut game);
        let mut steps = Vec::new();
        for fact in &race.progress.facts {
            if matches!(fact, Fact::Damage(_)) {
                steps.push("damage");
            }
            if let Fact::StatChange(change) = fact {
                steps.push("buff");
                assert_eq!(change.event.facets.first.delta, Int(1));
                assert_eq!(change.event.facets.rest.len(), 1);
                assert_eq!(change.event.facets.rest[0].delta, Int(1));
                assert_ne!(
                    change.event.facets.first.property,
                    change.event.facets.rest[0].property
                );
            }
        }
        let expected: &[&str] = if has_enemy {
            &["damage", "buff", "damage"]
        } else {
            &["buff", "damage"]
        };
        assert_eq!(steps.as_slice(), expected);
        meal_race::settle(&mut game);
        let after = game.projection(View::Referee).unwrap();
        assert_eq!(after["P1"]["pp"]["current"], 9_u8);
        assert_eq!(after["P1"]["cemetery"], json!(["source"]));
        assert_eq!(after["P1"]["field"], json!([]));
        assert_eq!(after["P1"]["race"], json!([]));
        assert_eq!(after["P1"]["evolve_deck"], json!(["carrot"]));
        assert_eq!(after["objects"]["carrot"]["face_up"], true);
        if has_enemy {
            assert_eq!(after["P2"]["field"], json!(["enemy"]));
            assert_eq!(after["objects"]["enemy"]["damage"], 4_u8);
            assert_eq!(after["objects"]["enemy"]["hp"], 1_u8);
        }
        assert_eq!(after["P1"]["leader"]["life"], 20_u8);
    }
}

fn self_damage_exam_reports() -> &'static [ScenarioReport] {
    use sve_engine::adapter::TypedAdapter;
    use sve_scenario_runner::{AwaitingMode, RunOptions, parse_question, run};
    static REPORTS: OnceLock<Vec<ScenarioReport>> = OnceLock::new();
    REPORTS.get_or_init(|| {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let questions = ["card-ECP01-019", "rule-14.2-01"].map(|name| {
            parse_question(
                &read_to_string(root.join(format!("tests/rules-scenarios/questions/{name}.yaml")))
                    .unwrap(),
            )
            .unwrap()
        });
        let reports = run(
            &mut TypedAdapter::new(selection_with_rules().clone()),
            &questions,
            None,
            RunOptions {
                require_verified: true,
                awaiting: AwaitingMode::FirstStage,
            },
        )
        .unwrap();
        assert_eq!(reports.len(), 7, "both questions contain seven scenarios");
        reports
    })
}

fn self_damage_exam_verdict(question: &str, scenario: char) -> &'static Verdict {
    &self_damage_exam_reports()
        .iter()
        .find(|report| report.question == question && report.scenario.starts_with(scenario))
        .unwrap()
        .verdict
}

fn assert_self_damage_exam_passes(question: &str, scenario: char) {
    let verdict = self_damage_exam_verdict(question, scenario);
    assert!(
        matches!(verdict, Verdict::Pass),
        "{question} {scenario}: {verdict:?}"
    );
}

#[test]
fn admitted_self_damage_card_unblocks_carrot_reuse_after_lethal_race_damage() {
    assert_self_damage_exam_passes("card-ECP01-019", 'A');
}

#[test]
fn admitted_self_damage_card_unblocks_cancelling_when_no_face_up_carrot_exists() {
    assert_self_damage_exam_passes("card-ECP01-019", 'B');
}

#[test]
fn admitted_self_damage_card_unblocks_the_nine_card_cemetery_counterexample() {
    assert_self_damage_exam_passes("card-ECP01-019", 'C');
}

#[test]
fn a_named_race_event_still_requires_the_existing_typed_runner_interface() {
    let verdict = self_damage_exam_verdict("rule-14.2-01", 'A');
    assert!(
        matches!(verdict, Verdict::Unsupported { reason }
            if reason == r#"typed runner: pending event {"n":1,"raced":"a1"} cannot be checked"#),
        "the runner must verify the explicitly named occurrence: {verdict:?}"
    );
}

#[test]
fn admitted_self_damage_card_unblocks_the_two_meal_material_requirement() {
    assert_self_damage_exam_passes("rule-14.2-01", 'B');
}

#[test]
fn admitted_self_damage_card_unblocks_the_meal_and_evolution_exclusion() {
    assert_self_damage_exam_passes("rule-14.2-01", 'C');
}

#[test]
fn admitted_self_damage_card_unblocks_paying_one_meal_pp_with_ep() {
    assert_self_damage_exam_passes("rule-14.2-01", 'D');
}
