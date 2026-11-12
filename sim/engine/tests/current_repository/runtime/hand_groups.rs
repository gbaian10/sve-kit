use super::*;
use alloc::sync::Arc;
use sve_engine::catalog::EngineIdentityInput;
use sve_engine::game::internal::InstructionOutcome;

fn payment_catalog() -> Arc<Catalog> {
    static CATALOG: OnceLock<Arc<Catalog>> = OnceLock::new();
    let catalog = CATALOG.get_or_init(|| {
        let (snapshot, keywords, _) = inputs();
        let mut cards = serde_json::Map::new();
        for number in ["ECP02-039", "ECP02-010", "ECP02-001", "ECP02-008"] {
            cards.insert(
                number.into(),
                json!({"status":"complete","review":"payment fixture only","abilities":[]}),
            );
        }
        let document = json!({"format":1_u8,"kind":"effect_set","cards":cards}).to_string();
        let rules = rule_fixtures::rules(
            snapshot,
            json!([rule_fixtures::title(
                "idolmaster_cinderella_girls",
                "ECP02-001",
                json!([])
            )]),
            json!([]),
        );
        Arc::new(
            Catalog::from_documents_with_rules(
                snapshot,
                keywords,
                &[("payment-fixture.yaml".into(), document)],
                &rules.to_string(),
                &EngineIdentityInput::LegacyJp,
            )
            .unwrap(),
        )
    });
    Arc::clone(catalog)
}

#[test]
fn ecp02_039_payment_uses_three_distinct_real_trait_cards_before_acting() {
    let mut tree = expanded::tree().clone();
    let mut entry = tree["cards"]["BP08-038"]["abilities"][0].clone();
    entry.as_object_mut().unwrap().remove("fanfare");
    entry["activate"] = json!({"cost":{"steps":[{"act_self":true},{"discard":{"groups":[
        {"sel":{"who":"self","zone":"hand","trait":"キュート"},"n":1_u8},
        {"sel":{"who":"self","zone":"hand","trait":"クール"},"n":1_u8},
        {"sel":{"who":"self","zone":"hand","trait":"パッション"},"n":1_u8}
    ]}}]},"do":[{"draw":{"n":0_u8}}]});
    let meta = tree["cards"]["BP08-038"]["meta"].clone();
    tree["cards"] = json!({"ECP02-039":{"meta":meta,"abilities":[entry]}});
    let mut fragment = lowerer()
        .lower(&tree)
        .unwrap()
        .remove("ECP02-039")
        .unwrap()
        .card()
        .clone();
    // Only the printed payment is under test; no complete card definition is admitted here.
    fragment.abilities[0]
        .node
        .body
        .as_mut()
        .unwrap()
        .steps
        .clear();
    let fragment = WellFormedCardProgram::check(fragment).unwrap();
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"payer","card":"ECP02-039"}]);
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"cute","card":"ECP02-010"},{"id":"cool","card":"ECP02-001"},{"id":"passion","card":"ECP02-008"}
    ]);
    let mut game = Game::new(
        payment_catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "ecp02-payment",
    )
    .unwrap();
    game.load_internal_card("ECP02-039", &fragment).unwrap();
    let empty = WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![],
    })
    .unwrap();
    for number in ["ECP02-010", "ECP02-001", "ECP02-008"] {
        game.load_internal_card(number, &empty).unwrap();
    }
    let CardPlay::Awaiting(question) = game
        .activate_internal_card("payer", AbilityId(1), PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("payment materials");
    };
    assert!(game.internal_decision(View::P2).is_none());
    let before = game.projection(View::Referee).unwrap();
    assert_eq!(before["objects"]["payer"]["acted"], false);
    assert_eq!(before["P1"]["cemetery"], json!([]));
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::CostMaterials(vec![0, 0, 0]),
    )
    .unwrap_err();
    assert_eq!(game.projection(View::Referee).unwrap(), before);
    let result = resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::CostMaterials(vec![0, 1, 2]),
        )
        .unwrap(),
    );
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["objects"]["payer"]["acted"], true);
    assert_eq!(after["P1"]["cemetery"], json!(["cute", "cool", "passion"]));
    assert_eq!(after["P1"]["hand"], json!([]));
    assert_eq!(
        result
            .progress
            .receipts
            .values()
            .filter_map(
                |receipt| if let InstructionOutcome::Discard(outcome) = receipt {
                    Some(outcome.moves.len())
                } else {
                    None
                }
            )
            .collect::<Vec<_>>(),
        [3]
    );
}
