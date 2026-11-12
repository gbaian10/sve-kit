//! BP07-004 uses the ordinary Field Put exchange, including tokens.
#![expect(
    clippy::panic,
    reason = "Unexpected exchange questions invalidate these scenarios."
)]
use super::*;
use sve_engine::game::internal::{DecisionView, InputDomain};

fn program() -> WellFormedCardProgram {
    let tree = exchange::candidate(
        "unit-follower",
        json!({
            "optional_cost":{"to_cemetery":{"sel":{"who":"self","zone":"field","trait":"自然"},"n":4}},
            "optional_cost_delta":-4
        }),
        false,
    );
    lowerer()
        .lower(&tree)
        .unwrap()
        .remove("unit-follower")
        .unwrap()
}

fn catalogue() -> Arc<Catalog> {
    let (snapshot, registry, document) = catalog_documents(&json!("4"));
    let mut rows = snapshot
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    for row in &mut rows {
        row["faces"][0]["traits"] = json!(["自然"]);
        row["faces"][0]["card_class"] = json!("ナイトメア");
    }
    let template = rows
        .iter()
        .find(|row| row["number"] == "unit-follower")
        .unwrap()
        .clone();
    let mut document: Value = serde_json::from_str(&document).unwrap();
    for (number, kind, cost, trait_name) in [
        ("follower-token", "フォロワー・トークン", "-", "自然"),
        ("amulet-token", "アミュレット・トークン", "0", "自然"),
        ("advance", "フォロワー・アドバンス", "4", "自然"),
        ("bad-token", "スペル・トークン", "4", "自然"),
        ("other", "フォロワー", "4", "反例"),
    ] {
        let mut row = template.clone();
        row["number"] = json!(number);
        row["faces"][0]["name"] = json!(number);
        row["faces"][0]["card_type"] = json!(kind);
        row["faces"][0]["cost"] = json!(cost);
        row["faces"][0]["traits"] = json!([trait_name]);
        if kind == "アミュレット・トークン" {
            row["faces"][0]["power"] = json!("-");
            row["faces"][0]["hp"] = json!("-");
        }
        rows.push(row);
        document["cards"][number] = document["cards"]["unit-follower"].clone();
    }
    Arc::new(
        Catalog::from_documents(
            &rows
                .iter()
                .map(Value::to_string)
                .collect::<Vec<_>>()
                .join("\n"),
            &registry,
            &[("synthetic.yaml".into(), document.to_string())],
        )
        .unwrap(),
    )
}

fn initial(seat: u32, pp: u64) -> Value {
    let actor = if seat == 1 { "P1" } else { "P2" };
    let other = if seat == 1 { "P2" } else { "P1" };
    let mut initial = setup(seat, json!([]));
    initial["turn"]["active"] = json!(actor);
    initial["players"][actor]["zones"]["hand"] = json!([{"id":"s","card":"unit-follower"}]);
    initial["players"][actor]["zones"]["field"] = json!([
        {"id":"m0","card":"unit-follower","state":{"acted":true}},
        {"id":"m1","card":"unit-amulet"},
        {"id":"m2","card":"follower-token","state":{"acted":true}},
        {"id":"m3","card":"amulet-token"}
    ]);
    initial["players"][actor]["pp"] = json!({"current":pp,"max":pp});
    initial["players"][other]["zones"]["field"] = json!([{"id":"foreign","card":"follower-token"}]);
    initial
}

fn board(initial: &Value) -> Game {
    position(initial, &program())
}

fn position(initial: &Value, program: &WellFormedCardProgram) -> Game {
    let mut game = Game::new(
        catalogue(),
        initial,
        &Value::Null,
        &Value::Null,
        "field-token-exchange",
    )
    .unwrap();
    game.load_internal_card("unit-follower", program).unwrap();
    for number in [
        "unit-amulet",
        "unit-spell",
        "follower-token",
        "amulet-token",
        "bad-token",
        "advance",
        "other",
    ] {
        game.load_internal_card(number, &card(vec![])).unwrap();
    }
    game
}

fn awaiting(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(question) = play else {
        panic!("exchange question")
    };
    question
}

fn start(game: &mut Game, seat: u32) -> DecisionView {
    awaiting(game.play_internal_card("s", PlayerId { seat }).unwrap())
}

fn materials(game: &mut Game, accept: &DecisionView) -> DecisionView {
    awaiting(
        game.answer_internal_card(accept.actor, accept.token, Answer::OptionalCost(true))
            .unwrap(),
    )
}

