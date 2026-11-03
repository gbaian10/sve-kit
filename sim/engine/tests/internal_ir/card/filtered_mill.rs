use super::*;

fn count() -> Value {
    json!({"count_of":{"receipt":"milled","where":{"trait":"trait-a"},"view":"before"}})
}

fn tree(amount: &Value) -> Value {
    let mut tree = fanfare::expanded::tree().clone();
    let mut seed = tree["cards"]["BP01-041"].clone();
    seed["abilities"][0]["activate"] = json!({"cost":{"pp":0},"do":[{"move":{"top":{"who":"self","n":4},"to":"cemetery","result":"milled"}},{"damage":{"to":{"leaders":"opponents"},"n":amount}}]});
    tree["cards"] = json!({"unit-follower":seed});
    tree
}

#[test]
fn filtered_count_requires_a_prior_deck_to_cemetery_result_and_a_before_trait_filter() {
    lowerer().lower(&tree(&count())).unwrap();
    for altered in [
        json!({"count_of":{"receipt":"missing","where":{"trait":"trait-a"},"view":"before"}}),
        json!({"count_of":{"receipt":"milled","where":{"trait":"trait-a"},"view":"after"}}),
        json!({"count_of":{"receipt":"milled","where":{"kind":"follower"},"view":"before"}}),
    ] {
        lowerer().lower(&tree(&altered)).unwrap_err();
    }
    let mut to_ex = tree(&count());
    to_ex["cards"]["unit-follower"]["abilities"][0]["activate"]["do"][0]["move"]["to"] =
        json!("ex");
    assert!(
        lowerer().lower(&to_ex).is_err(),
        "a different destination cannot provide this count"
    );
    let mut opposing = tree(&count());
    opposing["cards"]["unit-follower"]["abilities"][0]["activate"]["do"][0]["move"]["top"]["who"] =
        json!("opp");
    assert!(
        lowerer().lower(&opposing).is_err(),
        "the first filtered count only opens this controller's direct top movement"
    );
    let mut early = tree(&count());
    early["cards"]["unit-follower"]["abilities"][0]["activate"]["do"]
        .as_array_mut()
        .unwrap()
        .swap(0, 1);
    assert!(
        lowerer().lower(&early).is_err(),
        "the producer must dominate its read"
    );
}

fn admitted(program: &WellFormedCardProgram) -> bool {
    Game::new(
        catalog(),
        &setup(1, json!([])),
        &Value::Null,
        &Value::Null,
        "count-proof",
    )
    .unwrap()
    .load_internal_card("unit-follower", program)
    .is_ok()
}

#[test]
fn raw_ir_cannot_disguise_a_field_move_or_another_numeric_context_as_a_filtered_mill() {
    let original = lowerer().lower(&tree(&count())).unwrap()["unit-follower"]
        .card()
        .clone();
    let baseline = WellFormedCardProgram::check(original.clone()).unwrap();
    assert!(admitted(&baseline));
    // Keep producer receipt ids and valid types, then change only the producer's source zone.
    let mut changed = original.clone();
    let Node::Instruction { instruction } =
        &mut changed.abilities[0].node.body.as_mut().unwrap().steps[0].node
    else {
        panic!("move")
    };
    let Action::Move { objects, .. } = &mut instruction.action else {
        panic!("move")
    };
    *objects = Selector::Objects {
        objects: vec![ObjectOperand::SourceHolder {}],
    };
    let changed = WellFormedCardProgram::check(changed).unwrap();
    assert!(
        !admitted(&changed),
        "valid receipt types do not prove a deck producer"
    );
    // Change the producer without asking lowering to accept its filtered read.
    for case in ["opponent", "ex"] {
        let mut plain = tree(&json!(0));
        let movement =
            &mut plain["cards"]["unit-follower"]["abilities"][0]["activate"]["do"][0]["move"];
        if case == "opponent" {
            movement["top"]["who"] = json!("opp");
        } else {
            movement["to"] = json!("ex");
        }
        let alternative = lowerer().lower(&plain).unwrap()["unit-follower"]
            .card()
            .clone();
        assert!(
            admitted(&WellFormedCardProgram::check(alternative.clone()).unwrap()),
            "the plain alternate producer is already supported"
        );
        let Node::Instruction {
            instruction: alternative,
        } = &alternative.abilities[0].node.body.as_ref().unwrap().steps[0].node
        else {
            panic!("alternate move")
        };
        let mut changed = original.clone();
        let Node::Instruction { instruction } =
            &mut changed.abilities[0].node.body.as_mut().unwrap().steps[0].node
        else {
            panic!("move")
        };
        instruction.action = alternative.action.clone();
        let changed = WellFormedCardProgram::check(changed).unwrap();
        assert!(
            !admitted(&changed),
            "a different deck holder or destination cannot provide this count"
        );
    }
    let mut changed = original;
    let Node::Instruction { instruction } =
        &mut changed.abilities[0].node.body.as_mut().unwrap().steps[1].node
    else {
        panic!("damage")
    };
    let Action::Damage {
        amount: DamageAmount::Uniform { amount },
        ..
    } = &instruction.action
    else {
        panic!("damage")
    };
    instruction.action = Action::Draw {
        player: PlayerRef::Controller {},
        count: amount.clone(),
        mode: StepwiseMode::Exact,
    };
    let changed = WellFormedCardProgram::check(changed).unwrap();
    assert!(
        !admitted(&changed),
        "this narrow damage count does not authorize draw amounts"
    );
}

