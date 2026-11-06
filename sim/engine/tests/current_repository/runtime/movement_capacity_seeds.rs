#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Printed capacity seeds have a fixed set of payment, allocation and ordering questions."
)]
use super::*;
use sve_engine::game::internal::DecisionView;

fn question(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(choice) = play else {
        panic!("printed seed question")
    };
    choice
}
fn pending(game: &mut Game, card: &str) -> CardPlay {
    let q = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &q.domain else {
        panic!("printed occurrence")
    };
    let handle = options
        .iter()
        .find(|option| option.card == card)
        .unwrap()
        .handle;
    game.answer_internal_card(q.actor, q.token, Answer::Pending(handle))
        .unwrap()
}
fn activated(card: &str) -> AbilityId {
    selection().current[card]
        .program
        .card()
        .abilities
        .iter()
        .find(|ability| ability.node.kind == AbilityKind::Activated)
        .unwrap()
        .node
        .id
}
fn finish(game: &mut Game, mut play: CardPlay, take: usize, accept: bool) {
    while let CardPlay::Awaiting(q) = play {
        let answer = match &q.domain {
            InputDomain::OptionalCost => Answer::OptionalCost(accept),
            InputDomain::CostMaterials(domain) => Answer::CostMaterials(
                domain
                    .members
                    .iter()
                    .take(1)
                    .map(|member| member.handle)
                    .collect(),
            ),
            InputDomain::Targets(domain) => Answer::Targets(
                domain
                    .members
                    .iter()
                    .take(take)
                    .map(|member| member.handle)
                    .collect(),
            ),
            InputDomain::SearchPartition(domain) => Answer::SearchPartition(
                domain
                    .cards
                    .iter()
                    .take(usize::try_from(domain.cardinality.minimum.0).unwrap())
                    .map(|card| card.handle)
                    .collect(),
            ),
            InputDomain::Order(cards) => {
                Answer::Order(cards.iter().rev().map(|card| card.handle).collect())
            }
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            other => panic!("unexpected printed seed choice {other:?}"),
        };
        play = game.answer_internal_card(q.actor, q.token, answer).unwrap();
    }
    assert!(
        matches!(play, CardPlay::Resolved(_) | CardPlay::Cancelled),
        "printed ability completes or its head fee is declined"
    );
    game.resume_internal_card().unwrap();
}
fn old_ex(free: usize) -> Value {
    json!(
        (0..5_usize.saturating_sub(free))
            .map(|i| json!({"id":format!("old-ex-{i}"),"card":"BP01-171"}))
            .collect::<Vec<_>>()
    )
}

fn cernunnos_position() -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP22-077"},{"id":"fee","card":"BP02-081"},{"id":"wrong-cost","card":"BP01-171"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"drawn","card":"BP01-171"}]);
    initial["players"]["P1"]["zones"]["field"] = json!(
        (0_u8..3)
            .map(|i| json!({"id":format!("old-{i}"),"card":"BP01-171"}))
            .collect::<Vec<_>>()
    );
    let mut grave = vec![
        json!({"id":"vampy","card":"BP02-069"}),
        json!({"id":"veight","card":"BP02-078"}),
        json!({"id":"same-name","card":"BP22-077"}),
    ];
    grave.extend((0_u8..6).map(|i| json!({"id":format!("spell-{i}"),"card":"BP02-081"})));
    initial["players"]["P1"]["zones"]["cemetery"] = json!(grave);
    initial
}