#[test]
fn bp07_004_single_field_route_pays_four_nature_cards_including_both_token_types_both_seats() {
    for seat in [1, 2] {
        let actor = if seat == 1 { "P1" } else { "P2" };
        let opponent = if seat == 1 { "P2" } else { "P1" };
        let mut game = board(&initial(seat, 0));
        let accept = start(&mut game, seat);
        let choice = materials(&mut game, &accept);
        let InputDomain::CostMaterials(domain) = &choice.domain else {
            panic!("material domain")
        };
        assert_eq!(domain.members.len(), 4);
        let result = game
            .answer_internal_card(
                choice.actor,
                choice.token,
                Answer::CostMaterials(vec![0, 1, 2, 3]),
            )
            .unwrap();
        let CardPlay::Entered(entry) = result else {
            panic!("paid follower enters")
        };
        assert!(
            entry
                .progress
                .receipts
                .values()
                .any(|receipt| matches!(receipt,
            InstructionOutcome::ExchangePayment(payment) if payment.complete == Bool(true)))
        );
        assert!(entry.progress.receipts.values().any(|receipt| matches!(receipt,
            InstructionOutcome::Move(outcome) if outcome.moved.len() == 4 && outcome.moved.iter().all(|movement| movement.after.is_some()))));
        assert_eq!(entry.progress.facts.iter().filter(|fact| matches!(fact,
            Fact::Movement(movement) if movement.method == MoveMethod::Put && movement.from.kind == ZoneKind::Field)).count(), 4);
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view[actor]["field"], json!(["s"]));
        assert_eq!(view[actor]["cemetery"], json!(["m0", "m1"]));
        assert_eq!(view[actor]["pp"]["current"], 0);
        assert!(view["objects"].get("m2").is_none());
        assert!(view["objects"].get("m3").is_none());
        assert_eq!(view[opponent]["field"], json!(["foreign"]));
    }
}

#[test]
fn invalid_counts_duplicate_and_foreign_material_handles_preserve_the_pending_exchange() {
    let mut game = board(&initial(1, 0));
    let accept = start(&mut game, 1);
    let choice = materials(&mut game, &accept);
    let before = exact_authority(&game);
    for handles in [
        vec![],
        vec![0, 1, 2],
        vec![0, 1, 2, 3, 4],
        vec![0, 1, 2, 2],
        vec![0, 1, 2, 99],
    ] {
        assert!(matches!(
            game.answer_internal_card(choice.actor, choice.token, Answer::CostMaterials(handles)),
            Err(sve_engine::EngineFailure::Invalid(_))
        ));
        assert_eq!(exact_authority(&game), before);
    }
    assert!(matches!(
        game.answer_internal_card(
            choice.actor,
            choice.token,
            Answer::CostMaterials(vec![0, 1, 2, 3])
        )
        .unwrap(),
        CardPlay::Entered(_)
    ));
}

#[test]
fn declining_an_unaffordable_exchange_restores_the_entire_play_without_material_events() {
    let mut game = board(&initial(1, 0));
    let origin = game.clone();
    let accept = start(&mut game, 1);
    let published = game.clone();
    assert!(matches!(
        game.answer_internal_card(accept.actor, accept.token, Answer::OptionalCost(false)),
        Err(sve_engine::EngineFailure::IllegalPlay(_))
    ));
    transactions::assert_reverted_from(&game, &origin, &published);
    let before = exact_authority(&game);
    assert!(matches!(
        game.answer_internal_card(accept.actor, accept.token, Answer::OptionalCost(true)),
        Err(sve_engine::EngineFailure::Invalid(_))
    ));
    assert_eq!(exact_authority(&game), before);
}

fn attempt_payment(
    game: &mut Game,
    published: &mut Game,
) -> Result<CardPlay, sve_engine::EngineFailure> {
    let accept = awaiting(game.play_internal_card("s", PlayerId { seat: 1 })?);
    *published = game.clone();
    let choice = awaiting(game.answer_internal_card(
        accept.actor,
        accept.token,
        Answer::OptionalCost(true),
    )?);
    *published = game.clone();
    game.answer_internal_card(
        choice.actor,
        choice.token,
        Answer::CostMaterials(vec![0, 1, 2, 3]),
    )
}

#[test]
fn foreign_owned_tokens_advance_and_other_token_types_keep_the_entire_play_unchanged() {
    for variant in ["owner", "advance", "bad-token"] {
        let mut initial = initial(1, 0);
        match variant {
            "owner" => initial["players"]["P1"]["zones"]["field"][3]["owner"] = json!("P2"),
            _ => initial["players"]["P1"]["zones"]["field"][2]["card"] = json!(variant),
        }
        let mut game = board(&initial);
        let origin = game.clone();
        let mut published = origin.clone();
        assert!(
            matches!(
                attempt_payment(&mut game, &mut published),
                Err(sve_engine::EngineFailure::Unsupported(_))
            ),
            "{variant}"
        );
        transactions::assert_reverted_from(&game, &origin, &published);
    }
}

#[test]
fn non_nature_cards_do_not_fill_the_exact_four_material_requirement() {
    let mut initial = initial(1, 0);
    initial["players"]["P1"]["zones"]["field"][3]["card"] = json!("other");
    let mut game = board(&initial);
    let origin = game.clone();
    let mut published = origin.clone();
    assert!(matches!(
        attempt_payment(&mut game, &mut published),
        Err(sve_engine::EngineFailure::IllegalPlay(_))
    ));
    transactions::assert_reverted_from(&game, &origin, &published);
}

