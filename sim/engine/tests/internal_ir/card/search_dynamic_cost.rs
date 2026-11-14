#![expect(
    clippy::wildcard_enum_match_arm,
    reason = "An unexpected question invalidates the synthetic search scenario."
)]
use super::*;
use alloc::collections::BTreeMap;
use sve_engine::game::internal::{InputDomain, SearchAnswer};

/// Followers named by their printed original cost.
const FOLLOWERS: [(&str, &str); 3] = [("one", "1"), ("two", "2"), ("three", "3")];

fn catalog() -> Arc<Catalog> {
    let mut lines = vec![json!({"number":"unit-spell","faces":[{"name":"unit-spell","card_class":"ニュートラル","card_type":"スペル","cost":"1","power":"-","hp":"-","traits":[],"text":null,"sections":[]}]}).to_string()];
    let mut cards = serde_json::Map::new();
    cards.insert(
        "unit-spell".into(),
        json!({"status":"complete","review":"synthetic","abilities":[]}),
    );
    for (number, cost) in FOLLOWERS {
        lines.push(json!({"number":number,"faces":[{"name":number,"card_class":"ニュートラル","card_type":"フォロワー","cost":cost,"power":"1","hp":"1","traits":[],"text":null,"sections":[]}]}).to_string());
        cards.insert(
            number.into(),
            json!({"status":"complete","review":"synthetic","abilities":[]}),
        );
    }
    let registry = json!({"format":1,"kind":"keyword_registry","keywords":{}}).to_string();
    let document = json!({"format":1,"kind":"effect_set","cards":cards}).to_string();
    Arc::new(
        Catalog::from_documents(
            &lines.join("\n"),
            &registry,
            &[("unit.yaml".into(), document)],
        )
        .unwrap(),
    )
}

fn spell(from: &Value) -> Result<WellFormedCardProgram, lowering::LoweringError> {
    unit_spell(&json!({
        "targets":[{"bind":"chosen","from":from,"n":1}],
        "do":[{"search":{"sel":{"who":"self","zone":"deck","kind":"follower",
            "orig_cost_eq":{"plus":[{"orig_cost_of":"chosen"},1]}},"to":"hand","n":1}}]
    }))
}

fn unit_spell(body: &Value) -> Result<WellFormedCardProgram, lowering::LoweringError> {
    let mut tree = fanfare::expanded::tree().clone();
    let mut entry = tree["cards"]["BP08-038"]["abilities"][0].clone();
    entry.as_object_mut().unwrap().remove("fanfare");
    entry["spell"] = body.clone();
    let meta = tree["cards"]["BP08-038"]["meta"].clone();
    tree["cards"] = json!({"unit-spell":{"meta":meta,"abilities":[entry]}});
    lowerer()
        .lower_with_printed_costs(&tree, &BTreeMap::from([("unit-spell".to_owned(), Nat(1))]))
        .map(|mut cards| cards.remove("unit-spell").unwrap())
}

#[test]
fn search_reads_the_original_cost_of_the_chosen_target() {
    let program = spell(&json!({"who":"opp","zone":"field","kind":"follower"})).unwrap();
    let mut initial = setup(
        1,
        json!([
            {"id":"d1","card":"one"},{"id":"d2","card":"two"},{"id":"d3","card":"three"}
        ]),
    );
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"two"}]);
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "dynamic-cost",
    )
    .unwrap();
    game.load_internal_card("unit-spell", &program).unwrap();
    for (number, _) in FOLLOWERS {
        game.load_internal_card(number, &card(vec![])).unwrap();
    }
    let mut step = Some(game.play_internal_card("s", PlayerId { seat: 1 }).unwrap());
    let mut offered = Vec::new();
    while let Some(CardPlay::Awaiting(q)) = step.take() {
        let answer = match &q.domain {
            InputDomain::Targets(d) => Answer::Targets(vec![d.members[0].handle]),
            InputDomain::Search(d) => {
                offered.extend(d.cards.iter().map(|card| card.card.clone()));
                Answer::Search(SearchAnswer::Ungrouped(
                    d.cards.iter().map(|card| card.handle).collect(),
                ))
            }
            _ => panic!("unexpected question: {:?}", q.domain),
        };
        step = Some(game.answer_internal_card(q.actor, q.token, answer).unwrap());
    }
    assert_eq!(
        offered,
        ["three"],
        "the enemy costs two, so only cost three matches"
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["hand"],
        json!(["d3"])
    );
}