#[test]
fn bp22_077_pays_its_filtered_discard_then_resolves_two_formal_targets_into_one_field_space() {
    for accept in [false, true] {
        let initial = cernunnos_position();
        let mut game = game(&initial);
        assert!(
            matches!(
                game.play_internal_card("source", PlayerId { seat: 1 })
                    .unwrap(),
                CardPlay::Entered(_)
            ),
            "Cernunnos enters in the fourth field space"
        );
        let fanfare = pending(&mut game, "BP22-077");
        finish(&mut game, fanfare, 1, accept);
        if !accept {
            game.activate_internal_card("source", activated("BP22-077"), PlayerId { seat: 1 })
                .unwrap_err();
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["P1"]["pp"]["current"], 8_u8,
                "nine original-cost-two cemetery cards cannot pay the activation"
            );
            assert_eq!(state["P1"]["hand"], json!(["fee", "wrong-cost"]));
            continue;
        }
        let after = game.projection(View::Referee).unwrap();
        assert_eq!(
            after["P1"]["hand"],
            json!(["wrong-cost", "drawn"]),
            "the printed filtered fee discards the cost-two card then draws"
        );
        let targets = question(
            game.activate_internal_card("source", activated("BP22-077"), PlayerId { seat: 1 })
                .unwrap(),
        );
        let InputDomain::Targets(domain) = &targets.domain else {
            panic!("formal cemetery targets")
        };
        assert_eq!(
            domain.members.len(),
            2,
            "same-name followers and cost-two spells do not qualify"
        );
        let capacity = question(
            game.answer_internal_card(
                targets.actor,
                targets.token,
                Answer::Targets(domain.members.iter().map(|member| member.handle).collect()),
            )
            .unwrap(),
        );
        let InputDomain::SearchPartition(allocation) = &capacity.domain else {
            panic!("one remaining field space")
        };
        assert_eq!(
            allocation.cardinality,
            Cardinality {
                minimum: Nat(1),
                maximum: Some(Nat(1))
            }
        );
        let chosen = allocation
            .cards
            .iter()
            .find(|card| card.card == "BP02-078")
            .unwrap()
            .handle;
        let result = game
            .answer_internal_card(
                capacity.actor,
                capacity.token,
                Answer::SearchPartition(vec![chosen]),
            )
            .unwrap();
        finish(&mut game, result, 1, true);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["field"].as_array().unwrap().len(), 5);
        assert!(
            state["P1"]["field"]
                .as_array()
                .unwrap()
                .contains(&json!("veight")),
            "the allocated formal target enters"
        );
        assert!(
            state["P1"]["cemetery"]
                .as_array()
                .unwrap()
                .contains(&json!("vampy")),
            "the other formal target stays in cemetery"
        );
        assert_eq!(state["P1"]["pp"]["current"], 4_u8);
        assert_eq!(state["objects"]["source"]["acted"], true);
    }
}

#[test]
fn bp07_071_returns_only_the_allocated_vampires_at_zero_cost_then_counts_its_field_vampires() {
    for free in [0_usize, 1, 3] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP07-071"}]);
        initial["players"]["P1"]["zones"]["cemetery"] =
            json!([{"id":"vampy","card":"BP02-069"},{"id":"veight","card":"BP02-078"}]);
        initial["players"]["P1"]["zones"]["ex"] = old_ex(free);
        initial["players"]["P2"]["zones"]["field"] =
            json!([{"id":"enemy","card":"BP01-037","state":{"hp":10_u8}}]);
        let mut game = game(&initial);
        assert!(
            matches!(
                game.play_internal_card("source", PlayerId { seat: 1 })
                    .unwrap(),
                CardPlay::Entered(_)
            ),
            "Kudlak enters normally"
        );
        let targets = question(pending(&mut game, "BP07-071"));
        let InputDomain::Targets(domain) = &targets.domain else {
            panic!("formal vampire targets")
        };
        assert_eq!(domain.members.len(), 2);
        let play = game
            .answer_internal_card(
                targets.actor,
                targets.token,
                Answer::Targets(domain.members.iter().map(|member| member.handle).collect()),
            )
            .unwrap();
        let play = if free == 1 {
            let capacity = question(play);
            let InputDomain::SearchPartition(allocation) = &capacity.domain else {
                panic!("one remaining EX space")
            };
            let handle = allocation
                .cards
                .iter()
                .find(|card| card.card == "BP02-069")
                .unwrap()
                .handle;
            game.answer_internal_card(
                capacity.actor,
                capacity.token,
                Answer::SearchPartition(vec![handle]),
            )
            .unwrap()
        } else {
            play
        };
        finish(&mut game, play, 1, true);
        let moved = game.projection(View::Referee).unwrap();
        assert_eq!(
            moved["P1"]["ex"].as_array().unwrap().len(),
            5_usize.saturating_sub(free) + free.min(2)
        );
        assert_eq!(
            moved["P1"]["cemetery"].as_array().unwrap().len(),
            2_usize.saturating_sub(free.min(2))
        );
        if free > 0 {
            assert!(
                matches!(
                    game.play_internal_card("vampy", PlayerId { seat: 1 })
                        .unwrap(),
                    CardPlay::Entered(_)
                ),
                "the allocated vampire is playable for zero PP"
            );
            let fanfare = pending(&mut game, "BP02-069");
            finish(&mut game, fanfare, 1, true);
            assert_eq!(
                game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
                3_u8
            );
        }
        let damage = game
            .activate_internal_card("source", activated("BP07-071"), PlayerId { seat: 1 })
            .unwrap();
        finish(&mut game, damage, 1, true);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["objects"]["enemy"]["hp"],
            if free > 0 { 8_u8 } else { 9_u8 },
            "the activated damage counts the actual field vampires"
        );
        assert_eq!(state["objects"]["source"]["acted"], true);
    }
}

