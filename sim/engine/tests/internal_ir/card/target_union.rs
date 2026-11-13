#![expect(
    clippy::panic,
    reason = "An unexpected question invalidates the synthetic union scenario."
)]
use super::*;
use alloc::collections::BTreeMap;
use sve_engine::game::internal::{DecisionView, InputDomain};

/// Synthetic followers: `alpha` and `beta` carry one trait each, `both` carries both.
const FOLLOWERS: [(&str, &str, &str, &str); 4] = [
    ("alpha", "1", "2", "A"),
    ("beta", "2", "5", "B"),
    ("both", "2", "2", "A・B"),
    ("plain", "1", "2", ""),
];

fn catalog() -> Arc<Catalog> {
    let mut lines = vec![json!({"number":"unit-spell","faces":[{"name":"unit-spell","card_class":"ニュートラル","card_type":"スペル","cost":"1","power":"-","hp":"-","traits":[],"text":null,"sections":[]}]}).to_string()];
    let mut cards = serde_json::Map::new();
    cards.insert(
        "unit-spell".into(),
        json!({"status":"complete","review":"synthetic","abilities":[]}),
    );
    for (number, cost, power, traits) in FOLLOWERS {
        let traits: Vec<&str> = traits.split('・').filter(|t| !t.is_empty()).collect();
        lines.push(json!({"number":number,"faces":[{"name":number,"card_class":"ニュートラル","card_type":"フォロワー","cost":cost,"power":power,"hp":"3","traits":traits,"text":null,"sections":[]}]}).to_string());
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

fn spell(targets: &Value, steps: &Value) -> Value {
    let mut tree = fanfare::expanded::tree().clone();
    let mut entry = tree["cards"]["BP08-038"]["abilities"][0].clone();
    entry.as_object_mut().unwrap().remove("fanfare");
    entry["spell"] = json!({"targets":targets,"do":steps});
    let meta = tree["cards"]["BP08-038"]["meta"].clone();
    tree["cards"] = json!({"unit-spell":{"meta":meta,"abilities":[entry]}});
    tree
}

fn lower(tree: &Value) -> Result<WellFormedCardProgram, lowering::LoweringError> {
    lowerer()
        .lower_with_printed_costs(tree, &BTreeMap::from([("unit-spell".to_owned(), Nat(1))]))
        .map(|mut cards| cards.remove("unit-spell").unwrap())
}

fn loaded(program: &WellFormedCardProgram, zones: &Value) -> Game {
    let mut initial = setup(1, json!([]));
    initial["players"]["P2"]["zones"]["field"] = json!([]);
    for (seat, seat_zones) in zones.as_object().unwrap() {
        for (zone, objects) in seat_zones.as_object().unwrap() {
            initial["players"][seat]["zones"][zone] = objects.clone();
        }
    }
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "target-union",
    )
    .unwrap();
    game.load_internal_card("unit-spell", program).unwrap();
    for (number, ..) in FOLLOWERS {
        game.load_internal_card(number, &card(vec![])).unwrap();
    }
    game
}

/// Sorted printed numbers offered by the first target question.
fn offered(game: &mut Game) -> (Vec<String>, DecisionView) {
    let CardPlay::Awaiting(question) = game.play_internal_card("s", PlayerId { seat: 1 }).unwrap()
    else {
        panic!("expected a target question")
    };
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("expected targets")
    };
    let mut cards: Vec<String> = domain
        .members
        .iter()
        .map(|member| member.card.clone().unwrap())
        .collect();
    cards.sort();
    (cards, question)
}

fn object(id: &str, number: &str) -> Value {
    json!({"id":id,"card":number})
}

#[test]
fn a_card_matching_several_branches_is_offered_once() {
    let program = lower(&spell(
        &json!([{"bind":"chosen","n":1,"from":{"any_of":[
            {"who":"self","zone":"field","kind":"follower","trait":"A"},
            {"who":"self","zone":"field","kind":"follower","trait":"B"}
        ]}}]),
        &json!([{"buff":{"on":"chosen","power":1,"hp":0}}]),
    ))
    .unwrap();
    let mut game = loaded(
        &program,
        &json!({"P1":{"field":[object("a","alpha"),object("b","beta"),object("ab","both"),object("p","plain")]}}),
    );
    let (cards, _) = offered(&mut game);
    assert_eq!(cards, ["alpha", "beta", "both"]);
}

#[test]
fn groups_narrow_every_branch_without_dropping_its_filter() {
    let program = lower(&spell(
        &json!([{"bind":"chosen","upto":2,
            "from":{"any_of":[
                {"who":"self","zone":"cemetery","kind":"follower","trait":"A"},
                {"who":"self","zone":"cemetery","kind":"follower","trait":"B"}
            ]},
            "groups":[{"orig_cost_eq":1},{"orig_cost_eq":2}]}]),
        &json!([{"move":{"ref":"chosen","to":"hand"}}]),
    ))
    .unwrap();
    let mut game = loaded(
        &program,
        &json!({"P1":{"cemetery":[object("a","alpha"),object("b","beta"),object("p","plain")]}}),
    );
    let (cards, question) = offered(&mut game);
    assert_eq!(
        cards,
        ["alpha", "beta"],
        "the traitless follower is never offered"
    );
    resolved(
        game.answer_internal_card(question.actor, question.token, Answer::Targets(vec![0, 1]))
            .unwrap(),
    );
    let state = game.projection(View::Referee).unwrap();
    let mut hand: Vec<String> = serde_json::from_value(state["P1"]["hand"].clone()).unwrap();
    hand.sort();
    assert_eq!(hand, ["a", "b"]);
}