#[test]
fn only_a_single_field_or_ex_target_lends_its_original_cost() {
    spell(&json!({"who":"self","zone":"ex","kind":"follower"})).unwrap();
    spell(&json!({"who":"self","zone":"cemetery","kind":"follower"})).unwrap_err();
    unit_spell(&json!({
        "targets":[{"bind":"chosen","from":{"who":"opp","zone":"field","kind":"follower"},"n":2}],
        "do":[{"search":{"sel":{"who":"self","zone":"deck","kind":"follower",
            "orig_cost_eq":{"orig_cost_of":"chosen"}},"to":"hand","n":1}}]
    }))
    .unwrap_err();
}

fn original_cost_of(reference: Binding<Selector>) -> Expr<Int> {
    Expr::Compute(Box::new(IntOperation::Sum {
        objects: Selector::Bound { reference },
        property: NumericProperty::OriginalCost,
    }))
}

#[test]
fn a_private_hand_pick_never_lends_its_original_cost_as_a_public_number() {
    // P2 privately picks a hand card; a public draw count must not reveal its printed cost.
    let program = unit_spell(&json!({
        "targets":[{"bind":"chosen","from":{"who":"opp","zone":"field","kind":"follower"},"n":1}],
        "do":[
            {"choose":{"bind":"h","from":{"who":"opp","zone":"hand","kind":"follower"},"upto":1,"by":"opp"}},
            {"move":{"ref":"h","to":"field","result":"entered"}},
            {"draw":{"n":1}}
        ]
    }))
    .unwrap();
    let ability = &program.card().abilities[0].node;
    let target = ability
        .play
        .as_ref()
        .unwrap()
        .parameters
        .iter()
        .find_map(|decision| match &decision.domain {
            ChoiceDomain::Objects { binding, .. } => Some(*binding),
            _ => None,
        });
    let steps = &ability.body.as_ref().unwrap().steps;
    let private = steps.iter().find_map(|step| match &step.node {
        Node::Decision { decision } => match &decision.domain {
            ChoiceDomain::Objects { binding, .. } => Some(*binding),
            _ => None,
        },
        _ => None,
    });
    let draw = steps
        .iter()
        .position(|step| matches!(&step.node, Node::Instruction { instruction } if matches!(instruction.action, Action::Draw { .. })))
        .unwrap();
    let with_count = |reference: Binding<Selector>| {
        let mut card = program.card().clone();
        if let Node::Instruction { instruction } =
            &mut card.abilities[0].node.body.as_mut().unwrap().steps[draw].node
            && let Action::Draw { count, .. } = &mut instruction.action
        {
            *count = original_cost_of(reference);
        }
        WellFormedCardProgram::check(card)
    };
    assert!(
        with_count(target.unwrap()).is_ok(),
        "the public field target's printed cost is a public number"
    );
    assert!(
        with_count(private.unwrap()).is_err(),
        "the private hand pick's printed cost is not"
    );
}

fn mode_spell(second_limit: &Value) -> WellFormedCardProgram {
    unit_spell(&json!({
        "targets":[
            {"bind":"chosen","option":1,"from":{"who":"self","zone":"field","kind":"follower"},"n":1},
            {"bind":"second","option":1,"from":{"who":"self","zone":"field","kind":"follower",
                "orig_cost_le":second_limit},"n":1}
        ],
        "do":[{"choice":{"n":1,"options":[
            [{"damage":{"to":"second","n":1}}],
            [{"draw":{"n":1}}]
        ]}}]
    }))
    .unwrap()
}

