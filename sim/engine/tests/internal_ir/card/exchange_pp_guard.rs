use super::exchange::{candidate, initial, lower};
use super::*;
use sve_engine::game::internal::{Answer, InputDomain};

fn observer(resource: &str, installed: bool) -> WellFormedCardProgram {
    let static_head = json!({"static":{
        "while":{"cmp":{"left":{"resource":{"who":"self","kind":resource}},"op":"ge","right":3}},
        "effects":[{"grant":{"keywords":["ward"]}}]
    }});
    let head = if installed {
        json!({"activate":{"cost":{"pp":0},"do":[{"grant":{"on":"this","abilities":[static_head]}}]}})
    } else {
        static_head
    };
    let mut tree = same_name::tree(&head, "unit-follower");
    tree["cards"]["unit-follower"]["abilities"][0]["line"] = json!(1);
    let mut entry = tree["cards"]["unit-follower"]["abilities"][0].clone();
    entry.as_object_mut().unwrap().remove("static");
    entry.as_object_mut().unwrap().remove("activate");
    entry["line"] = json!(2);
    entry["auto"] = json!({
        "on":{"moved":{"from":{"who":"self","zone":"field"},"to":"cemetery","sel":{"kind":"follower","keyword":"ward"}}},
        "do":[{"if":{"cond":{"cmp":{"left":{"resource":{"who":"self","kind":"pp"}},"op":"le","right":2}},"then":[{"life":{"who":"self","delta":1}}]}}]
    });
    tree["cards"]["unit-follower"]["abilities"]
        .as_array_mut()
        .unwrap()
        .push(entry);
    lowerer()
        .lower(&tree)
        .unwrap()
        .remove("unit-follower")
        .unwrap()
}

fn position(resource: &str, installed: bool) -> Game {
    let spell = lower(
        &candidate(
            "unit-spell",
            json!({"optional_cost":{"to_cemetery":{"sel":{"who":"self","zone":"field"},"n":1},"result":"exchange"},"optional_cost_delta":-2}),
            true,
        ),
        4,
    );
    let (snapshot, _, document) = catalog_documents(&json!("4"));
    let catalog = Arc::new(
        Catalog::from_documents(
            &snapshot,
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../authored/rules/keywords.yaml"
            )),
            &[("unit.yaml".into(), document)],
        )
        .unwrap(),
    );
    let mut input = initial("unit-spell", "field", 1, 4);
    input["players"]["P1"]["leader"]["life"] = json!(10);
    let mut game = Game::new(
        catalog,
        &input,
        &Value::Null,
        &Value::Null,
        "exchange-pp-boundary",
    )
    .unwrap();
    game.load_internal_card("unit-spell", &spell).unwrap();
    let observer = observer(resource, installed);
    let ability = observer.card().abilities[0].node.id;
    game.load_internal_card("unit-follower", &observer).unwrap();
    if installed {
        resolved(
            game.activate_internal_card("m0", ability, PlayerId { seat: 1 })
                .unwrap(),
        );
        game.resume_internal_card().unwrap();
    }
    game
}

#[test]
fn exchange_rejects_live_pp_dependent_ward_before_payment_for_printed_and_installed_declarations() {
    for installed in [false, true] {
        let mut game = position("pp", installed);
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["m0"]["keywords"],
            json!(["守護"])
        );
        let origin = game.clone();
        let error = game
            .play_internal_card("s", PlayerId { seat: 1 })
            .unwrap_err();
        assert!(
            matches!(error,sve_engine::EngineFailure::Unsupported(reason) if reason.contains("PP-dependent"))
        );
        transactions::assert_restored(&game, &origin);
    }
}

#[test]
fn exchange_allows_pp_max_dependent_ward_and_reads_current_pp_only_when_the_pending_body_resolves()
{
    let mut game = position("pp_max", false);
    let CardPlay::Awaiting(accept) = game.play_internal_card("s", PlayerId { seat: 1 }).unwrap()
    else {
        panic!("exchange acceptance")
    };
    let CardPlay::Awaiting(materials) = game
        .answer_internal_card(accept.actor, accept.token, Answer::OptionalCost(true))
        .unwrap()
    else {
        panic!("material choice")
    };
    resolved(
        game.answer_internal_card(
            materials.actor,
            materials.token,
            Answer::CostMaterials(vec![0]),
        )
        .unwrap(),
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        10
    );
    game.resume_internal_card().unwrap();
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("the committed Ward departure")
    };
    assert_eq!(options.len(), 1);
    resolved(
        game.answer_internal_card(
            pending.actor,
            pending.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap(),
    );
    game.resume_internal_card().unwrap();
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["P1"]["pp"]["current"], 2);
    assert_eq!(after["P1"]["leader"]["life"], 11);
    assert!(game.internal_pending_choice().unwrap().is_none());
}