#[test]
fn bp22_110_looks_allocates_one_ex_arrival_returns_all_others_and_observes_the_zero_cost_goblin() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP22-110"}]);
    initial["players"]["P1"]["zones"]["ex"] = old_ex(1);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"goblin-a","card":"BP01-171"},{"id":"goblin-b","card":"BP01-171"},
        {"id":"wrong","card":"BP01-037"},{"id":"costly","card":"BP22-110"},{"id":"last","card":"BP01-037"}
    ]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-037","state":{"hp":10_u8}}]);
    let mut game = game(&initial);
    assert!(
        matches!(
            game.play_internal_card("source", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ),
        "the emperor enters without observing itself"
    );
    let look = question(pending(&mut game, "BP22-110"));
    let InputDomain::Targets(domain) = &look.domain else {
        panic!("private looked goblin choice")
    };
    let handles = domain
        .members
        .iter()
        .filter(|member| member.card.as_deref() == Some("BP01-171"))
        .map(|member| member.handle)
        .collect::<Vec<_>>();
    assert_eq!(handles.len(), 2);
    let capacity = question(
        game.answer_internal_card(look.actor, look.token, Answer::Targets(handles))
            .unwrap(),
    );
    let InputDomain::SearchPartition(allocation) = &capacity.domain else {
        panic!("private EX allocation")
    };
    assert_eq!(allocation.cards.len(), 2);
    assert!(
        allocation.cards.iter().all(|card| card.object.is_none()),
        "private looked cards have no public physical identity"
    );
    let moved = game
        .answer_internal_card(
            capacity.actor,
            capacity.token,
            Answer::SearchPartition(vec![allocation.cards[0].handle]),
        )
        .unwrap();
    finish(&mut game, moved, 1, true);
    let state = game.projection(View::Referee).unwrap();
    let arrival = state["P1"]["ex"]
        .as_array()
        .unwrap()
        .last()
        .unwrap()
        .as_str()
        .unwrap()
        .to_owned();
    assert!(
        ["goblin-a", "goblin-b"].contains(&arrival.as_str()),
        "one of the selected goblins arrives"
    );
    assert_eq!(
        state["P1"]["deck"].as_array().unwrap().len(),
        4,
        "the unallocated selected card joins the other bottom cards"
    );
    assert!(
        !state["P1"]["deck"]
            .as_array()
            .unwrap()
            .contains(&json!(arrival))
    );
    assert!(
        matches!(
            game.play_internal_card(&arrival, PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ),
        "the successful EX arrival has zero play cost"
    );
    let damage = pending(&mut game, "BP22-110");
    finish(&mut game, damage, 1, true);
    let final_state = game.projection(View::Referee).unwrap();
    assert_eq!(final_state["P1"]["pp"]["current"], 5_u8);
    assert_eq!(
        final_state["objects"]["enemy"]["hp"], 8_u8,
        "the other goblin entry triggers the printed observer"
    );
}
