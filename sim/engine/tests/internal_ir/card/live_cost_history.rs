use super::*;
use sve_engine::game::internal::{InputDomain, ResumeStage};

fn finish(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}

#[test]
fn installed_cost_rechecks_current_history_after_the_original_provider_leaves() {
    let input = equipment_played::tree(
        &json!({"activate":{"cost":{"pp":0},
            "targets":[{"bind":"discounted","from":{"who":"self","zone":"ex","kind":"follower"},"n":1}],
            "do":[{"apply":{"on":"discounted","until":"turn","effect":{"cost_mod":{
                "when_playing":{"in":"affected"},"delta":{"minus":[0,{"history_count":{"event":"played","window":"this_turn","who":"self"}}]}
            }}}},{"move":{"ref":"this","to":"cemetery"}}]
        }}),
        "unit-amulet",
    );
    let provider = lowerer()
        .lower(&input)
        .unwrap()
        .remove("unit-amulet")
        .unwrap();
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["pp"] = json!({"current":5,"max":5});
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"s","card":"unit-amulet"}]);
    initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"target","card":"unit-follower"}]);
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"first","card":"unit-spell"},{"id":"second","card":"unit-spell"}
    ]);
    let (snapshot, registry, document) = catalog_documents(&json!("1"));
    let snapshot = snapshot
        .lines()
        .map(|line| {
            let mut row: Value = serde_json::from_str(line).unwrap();
            if row["number"] == "unit-follower" {
                row["faces"][0]["cost"] = json!("5");
            }
            row.to_string()
        })
        .collect::<Vec<_>>()
        .join("\n");
    let catalog = Arc::new(
        Catalog::from_documents(&snapshot, &registry, &[("unit.yaml".into(), document)]).unwrap(),
    );
    let mut game = Game::new(
        catalog,
        &initial,
        &Value::Null,
        &Value::Null,
        "live-cost-history",
    )
    .unwrap();
    game.load_internal_card("unit-amulet", &provider).unwrap();
    game.load_internal_card("unit-follower", &card(vec![]))
        .unwrap();
    game.load_internal_card("unit-spell", &card(vec![play::spell(vec![])]))
        .unwrap();
    let CardPlay::Awaiting(question) = game
        .activate_internal_card(
            "s",
            provider.card().abilities[0].node.id,
            PlayerId { seat: 1 },
        )
        .unwrap()
    else {
        panic!("EX target");
    };
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("target domain");
    };
    resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Targets(vec![domain.members[0].handle]),
        )
        .unwrap(),
    );
    finish(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["cemetery"],
        json!(["s"])
    );
    for id in ["first", "second"] {
        resolved(game.play_internal_card(id, PlayerId { seat: 1 }).unwrap());
        finish(&mut game);
    }
    assert!(matches!(
        game.play_internal_card("target", PlayerId { seat: 1 })
            .unwrap(),
        CardPlay::Entered(_)
    ));
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        0,
        "the target costs 5 minus the two plays after installation, not a captured zero or a self-inclusive three"
    );
}
