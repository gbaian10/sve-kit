use super::*;

fn cemetery_pool() -> (Game, AbilityId) {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"source","card":"BP22-077"},{"filler":3_u8}
    ]);
    let mut cemetery = Vec::new();
    for (kind, card) in ["BP01-109", "BP01-112", "BP02-069", "BP02-078"]
        .iter()
        .enumerate()
    {
        for copy in 0_u8..3 {
            cemetery.push(json!({"id":format!("grave-{kind}-{copy}"),"card":card}));
        }
    }
    initial["players"]["P1"]["zones"]["cemetery"] = json!(cemetery);
    let game = game(&initial);
    let activated = selection().current["BP22-077"]
        .program
        .card()
        .abilities
        .iter()
        .find(|ability| ability.node.kind == AbilityKind::Activated)
        .unwrap()
        .node
        .id;
    (game, activated)
}

#[test]
fn bp22_077_can_select_a_named_pair_from_twelve_cemetery_candidates_before_allocating_one_space() {
    let (mut game, activated) = cemetery_pool();
    let CardPlay::Awaiting(targets) = game
        .activate_internal_card("source", activated, PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("all twelve legal cemetery candidates must be offered")
    };
    let InputDomain::Targets(domain) = &targets.domain else {
        panic!("formal named pair")
    };
    assert_eq!(
        domain.members.len(),
        12,
        "the cemetery is not bounded by the two fields' ten spaces"
    );
    assert!(
        domain.distinct_names,
        "the printed pair must have different names"
    );
    let before_payment = game.projection(View::Referee).unwrap();
    assert_eq!(
        before_payment["P1"]["pp"]["current"], 10_u8,
        "formal targets precede payment"
    );
    assert_eq!(
        before_payment["objects"]["source"]["acted"], false,
        "the source is not acted before its targets"
    );
    let same_name = domain
        .members
        .iter()
        .filter(|member| member.card.as_deref() == Some("BP01-109"))
        .take(2)
        .map(|member| member.handle)
        .collect();
    game.answer_internal_card(targets.actor, targets.token, Answer::Targets(same_name))
        .unwrap_err();
    assert_eq!(
        game.projection(View::Referee).unwrap(),
        before_payment,
        "same-name selection is rejected atomically"
    );
    let pair = ["BP01-109", "BP01-112"]
        .iter()
        .map(|card| {
            domain
                .members
                .iter()
                .find(|member| member.card.as_deref() == Some(*card))
                .unwrap()
                .handle
        })
        .collect();
    let CardPlay::Awaiting(capacity) = game
        .answer_internal_card(targets.actor, targets.token, Answer::Targets(pair))
        .unwrap()
    else {
        panic!("the selected pair must compete for the last field slot")
    };
    let InputDomain::SearchPartition(allocation) = &capacity.domain else {
        panic!("field allocation")
    };
    assert_eq!(
        allocation.cards.len(),
        2,
        "the formal pair is not clamped to the remaining slot"
    );
    let first = allocation
        .cards
        .iter()
        .find(|card| card.card == "BP01-109")
        .unwrap()
        .handle;
    resolved(
        game.answer_internal_card(
            capacity.actor,
            capacity.token,
            Answer::SearchPartition(vec![first]),
        )
        .unwrap(),
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        projected_zone_count(&state["P1"]["field"]),
        5_u64,
        "only the allocated card enters"
    );
    assert_eq!(
        projected_zone_count(&state["P1"]["cemetery"]),
        11_u64,
        "the unallocated target stays in cemetery"
    );
    assert_eq!(
        state["P1"]["pp"]["current"], 6_u8,
        "the printed activation costs four PP"
    );
    assert_eq!(
        state["objects"]["source"]["acted"], true,
        "the printed activation acts its source"
    );
}
