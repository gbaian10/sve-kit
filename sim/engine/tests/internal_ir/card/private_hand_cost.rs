use super::*;
use alloc::collections::BTreeSet;
use sve_engine::game::internal::InputDomain;

fn program() -> WellFormedCardProgram {
    let tree = evolution_linked_movement::tree(&[(
        "activate",
        json!({
            "cost":{"pp":1},"do":[
                {"choose":{"bind":"picked","from":{"who":"self","zone":"hand","kind":"follower","trait":"target-trait","not_same_name_as_this":true,"orig_cost_le":{"resource":{"who":"self","kind":"pp"}}},"upto":1}},
                {"move":{"ref":"picked","to":"field","result":"entered"}}
            ]
        }),
    )]);
    lowerer()
        .lower(&tree)
        .unwrap()
        .remove("unit-follower")
        .unwrap()
}

fn catalogue() -> Arc<Catalog> {
    let (snapshot, keywords, effects) = catalog_documents(&json!("1"));
    let mut rows: Vec<Value> = snapshot
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let mut effects: Value = serde_json::from_str(&effects).unwrap();
    let template = rows
        .iter()
        .find(|row| row["number"] == "unit-follower")
        .unwrap()
        .clone();
    for row in &mut rows {
        row["faces"][0]["traits"] = json!(["target-trait"]);
    }
    for (number, cost, matched) in [
        ("unit-cheap", "1", true),
        ("unit-high", "2", true),
        ("unit-other", "1", false),
    ] {
        let mut row = template.clone();
        row["number"] = json!(number);
        row["faces"][0]["name"] = json!(number);
        row["faces"][0]["cost"] = json!(cost);
        row["faces"][0]["traits"] = if matched {
            json!(["target-trait"])
        } else {
            json!([])
        };
        rows.push(row);
        effects["cards"][number] = json!({"status":"complete","review":"synthetic","abilities":[]});
    }
    let snapshot = rows
        .iter()
        .map(Value::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    Arc::new(
        Catalog::from_documents(
            &snapshot,
            &keywords,
            &[("unit.yaml".into(), effects.to_string())],
        )
        .unwrap(),
    )
}

fn position(program: &WellFormedCardProgram, seat: u32, pp: u8) -> Game {
    let actor = if seat == 1 { "P1" } else { "P2" };
    let other = if seat == 1 { "P2" } else { "P1" };
    let mut initial = setup(seat, json!([]));
    initial["turn"]["active"] = json!(actor);
    initial["players"][actor]["pp"] = json!({"current":pp,"max":10});
    initial["players"][other]["pp"] = json!({"current":9,"max":9});
    initial["players"][actor]["zones"]["field"] = json!([{"id":"source","card":"unit-follower"}]);
    initial["players"][actor]["zones"]["hand"] = json!([
        {"id":"same","card":"unit-follower"},
        {"id":"unit-cheap","card":"unit-cheap"},
        {"id":"unit-high","card":"unit-high"},
        {"id":"wrong-kind","card":"unit-amulet"},
        {"id":"wrong-trait","card":"unit-other"}
    ]);
    let mut game = Game::new(
        catalogue(),
        &initial,
        &Value::Null,
        &Value::Null,
        "private-hand-cost",
    )
    .unwrap();
    game.load_internal_card("unit-follower", program).unwrap();
    for number in ["unit-cheap", "unit-high", "unit-other", "unit-amulet"] {
        game.load_internal_card(number, &card(vec![])).unwrap();
    }
    game
}

#[test]
fn private_hand_dynamic_cost_reads_remaining_pp_after_payment_and_excludes_source_names() {
    let program = program();
    for seat in [1_u32, 2] {
        let actor = if seat == 1 { "P1" } else { "P2" };
        let other_view = if seat == 1 { View::P2 } else { View::P1 };
        for (pp, chosen, expected) in [
            (1_u8, None, vec![]),
            (2, Some("unit-cheap"), vec!["unit-cheap"]),
            (3, Some("unit-high"), vec!["unit-cheap", "unit-high"]),
            (3, None, vec!["unit-cheap", "unit-high"]),
        ] {
            let mut game = position(&program, seat, pp);
            let CardPlay::Awaiting(question) = game
                .activate_internal_card(
                    "source",
                    program.card().abilities[0].node.id,
                    PlayerId { seat },
                )
                .unwrap()
            else {
                panic!("private hand cost choice");
            };
            let InputDomain::Targets(domain) = &question.domain else {
                panic!("private hand candidates");
            };
            let offered: BTreeSet<_> = domain
                .members
                .iter()
                .filter_map(|member| member.card.as_deref())
                .collect();
            assert_eq!(offered, expected.into_iter().collect());
            assert!(game.internal_decision(other_view).is_none());
            assert!(game.projection(other_view).unwrap()["objects"]["unit-cheap"].is_null());
            assert_eq!(
                game.projection(View::Referee).unwrap()[actor]["pp"]["current"],
                pp.checked_sub(1).unwrap()
            );
            let answer = chosen.map_or_else(Vec::new, |number| {
                vec![
                    domain
                        .members
                        .iter()
                        .find(|member| member.card.as_deref() == Some(number))
                        .unwrap()
                        .handle,
                ]
            });
            resolved(
                game.answer_internal_card(question.actor, question.token, Answer::Targets(answer))
                    .unwrap(),
            );
            let view = game.projection(View::Referee).unwrap();
            assert_eq!(view[actor]["pp"]["current"], pp.checked_sub(1).unwrap());
            assert_eq!(
                view[actor]["field"].as_array().unwrap().len(),
                if chosen.is_some() { 2 } else { 1 }
            );
            if let Some(number) = chosen {
                assert!(
                    view[actor]["field"]
                        .as_array()
                        .unwrap()
                        .contains(&json!(number))
                );
            }
        }
    }
}
