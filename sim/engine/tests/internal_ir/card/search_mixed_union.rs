use super::*;
use alloc::collections::BTreeMap;
use sve_engine::game::internal::{InputDomain, SearchAnswer};

fn programs() -> BTreeMap<String, WellFormedCardProgram> {
    let mut input = same_name::tree(
        &json!({"activate":{"cost":{"pp":0},"do":[{"search":{
            "sel":{"any_of":[
                {"who":"self","zone":"deck","kind":"follower","keyword":"earth_rite","orig_cost_le":1},
                {"who":"self","zone":"deck","kind":"amulet","trait":"sigil","orig_cost_le":1}
            ]},"n":1,"to":"field"
        }}]}}),
        "unit-amulet",
    );
    let rite = same_name::tree(
        &json!({"activate":{
            "cost":{"earth_rite":1},"do":[{"life":{"who":"self","delta":1}}]
        }}),
        "unit-follower",
    )["cards"]["unit-follower"]
        .clone();
    input["cards"]["unit-follower"] = rite.clone();
    input["cards"]["expensive-rite"] = rite;
    lowerer().lower(&input).unwrap()
}

fn catalogue() -> Arc<Catalog> {
    let (snapshot, keywords, document) = catalog_documents(&json!("1"));
    let mut rows = snapshot
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    let amulet = rows
        .iter_mut()
        .find(|row| row["number"] == "unit-amulet")
        .unwrap();
    amulet["faces"][0]["traits"] = json!(["sigil"]);
    let follower = rows
        .iter()
        .find(|row| row["number"] == "unit-follower")
        .unwrap()
        .clone();
    let amulet = rows
        .iter()
        .find(|row| row["number"] == "unit-amulet")
        .unwrap()
        .clone();
    let mut document: Value = serde_json::from_str(&document).unwrap();
    for (number, template, cost) in [
        ("plain-follower", follower.clone(), "1"),
        ("plain-amulet", amulet, "1"),
        ("expensive-rite", follower, "2"),
    ] {
        let mut row = template;
        row["number"] = json!(number);
        row["faces"][0]["name"] = json!(number);
        row["faces"][0]["traits"] = json!([]);
        row["faces"][0]["cost"] = json!(cost);
        rows.push(row);
        document["cards"][number] =
            json!({"status":"complete","review":"synthetic","abilities":[]});
    }
    let snapshot = rows
        .iter()
        .map(Value::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    Arc::new(
        Catalog::from_documents(
            &snapshot,
            &keywords,
            &[("synthetic.yaml".into(), document.to_string())],
        )
        .unwrap(),
    )
}

fn board(programs: &BTreeMap<String, WellFormedCardProgram>) -> Game {
    let mut initial = setup(
        1,
        json!([
            {"id":"rite","card":"unit-follower"},
            {"id":"sigil","card":"unit-amulet"},
            {"id":"plain","card":"plain-follower"},
            {"id":"amulet","card":"plain-amulet"},
            {"id":"expensive","card":"expensive-rite"},
            {"id":"spell","card":"unit-spell"}
        ]),
    );
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"reader","card":"unit-amulet"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([]);
    let mut game = Game::new(
        catalogue(),
        &initial,
        &Value::Null,
        &Value::Null,
        "mixed-search-union",
    )
    .unwrap();
    for (number, program) in programs {
        game.load_internal_card(number, program).unwrap();
    }
    for number in ["unit-spell", "plain-follower", "plain-amulet"] {
        game.load_internal_card(number, &card(vec![])).unwrap();
    }
    game
}

#[test]
fn author_search_union_combines_follower_keyword_and_amulet_trait_branches() {
    let programs = programs();
    for selected in ["unit-follower", "unit-amulet"] {
        let mut game = board(&programs);
        let CardPlay::Awaiting(question) = game
            .activate_internal_card(
                "reader",
                programs["unit-amulet"].card().abilities[0].node.id,
                PlayerId { seat: 1 },
            )
            .unwrap()
        else {
            panic!("search question");
        };
        let InputDomain::Search(domain) = &question.domain else {
            panic!("search domain");
        };
        assert_eq!(
            domain.cards.len(),
            2,
            "the keywordless follower, wrong-trait amulet, expensive rite and spell are excluded"
        );
        assert!(
            game.internal_decision(View::P2).is_none(),
            "private search candidates stay hidden from the opponent"
        );
        let handle = domain
            .cards
            .iter()
            .find(|card| card.card == selected)
            .unwrap()
            .handle;
        resolved(
            game.answer_internal_card(
                question.actor,
                question.token,
                Answer::Search(SearchAnswer::Ungrouped(vec![handle])),
            )
            .unwrap(),
        );
        game.resume_internal_card().unwrap();
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(
            view["P1"]["field"],
            json!([
                "reader",
                if selected == "unit-follower" {
                    "rite"
                } else {
                    "sigil"
                }
            ])
        );
    }
}