#[test]
fn raw_exchange_cannot_redirect_the_field_put_or_rebind_a_foreign_material() {
    for variant in ["ex", "hand", "bind"] {
        let mut raw = program().card().clone();
        let exchange = &mut raw.card_play_costs[0].node.exchange.as_mut().unwrap().node;
        let CostSpec::Ordered { steps } = &mut exchange.cost else {
            panic!("one route")
        };
        let Action::Move {
            objects, movement, ..
        } = &mut steps[0].instruction.action
        else {
            panic!("Put route")
        };
        match variant {
            "ex" => {
                movement.to = MoveDestination::Owners {
                    zone: OwnerZone::Ex,
                }
            }
            "hand" => {
                movement.to = MoveDestination::Owners {
                    zone: OwnerZone::Hand,
                }
            }
            _ => {
                *objects = Selector::Objects {
                    objects: vec![ObjectOperand::SourceHolder {}],
                }
            }
        }
        let error = WellFormedCardProgram::check(raw).unwrap_err();
        assert_eq!(
            error.reason,
            WellFormedReason::UnsupportedShape,
            "{variant}: {error:?}"
        );
    }
}

#[test]
fn the_existing_two_route_field_put_still_pays_tokens_and_keeps_its_standing_filter() {
    let tree = exchange::candidate(
        "unit-follower",
        json!({
            "optional_cost":{"one_of":[
                {"to_cemetery":{"sel":{"who":"self","zone":"field","class":"ナイトメア","acted":false},"n":4}},
                {"banish":{"sel":{"who":"self","zone":"ex","class":"ナイトメア"},"n":4}}
            ]},"optional_cost_delta":-4
        }),
        false,
    );
    let program = lowerer()
        .lower(&tree)
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    let mut initial = initial(1, 0);
    for material in initial["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
    {
        material["state"]["acted"] = json!(false);
    }
    let mut game = position(&initial, &program);
    let accept = start(&mut game, 1);
    let route = materials(&mut game, &accept);
    assert!(matches!(route.domain, InputDomain::Integer(_)));
    let choice = awaiting(
        game.answer_internal_card(route.actor, route.token, Answer::Integer(Nat(0)))
            .unwrap(),
    );
    assert!(matches!(
        game.answer_internal_card(
            choice.actor,
            choice.token,
            Answer::CostMaterials(vec![0, 1, 2, 3])
        )
        .unwrap(),
        CardPlay::Entered(_)
    ));
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["field"], json!(["s"]));
    assert_eq!(view["P1"]["cemetery"], json!(["m0", "m1"]));

    initial["players"]["P1"]["zones"]["field"][2]["state"]["acted"] = json!(true);
    let mut game = position(&initial, &program);
    let origin = game.clone();
    let accept = start(&mut game, 1);
    let published = game.clone();
    assert!(matches!(
        game.answer_internal_card(accept.actor, accept.token, Answer::OptionalCost(true)),
        Err(sve_engine::EngineFailure::IllegalPlay(_))
    ));
    transactions::assert_reverted_from(&game, &origin, &published);
}

#[test]
fn a_spell_exchange_does_not_gain_field_token_movement_authority() {
    let mut tree = exchange::candidate(
        "unit-spell",
        json!({
            "optional_cost":{"to_cemetery":{"sel":{"who":"self","zone":"field","trait":"自然"},"n":4}},
            "optional_cost_delta":-4
        }),
        true,
    );
    tree["cards"]["unit-spell"]["abilities"][1]["spell"]["do"] = json!([{"draw":{"n":0}}]);
    let program = exchange::lower(&tree, 4);
    let mut initial = initial(1, 0);
    initial["players"]["P1"]["zones"]["hand"][0]["card"] = json!("unit-spell");
    let mut game = Game::new(
        catalogue(),
        &initial,
        &Value::Null,
        &Value::Null,
        "spell-exchange-authority",
    )
    .unwrap();
    game.load_internal_card("unit-spell", &program).unwrap();
    for number in [
        "unit-follower",
        "unit-amulet",
        "follower-token",
        "amulet-token",
        "bad-token",
        "advance",
        "other",
    ] {
        game.load_internal_card(number, &card(vec![])).unwrap();
    }
    let origin = game.clone();
    assert!(matches!(
        game.play_internal_card("s", PlayerId { seat: 1 }),
        Err(sve_engine::EngineFailure::Unsupported(_))
    ));
    transactions::assert_restored(&game, &origin);
}

#[test]
fn opposing_field_tokens_cannot_fill_the_own_four_material_requirement() {
    let mut initial = initial(1, 0);
    let foreign = initial["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .pop()
        .unwrap();
    initial["players"]["P2"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(foreign);
    let mut game = board(&initial);
    let origin = game.clone();
    let mut published = origin.clone();
    assert!(matches!(
        attempt_payment(&mut game, &mut published),
        Err(sve_engine::EngineFailure::IllegalPlay(_))
    ));
    transactions::assert_reverted_from(&game, &origin, &published);
}
