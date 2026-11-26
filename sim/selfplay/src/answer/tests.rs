//! Every answer must satisfy the constraints its question states, for any seed.

#![expect(
    clippy::default_numeric_fallback,
    reason = "test fixtures are literal JSON"
)]

use super::{NoAnswer, choose};
use alloc::collections::BTreeSet;
use rand::SeedableRng as _;
use rand_chacha::ChaCha8Rng;
use serde_json::{Value, json};
use sve_battle_contract::Answer;
use sve_battle_contract::question::Question;

fn question(usage: &str, shape: &Value) -> Question {
    serde_json::from_value(
        json!({"id":"q","actor":"p1","purpose":"resolution_choice","use":usage,
        "processing":{"stage":null,"actor":null,"controller":null,"source":null,"subject":null,
            "rule":null,"procedure":null,"declared":[],"payments":[]},
        "shape":shape,"can_withdraw":false,"auto_answer":{"kind":"unknown"},
        "assistance":null,"rules":null}),
    )
    .unwrap()
}

fn candidate(id: &str, name: &str, cost: i64) -> Value {
    json!({"id":id,"subject":null,"controller":null,"origin":null,
        "rules_names":[name],"original_cost":cost,"classes":null})
}

fn selection(candidates: &[Value], min: u64, max: Option<u64>) -> Value {
    json!({"candidates":candidates,"shown":[],"bounds":{"min":min,"max":max},"groups":[],
        "group_assignment":"existence","roles":[],"distinct":[],
        "original_cost_total":{"min":null,"max":null},"capacity":null})
}

/// The answers of forty seeds; any refusal fails the test.
fn answers(question: &Question) -> Vec<Answer> {
    (0..40)
        .map(|seed| choose(question, &mut ChaCha8Rng::seed_from_u64(seed)).unwrap())
        .collect()
}