/// The modes offered when P1 plays the spell with `field` followers (number, id) on its field.
fn offered_modes(program: &WellFormedCardProgram, field: &[(&str, &str)]) -> Option<Vec<ModeId>> {
    let mut initial = setup(1, json!([{"id":"d1","card":"one"}]));
    initial["players"]["P1"]["zones"]["field"] = field
        .iter()
        .map(|(number, id)| json!({"id":id,"card":number}))
        .collect();
    initial["players"]["P2"]["zones"]["field"] = json!([]);
    let mut game = Game::new(catalog(), &initial, &Value::Null, &Value::Null, "mode-cost").unwrap();
    game.load_internal_card("unit-spell", program).unwrap();
    for (number, _) in FOLLOWERS {
        game.load_internal_card(number, &card(vec![])).unwrap();
    }
    let CardPlay::Awaiting(question) = game.play_internal_card("s", PlayerId { seat: 1 }).unwrap()
    else {
        return None;
    };
    let InputDomain::Modes { modes, .. } = question.domain else {
        return None;
    };
    Some(modes)
}

#[test]
fn a_mode_target_may_read_the_original_cost_of_an_earlier_target() {
    let same_or_cheaper = mode_spell(&json!({"orig_cost_of":"chosen"}));
    assert_eq!(
        offered_modes(&same_or_cheaper, &[("two", "f2")]),
        Some(vec![ModeId(1), ModeId(2)]),
        "both targets may name the same follower"
    );
    let cheaper = mode_spell(&json!({"minus":[{"orig_cost_of":"chosen"},1]}));
    assert_eq!(
        offered_modes(&cheaper, &[("one", "f1"), ("two", "f2")]),
        Some(vec![ModeId(1), ModeId(2)]),
        "choosing the cost two follower first leaves the cost one follower"
    );
    assert_eq!(
        offered_modes(&cheaper, &[("two", "f2")]),
        Some(vec![ModeId(2)]),
        "no follower is cheaper than the only one, so only the draw mode stays"
    );
}

#[test]
fn a_mode_with_many_cemetery_candidates_keeps_its_menu() {
    // Three of twenty is 1140 answers; the independent enemy half must not force trying them.
    for (picked, graves) in [(1_u8, 11_u8), (3, 20)] {
        let program = unit_spell(&json!({
        "targets":[
            {"bind":"grave","option":1,"from":{"who":"self","zone":"cemetery","kind":"follower"},"n":picked},
            {"bind":"enemy","option":1,"from":{"who":"opp","zone":"field","kind":"follower"},"n":1}
        ],
        "do":[{"choice":{"n":1,"options":[
            [{"move":{"ref":"grave","to":"ex"}},{"damage":{"to":"enemy","n":1}}],
            [{"draw":{"n":1}}]
        ]}}]
    }))
    .unwrap();
        for (enemies, expected) in [
            (
                json!([{"id":"enemy","card":"two"}]),
                vec![ModeId(1), ModeId(2)],
            ),
            (json!([]), vec![ModeId(2)]),
        ] {
            let mut initial = setup(1, json!([{"id":"d1","card":"one"}]));
            initial["players"]["P1"]["zones"]["cemetery"] = (0..graves)
                .map(|index| json!({"id":format!("g{index}"),"card":"one"}))
                .collect();
            initial["players"]["P2"]["zones"]["field"] = enemies;
            let mut game = Game::new(
                catalog(),
                &initial,
                &Value::Null,
                &Value::Null,
                "many-graves",
            )
            .unwrap();
            game.load_internal_card("unit-spell", &program).unwrap();
            for (number, _) in FOLLOWERS {
                game.load_internal_card(number, &card(vec![])).unwrap();
            }
            let CardPlay::Awaiting(question) =
                game.play_internal_card("s", PlayerId { seat: 1 }).unwrap()
            else {
                panic!("a mode question")
            };
            let InputDomain::Modes { modes, .. } = &question.domain else {
                panic!("a mode question: {:?}", question.domain)
            };
            assert_eq!(
                *modes, expected,
                "eleven cemetery candidates still leave the menu, and the draw mode, available"
            );
            if expected.len() == 1 {
                let step = game
                    .answer_internal_card(question.actor, question.token, Answer::Modes(expected))
                    .unwrap();
                assert!(
                    !matches!(step, CardPlay::Awaiting(_)),
                    "the draw mode asks nothing more"
                );
                assert_eq!(
                    game.projection(View::Referee).unwrap()["P1"]["hand"],
                    json!(["d1"]),
                    "the draw mode resolves"
                );
            }
        }
    }
}
