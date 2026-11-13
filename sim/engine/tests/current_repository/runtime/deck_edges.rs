//! A deck-edge partition orders only its own piles (Q156): a later multi-card deck move of the
//! question's whole selection still needs its own order choice.
#![expect(
    clippy::panic,
    reason = "A missing placement question invalidates the scenario."
)]
use super::*;
use alloc::collections::BTreeMap;
use sve_engine::game::internal::TargetRoleAssignment;

/// The producer splits the opposing Stack amulets between the deck edges, then puts `pile`
/// (the top role, or the whole selection) on the deck top once more.
fn program(whole_selection: bool) -> (WellFormedCardProgram, AbilityId) {
    let mut tree = expanded::tree().clone();
    let mut entry = tree["cards"]["BP08-038"]["abilities"][0].clone();
    entry.as_object_mut().unwrap().remove("fanfare");
    entry["activate"] = json!({"cost":{"pp":0_u8},"do":[{"move":{
        "ref":{"who":"opp","zone":"field"},"to":"deck",
        "position":{"one_of":["top","bottom"],"by":"opp"}}}]});
    let meta = tree["cards"]["BP08-038"]["meta"].clone();
    tree["cards"] = json!({"producer":{"meta":meta,"abilities":[entry]}});
    let lowered = lowering::ExpandedLowerer::new()
        .unwrap()
        .lower_with_printed_costs(&tree, &BTreeMap::from([("producer".into(), Nat(1))]))
        .unwrap();
    let mut raw = lowered["producer"].card().clone();
    let ability = &mut raw.abilities[0].node;
    let id = ability.id;
    let steps = &mut ability.body.as_mut().unwrap().steps;
    let Node::Decision { decision } = &steps[0].node else {
        panic!("the placement question comes first");
    };
    let ChoiceDomain::Objects {
        binding,
        target_roles: Some(spec),
        ..
    } = &decision.domain
    else {
        panic!("a deck-edge partition");
    };
    let pile = if whole_selection {
        *binding
    } else {
        spec.roles[0].binding
    };
    let mut followup = steps[1].clone();
    if let Node::Instruction { instruction } = &mut followup.node {
        instruction.id = InstructionId(900);
        instruction.receipt = ReceiptId(900);
        if let Action::Move { objects, .. } = &mut instruction.action {
            *objects = Selector::Bound { reference: pile };
        }
    }
    steps.push(followup);
    (WellFormedCardProgram::check(raw).unwrap(), id)
}

/// Both Stack amulets go on top as `[s0, s1]` and stay on the field by losing their counter;
/// the follow-up move then really moves them.
fn play(whole_selection: bool) -> (Game, Result<CardPlay, EngineFailure>) {
    let (program, id) = program(whole_selection);
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"producer","card":"producer"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"s0","card":"BP01-075","state":{"counters":{"stack_counter":1_u8}}},
        {"id":"s1","card":"BP01-075","state":{"counters":{"stack_counter":1_u8}}}
    ]);
    initial["players"]["P2"]["zones"]["deck"] = json!([{"id":"d0","card":"BP01-075"}]);
    let mut game = Game::from_selected_effects(
        producer_selection().clone(),
        &initial,
        &Value::Null,
        &Value::Null,
        "deck-edges",
    )
    .unwrap();
    game.load_internal_card("producer", &program).unwrap();
    let CardPlay::Awaiting(question) = game
        .activate_internal_card("producer", id, PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("the opponent places its amulets");
    };
    let answered = game.answer_internal_card(
        question.actor,
        question.token,
        Answer::TargetsWithRoles {
            targets: vec![1, 0],
            roles: vec![
                TargetRoleAssignment {
                    role: Nat(1),
                    targets: vec![0, 1],
                },
                TargetRoleAssignment {
                    role: Nat(2),
                    targets: vec![],
                },
            ],
        },
    );
    (game, answered)
}

fn zone(game: &Game, zone: &str) -> Vec<String> {
    game.projection(View::Referee).unwrap()["P2"][zone]
        .as_array()
        .unwrap()
        .iter()
        .map(|id| id.as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn only_a_placed_pile_keeps_its_answered_order_for_a_later_deck_move() {
    let (game, answered) = play(false);
    assert!(
        matches!(answered, Ok(CardPlay::Resolved(_))),
        "{answered:?}"
    );
    assert_eq!(zone(&game, "deck"), ["s0", "s1", "d0"], "the top pile's order");

    let (whole, rejected) = play(true);
    assert!(
        matches!(&rejected, Err(EngineFailure::Unsupported(reason)) if reason.contains("order choice")),
        "the whole selection was never ordered: {rejected:?}"
    );
    assert_eq!(zone(&whole, "deck"), ["d0"]);
    assert_eq!(zone(&whole, "field"), ["s0", "s1"]);
}
