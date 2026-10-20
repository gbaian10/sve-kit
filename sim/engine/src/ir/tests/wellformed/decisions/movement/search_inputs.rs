use super::*;

fn partition(first: u64, second: u64) -> Located<Node> {
    let mut step = search();
    if let Action::Search { destination, .. } = action(&mut step) {
        *destination = Some(SearchDestination::Partition {
            destinations: [(first, OwnerZone::Hand), (second, OwnerZone::Cemetery)]
                .into_iter()
                .map(|(id, zone)| SearchPartition {
                    decision: DecisionId(id),
                    zone: MoveDestination::Owners { zone },
                    cardinality: bounds(1, Some(1)),
                })
                .collect(),
        });
    }
    step
}

fn replacement(id: u64) -> Located<Node> {
    let mut step = search();
    if let Action::Search { destination, .. } = action(&mut step) {
        *destination = Some(SearchDestination::OptionalSelfReplacement {
            decision: DecisionId(id),
            predicate: Expr::Literal(Bool(false)),
            movement: movement(shared(ZoneKind::Field)),
        });
    }
    step
}

#[test]
fn search_and_all_destination_inputs_share_the_decision_namespace() {
    for step in [search(), partition(9100, 9101), replacement(9100)] {
        accept(body(1, vec![step]));
    }
    for step in [
        partition(9001, 9100),
        partition(9100, 9001),
        partition(9100, 9100),
        replacement(9001),
    ] {
        reject(body(1, vec![step]), WellFormedReason::DuplicateDecision);
    }
    for (step, id) in [
        (search(), 9001),
        (partition(9100, 9101), 9100),
        (partition(9100, 9101), 9101),
        (replacement(9100), 9100),
    ] {
        let choice = decision(chosen_objects(id, 1, objects()));
        let order = instruction(
            Action::Order {
                decision: DecisionId(id),
                cards: objects(),
                destinations: vec![Placement {
                    cards: objects(),
                    zone: MoveDestination::Owners {
                        zone: OwnerZone::Deck,
                    },
                    position: Position::Bottom {},
                }],
                chooser: PlayerRef::Controller {},
                visibility: Visibility::Public {},
            },
            4,
            "synthetic",
        );
        for steps in [
            vec![choice.clone(), step.clone()],
            vec![step.clone(), choice.clone()],
            vec![step.clone(), up_to_draw(id)],
            vec![order.clone(), step.clone()],
            vec![step.clone(), order],
        ] {
            reject(body(1, steps), WellFormedReason::DuplicateDecision);
        }
    }
}

#[test]
fn search_input_collisions_are_checked_in_unexecuted_branches() {
    let branch = control(Control::If {
        condition: Expr::Literal(Bool(false)),
        then_body: body(10, vec![replacement(9100)]),
        else_body: None,
    });
    reject(
        body(
            1,
            vec![decision(chosen_objects(9100, 1, objects())), branch],
        ),
        WellFormedReason::DuplicateDecision,
    );
}

#[test]
fn independent_searches_keep_separate_input_and_continuation_identities() {
    let first = search();
    let mut second = search();
    if let Node::Instruction { instruction } = &mut second.node {
        instruction.id = InstructionId(2);
        instruction.receipt = ReceiptId(2);
    }
    if let Action::Search {
        decision,
        candidate,
        filter,
        continuation,
        ..
    } = action(&mut second)
    {
        *decision = DecisionId(9002);
        *candidate = Binding::new(1, ScopeId(4));
        *filter = cp04_filter(*candidate);
        *continuation = SearchContinuation {
            found: Binding::new(2, ScopeId(5)),
            revealed: Binding::new(3, ScopeId(5)),
        };
    }
    accept(body(1, vec![first.clone(), second.clone()]));
    if let Action::Search { decision, .. } = action(&mut second) {
        *decision = DecisionId(9001);
    }
    reject(
        body(1, vec![first, second]),
        WellFormedReason::DuplicateDecision,
    );
}

fn card(id: u64, generation: u64) -> ObjectRef {
    ObjectRef {
        id: ObjectId(id),
        generation: Generation(generation),
    }
}

#[test]
fn search_selection_answers_preserve_all_groups_and_disjoint_generations() {
    let a = card(1, 1);
    let b = card(2, 1);
    let members = [a, b];
    let domains = [vec![a, b], vec![b]];
    for groups in [
        vec![vec![a], vec![b]],
        vec![vec![], vec![b]],
        vec![vec![], vec![]],
    ] {
        assert!(SearchSelectionAnswer::Grouped { groups }.matches_domain(&members, Some(&domains)));
    }
    for groups in [
        vec![vec![b], vec![b]],
        vec![vec![a, a], vec![]],
        vec![vec![], vec![a]],
        vec![vec![card(1, 2)], vec![]],
        vec![vec![card(3, 1)], vec![]],
        vec![vec![a]],
        vec![vec![a], vec![], vec![]],
    ] {
        assert!(
            !SearchSelectionAnswer::Grouped { groups }.matches_domain(&members, Some(&domains))
        );
    }
    let ungrouped = SearchSelectionAnswer::Ungrouped { cards: vec![a] };
    assert!(ungrouped.matches_domain(&members, None));
    assert!(!ungrouped.matches_domain(&members, Some(&domains)));
    assert!(!ungrouped.matches_domain(&[a, a], None));
    assert!(!SearchSelectionAnswer::Ungrouped { cards: vec![a, a] }.matches_domain(&members, None));
    assert!(SearchSelectionAnswer::Ungrouped { cards: vec![] }.matches_domain(&[], None));
    let grouped = SearchSelectionAnswer::Grouped {
        groups: vec![vec![]],
    };
    assert!(!grouped.matches_domain(&members, None));
    assert!(!grouped.matches_domain(&members, Some(&[vec![card(3, 1)]])));
    assert!(!grouped.matches_domain(&members, Some(&[vec![a, a]])));
    assert!(!SearchSelectionAnswer::Grouped { groups: vec![] }.matches_domain(&members, Some(&[])));
}

#[test]
fn partition_answers_consume_found_in_printed_order_even_when_short() {
    let a = card(1, 1);
    let b = card(2, 1);
    let one = bounds(1, Some(1));
    assert!(SearchPartitionAnswer { cards: vec![b] }.matches_remainder(&[a, b], one));
    assert!(SearchPartitionAnswer { cards: vec![a] }.matches_remainder(&[a], one));
    assert!(SearchPartitionAnswer { cards: vec![] }.matches_remainder(&[], one));
    assert!(!SearchPartitionAnswer { cards: vec![] }.matches_remainder(&[a], one));
    assert!(!SearchPartitionAnswer { cards: vec![a, b] }.matches_remainder(&[a, b], one));
    for cards in [vec![a, a], vec![card(1, 2)], vec![card(3, 1)]] {
        assert!(!SearchPartitionAnswer { cards }.matches_remainder(&[a, b], bounds(0, None)));
    }
    assert!(
        SearchPartitionAnswer { cards: vec![a, b] }.matches_remainder(&[a, b], bounds(3, None))
    );
    assert!(!SearchPartitionAnswer { cards: vec![a] }.matches_remainder(&[a, a], one));
    assert!(!SearchPartitionAnswer { cards: vec![a] }.matches_remainder(&[a], bounds(2, Some(1))));
}
