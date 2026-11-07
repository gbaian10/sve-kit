use super::*;
use sve_engine::game::internal::DecisionView;

#[expect(
    clippy::panic,
    reason = "The printed play exposes a specific decision at each stage."
)]
fn question(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(question) = play else {
        panic!("printed exchange decision")
    };
    question
}

fn checks(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}

#[expect(
    clippy::panic,
    reason = "Pending card selection is part of the printed scenario protocol."
)]
fn pending(game: &mut Game, printing: &str) -> CardPlay {
    checks(game);
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("one printed pending ability")
    };
    assert_eq!(
        options.len(),
        1,
        "spent materials cannot leave an extra later-play subscription pending"
    );
    let handle = options
        .iter()
        .find(|option| option.card == printing)
        .unwrap()
        .handle;
    game.answer_internal_card(question.actor, question.token, Answer::Pending(handle))
        .unwrap()
}

#[expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "The three printed bodies only select targets, order cards or enter with Ward."
)]
fn finish(game: &mut Game, mut play: CardPlay) {
    while let CardPlay::Awaiting(question) = play {
        let answer = match &question.domain {
            InputDomain::Targets(domain) => Answer::Targets(
                domain
                    .members
                    .iter()
                    .take(1)
                    .map(|member| member.handle)
                    .collect(),
            ),
            InputDomain::Order(cards) => {
                Answer::Order(cards.iter().rev().map(|card| card.handle).collect())
            }
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            other => panic!("unexpected printed body decision {other:?}"),
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    resolved(play);
    checks(game);
}

fn start(game: &mut Game) -> DecisionView {
    let accept = question(
        game.play_internal_card("source", PlayerId { seat: 1 })
            .unwrap(),
    );
    assert!(
        matches!(accept.domain, InputDomain::OptionalCost),
        "exchange acceptance precedes all payment"
    );
    question(
        game.answer_internal_card(accept.actor, accept.token, Answer::OptionalCost(true))
            .unwrap(),
    )
}

#[expect(
    clippy::panic,
    reason = "The exact printed material count is checked before answering."
)]
fn pay(game: &mut Game, question: &DecisionView, count: usize) -> CardPlay {
    let InputDomain::CostMaterials(domain) = &question.domain else {
        panic!("exchange materials")
    };
    assert_eq!(
        domain.members.len(),
        count,
        "only the printed material filter supplies payment"
    );
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::CostMaterials(domain.members.iter().map(|member| member.handle).collect()),
    )
    .unwrap()
}

#[test]
fn geno_all_three_printings_pay_a_triggered_levion_before_full_field_entry_and_fanfare() {
    for printing in ["BP13-028", "PR-354", "SD07-015"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["pp"] = json!({"current":1_u8,"max":1_u8});
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":printing}]);
        initial["players"]["P1"]["zones"]["field"] = json!([
            {"filler":3_u8},{"id":"material","card":"BP13-032"},{"id":"wrong-trait","card":"BP05-046"}
        ]);
        initial["players"]["P2"]["zones"]["field"] =
            json!([{"id":"enemy","card":"BP01-171","state":{"hp":10_u8}}]);
        let mut game = game(&initial);
        let materials = start(&mut game);
        assert!(
            game.internal_pending_choice().is_err(),
            "payment does not expose a check timing"
        );
        assert!(matches!(
            pay(&mut game, &materials, 1),
            CardPlay::Entered(_)
        ));
        let after_payment = game.projection(View::Referee).unwrap();
        assert_eq!(after_payment["P1"]["pp"]["current"], 0_u8);
        assert_eq!(after_payment["P1"]["cemetery"], json!(["material"]));
        assert_eq!(projected_zone_count(&after_payment["P1"]["field"]), 5);
        let fanfare = pending(&mut game, printing);
        finish(&mut game, fanfare);
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["enemy"]["hp"],
            6_u8
        );
        assert!(game.internal_pending_choice().unwrap().is_none());
    }
}

fn limonium_position(printing: &str, full: bool) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["pp"] = json!({"current":0_u8,"max":0_u8});
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"source","card":printing},{"id":"fuel","card":"BP02-089"}
    ]);
    initial["players"]["P1"]["zones"]["cemetery"] = json!([
        {"id":"revived","card":"BP07-086"},{"id":"wrong-trait","card":"BP05-046"}
    ]);
    if full {
        initial["players"]["P1"]["zones"]["field"] = json!([{"filler":5_u8}]);
    }
    initial
}

