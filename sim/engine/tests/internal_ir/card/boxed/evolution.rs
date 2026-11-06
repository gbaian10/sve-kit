use super::*;

fn input() -> Value {
    let mut input = tree();
    let mut evolve = input["cards"]["unit-follower"]["abilities"][1].clone();
    evolve["line"] = json!(9);
    evolve["activate"] = json!({"cost":{"pp":0},"targets":[{
        "bind":"ally","from":{"who":"self","zone":"field","kind":"follower"},"n":1
    }],"do":[{"evolve":{"ref":"ally"}}]});
    input["cards"]["unit-follower"]["abilities"]
        .as_array_mut()
        .unwrap()
        .push(evolve);
    let mut named = input["cards"]["unit-follower"]["abilities"][1].clone();
    named["line"] = json!(10);
    named["activate"] = json!({"cost":{"pp":0},"targets":[{
        "bind":"named","from":{"who":"self","zone":"field","kind":"follower","name":"new-name"},"n":1
    }],"do":[{"life":{"who":"self","delta":1}}]});
    input["cards"]["unit-follower"]["abilities"]
        .as_array_mut()
        .unwrap()
        .push(named);
    let mut form = input["cards"]["unit-follower"].clone();
    let mut trigger = form["abilities"][1].clone();
    trigger.as_object_mut().unwrap().remove("activate");
    trigger["line"] = json!(2);
    trigger["evolve_time"] = json!({"do":[{"life":{"who":"self","delta":1}}]});
    let mut alias = trigger.clone();
    alias.as_object_mut().unwrap().remove("evolve_time");
    alias["line"] = json!(3);
    alias["static"] = json!({"effects":[{"also_named":{"name":"new-name"}}]});
    form["abilities"] = json!([form["abilities"][0], trigger, alias]);
    input["cards"]["unit-form"] = form;
    input
}

#[test]
fn evolution_keeps_box_but_gains_the_new_printed_abilities_after_its_cutoff() {
    let input = input();
    let (mut game, card) = board_in_zones(
        &input,
        false,
        &json!({"P2":{
            "field":[{"id":"enemy","card":"unit-follower"},{"id":"helper","card":"unit-follower"}],
            "evolve_deck":[{"id":"form","card":"unit-form"}]
        }}),
    );
    activate(&mut game, &card, 3);
    activate(&mut game, &card, 2);
    assert_eq!(keywords(&game), json!([]));
    end_turn(&mut game);
    let id = card
        .card()
        .abilities
        .iter()
        .find(|ability| ability.source.line == 9)
        .unwrap()
        .node
        .id;
    let mut play = game
        .activate_internal_card("helper", id, PlayerId { seat: 2 })
        .unwrap();
    while let CardPlay::Awaiting(question) = play {
        let InputDomain::Targets(domain) = &question.domain else {
            panic!("evolution target or resource")
        };
        play = game
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Targets(vec![domain.members[0].handle]),
            )
            .unwrap();
    }
    resolved(play);
    game.resume_internal_card().unwrap();
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["boxed"], true);
    assert_eq!(state["objects"]["enemy"]["generation"], 0);
    assert_eq!(state["objects"]["enemy"]["keywords"], json!(["guard"]));
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("newly gained evolution trigger")
    };
    assert_eq!(options.len(), 1);
    resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap(),
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
        21
    );
    game.resume_internal_card().unwrap();
    let alias = card
        .card()
        .abilities
        .iter()
        .find(|ability| ability.source.line == 10)
        .unwrap()
        .node
        .id;
    let CardPlay::Awaiting(question) = game
        .activate_internal_card("helper", alias, PlayerId { seat: 2 })
        .unwrap()
    else {
        panic!("new printed alias")
    };
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("alias target")
    };
    assert_eq!(domain.members.len(), 1);
    resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Targets(vec![domain.members[0].handle]),
        )
        .unwrap(),
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
        22
    );
}