#[test]
fn filtered_count_uses_only_the_successful_top_members_for_either_controller() {
    let (snapshot, keywords, document) = catalog_documents(&json!("1"));
    let snapshot = snapshot
        .lines()
        .map(|line| {
            let mut card: Value = serde_json::from_str(line).unwrap();
            if card["number"] == "unit-follower" {
                card["faces"][0]["traits"] = json!(["trait-a"]);
            }
            card.to_string()
        })
        .collect::<Vec<_>>()
        .join("\n");
    let catalogue = Arc::new(
        Catalog::from_documents(&snapshot, &keywords, &[("unit.yaml".into(), document)]).unwrap(),
    );
    let program = lowerer().lower(&tree(&count())).unwrap()["unit-follower"].clone();
    let blank = WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![],
    })
    .unwrap();
    for seat in [1, 2] {
        let actor = if seat == 1 { "P1" } else { "P2" };
        let opponent = if seat == 1 { "P2" } else { "P1" };
        let mut initial = setup(
            seat,
            json!([
                {"id":"d0","card":"unit-follower"},
                {"id":"d1","card":"unit-spell"},
                {"id":"d2","card":"unit-follower"},
                {"id":"d3","card":"unit-amulet"},
                {"id":"tail","card":"unit-follower"}
            ]),
        );
        initial["turn"]["active"] = json!(actor);
        initial["players"][actor]["zones"]["hand"] = json!([]);
        initial["players"][actor]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
        let mut board = Game::new(
            Arc::clone(&catalogue),
            &initial,
            &Value::Null,
            &Value::Null,
            "both-seat-mill",
        )
        .unwrap();
        board.load_internal_card("unit-follower", &program).unwrap();
        for number in ["unit-spell", "unit-amulet"] {
            board.load_internal_card(number, &blank).unwrap();
        }
        resolved(
            board
                .activate_internal_card("s", AbilityId(1), PlayerId { seat })
                .unwrap(),
        );
        let state = board.projection(View::Referee).unwrap();
        assert_eq!(
            state[opponent]["leader"]["life"], 18_u8,
            "two matching top cards count, not all four or the remaining tail"
        );
        assert_eq!(state[actor]["deck"], json!(["tail"]));
        assert_eq!(state[actor]["cemetery"], json!(["d0", "d1", "d2", "d3"]));
        let public = board
            .projection(if seat == 1 { View::P2 } else { View::P1 })
            .unwrap();
        assert!(
            !public["known_cards"]
                .as_array()
                .unwrap()
                .iter()
                .any(|card| card["id"] == "tail"),
            "reading moved members does not publish the unobserved remainder"
        );
    }
}
