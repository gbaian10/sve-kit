#![expect(
    clippy::panic,
    reason = "Synthetic movement fixtures require their declared decision domains."
)]
use super::*;
use sve_engine::game::internal::{InputDomain, SearchAnswer};

fn tree(from_deck: bool) -> Value {
    let mut tree = fanfare::expanded::tree().clone();
    let mut source = tree["cards"]["BP01-041"].clone();
    source["abilities"][0]["activate"] = if from_deck {
        json!({"cost":{"pp":0},"do":[
            {"search":{"sel":{"who":"self","zone":"deck","kind":"follower"},"n":1,"to":"field","result":"entered"}},
            {"buff":{"on":{"follow":"entered"},"power":1,"hp":1}},
            {"buff":{"on":"this","power":1,"hp":1}}
        ]})
    } else {
        json!({"cost":{"pp":0},"targets":[
            {"bind":"recipient","from":{"who":"self","zone":"cemetery","kind":"follower"},"n":1}
        ],"do":[
            {"move":{"ref":"recipient","to":"field","result":"entered"}},
            {"buff":{"on":{"follow":"entered"},"power":1,"hp":1}},
            {"buff":{"on":"this","power":1,"hp":1}}
        ]})
    };
    tree["cards"] = json!({"unit-follower":source});
    tree
}

fn position(program: &WellFormedCardProgram, from_deck: bool, full: bool) -> Game {
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"unit-follower"}]);
    if full {
        for n in 0_u8..4 {
            initial["players"]["P1"]["zones"]["field"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":format!("old-{n}"),"card":"unit-follower"}));
        }
    }
    let zone = if from_deck { "deck" } else { "cemetery" };
    initial["players"]["P1"]["zones"][zone] = json!([{"id":"recipient","card":"unit-follower"}]);
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "move-followup",
    )
    .unwrap();
    game.load_internal_card("unit-follower", program).unwrap();
    game
}

fn run_case(from_deck: bool, accept: bool, full: bool) {
    let program = lowerer()
        .lower(&tree(from_deck))
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    let mut game = position(&program, from_deck, full);
    let play = game
        .activate_internal_card("source", AbilityId(1), PlayerId { seat: 1 })
        .unwrap();
    let CardPlay::Awaiting(q) = play else {
        panic!("movement asks for its target or private search even when the field is full");
    };
    let answer = if let InputDomain::Search(domain) = &q.domain {
        Answer::Search(SearchAnswer::Ungrouped(
            domain
                .cards
                .iter()
                .take(usize::from(accept))
                .map(|c| c.handle)
                .collect(),
        ))
    } else if let InputDomain::Targets(domain) = &q.domain {
        assert_eq!(
            domain.members.len(),
            1,
            "the declared cemetery target remains selectable"
        );
        Answer::Targets(vec![domain.members[0].handle])
    } else {
        panic!("declared movement decision");
    };
    resolved(game.answer_internal_card(q.actor, q.token, answer).unwrap());
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(
        view["objects"]["source"]["power"], 3_u8,
        "the independent source buff always resolves"
    );
    assert_eq!(
        view["objects"]["source"]["hp"], 4_u8,
        "an empty follow does not skip later instructions"
    );
    let entered = view["P1"]["field"]
        .as_array()
        .unwrap()
        .contains(&json!("recipient"));
    assert_eq!(
        entered,
        accept && !full,
        "only a selected successful movement creates a field member"
    );
    if entered {
        assert_eq!(
            view["objects"]["recipient"]["power"], 3_u8,
            "the actual entry gets attack plus one"
        );
        assert_eq!(
            view["objects"]["recipient"]["hp"], 4_u8,
            "the actual entry gets life plus one"
        );
        assert_eq!(
            view["objects"]["recipient"]["generation"], 1_u8,
            "the modification follows the new generation"
        );
    } else {
        assert_eq!(
            view["objects"]["recipient"]["power"], 2_u8,
            "an unmoved card receives no modification"
        );
        assert_eq!(
            view["objects"]["recipient"]["generation"], 0_u8,
            "failed movement does not change generation"
        );
    }
}

