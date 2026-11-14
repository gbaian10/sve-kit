use super::*;
use sve_engine::game::internal::{Answer, InputDomain, ResumeStage};

fn program(depart: bool, phase: &str) -> WellFormedCardProgram {
    let mut tree = equipment_played::tree(
        &json!({"line":1,"static":{"effects":[
            {"extra_trigger":{"phase":phase,"who":"self","n":1}}
        ]}}),
        "unit-follower",
    );
    let mut observer = tree["cards"]["unit-follower"]["abilities"][0].clone();
    observer.as_object_mut().unwrap().remove("static");
    observer["line"] = json!(2);
    observer["auto"] = json!({"on":{"phase":{"whose":"self","which":"end"}},
        "do":[{"life":{"who":"self","delta":1}}]});
    if depart {
        observer["auto"]["do"]
            .as_array_mut()
            .unwrap()
            .push(json!({"move":{"ref":"this","to":"cemetery"}}));
    }
    tree["cards"]["unit-follower"]["abilities"]
        .as_array_mut()
        .unwrap()
        .push(observer);
    lowerer()
        .lower(&tree)
        .unwrap()
        .remove("unit-follower")
        .unwrap()
}

fn finish(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}

#[test]
fn phase_multiplicity_is_additive_live_and_keeps_original_sources_after_departure() {
    for (providers, silenced, depart, phase) in [
        (1, false, false, "end"),
        (2, false, false, "end"),
        (1, true, false, "end"),
        (1, false, true, "end"),
        (1, false, false, "main"),
    ] {
        let mut initial = setup(1, json!([]));
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] = json!(
            (0..providers)
                .map(
                    |i| json!({"id":format!("provider-{i}"),"card":"unit-follower",
                "owner":"P2","state":{"silenced":silenced}})
                )
                .collect::<Vec<_>>()
        );
        initial["players"]["P2"]["zones"]["field"] = json!([]);
        let mut game = Game::new(
            catalog(),
            &initial,
            &Value::Null,
            &Value::Null,
            "extra-phase",
        )
        .unwrap();
        game.load_internal_card("unit-follower", &program(depart, phase))
            .unwrap();
        game.load_internal_card("unit-spell", &card(vec![]))
            .unwrap();
        game.internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap();
        let expected = if silenced {
            0
        } else if phase == "end" {
            providers * (providers + 1)
        } else {
            providers
        };
        let mut sources = Vec::new();
        while let Some(question) = game.internal_pending_choice().unwrap() {
            let InputDomain::Pending(options) = &question.domain else {
                panic!("pending");
            };
            assert_eq!(question.actor, PlayerId { seat: 1 });
            assert!(
                options
                    .iter()
                    .all(|option| option.card == "unit-follower" && option.line == 2)
            );
            let activation = resolved(
                game.answer_internal_card(
                    question.actor,
                    question.token,
                    Answer::Pending(options[0].handle),
                )
                .unwrap(),
            );
            assert_eq!(activation.source.controller, PlayerId { seat: 1 });
            sources.push(activation.source.holder);
            finish(&mut game);
        }
        assert_eq!(sources.len(), expected);
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["P1"]["leader"]["life"], 20 + expected);
        assert_eq!(view["P2"]["leader"]["life"], 20);
        if depart {
            assert_eq!(sources[0], sources[1]);
        }
    }
}

#[test]
fn conditional_mode_cap_counts_other_public_allies_without_counting_source_or_opponent() {
    let input = equipment_played::tree(
        &json!({"auto":{"on":{"phase":{"whose":"self","which":"end"}},
            "do":[{"choice":{"n":1,"instead":[{"if":{"cmp":{
                "left":{"count":{"who":"self","zone":"field","kind":"follower","other":true}},
                "op":"ge","right":1}},"upto":2}],
                "options":[[{"life":{"who":"self","delta":1}}],
                            [{"life":{"who":"self","delta":2}}]]}}]}}),
        "unit-follower",
    );
    let program = lowerer()
        .lower(&input)
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    let mut private = input;
    let count = &mut private["cards"]["unit-follower"]["abilities"][0]["auto"]["do"][0]["choice"]["instead"]
        [0]["if"]["cmp"]["left"]["count"];
    count["zone"] = json!("hand");
    count["other"] = json!(false);
    lowerer().lower(&private).unwrap_err();
    for other in [false, true] {
        let mut initial = setup(1, json!([]));
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
        if other {
            initial["players"]["P1"]["zones"]["field"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":"ally","card":"unit-follower"}));
        }
        let mut game = Game::new(
            catalog(),
            &initial,
            &Value::Null,
            &Value::Null,
            "other-mode-count",
        )
        .unwrap();
        game.load_internal_card("unit-follower", &program).unwrap();
        game.load_internal_card("unit-spell", &card(vec![]))
            .unwrap();
        game.internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap();
        let question = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = &question.domain else {
            panic!("pending");
        };
        let CardPlay::Awaiting(modes) = game
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap()
        else {
            panic!("mode declaration");
        };
        let InputDomain::Modes { count, .. } = &modes.domain else {
            panic!("modes");
        };
        assert_eq!(*count, Nat(if other { 2 } else { 1 }));
        resolved(
            game.answer_internal_card(
                modes.actor,
                modes.token,
                Answer::Modes(if other {
                    vec![ModeId(1), ModeId(2)]
                } else {
                    vec![ModeId(1)]
                }),
            )
            .unwrap(),
        );
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
            if other { 23 } else { 21 }
        );
    }
}
