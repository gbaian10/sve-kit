//! Search regressions keep found cards separate from arrivals under field capacity.
#![expect(
    clippy::panic,
    reason = "Unexpected choices invalidate these Search scenarios."
)]
use super::*;
use sve_engine::game::internal::{DecisionView, InputDomain, SearchAnswer};

fn program(source: &str, search: &Value, pp: u64) -> WellFormedCardProgram {
    lowerer()
        .lower(&same_name::tree(
            &json!({"activate":{"cost":{"pp":pp},"do":[{"search":search}]}}),
            source,
        ))
        .unwrap()
        .remove(source)
        .unwrap()
}

fn catalogue() -> Arc<Catalog> {
    let (snapshot, registry, document) = catalog_documents(&json!("1"));
    let mut rows = snapshot
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    let template = rows
        .iter()
        .find(|row| row["number"] == "unit-follower")
        .unwrap()
        .clone();
    let mut document: Value = serde_json::from_str(&document).unwrap();
    for (number, cost, traits) in [
        ("mat-a", 3, &["trait-a", "trait-b"][..]),
        ("mat-b", 3, &["trait-a", "trait-b"][..]),
        ("mat-c", 2, &["trait-a", "trait-b"][..]),
        ("expensive", 6, &["trait-a", "trait-b"][..]),
        ("only-a", 1, &["trait-a"][..]),
        ("only-b", 1, &["trait-b"][..]),
    ] {
        let mut row = template.clone();
        row["number"] = json!(number);
        row["faces"][0]["name"] = json!(number);
        row["faces"][0]["cost"] = json!(cost.to_string());
        row["faces"][0]["traits"] = json!(traits);
        rows.push(row);
        document["cards"][number] = document["cards"]["unit-follower"].clone();
    }
    let snapshot = rows
        .iter()
        .map(Value::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    Arc::new(
        Catalog::from_documents(
            &snapshot,
            &registry,
            &[("synthetic.yaml".into(), document.to_string())],
        )
        .unwrap(),
    )
}

fn position(program: &WellFormedCardProgram, source: &str, deck: &Value, occupied: usize) -> Game {
    let mut initial = setup(1, deck.clone());
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    let mut field = vec![json!({"id":"source","card":source})];
    field.extend((1..occupied).map(|n| json!({"id":format!("old{n}"),"card":"unit-amulet"})));
    initial["players"]["P1"]["zones"]["field"] = json!(field);
    initial["players"]["P1"]["pp"] = json!({"current":4,"max":4});
    loaded(program, source, &initial)
}

fn loaded(program: &WellFormedCardProgram, source: &str, initial: &Value) -> Game {
    let mut game = Game::new(
        catalogue(),
        initial,
        &Value::Null,
        &Value::Null,
        "material-search",
    )
    .unwrap();
    game.load_internal_card(source, program).unwrap();
    for number in [
        "unit-follower",
        "unit-spell",
        "unit-amulet",
        "mat-a",
        "mat-b",
        "mat-c",
        "expensive",
        "only-a",
        "only-b",
    ] {
        if number != source {
            game.load_internal_card(number, &card(vec![])).unwrap();
        }
    }
    game
}

fn question(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(question) = play else {
        panic!("search decision")
    };
    question
}

fn assert_outcome(result: &Activation, found: usize, arrived: usize) {
    let outcome = result
        .progress
        .receipts
        .values()
        .find_map(|receipt| {
            if let InstructionOutcome::Search(outcome) = receipt {
                Some(outcome)
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!(
        outcome.found.len(),
        found,
        "Search must record every found card"
    );
    assert_eq!(
        outcome.moved.len(),
        found,
        "Search must record every attempted arrival"
    );
    assert_eq!(
        outcome
            .moved
            .iter()
            .filter(|movement| movement.after.is_some())
            .count(),
        arrived,
        "only available field spaces accept arrivals"
    );
    assert_eq!(
        result
            .progress
            .facts
            .iter()
            .filter(|fact| matches!(fact, Fact::Shuffled { .. }))
            .count(),
        1,
        "each Search shuffles exactly once after its attempts"
    );
}

fn arrive(game: &mut Game, capacity: &DecisionView, count: usize) -> Activation {
    let InputDomain::SearchPartition(domain) = &capacity.domain else {
        panic!("field capacity")
    };
    assert_eq!(domain.cards.len(), count, "capacity uses every found card");
    assert_eq!(
        domain.cardinality,
        Cardinality {
            minimum: Nat(1),
            maximum: Some(Nat(1))
        },
        "the one free field space requires exactly one choice"
    );
    let chosen = domain
        .cards
        .iter()
        .find(|card| card.card == "mat-b")
        .unwrap()
        .handle;
    resolved(
        game.answer_internal_card(
            capacity.actor,
            capacity.token,
            Answer::SearchPartition(vec![chosen]),
        )
        .unwrap(),
    )
}

#[test]
fn dual_trait_search_checks_names_and_total_cost_before_allocating_one_field_space() {
    let search = json!({"sel":{"who":"self","zone":"deck","kind":"follower","traits_all":["trait-a","trait-b"]},"upto":3,"distinct_by":"name","cost_sum_le":8,"to":"field"});
    let program = program("unit-amulet", &search, 0);
    let deck = json!([
        {"id":"a0","card":"mat-a"},{"id":"a1","card":"mat-a"},
        {"id":"b","card":"mat-b"},{"id":"c","card":"mat-c"},
        {"id":"expensive","card":"expensive"},{"id":"one-trait-a","card":"only-a"},
        {"id":"one-trait-b","card":"only-b"}
    ]);
    for count in [2, 3] {
        let mut game = position(&program, "unit-amulet", &deck, 4);
        let search_question = question(
            game.activate_internal_card(
                "source",
                program.card().abilities[0].node.id,
                PlayerId { seat: 1 },
            )
            .unwrap(),
        );
        let InputDomain::Search(domain) = &search_question.domain else {
            panic!("Search, not Look")
        };
        assert_eq!(
            domain.cardinality.maximum,
            Some(Nat(3)),
            "finding is not truncated to the free field space"
        );
        assert_eq!(domain.cards.len(), 5, "both traits are required");
        assert!(domain.cards.iter().all(|card| card.object.is_none()));
        assert!(game.internal_decision(View::P2).is_none());
        let pick = |name: &str| {
            domain
                .cards
                .iter()
                .find(|card| card.card == name)
                .unwrap()
                .handle
        };
        let a = domain
            .cards
            .iter()
            .filter(|card| card.card == "mat-a")
            .map(|card| card.handle)
            .collect::<Vec<_>>();
        let before = exact_authority(&game);
        for bad in [vec![a[0], a[1]], vec![a[0], pick("expensive")]] {
            assert!(matches!(
                game.answer_internal_card(
                    search_question.actor,
                    search_question.token,
                    Answer::Search(SearchAnswer::Grouped(vec![bad]))
                ),
                Err(sve_engine::EngineFailure::Invalid(_))
            ));
            assert_eq!(exact_authority(&game), before);
        }
        let mut found = vec![a[0], pick("mat-b")];
        if count == 3 {
            found.push(pick("mat-c"));
        }
        let capacity = question(
            game.answer_internal_card(
                search_question.actor,
                search_question.token,
                Answer::Search(SearchAnswer::Grouped(vec![found])),
            )
            .unwrap(),
        );
        let result = arrive(&mut game, &capacity, count);
        assert_outcome(&result, count, 1);
        let state = game.projection(View::Referee).unwrap();
        assert!(
            state["P1"]["field"]
                .as_array()
                .unwrap()
                .contains(&json!("b"))
        );
        assert!(
            state["P1"]["deck"]
                .as_array()
                .unwrap()
                .contains(&json!("a0"))
        );
        if count == 3 {
            assert!(
                state["P1"]["deck"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("c"))
            );
        }
        assert_eq!(state["objects"]["a0"]["generation"], 0);
    }
}

#[test]
fn same_name_search_can_find_zero_or_fail_capacity_without_refunding_the_paid_cost() {
    let search = json!({"sel":{"who":"self","zone":"deck","kind":"follower","name":"mat-a"},"n":1,"to":"field"});
    let program = program("mat-a", &search, 2);
    for (matching, occupied) in [(false, 4), (true, 5)] {
        let deck = json!([{"id":"d","card":if matching {"mat-a"} else {"mat-b"}}]);
        let mut game = position(&program, "mat-a", &deck, occupied);
        let question = question(
            game.activate_internal_card(
                "source",
                program.card().abilities[0].node.id,
                PlayerId { seat: 1 },
            )
            .unwrap(),
        );
        let InputDomain::Search(domain) = &question.domain else {
            panic!("named Search")
        };
        assert_eq!(domain.cards.len(), usize::from(matching));
        let result = resolved(
            game.answer_internal_card(
                question.actor,
                question.token,
                Answer::Search(SearchAnswer::Ungrouped(
                    domain.cards.iter().map(|card| card.handle).collect(),
                )),
            )
            .unwrap(),
        );
        assert_outcome(&result, usize::from(matching), 0);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["pp"]["current"], 2);
        assert_eq!(state["P1"]["deck"], json!(["d"]));
        assert_eq!(state["objects"]["d"]["generation"], 0);
    }
}

#[test]
fn same_name_search_entry_queues_a_fresh_fanfare_and_pays_its_cost_again() {
    let program = lowerer()
        .lower(&same_name::tree(
            &json!({"fanfare":{"cost":{"pp":2},"do":[{"search":{
                "sel":{"who":"self","zone":"deck","kind":"follower","name":"mat-a"},
                "n":1,"to":"field"
            }}]}}),
            "mat-a",
        ))
        .unwrap()
        .remove("mat-a")
        .unwrap();
    let mut initial = setup(1, json!([{"id":"d","card":"mat-a"}]));
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"mat-a"}]);
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    initial["players"]["P1"]["pp"] = json!({"current":10,"max":10});
    let mut game = loaded(&program, "mat-a", &initial);
    game.play_internal_card("source", PlayerId { seat: 1 })
        .unwrap();
    for found in [1, 0] {
        if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
        let pending = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = &pending.domain else {
            panic!("fanfare ordering")
        };
        assert_eq!(options.len(), 1);
        let declaration = question(
            game.answer_internal_card(
                pending.actor,
                pending.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap(),
        );
        assert_eq!(declaration.domain, InputDomain::OptionalCost);
        let search = question(
            game.answer_internal_card(
                declaration.actor,
                declaration.token,
                Answer::OptionalCost(true),
            )
            .unwrap(),
        );
        let InputDomain::Search(domain) = &search.domain else {
            panic!("named Search")
        };
        assert_eq!(domain.cards.len(), found);
        let result = resolved(
            game.answer_internal_card(
                search.actor,
                search.token,
                Answer::Search(SearchAnswer::Ungrouped(
                    domain.cards.iter().map(|card| card.handle).collect(),
                )),
            )
            .unwrap(),
        );
        assert!(result.progress.receipts.values().any(|receipt| matches!(
            receipt, InstructionOutcome::Search(outcome) if outcome.found.len() == found
        )));
    }
    if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
    assert!(game.internal_pending_choice().unwrap().is_none());
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        state["P1"]["pp"]["current"], 3,
        "play cost 3 and two separate fanfare payments of 2"
    );
    assert_eq!(state["P1"]["field"], json!(["source", "d"]));
    assert_eq!(state["P1"]["deck"], json!([]));
}