fn ids(answer: &Value, pointer: &str) -> Vec<String> {
    answer
        .pointer(pointer)
        .and_then(Value::as_array)
        .unwrap()
        .iter()
        .map(|id| id.as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn a_selection_respects_bounds_distinct_names_and_the_cost_total() {
    let candidates = [
        candidate("a", "one", 1),
        candidate("b", "one", 2),
        candidate("c", "two", 3),
        candidate("d", "three", 4),
        candidate("e", "three", 5),
    ];
    let mut shape = selection(&candidates, 2, Some(3));
    shape["distinct"] = json!(["rules_name"]);
    shape["original_cost_total"] = json!({"min":null,"max":8});
    let asked = question(
        "resolution_selection",
        &json!({"kind":"select","selection":shape}),
    );
    let mut sizes = BTreeSet::new();
    for answer in answers(&asked) {
        let chosen = ids(&serde_json::to_value(&answer).unwrap(), "/selected");
        sizes.insert(chosen.len());
        let picked: Vec<&Value> = candidates
            .iter()
            .filter(|candidate| chosen.contains(&candidate["id"].as_str().unwrap().to_owned()))
            .collect();
        let names: BTreeSet<&str> = picked
            .iter()
            .map(|candidate| candidate["rules_names"][0].as_str().unwrap())
            .collect();
        assert_eq!(names.len(), picked.len(), "{chosen:?} repeats a name");
        let total: i64 = picked
            .iter()
            .map(|candidate| candidate["original_cost"].as_i64().unwrap())
            .sum();
        assert!(total <= 8, "{chosen:?} costs {total}");
    }
    assert_eq!(sizes, BTreeSet::from([2, 3]));
}

#[test]
fn explicit_groups_and_roles_partition_the_selection() {
    let candidates = [
        candidate("a", "a", 1),
        candidate("b", "b", 1),
        candidate("c", "c", 1),
    ];
    let mut shape = selection(&candidates, 2, Some(2));
    shape["groups"] = json!([
        {"group":0,"eligible":["a","b"],"bounds":{"min":1,"max":1}},
        {"group":1,"eligible":["b","c"],"bounds":{"min":1,"max":1}},
    ]);
    shape["group_assignment"] = json!("explicit");
    let grouped = question("search", &json!({"kind":"select","selection":shape}));
    for answer in answers(&grouped) {
        let answer = serde_json::to_value(&answer).unwrap();
        let (first, second) = (
            ids(&answer, "/groups/0/selected"),
            ids(&answer, "/groups/1/selected"),
        );
        assert_eq!((first.len(), second.len()), (1, 1), "{answer}");
        assert!(
            ["a", "b"].contains(&first[0].as_str()) && ["b", "c"].contains(&second[0].as_str())
        );
        assert_ne!(first, second);
        let mut selected = ids(&answer, "/selected");
        selected.sort();
        let mut assigned = [first, second].concat();
        assigned.sort();
        assert_eq!(selected, assigned);
    }

    // Minimums that only one assignment meets: `b` must leave the group a greedy pass gives it.
    let mut chained = selection(&candidates, 2, Some(2));
    chained["groups"] = json!([
        {"group":0,"eligible":["c"],"bounds":{"min":0,"max":1}},
        {"group":1,"eligible":["b","c"],"bounds":{"min":1,"max":1}},
        {"group":2,"eligible":["b"],"bounds":{"min":1,"max":1}},
    ]);
    chained["group_assignment"] = json!("explicit");
    let chain = question("search", &json!({"kind":"select","selection":chained}));
    for answer in answers(&chain) {
        let answer = serde_json::to_value(&answer).unwrap();
        assert_eq!(
            ids(&answer, "/groups/0/selected"),
            Vec::<String>::new(),
            "{answer}"
        );
        assert_eq!(ids(&answer, "/groups/1/selected"), ["c"], "{answer}");
        assert_eq!(ids(&answer, "/groups/2/selected"), ["b"], "{answer}");
    }

    let mut with_roles = selection(&candidates, 0, Some(3));
    with_roles["roles"] = json!([
        {"role":0,"size":{"kind":"if_nonempty","count":1},"destination":null},
        {"role":1,"size":{"kind":"remainder"},"destination":null},
    ]);
    let roles = question("targets", &json!({"kind":"select","selection":with_roles}));
    for answer in answers(&roles) {
        let answer = serde_json::to_value(&answer).unwrap();
        let selected = ids(&answer, "/selected");
        let (head, rest) = (
            ids(&answer, "/roles/0/selected"),
            ids(&answer, "/roles/1/selected"),
        );
        assert_eq!(head.len(), usize::from(!selected.is_empty()), "{answer}");
        assert_eq!(head.len() + rest.len(), selected.len(), "{answer}");
        assert!(head.iter().all(|id| !rest.contains(id)));
    }
}

#[test]
fn a_selection_without_any_answer_is_reported_and_proven_when_small() {
    let same: Vec<Value> = (0..3)
        .map(|index| candidate(&format!("c{index}"), "same", 1))
        .collect();
    let mut shape = selection(&same, 2, None);
    shape["distinct"] = json!(["rules_name"]);
    let small = question("targets", &json!({"kind":"select","selection":shape}));
    let mut rng = ChaCha8Rng::seed_from_u64(0);
    assert_eq!(choose(&small, &mut rng), Err(NoAnswer::Proven));

    let many: Vec<Value> = (0..20)
        .map(|index| candidate(&format!("c{index}"), "same", 1))
        .collect();
    let mut crowded = selection(&many, 2, None);
    crowded["distinct"] = json!(["rules_name"]);
    let large = question("targets", &json!({"kind":"select","selection":crowded}));
    assert_eq!(choose(&large, &mut rng), Err(NoAnswer::NotFound));

    // A constraint only an exact subset meets is still found once sampling gives up.
    let costs: Vec<Value> = (0..10)
        .map(|index| candidate(&format!("c{index}"), &format!("n{index}"), 1 << index))
        .collect();
    let mut priced = selection(&costs, 1, None);
    priced["original_cost_total"] = json!({"min":853,"max":853});
    let exact = question(
        "cost_material",
        &json!({"kind":"select","selection":priced}),
    );
    let Answer::Select { selected, .. } = choose(&exact, &mut rng).unwrap() else {
        panic!("a selection answers with a selection");
    };
    assert_eq!(selected.len(), 6);
}

#[test]
fn a_partition_fills_every_bucket_within_its_bounds() {
    let items: Vec<Value> = (0..5)
        .map(|index| candidate(&format!("i{index}"), "x", 1))
        .collect();
    let bucket = |index: u64, min: u64, max: Option<u64>| json!({"bucket":index,"destination":null,"bounds":{"min":min,"max":max},"ordered":false});
    let asked = question(
        "search_partition",
        &json!({"kind":"partition","items":items,"buckets":[bucket(0, 2, Some(2)), bucket(1, 0, None)]}),
    );
    for answer in answers(&asked) {
        let answer = serde_json::to_value(&answer).unwrap();
        let (first, second) = (
            ids(&answer, "/buckets/0/items"),
            ids(&answer, "/buckets/1/items"),
        );
        assert_eq!((first.len(), second.len()), (2, 3), "{answer}");
        let all: BTreeSet<String> = first.into_iter().chain(second).collect();
        assert_eq!(all.len(), 5);
    }
    let short = question(
        "search_partition",
        &json!({"kind":"partition","items":items,"buckets":[bucket(0, 6, None)]}),
    );
    assert_eq!(
        choose(&short, &mut ChaCha8Rng::seed_from_u64(0)),
        Err(NoAnswer::Proven)
    );
}

#[test]
fn a_distribution_reaches_its_total_within_every_share() {
    let recipients: Vec<Value> = (0..3)
        .map(|index| candidate(&format!("r{index}"), "x", 1))
        .collect();
    let shape = |members: &[Value], total: u64, empty: &str| {
        json!({"kind":"distribute","recipients":members,"total":total,
            "share":{"min":1,"max":3},"empty":empty})
    };
    let asked = question("damage_distribution", &shape(&recipients, 7, "reject"));
    for answer in answers(&asked) {
        let answer = serde_json::to_value(&answer).unwrap();
        let amounts: Vec<u64> = answer["shares"]
            .as_array()
            .unwrap()
            .iter()
            .map(|share| share["amount"].as_u64().unwrap())
            .collect();
        assert_eq!(amounts.len(), 3);
        assert_eq!(amounts.iter().sum::<u64>(), 7, "{answer}");
        assert!(
            amounts.iter().all(|amount| (1..=3).contains(amount)),
            "{answer}"
        );
    }
    let mut rng = ChaCha8Rng::seed_from_u64(0);
    let unreachable = question("damage_distribution", &shape(&recipients, 10, "reject"));
    assert_eq!(choose(&unreachable, &mut rng), Err(NoAnswer::Proven));
    let nobody = question("damage_distribution", &shape(&[], 2, "reject"));
    assert_eq!(choose(&nobody, &mut rng), Err(NoAnswer::Proven));
    let allowed = question("damage_distribution", &shape(&[], 2, "allow_without_total"));
    assert_eq!(
        choose(&allowed, &mut rng),
        Ok(Answer::Distribute { shares: Vec::new() })
    );
}

#[test]
fn each_simple_shape_answers_inside_its_listed_domain() {
    let option = |id: &str| json!({"id":id,"value":{"kind":"seat","player":"p1"},"cost":null});
    let cases = [
        (
            "modes",
            json!({"kind":"choose","options":[option("a"), option("b"), option("c")],
            "bounds":{"min":1,"max":3},"count":2,"earlier":[],"capacity":null}),
        ),
        (
            "mulligan",
            json!({"kind":"mulligan","hand":[candidate("h1", "x", 1), candidate("h2", "y", 1)]}),
        ),
        (
            "main_action",
            json!({"kind":"action_window","intents":[
            {"kind":"attack","id":"atk","attacker":"o1","targets":[{"id":"t1","target":{"kind":"leader","player":"p2"}}]},
            {"kind":"end_phase","id":"end"}]}),
        ),
        (
            "quick",
            json!({"kind":"quick","window":"attack","options":[]}),
        ),
        (
            "evolution_payment",
            json!({"kind":"evolution_payment","payments":[{"pp":1,"use_ep":false,"super_evolution":false}]}),
        ),
        (
            "resource_entry_payment",
            json!({"kind":"resource_payment","materials":selection(&[candidate("m", "m", 1)], 1, Some(1)),"ep_allowed":true}),
        ),
        (
            "may_effect",
            json!({"kind":"confirm","allowed":[true],"subject":null,"origin":null}),
        ),
        (
            "declare_number",
            json!({"kind":"integer","bounds":{"min":2,"max":4},"unit":null,"source":null}),
        ),
        (
            "declare_x",
            json!({"kind":"integer","bounds":{"min":2,"max":null},"unit":null,"source":null}),
        ),
        (
            "dice_reroll",
            json!({"kind":"dice_reroll","value":3,"remaining":1,"allowed":["keep","reroll"]}),
        ),
        (
            "order_cards",
            json!({"kind":"order","items":[{"id":"x","subject":null,"source":null},{"id":"y","subject":null,"source":null}],
            "direction":"top_down","destination":null}),
        ),
        (
            "drive_trigger",
            json!({"kind":"drive_trigger","icon":"critical","allowed":[true],
            "targets":[candidate("f", "f", 1)],"target_bounds":{"min":1,"max":1}}),
        ),
    ];
    for (usage, shape) in cases {
        let asked = question(usage, &shape);
        assert!(asked.is_coherent(), "{usage}");
        for answer in answers(&asked) {
            let answer = serde_json::to_value(&answer).unwrap();
            match usage {
                "modes" => assert!((1..=2).contains(&ids(&answer, "/chosen").len()), "{answer}"),
                "mulligan" => assert_eq!(
                    ids(&answer, "/bottom_order").len(),
                    if answer["redraw"] == true { 2 } else { 0 }
                ),
                "main_action" => assert_eq!(answer["target"] == "t1", answer["intent"] == "atk"),
                "quick" => assert_eq!(answer["option"], Value::Null),
                "evolution_payment" => assert_eq!(answer["pp"], 1),
                "resource_entry_payment" => assert_eq!(answer["materials"], json!(["m"])),
                "may_effect" => assert_eq!(answer["accept"], true),
                "declare_number" => assert!((2..=4).contains(&answer["value"].as_u64().unwrap())),
                "declare_x" => assert!(answer["value"].as_u64().unwrap() >= 2),
                "dice_reroll" => {
                    assert!(["keep", "reroll"].contains(&answer["choice"].as_str().unwrap()));
                }
                "order_cards" => assert_eq!(ids(&answer, "/order").len(), 2),
                _ => assert_eq!(answer["target"], "f"),
            }
        }
    }
    let empty = question("main_action", &json!({"kind":"action_window","intents":[]}));
    assert_eq!(
        choose(&empty, &mut ChaCha8Rng::seed_from_u64(0)),
        Err(NoAnswer::Proven)
    );
}

#[test]
fn a_trigger_that_cannot_be_accepted_still_has_its_listed_refusal() {
    // A trigger whose acceptance needs a missing target still has its listed refusal.
    let refusal = question(
        "drive_trigger",
        &json!({"kind":"drive_trigger","icon":"critical","allowed":[true, false],
            "targets":[],"target_bounds":{"min":1,"max":1}}),
    );
    for answer in answers(&refusal) {
        assert_eq!(
            answer,
            Answer::DriveTrigger {
                accept: false,
                target: None
            }
        );
    }
}
