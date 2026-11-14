use super::*;
use sve_engine::game::internal::{InputDomain, ResumeStage};

fn finish(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}

fn evolution_catalog(cost: u8, plant: bool) -> Arc<Catalog> {
    let (snapshot, registry, document) = catalog_documents(&json!(cost.to_string()));
    let mut rows = snapshot
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    for row in &mut rows {
        if row["number"] == "unit-follower" {
            row["faces"][0]["traits"] = if plant {
                json!(["植物族"])
            } else {
                json!([])
            };
        }
    }
    rows.push(json!({"number":"unit-form","faces":[{"name":"unit-follower","card_class":"ニュートラル","card_type":"フォロワー・エボルヴ","cost":"-","power":"4","hp":"5","traits":if plant {json!(["植物族"])} else {json!([])},"text":null,"sections":[]}]}));
    let snapshot = rows
        .into_iter()
        .map(|row| row.to_string())
        .collect::<Vec<_>>()
        .join("\n");
    let form_document = json!({"format":1,"kind":"effect_set","cards":{"unit-form":{"status":"complete","abilities":[]}}}).to_string();
    Arc::new(
        Catalog::from_documents(
            &snapshot,
            &registry,
            &[
                ("unit.yaml".into(), document),
                ("form.yaml".into(), form_document),
            ],
        )
        .unwrap(),
    )
}

#[test]
fn evolution_original_cost_intersects_traits_and_keeps_the_original_print() {
    let head = json!({"auto":{
        "on":{"evolved":{"who":"self","sel":{"kind":"follower","orig_cost_le":5,"trait":"植物族"}}},
        "do":[{"life":{"who":"self","delta":1}}]
    }});
    let observer = lowerer()
        .lower(&equipment_played::tree(&head, "unit-amulet"))
        .unwrap()
        .remove("unit-amulet")
        .unwrap();
    let form_observer = lowerer()
        .lower(&equipment_played::tree(&head, "unit-form"))
        .unwrap()
        .remove("unit-form")
        .unwrap();
    let base = lowerer()
        .lower(&equipment_played::tree(
            &json!({"evolve":{"cost":{"pp":0}}}),
            "unit-follower",
        ))
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    for (cost, plant, own, in_form, expected) in [
        (0_u8, true, true, false, true),
        (5, true, true, false, true),
        (5, true, true, true, true),
        (6, true, true, false, false),
        (5, false, true, false, false),
        (5, true, false, false, false),
    ] {
        let catalog = evolution_catalog(cost, plant);
        let actor = if own { "P1" } else { "P2" };
        let seat = if own { 1 } else { 2 };
        let mut initial = setup(seat, json!([]));
        for player in ["P1", "P2"] {
            initial["players"][player]["zones"]["field"] = json!([]);
            initial["players"][player]["zones"]["hand"] = json!([]);
        }
        initial["turn"]["active"] = json!(actor);
        initial["players"][actor]["zones"]["field"] = json!([{"id":"subject","card":"unit-follower","state":{"cost":if cost == 6 {1_u8} else {9_u8}}}]);
        initial["players"][actor]["zones"]["evolve_deck"] =
            json!([{"id":"form","card":"unit-form"}]);
        initial["players"]["P1"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"observer","card":"unit-amulet","owner":"P2"}));
        let mut game = Game::new(
            catalog,
            &initial,
            &Value::Null,
            &Value::Null,
            "evolution-original-cost",
        )
        .unwrap();
        let empty = card(vec![]);
        game.load_internal_card("unit-follower", &base).unwrap();
        game.load_internal_card("unit-form", if in_form { &form_observer } else { &empty })
            .unwrap();
        game.load_internal_card("unit-amulet", if in_form { &empty } else { &observer })
            .unwrap();
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"evolve","source":"subject","evolve_card":"form","pay":{"pp":0_u8}})
            )
            .unwrap()
            .outcome,
            "resolved",
            "the real procedure produces an evolution event"
        );
        finish(&mut game);
        let pending = game.internal_pending_choice().unwrap();
        assert_eq!(
            pending.is_some(),
            expected,
            "cost, trait and controller are intersected before admitting the observer"
        );
        if let Some(question) = pending {
            let InputDomain::Pending(options) = &question.domain else {
                panic!("evolution observer");
            };
            resolved(
                game.answer_internal_card(
                    question.actor,
                    question.token,
                    Answer::Pending(options[0].handle),
                )
                .unwrap(),
            );
            finish(&mut game);
            assert_eq!(
                game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
                21_u8,
                "the observer keeps its controller despite foreign ownership"
            );
        }
    }
}
