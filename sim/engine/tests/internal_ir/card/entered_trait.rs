use super::*;
use sve_engine::game::internal::{InputDomain, SearchAnswer};

fn program(search: bool) -> WellFormedCardProgram {
    let mut body = json!({"cost":{"pp":0},"do":[]});
    if search {
        body["do"] = json!([
            {"search":{"sel":{"who":"self","zone":"deck","kind":"follower"},"n":1,"to":"field","result":"entered"}}
        ]);
    } else {
        body["targets"] = json!([
            {"bind":"picked","from":{"who":"self","zone":"cemetery","kind":"follower"},"n":1}
        ]);
        body["do"] = json!([{"move":{"ref":"picked","to":"field","result":"entered"}}]);
    }
    body["do"].as_array_mut().unwrap().extend([
        json!({"if":{"cond":{"exists":{"in":{"follow":"entered"},"kind":"follower","trait":"target-trait"}},"then":[
            {"buff":{"on":{"follow":"entered"},"power":1,"hp":1}},
            {"life":{"who":"self","delta":1}}
        ]}}),
        json!({"buff":{"on":"this","power":1,"hp":1}}),
    ]);
    lowerer()
        .lower(&evolution_linked_movement::tree(&[("activate", body)]))
        .unwrap()
        .remove("unit-follower")
        .unwrap()
}

fn trait_catalog(matched: bool) -> Arc<Catalog> {
    let (snapshot, keywords, effects) = catalog_documents(&json!("1"));
    let snapshot = snapshot
        .lines()
        .map(|line| {
            let mut row: Value = serde_json::from_str(line).unwrap();
            if row["number"] == "unit-follower" {
                row["faces"][0]["traits"] = json!([if matched {
                    "target-trait"
                } else {
                    "other-trait"
                }]);
            }
            row.to_string()
        })
        .collect::<Vec<_>>()
        .join("\n");
    Arc::new(
        Catalog::from_documents(&snapshot, &keywords, &[("unit.yaml".into(), effects)]).unwrap(),
    )
}

#[derive(Clone, Copy)]
enum EntryRoute {
    CemeteryTarget,
    DeckSearch,
}

#[expect(
    clippy::panic,
    reason = "The synthetic movement must ask for its public target or private search."
)]
fn run_case(seat: u32, matched: bool, route: EntryRoute, accept: bool, full: bool) {
    let search = matches!(route, EntryRoute::DeckSearch);
    let program = program(search);
    let actor = if seat == 1 { "P1" } else { "P2" };
    let mut initial = setup(seat, json!([]));
    initial["turn"]["active"] = json!(actor);
    initial["players"][actor]["zones"]["hand"] = json!([]);
    initial["players"][actor]["zones"]["field"] = json!([{"id":"source","card":"unit-follower"}]);
    if full {
        initial["players"][actor]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .extend((0_u8..4).map(|n| json!({"id":format!("old{n}"),"card":"unit-follower"})));
    }
    initial["players"][actor]["zones"][if search { "deck" } else { "cemetery" }] =
        json!([{"id":"recipient","card":"unit-follower"}]);
    let mut game = Game::new(
        trait_catalog(matched),
        &initial,
        &Value::Null,
        &Value::Null,
        "entry-trait",
    )
    .unwrap();
    game.load_internal_card("unit-follower", &program).unwrap();
    let CardPlay::Awaiting(question) = game
        .activate_internal_card(
            "source",
            program.card().abilities[0].node.id,
            PlayerId { seat },
        )
        .unwrap()
    else {
        panic!("entry selection");
    };
    let answer = if let InputDomain::Search(domain) = &question.domain {
        Answer::Search(SearchAnswer::Ungrouped(
            domain
                .cards
                .iter()
                .take(usize::from(accept))
                .map(|card| card.handle)
                .collect(),
        ))
    } else if let InputDomain::Targets(domain) = &question.domain {
        Answer::Targets(vec![domain.members[0].handle])
    } else {
        panic!("search or public target");
    };
    resolved(
        game.answer_internal_card(question.actor, question.token, answer)
            .unwrap(),
    );
    let view = game.projection(View::Referee).unwrap();
    let entered = accept && !full;
    let matched = entered && matched;
    assert_eq!(
        view[actor]["field"]
            .as_array()
            .unwrap()
            .contains(&json!("recipient")),
        entered,
        "only successful movement adds the recipient to the field"
    );
    assert_eq!(
        view[actor]["leader"]["life"],
        if matched { 21_u8 } else { 20_u8 },
        "only a matching successful entry satisfies the followed trait condition"
    );
    assert_eq!(
        view["objects"]["recipient"]["power"],
        if matched { 3_u8 } else { 2_u8 },
        "the followed buff applies only when the entry condition succeeds"
    );
    assert_eq!(
        view["objects"]["source"]["power"], 3_u8,
        "the later source action runs even without a matching entry"
    );
}

#[test]
fn entered_trait_condition_reads_only_successful_field_members() {
    for seat in [1_u32, 2] {
        for matched in [false, true] {
            for full in [false, true] {
                run_case(seat, matched, EntryRoute::CemeteryTarget, true, full);
                run_case(seat, matched, EntryRoute::DeckSearch, true, full);
            }
            run_case(seat, matched, EntryRoute::DeckSearch, false, false);
        }
    }
}
