#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "The printed base, super evolution and activated choice have explicit decision domains."
)]
use super::event_cemetery::finish_checks;
use super::*;

fn settle(game: &mut Game) -> usize {
    let mut count = 0_usize;
    loop {
        finish_checks(game);
        let Some(pending) = game.internal_pending_choice().unwrap() else {
            break;
        };
        let InputDomain::Pending(options) = &pending.domain else {
            panic!("printed ability pending")
        };
        assert_eq!(
            options.len(),
            1,
            "Each printed occurrence resolves independently."
        );
        resolved(
            game.answer_internal_card(
                pending.actor,
                pending.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap(),
        );
        count = count.checked_add(1).unwrap();
    }
    count
}

fn activate(game: &mut Game, number: &str, mode: u64, super_evolution: bool) {
    let id = selection_with_rules().current[number]
        .program
        .card()
        .abilities
        .iter()
        .find(|entry| entry.node.kind == AbilityKind::Activated)
        .unwrap()
        .node
        .id;
    let mut step = game
        .activate_internal_card("source", id, PlayerId { seat: 1 })
        .unwrap();
    let mut selected = false;
    let mut paid = false;
    while let CardPlay::Awaiting(question) = step {
        let answer = match &question.domain {
            InputDomain::Modes {
                count, cardinality, ..
            } => {
                assert_eq!(*count, Nat(1), "The printed choice selects one mode.");
                assert_eq!(cardinality.minimum, Nat(1), "The printed choice is mandatory.");
                selected = true;
                Answer::Modes(vec![ModeId(mode)])
            }
            InputDomain::CostMaterials(domain) => {
                assert_eq!(domain.cardinality.minimum, Nat(2), "Two Gold tokens are required.");
                assert_eq!(domain.cardinality.maximum, Some(Nat(2)), "Only two Gold tokens are paid.");
                assert_eq!(domain.members.len(), if super_evolution { 3 } else { 2 }, "Every own Gold token is an eligible cost material.");
                paid = true;
                Answer::CostMaterials(domain.members.iter().take(2).map(|m| m.handle).collect())
            }
            InputDomain::Targets(domain) => {
                if mode == 1 {
                    let enemy = domain
                        .members
                        .iter()
                        .find(|m| m.card.as_deref() == Some("BP01-002"))
                        .unwrap();
                    Answer::Targets(vec![enemy.handle])
                } else {
                    assert!(
                        paid,
                        "Drawing and the private bottom choice follow the printed cost."
                    );
                    assert_eq!(
                        game.projection(View::Referee).unwrap()["P1"]["hand"]
                            .as_array()
                            .unwrap()
                            .len(),
                        2,
                        "The printed draw supplies two hand cards before the bottom choice."
                    );
                    assert!(
                        game.internal_decision(View::P2).is_none(),
                        "Only the controller sees the hand choice."
                    );
                    Answer::Targets(vec![domain.members[0].handle])
                }
            }
            _ => panic!("printed Almerize decisions"),
        };
        step = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    let CardPlay::Resolved(activation) = step else {
        panic!("The printed body resolves after the complete EX payment.")
    };
    assert_payment(&activation.progress.facts);
    assert!(
        selected && paid,
        "The mode and two-card EX payment must both complete."
    );
    assert_eq!(
        settle(game),
        0,
        "Banishment does not count as a new EX entry."
    );
}

fn assert_payment(facts: &[Fact]) {
    assert_eq!(
        facts.iter().filter(|fact| matches!(fact,
            Fact::Movement(movement) if movement.method == MoveMethod::Banish
                && movement.from.kind == ZoneKind::Ex
                && movement.movement.actual_to.as_ref().is_some_and(|zone| zone.kind == ZoneKind::Banish)
        )).count(),
        2,
        "Exactly two Gold tokens are banished as the printed cost."
    );
}

fn position(number: &str) -> Value {
    let mut initial = draw_position();
    initial["turn"]["elapsed_turns"]["P1"] = json!(7_u8);
    initial["players"]["P1"]["leader"]["class"] = json!("ロイヤル");
    initial["players"]["P1"]["sep"] = json!(1_u8);
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    initial["players"]["P1"]["zones"]["hand"] =
        json!([{"id":"source","card":"BP22-021"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] =
        json!([{"id":"form","card":number}]);
    initial["players"]["P1"]["zones"]["ex"] =
        json!([{"id":"printed-gold","card":"BP22-T07"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!(
        (0_u8..3)
            .map(|i| json!({"id":format!("d{i}"),"card":"BP01-043"}))
            .collect::<Vec<_>>()
    );
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"enemy-a","card":"BP01-002","state":{"hp":20_u8,"max_hp":20_u8}},
        {"id":"enemy-b","card":"BP01-043","state":{"hp":20_u8,"max_hp":20_u8}}
    ]);
    initial
}

fn evolved(number: &str, super_evolution: bool) -> Game {
    let initial = position(number);
    let mut game = Game::from_selected_effects(
        selection_with_rules().clone(),
        &initial,
        &Value::Null,
        &Value::Null,
        "r7b-almerize",
    )
    .unwrap();
    assert!(
        matches!(
            game.play_internal_card("source", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ),
        "The actual base card must enter."
    );
    assert_eq!(settle(&mut game), 1, "The actual base Fanfare resolves once.");
    let mut evolution =
        json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}});
    if super_evolution {
        evolution["pay"]["sep"] = json!(1_u8);
    }
    game.internal_rule_action(&evolution).unwrap();
    assert_eq!(
        settle(&mut game),
        if super_evolution { 2 } else { 0 },
        "Q2801: grant precedes the same ability’s Gold entry."
    );
    game
}

fn check_case(number: &str, super_evolution: bool, mode: u64) {
    let mut game = evolved(number, super_evolution);
    let before = game.projection(View::Referee).unwrap();
    assert_eq!(before["P1"]["leader"]["life"], 22_u8, "The base Fanfare heals two.");
    assert_eq!(
        before["P1"]["ex"].as_array().unwrap().len(),
        if super_evolution { 3 } else { 2 },
        "Only super evolution adds the third Gold token."
    );
    assert_eq!(
        before["P2"]["leader"]["life"],
        if super_evolution { 19_u8 } else { 20_u8 },
        "The newly granted ability deals one to the opponent leader."
    );
    assert_eq!(
        before["objects"]["enemy-a"]["damage"],
        u8::from(super_evolution),
        "The newly granted ability deals one to each enemy follower."
    );
    assert_eq!(
        before["objects"]["enemy-b"]["damage"],
        u8::from(super_evolution),
        "The newly granted ability deals one to each enemy follower."
    );
    activate(&mut game, number, mode, super_evolution);
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["P1"]["pp"]["current"], 7_u8, "The printed activation has no PP cost.");
    assert_eq!(
        after["P1"]["ex"].as_array().unwrap().len(),
        usize::from(super_evolution),
        "Exactly two Gold tokens leave EX as the cost."
    );
    assert_eq!(
        after["P1"]["banish"], json!([]),
        "Paid spell tokens cease in banish immediately (CR 9.1.4.4)."
    );
    assert_eq!(
        after["P1"]["hand"].as_array().unwrap().len(),
        usize::from(mode == 2),
        "Draw two then bottom one leaves one hand card."
    );
    assert_eq!(
        after["P1"]["deck_count"],
        if mode == 2 { 2_u8 } else { 3_u8 },
        "The second mode removes a net one card from the deck."
    );
    assert_eq!(
        after["objects"]["enemy-a"]["damage"],
        u8::from(super_evolution).checked_add(if mode == 1 { 3_u8 } else { 0_u8 }).unwrap(),
        "The first mode adds three damage to its formal target."
    );
    assert_eq!(
        after["objects"]["enemy-b"]["damage"],
        u8::from(super_evolution),
        "The other enemy follower receives only the granted ability damage."
    );
    assert_eq!(
        after["P2"]["leader"]["life"],
        if super_evolution { 19_u8 } else { 20_u8 },
        "The activated modes do not deal extra leader damage."
    );
}

#[test]
fn r7b_almerize_all_prints_play_evolve_and_pay_for_both_printed_modes() {
    for number in ["BP22-022", "BP22-SL08", "PR-625"] {
        assert!(
            selection_with_rules().current.contains_key(number),
            "Every JP printing must be admitted."
        );
        for super_evolution in [false, true] {
            for mode in [1_u64, 2] {
                check_case(number, super_evolution, mode);
            }
        }
    }
}