#[test]
fn an_extremum_branch_composes_with_an_ordinary_branch() {
    let strongest = json!({"who":"opp","zone":"field","kind":"follower","where":{"cmp":{
        "left":{"power_of":"item"},"op":"eq",
        "right":{"agg":{"of":{"who":"opp","zone":"field","kind":"follower"},"attr":"power","fn":"max"}}
    }}});
    let program = lower(&spell(
        &json!([{"bind":"chosen","n":1,"from":{"any_of":[
            strongest,
            {"who":"opp","zone":"field","kind":"follower","trait":"A"}
        ]}}]),
        &json!([{"destroy":"chosen"}]),
    ))
    .unwrap();
    let mut game = loaded(
        &program,
        &json!({"P2":{"field":[object("a","alpha"),object("b","beta"),object("p","plain")]}}),
    );
    let (cards, _) = offered(&mut game);
    assert_eq!(
        cards,
        ["alpha", "beta"],
        "the weak traitless follower is excluded"
    );
}

fn power_damage(zone: &Value, steps: &Value, enemy: &Value) -> u64 {
    let program = lower(&spell(&json!([{"bind":"chosen","n":1,"from":zone}]), steps)).unwrap();
    let mut game = loaded(&program, enemy);
    let (_, question) = offered(&mut game);
    resolved(
        game.answer_internal_card(question.actor, question.token, Answer::Targets(vec![0]))
            .unwrap(),
    );
    let life = game.projection(View::Referee).unwrap()["P2"]["leader"]["life"]
        .as_u64()
        .unwrap();
    20_u64.checked_sub(life).unwrap()
}

#[test]
fn power_of_reads_an_enemy_target_after_earlier_buffs_and_zero_once_it_leaves() {
    let enemy = json!({"who":"opp","zone":"field","kind":"follower"});
    let field = json!({"P2":{"field":[object("e","alpha")]}});
    let read = json!({"damage":{"to":{"leader":"opp"},"n":{"power_of":"chosen"}}});
    assert_eq!(
        power_damage(
            &enemy,
            &json!([{"buff":{"on":"chosen","power":3,"hp":0}}, read]),
            &field
        ),
        5
    );
    assert_eq!(
        power_damage(&enemy, &json!([{"destroy":"chosen"}, read]), &field),
        0
    );
}

#[test]
fn power_of_reads_the_value_an_ex_follower_currently_keeps() {
    let own_ex = json!({"who":"self","zone":"ex","kind":"follower"});
    let ex = json!({"P1":{"ex":[{"id":"x","card":"beta","state":{"power":7,"hp":3,"max_hp":3}}]}});
    assert_eq!(
        power_damage(
            &own_ex,
            &json!([{"damage":{"to":{"leader":"opp"},"n":{"power_of":"chosen"}}}]),
            &ex
        ),
        7
    );
}

fn grouped(branches: &Value, groups: &Value) -> WellFormedCardProgram {
    lower(&spell(
        &json!([{"bind":"chosen","upto":2,"from":{"any_of":branches},"groups":groups}]),
        &json!([{"move":{"ref":"chosen","to":"hand"}}]),
    ))
    .unwrap()
}

#[test]
fn group_conditions_and_branch_conditions_must_both_hold() {
    let program = grouped(
        &json!([
            {"who":"self","zone":"cemetery","kind":"follower","trait":"A","orig_cost_le":3},
            {"who":"self","zone":"cemetery","kind":"follower","trait":"B","orig_cost_le":3}
        ]),
        &json!([{"orig_cost_eq":1},{"orig_cost_eq":2,"trait":"B"}]),
    );
    let mut game = loaded(
        &program,
        &json!({"P1":{"cemetery":[object("a","alpha"),object("b","beta"),object("ab","both"),object("p","plain")]}}),
    );
    let (cards, _) = offered(&mut game);
    assert_eq!(cards, ["alpha", "beta", "both"]);
}

#[test]
fn a_branch_excluding_the_source_still_feeds_groups() {
    let program = grouped(
        &json!([
            {"who":"self","zone":"cemetery","kind":"follower","trait":"A","other":true},
            {"who":"self","zone":"cemetery","kind":"follower","trait":"B"}
        ]),
        &json!([{"orig_cost_eq":1},{"orig_cost_eq":2}]),
    );
    let mut game = loaded(
        &program,
        &json!({"P1":{"cemetery":[object("a","alpha"),object("b","beta"),object("p","plain")]}}),
    );
    let (cards, _) = offered(&mut game);
    assert_eq!(cards, ["alpha", "beta"]);
}

#[test]
fn a_union_needs_two_object_branches() {
    let follower = json!({"who":"opp","zone":"field","kind":"follower"});
    for branches in [json!([follower]), json!([follower, {"leader":"opp"}])] {
        lower(&spell(
            &json!([{"bind":"chosen","n":1,"from":{"any_of":branches}}]),
            &json!([{"destroy":"chosen"}]),
        ))
        .unwrap_err();
    }
}
