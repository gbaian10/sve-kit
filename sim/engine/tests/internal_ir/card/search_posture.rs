#![expect(
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected private search decisions invalidate the entry scenarios."
)]
use super::evolution_linked_movement::{loaded, tree};
use super::*;
use sve_engine::game::internal::{Fact, InputDomain, SearchAnswer};

fn input() -> Value {
    tree(&[(
        "activate",
        json!({"cost":{"pp":0},"do":[{"search":{
            "sel":{"who":"self","zone":"deck","kind":"follower","name":"unit-follower"},
            "n":1,"to":"field","acted":true
        }}]}),
    )])
}

#[test]
fn named_single_search_initializes_acted_before_entry_and_still_shuffles_on_capacity_failure() {
    let programs = lowerer().lower(&input()).unwrap();
    let program = &programs["unit-follower"];
    for full in [false, true] {
        let mut initial = setup(
            1,
            json!([{"id":"d0","card":"unit-follower"},{"id":"d1","card":"unit-follower"}]),
        );
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
        if full {
            initial["players"]["P1"]["zones"]["field"]
                .as_array_mut()
                .unwrap()
                .extend(
                    (0..4_u8).map(|n| json!({"id":format!("filler-{n}"),"card":"unit-follower"})),
                );
        }
        let mut game = loaded(&initial, program);
        let mut play = game
            .activate_internal_card(
                "s",
                program.card().abilities[0].node.id,
                PlayerId { seat: 1 },
            )
            .unwrap();
        while let CardPlay::Awaiting(question) = play {
            let answer = match &question.domain {
                InputDomain::Search(domain) => {
                    assert!(game.internal_decision(View::P2).is_none());
                    Answer::Search(SearchAnswer::Ungrouped(vec![domain.cards[0].handle]))
                }
                InputDomain::SearchPartition(_) => Answer::SearchPartition(vec![]),
                other => panic!("unexpected search entry choice {other:?}"),
            };
            play = game
                .answer_internal_card(question.actor, question.token, answer)
                .unwrap();
        }
        let result = resolved(play);
        assert_eq!(
            result
                .progress
                .facts
                .iter()
                .filter(|fact| matches!(fact, Fact::Shuffled { .. }))
                .count(),
            1
        );
        assert!(
            result
                .progress
                .facts
                .iter()
                .all(|fact| !matches!(fact, Fact::Posture(_)))
        );
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P1"]["field"]
                .as_array()
                .unwrap()
                .contains(&json!("d0")),
            !full
        );
        assert_eq!(state["objects"]["d0"]["acted"], !full);
        assert_eq!(state["objects"]["d0"]["generation"], u8::from(!full));
    }
}

#[test]
fn acted_search_rejects_other_quantities_destinations_and_false() {
    for patch in [json!({"n":2}), json!({"to":"hand"}), json!({"acted":false})] {
        let mut value = input();
        let search =
            &mut value["cards"]["unit-follower"]["abilities"][0]["activate"]["do"][0]["search"];
        search
            .as_object_mut()
            .unwrap()
            .extend(patch.as_object().unwrap().clone());
        search.as_object_mut().unwrap().remove("acted");
        lowerer().lower(&value).unwrap();
        let search =
            &mut value["cards"]["unit-follower"]["abilities"][0]["activate"]["do"][0]["search"];
        search["acted"] = patch.get("acted").cloned().unwrap_or(json!(true));
        lowerer().lower(&value).unwrap_err();
    }
    let mut value = input();
    let search =
        &mut value["cards"]["unit-follower"]["abilities"][0]["activate"]["do"][0]["search"];
    search.as_object_mut().unwrap().remove("n");
    search["upto"] = json!(1);
    lowerer().lower(&value).unwrap_err();
}

#[test]
fn raw_search_posture_cannot_admit_standing_other_zones_or_multiple_cards() {
    let programs = lowerer().lower(&input()).unwrap();
    let program = &programs["unit-follower"];
    for variant in 0..3_u8 {
        let mut raw = program.card().clone();
        let Node::Instruction { instruction } =
            &mut raw.abilities[0].node.body.as_mut().unwrap().steps[0].node
        else {
            panic!("search instruction")
        };
        let Action::Search {
            destination: Some(SearchDestination::Single { movement }),
            cardinality,
            ..
        } = &mut instruction.action
        else {
            panic!("single search")
        };
        match variant {
            0 => movement.initial_posture = Some(Posture::Standing),
            1 => {
                movement.to = MoveDestination::Owners {
                    zone: OwnerZone::Hand,
                }
            }
            _ => cardinality.maximum = Some(Nat(2)),
        }
        let mut game = loaded(&setup(1, json!([])), program);
        let before = format!("{game:?}");
        if let Ok(checked) = WellFormedCardProgram::check(raw) {
            game.load_internal_card("unit-follower", &checked)
                .unwrap_err();
        }
        assert_eq!(format!("{game:?}"), before);
    }
}
