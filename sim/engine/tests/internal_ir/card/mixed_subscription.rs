#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate mixed-family subscription scenarios."
)]
use super::*;
use sve_engine::game::internal::{InputDomain, ResumeStage};
fn finish(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}
fn resolve_all(game: &mut Game) {
    finish(game);
    while let Some(question) = game.internal_pending_choice().unwrap() {
        let InputDomain::Pending(options) = &question.domain else {
            panic!("mixed-family pending");
        };
        resolved(
            game.answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap(),
        );
        finish(game);
    }
}
fn subscription_position(base: &WellFormedCardProgram, observer: &WellFormedCardProgram) -> Game {
    let (snapshot, registry, document) = catalog_documents(&json!("1"));
    let form = json!({"number":"unit-form","faces":[{"name":"unit-follower",
        "card_class":"ニュートラル","card_type":"フォロワー・エボルヴ","cost":"-",
        "power":"4","hp":"5","traits":[],"text":null,"sections":[]}]});
    let snapshot = format!("{snapshot}\n{form}");
    let mut document: Value = serde_json::from_str(&document).unwrap();
    document["cards"]["unit-form"] = json!({"status":"complete","abilities":[]});
    let catalog = Arc::new(
        Catalog::from_documents(
            &snapshot,
            &registry,
            &[("mixed.yaml".into(), document.to_string())],
        )
        .unwrap(),
    );
    let mut initial = setup(1, json!([]));
    initial["turn"]["elapsed_turns"]["P1"] = json!(7);
    initial["players"]["P1"]["pp"] = json!({"current":10,"max":10});
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"observer","card":"unit-amulet"}]);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"incoming","card":"unit-follower"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"e","card":"unit-form"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([]);
    initial["players"]["P2"]["zones"]["hand"] = json!([]);
    let mut game = Game::new(
        catalog,
        &initial,
        &Value::Null,
        &Value::Null,
        "mixed-subscription",
    )
    .unwrap();
    game.load_internal_card("unit-follower", base).unwrap();
    game.load_internal_card("unit-form", &card(vec![])).unwrap();
    game.load_internal_card("unit-amulet", observer).unwrap();
    game
}

#[test]
fn mixed_subscription_routes_both_families_with_one_source_and_one_shared_quota() {
    for (limited, duplicate) in [(false, false), (true, false), (false, true)] {
        let mut head = json!({"auto":{"on":{"any_of":[
            {"enter_field":{"who":"self","sel":{"kind":"follower"}}},
            {"evolved":{"who":"self","sel":{"kind":"follower"}}}
        ]},"do":[{"life":{"who":"self","delta":1}}]}});
        if limited {
            head["auto"]["per_turn"] = json!(1);
        }
        if duplicate {
            let repeated = head["auto"]["on"]["any_of"][0].clone();
            head["auto"]["on"]["any_of"]
                .as_array_mut()
                .unwrap()
                .push(repeated);
        }
        let observer = lowerer()
            .lower(&equipment_played::tree(&head, "unit-amulet"))
            .unwrap()
            .remove("unit-amulet")
            .unwrap();
        let base = lowerer()
            .lower(&equipment_played::tree(
                &json!({"evolve":{"cost":{"pp":0}}}),
                "unit-follower",
            ))
            .unwrap()
            .remove("unit-follower")
            .unwrap();
        let mut game = subscription_position(&base, &observer);
        assert!(matches!(
            game.play_internal_card("incoming", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ));
        resolve_all(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
            21
        );
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"evolve","source":"incoming","evolve_card":"e","pay":{"pp":0}})
            )
            .unwrap()
            .outcome,
            "resolved"
        );
        resolve_all(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
            if limited { 21 } else { 22 },
            "two alternative event families share the original ability instance and quota"
        );
    }
}
