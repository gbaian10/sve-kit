use super::*;
use alloc::collections::BTreeMap;
use sve_engine::game::internal::InputDomain;

fn end_step(zone: &str, steps: &Value) -> Value {
    json!({"zone":zone,"on":{"phase":{"whose":"self","which":"end"}},"do":steps})
}

fn source_program(condition: &Value, driver: Value) -> Value {
    let steps = json!([{"if":{"cond":condition,"then":[{"life":{"who":"self","delta":1}}]}}]);
    let mut tree = evolution_linked_movement::tree(&[
        ("auto", end_step("field", &steps)),
        ("auto", end_step("ex", &steps)),
        (
            "lastword",
            json!({"do":[{"move":{"ref":"this","to":"ex"}}]}),
        ),
    ]);
    let mut mover = evolution_linked_movement::tree(&[("auto", driver)]);
    mover["cards"]["unit-follower"]["abilities"][0]["line"] = json!(4_u8);
    tree["cards"]["unit-amulet"] = mover["cards"]["unit-follower"].take();
    tree
}

fn resolve_next(game: &mut Game, line: u64) {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("expected pending abilities");
    };
    let picked = options.iter().find(|option| option.line == line).unwrap();
    let mut step = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(picked.handle),
        )
        .unwrap();
    while let CardPlay::Awaiting(choice) = step {
        let InputDomain::Targets(targets) = &choice.domain else {
            panic!("expected a selection of the source follower");
        };
        assert_eq!(
            targets.members.len(),
            1,
            "only the source follower is a candidate"
        );
        step = game
            .answer_internal_card(
                choice.actor,
                choice.token,
                Answer::Targets(vec![targets.members[0].handle]),
            )
            .unwrap();
    }
    resolved(step);
    game.resume_internal_card().unwrap();
}

fn board(
    source_zone: &str,
    full_ex: bool,
    programs: BTreeMap<String, WellFormedCardProgram>,
) -> Game {
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"driver","card":"unit-amulet"}]);
    let mut members = if source_zone == "field" {
        vec![json!({"id":"driver","card":"unit-amulet"})]
    } else {
        vec![]
    };
    members.push(json!({"id":"source","card":"unit-follower"}));
    initial["players"]["P1"]["zones"][source_zone] = json!(members);
    if full_ex {
        initial["players"]["P1"]["zones"]["ex"] = json!(
            (0_u8..5_u8)
                .map(|index| json!({"id":format!("block-{index}"),"card":"unit-spell"}))
                .collect::<Vec<_>>()
        );
    }
    initial["players"]["P2"]["zones"]["deck"] = json!([{"id":"next","card":"unit-spell"}]);
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "physical-source-zone",
    )
    .unwrap();
    for (number, program) in programs {
        game.load_internal_card(&number, &program).unwrap();
    }
    game.load_internal_card("unit-spell", &card(vec![]))
        .unwrap();
    game
}

#[test]
fn pending_ability_follows_its_physical_source_into_only_its_own_field_or_ex() {
    let return_to_hand = json!({"on":{"phase":{"whose":"self","which":"end"}},
        "targets":[{"bind":"picked","from":{"who":"self","zone":"field","kind":"follower"},"n":1}],
        "do":[{"move":{"ref":"picked","to":"hand"}}]
    });
    let destroy = json!({"on":{"phase":{"whose":"self","which":"end"}},
        "targets":[{"bind":"picked","from":{"who":"self","zone":"field","kind":"follower"},"n":1}],
        "do":[{"destroy":"picked"}]
    });
    for (source_zone, destination, driver, last_word, full_ex) in [
        (
            "field",
            "field",
            end_step("field", &json!([{"life":{"who":"self","delta":0}}])),
            false,
            false,
        ),
        ("field", "hand", return_to_hand, false, false),
        ("field", "ex", destroy.clone(), true, false),
        ("field", "cemetery", destroy, true, true),
        (
            "ex",
            "field",
            end_step(
                "field",
                &json!([
                    {"choose":{"bind":"picked","from":{"who":"self","zone":"ex","kind":"follower"},"upto":1}},
                    {"move":{"ref":"picked","to":"field","result":"entered"}}
                ]),
            ),
            false,
            false,
        ),
        (
            "ex",
            "cemetery",
            end_step(
                "field",
                &json!([
                    {"move":{"ref":{"who":"all","zone":"ex"},"to":"cemetery"}}
                ]),
            ),
            false,
            false,
        ),
    ] {
        let condition =
            json!({"any":[{"physical_source_in_zone":"field"},{"physical_source_in_zone":"ex"}]});
        let programs = lowerer()
            .lower(&source_program(&condition, driver))
            .unwrap_or_else(|error| panic!("{source_zone} to {destination}: {error:?}"));
        let mut game = board(source_zone, full_ex, programs);
        let before = game.projection(View::Referee).unwrap()["P1"]["leader"]["life"]
            .as_u64()
            .unwrap();
        game.internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap();
        resolve_next(&mut game, 4);
        if last_word {
            resolve_next(&mut game, 3);
        }
        resolve_next(&mut game, if source_zone == "field" { 1 } else { 2 });
        let projected = game.projection(View::Referee).unwrap();
        assert!(
            projected["P1"][destination]
                .as_array()
                .unwrap()
                .contains(&json!("source")),
            "the driver left source in {destination}"
        );
        assert_eq!(
            projected["P1"]["leader"]["life"],
            before + u64::from(matches!(destination, "field" | "ex")),
            "{source_zone} to {destination}"
        );
    }
}

#[test]
fn physical_source_zone_rejects_private_zones_and_other_operands() {
    for condition in [
        json!({"physical_source_in_zone":"hand"}),
        json!({"physical_source_in_zone":{"ref":"target","zone":"field"}}),
    ] {
        let driver = end_step("field", &json!([{"life":{"who":"self","delta":0}}]));
        lowerer()
            .lower(&source_program(&condition, driver))
            .unwrap_err();
    }
}