#[test]
fn moved_follower_receives_numeric_changes_only_after_successful_entry() {
    for full in [false, true] {
        run_case(false, true, full);
    }
}

#[test]
fn searched_follower_receives_numeric_changes_and_no_find_still_buffs_the_source() {
    for (accept, full) in [(true, false), (false, false), (true, true)] {
        run_case(true, accept, full);
    }
}

fn reject_follow_at(input: &Value, index: usize, label: &str) {
    let mut prefix = input.clone();
    prefix["cards"]["unit-follower"]["abilities"][0]["activate"]["do"]
        .as_array_mut()
        .unwrap()
        .truncate(index);
    lowerer()
        .lower(&prefix)
        .unwrap_or_else(|error| panic!("{label} producer: {error}"));
    let error = lowerer().lower(input).unwrap_err();
    assert_eq!(
        error.code, "unsupported_construct",
        "{label}: the follow capability is not authorized"
    );
    assert!(
        error.path.ends_with(&format!("/do/{index}")),
        "{label}: the unsupported step is the consumer, not its valid producer: {}",
        error.path
    );
}

#[test]
fn numeric_follow_rejects_hidden_destinations_unknown_names_and_escaped_bindings() {
    for destination in ["hand", "cemetery", "deck"] {
        let mut input = tree(true);
        let mut movement =
            json!({"move":{"ref":{"in":"seen"},"to":destination,"result":"entered"}});
        if destination == "deck" {
            movement["move"]["position"] = json!("bottom");
            movement["move"]["order"] = json!("chooser");
        }
        input["cards"]["unit-follower"]["abilities"][0]["activate"]["do"] = json!([
            {"look":{"bind":"seen","top":{"who":"self","n":1}}},
            movement,
            {"buff":{"on":{"follow":"entered"},"power":1}}
        ]);
        let index = if destination == "hand" {
            input["cards"]["unit-follower"]["abilities"][0]["activate"]["do"] = json!([
                {"look":{"bind":"seen","top":{"who":"self","n":1}}},
                {"choose":{"bind":"picked","from":{"in":"seen","kind":"follower"},"may":true,"n":1}},
                {"move":{"ref":"picked","to":"hand","result":"entered"}},
                {"buff":{"on":{"follow":"entered"},"power":1}}
            ]);
            3
        } else {
            2
        };
        reject_follow_at(&input, index, destination);
    }
    let mut unknown = tree(false);
    unknown["cards"]["unit-follower"]["abilities"][0]["activate"]["do"][1]["buff"]["on"] =
        json!({"follow":"unknown"});
    reject_follow_at(&unknown, 1, "unknown name");
    let mut escaped = tree(false);
    let steps = &mut escaped["cards"]["unit-follower"]["abilities"][0]["activate"]["do"];
    let movement = steps[0].clone();
    steps[0] = json!({"optional":{"do":[movement]}});
    reject_follow_at(&escaped, 1, "escaped optional result");
}

#[test]
fn moving_a_follower_to_the_field_does_not_authorize_effect_evolution() {
    let mut input = tree(false);
    input["cards"]["unit-follower"]["abilities"][0]["activate"]["do"][1] =
        json!({"evolve":{"ref":{"follow":"entered"}}});
    reject_follow_at(&input, 1, "move does not authorize evolution");
}

#[test]
fn field_entry_numeric_follow_does_not_authorize_unrelated_object_actions() {
    let mut input = tree(false);
    input["cards"]["unit-follower"]["abilities"][0]["activate"]["do"][1] =
        json!({"destroy":{"follow":"entered"}});
    reject_follow_at(&input, 1, "numeric follow does not authorize destroy");
}
