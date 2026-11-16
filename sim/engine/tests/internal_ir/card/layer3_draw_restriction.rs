use super::*;

fn input() -> Value {
    evolution_linked_movement::tree(&[
        (
            "static",
            json!({"effects":[{"cannot":{"what":"draw","who":"opp","during":{"not_phase":"start"}}}]}),
        ),
        (
            "activate",
            json!({"cost":{"pp":0},"do":[{"apply":{"on":"this","effect":{"lose_abilities":{}},"until":"turn"}}]}),
        ),
    ])
}

fn position(program: &WellFormedCardProgram, phase: &str) -> Game {
    let mut initial = setup(1, json!([{"id":"myd","card":"unit-spell"}]));
    initial["turn"]["phase"] = json!(phase);
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"other","card":"unit-amulet"}]);
    initial["players"]["P2"]["zones"]["deck"] = json!([{"id":"d","card":"unit-spell"}]);
    let mut game = evolution_linked_movement::loaded(&initial, program);
    game.load_internal_card("unit-spell", &card(vec![]))
        .unwrap();
    game.load_internal_card("unit-amulet", &card(vec![]))
        .unwrap();
    game
}

fn draw_as(game: &mut Game, seat: u32) -> Progress {
    let mut draw_context = context(seat);
    if seat == 2 {
        draw_context.source = "other".into();
    }
    game.execute_internal(
        &checked(vec![draw(1, 1)]),
        &source("synthetic"),
        draw_context,
    )
    .unwrap()
}

#[test]
fn the_restriction_uses_its_affected_player_and_allows_draws_during_start() {
    for who in ["self", "opp"] {
        let mut value = input();
        value["cards"]["unit-follower"]["abilities"][0]["static"]["effects"][0]["cannot"]["who"] =
            json!(who);
        let program = lowerer()
            .lower(&value)
            .unwrap()
            .remove("unit-follower")
            .unwrap();
        for phase in ["start", "main", "end"] {
            for seat in [1, 2] {
                let blocked = phase != "start" && (seat == 1) == (who == "self");
                let expected = u64::from(!blocked);
                let mut game = position(&program, phase);
                let progress = draw_as(&mut game, seat);
                assert!(
                    matches!(progress.receipts[&ReceiptId(1)],InstructionOutcome::Cards(CardQuantityOutcome{actual:Nat(n),..}) if n==expected)
                );
                let player = if seat == 1 { "P1" } else { "P2" };
                assert_eq!(
                    game.projection(View::Referee).unwrap()[player]["hand"]
                        .as_array()
                        .unwrap()
                        .len(),
                    usize::try_from(expected).unwrap()
                );
            }
        }
    }
}

#[test]
fn the_draw_prohibition_ends_when_the_provider_leaves_or_loses_its_ability() {
    let program = lowerer()
        .lower(&input())
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    for leave in [true, false] {
        let mut game = position(&program, "main");
        assert!(matches!(
            draw_as(&mut game, 2).receipts[&ReceiptId(1)],
            InstructionOutcome::Cards(CardQuantityOutcome { actual: Nat(0), .. })
        ));
        if leave {
            game.execute_internal(
                &checked(vec![instruction(1, payment::self_to_cemetery())]),
                &source("synthetic"),
                context(1),
            )
            .unwrap();
        } else {
            resolved(
                game.activate_internal_card(
                    "s",
                    program.card().abilities[1].node.id,
                    PlayerId { seat: 1 },
                )
                .unwrap(),
            );
            while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
                game.resume_internal_card().unwrap();
            }
        }
        assert!(matches!(
            draw_as(&mut game, 2).receipts[&ReceiptId(1)],
            InstructionOutcome::Cards(CardQuantityOutcome { actual: Nat(1), .. })
        ));
    }
}

#[test]
fn draw_permissions_keep_noncanonical_scopes_conditions_and_installed_forms_closed() {
    for (key, patch) in [
        ("who", json!("all")),
        ("during", json!({"not_phase":"main"})),
        ("during", Value::Null),
    ] {
        let mut value = input();
        value["cards"]["unit-follower"]["abilities"][0]["static"]["effects"][0]["cannot"][key] =
            patch;
        lowerer().lower(&value).unwrap_err();
    }
    let mut value = input();
    value["cards"]["unit-follower"]["abilities"][0]["static"]["zone"] = json!("ex");
    lowerer().lower(&value).unwrap_err();
    let mut raw = lowerer()
        .lower(&input())
        .unwrap()
        .remove("unit-follower")
        .unwrap()
        .card()
        .clone();
    raw.abilities[0].node.active_zones.push(ZoneKind::Ex);
    WellFormedCardProgram::check(raw).unwrap_err();
    let installed = evolution_linked_movement::tree(&[(
        "activate",
        json!({"cost":{"pp":0},"do":[
            {"player_effect":{"who":"opp","until":"turn","effect":{"cannot":{"what":"draw","who":"opp","during":{"not_phase":"start"}}}}}
        ]}),
    )]);
    lowerer().lower(&installed).unwrap_err();
}