#[test]
fn limonium_both_printings_choose_before_discard_and_create_repair_even_when_revival_has_no_space()
{
    for printing in ["BP12-096", "BP12-P18"] {
        for full in [false, true] {
            let mut game = game(&limonium_position(printing, full));
            let targets = start(&mut game);
            let InputDomain::Targets(domain) = &targets.domain else {
                panic!("formal cemetery target before hand payment")
            };
            assert_eq!(domain.members.len(), 1);
            assert_eq!(
                game.projection(View::Referee).unwrap()["P1"]["hand"],
                json!(["fuel"])
            );
            let materials = question(
                game.answer_internal_card(
                    targets.actor,
                    targets.token,
                    Answer::Targets(vec![domain.members[0].handle]),
                )
                .unwrap(),
            );
            let body = pay(&mut game, &materials, 1);
            finish(&mut game, body);
            let after = game.projection(View::Referee).unwrap();
            assert_eq!(after["P1"]["pp"]["current"], 0_u8);
            assert_eq!(projected_zone_count(&after["P1"]["ex"]), 1);
            assert!(
                after["P1"][if full { "cemetery" } else { "field" }]
                    .as_array()
                    .unwrap()
                    .contains(&json!("revived"))
            );
            if full {
                assert!(
                    game.internal_pending_choice().unwrap().is_none(),
                    "Q1701 still creates the spell's Repair without entering Limonium"
                );
            } else {
                let fanfare = pending(&mut game, "BP07-086");
                finish(&mut game, fanfare);
                assert_eq!(
                    projected_zone_count(&game.projection(View::Referee).unwrap()["P1"]["ex"]),
                    2
                );
            }
        }
    }
}

fn raio_position(printing: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    initial["players"]["P1"]["pp"] = json!({"current":0_u8,"max":0_u8});
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"source","card":printing},{"id":"fuel-a","card":"BP05-035"},{"id":"fuel-b","card":"BP05-035"}
    ]);
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"material-a","card":"BP05-046"},{"id":"material-b","card":"BP05-046"},{"id":"material-c","card":"BP05-046"}
    ]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"look-a","card":"BP01-171"},{"id":"look-b","card":"BP01-171"},{"id":"look-c","card":"BP01-171"}
    ]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-171","state":{"hp":9_u8}}]);
    initial
}

#[test]
fn raio_both_printings_remove_play_observers_before_playing_then_resolve_look_and_filtered_discard()
{
    for printing in ["BP15-038", "BP15-SL10"] {
        let mut game = game(&raio_position(printing));
        let exchange_materials = start(&mut game);
        assert!(matches!(
            pay(&mut game, &exchange_materials, 3),
            CardPlay::Entered(_)
        ));
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
            10_u8,
            "Q2052 removed believers cannot observe the later CardPlayed"
        );
        let fanfare = pending(&mut game, printing);
        finish(&mut game, fanfare);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(projected_zone_count(&state["P1"]["ex"]), 1);
        assert_eq!(projected_zone_count(&state["P1"]["deck"]), 2);
        assert_eq!(state["P1"]["leader"]["life"], 10_u8);
        let ability = selection().current[printing]
            .program
            .card()
            .abilities
            .iter()
            .find(|ability| ability.node.kind == AbilityKind::Activated)
            .unwrap()
            .node
            .id;
        let targets = question(
            game.activate_internal_card("source", ability, PlayerId { seat: 1 })
                .unwrap(),
        );
        assert!(matches!(targets.domain, InputDomain::Targets(_)));
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["hand"],
            json!(["fuel-a", "fuel-b"])
        );
        let hand_materials = question(
            game.answer_internal_card(targets.actor, targets.token, Answer::Targets(vec![0]))
                .unwrap(),
        );
        let InputDomain::CostMaterials(domain) = &hand_materials.domain else {
            panic!("named original Raio hand cost")
        };
        assert_eq!(domain.members.len(), 2);
        let body = game
            .answer_internal_card(
                hand_materials.actor,
                hand_materials.token,
                Answer::CostMaterials(vec![domain.members[0].handle]),
            )
            .unwrap();
        finish(&mut game, body);
        let after = game.projection(View::Referee).unwrap();
        assert_eq!(after["P2"]["cemetery"], json!(["enemy"]));
        assert_eq!(projected_zone_count(&after["P1"]["ex"]), 2);
        let before_failure = game.projection(View::Referee).unwrap();
        assert!(matches!(
            game.activate_internal_card("source", ability, PlayerId { seat: 1 }),
            Err(EngineFailure::IllegalPlay(_))
        ));
        assert_eq!(
            game.projection(View::Referee).unwrap(),
            before_failure,
            "Q1998 no enemy target preserves the remaining hand cost and creates no token"
        );
    }
}
