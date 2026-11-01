use super::evolution_linked_movement::tree;
use super::*;
use alloc::collections::BTreeMap;
use sve_engine::game::internal::{InputDomain, SearchAnswer};

fn programs(kind: &str, from: &str) -> BTreeMap<String, WellFormedCardProgram> {
    let body = if from == "look" {
        json!({"cost":{"pp":0},"do":[
            {"look":{"top":{"who":"self","n":1},"bind":"seen"}},
            {"choose":{"bind":"incoming","from":{"in":"seen"},"n":1}},
            {"move":{"ref":"incoming","to":"field","result":"entered"}}
        ]})
    } else if from == "deck" {
        json!({"cost":{"pp":0},"do":[{"search":{
            "sel":{"who":"self","zone":"deck","name":kind},"n":1,"to":"field"
        }}]})
    } else {
        json!({"cost":{"pp":0},"targets":[{
            "bind":"incoming","from":{"who":"self","zone":"cemetery"},"n":1
        }],"do":[{"move":{"ref":"incoming","to":"field","result":"entered"}}]})
    };
    let mut tree = tree(&[("activate", body)]);
    let producer = tree["cards"]["unit-follower"].clone();
    let mut incoming = producer.clone();
    let mut header = incoming["abilities"][0].clone();
    header.as_object_mut().unwrap().remove("activate");
    let origin = if from == "look" { "deck" } else { from };
    header["fanfare"] = json!({"do":[
        {"if":{"cond":{"entered_from":{"ref":"this","from":origin}},"then":[{"draw":{"n":1}}]}},
        {"if":{"cond":{"entered_from":{"ref":"this","from":"hand"}},"then":[{"life":{"who":"self","delta":7}}]}}
    ]});
    incoming["abilities"] = json!([header]);
    tree["cards"]["unit-amulet"] = producer;
    tree["cards"][kind] = incoming;
    lowerer().lower(&tree).unwrap()
}

fn position(kind: &str, from: &str) -> (Game, AbilityId) {
    let programs = programs(kind, from);
    let zone = if from == "look" { "deck" } else { from };
    let mut initial = setup(1, json!([{"id":"drawn","card":"unit-spell"}]));
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"unit-amulet"}]);
    // For an entering amulet, the producer is the follower instead.
    if kind == "unit-amulet" {
        initial["players"]["P1"]["zones"]["field"][0]["card"] = json!("unit-follower");
    }
    initial["players"]["P1"]["zones"][zone] = json!([{
        "id":"incoming","card":kind,"state":{"entered_from":"hand"}
    }]);
    if zone == "deck" {
        initial["players"]["P1"]["zones"]["deck"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"drawn","card":"unit-spell"}));
    }
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "entry-origin",
    )
    .unwrap();
    for number in ["unit-follower", "unit-amulet"] {
        game.load_internal_card(number, &programs[number]).unwrap();
    }
    game.load_internal_card("unit-spell", &card(vec![]))
        .unwrap();
    let producer = if kind == "unit-amulet" {
        "unit-follower"
    } else {
        "unit-amulet"
    };
    let id = programs[producer].card().abilities[0].node.id;
    (game, id)
}

#[test]
fn deck_and_cemetery_entries_preserve_origin_for_followers_and_amulets() {
    for kind in ["unit-follower", "unit-amulet"] {
        for from in ["deck", "cemetery", "look"] {
            let (mut game, ability) = position(kind, from);
            let CardPlay::Awaiting(q) = game
                .activate_internal_card("source", ability, PlayerId { seat: 1 })
                .unwrap()
            else {
                panic!("entry selection")
            };
            let answer = if let InputDomain::Search(domain) = &q.domain {
                Answer::Search(SearchAnswer::Ungrouped(vec![domain.cards[0].handle]))
            } else {
                let InputDomain::Targets(domain) = &q.domain else {
                    panic!("selection domain")
                };
                Answer::Targets(vec![domain.members[0].handle])
            };
            resolved(game.answer_internal_card(q.actor, q.token, answer).unwrap());
            game.resume_internal_card().unwrap();
            let q = game.internal_pending_choice().unwrap().unwrap();
            let InputDomain::Pending(options) = &q.domain else {
                panic!("entry fanfare")
            };
            assert_eq!(options.len(), 1);
            assert!(options[0].event.contains(if from == "cemetery" {
                "from Cemetery"
            } else {
                "from Deck"
            }));
            resolved(
                game.answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
                    .unwrap(),
            );
            game.resume_internal_card().unwrap();
            let view = game.projection(View::Referee).unwrap();
            assert!(
                view["P1"]["field"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("incoming"))
            );
            assert_eq!(view["objects"]["incoming"]["generation"], 1);
            assert_eq!(view["P1"]["hand"], json!(["drawn"]));
            assert_eq!(
                view["P1"]["leader"]["life"], 20,
                "spoofed hand origin cannot select the other branch"
            );
            assert!(game.internal_pending_choice().unwrap().is_none());
        }
    }
}

#[test]
fn cemetery_spell_cannot_enter_and_rejected_movement_restores_the_transaction() {
    let (mut game, ability) = position("unit-spell", "cemetery");
    let before = game.projection(View::Referee).unwrap();
    let CardPlay::Awaiting(q) = game
        .activate_internal_card("source", ability, PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("target")
    };
    let InputDomain::Targets(domain) = &q.domain else {
        panic!("cemetery target")
    };
    let error = game
        .answer_internal_card(
            q.actor,
            q.token,
            Answer::Targets(vec![domain.members[0].handle]),
        )
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("admitted controller field entry")
    );
    assert_eq!(game.projection(View::Referee).unwrap(), before);
    assert!(game.internal_pending_choice().unwrap().is_none());
}
