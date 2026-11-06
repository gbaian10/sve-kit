#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "The printed role scenario must ask only its declared decisions."
)]
use super::*;
use sve_engine::game::internal::TargetRoleAssignment;

fn advance_producer() -> WellFormedCardProgram {
    let mut tree = expanded::tree().clone();
    let mut source = tree["cards"]["BP01-041"].clone();
    source["abilities"][0]["activate"] = json!({
        "advance":true,"cost":{"steps":[{"pp":4_u8},{"banish_self":true}]},
        "do":[
            {"choose":{"bind":"incoming","from":{"who":"self","zone":"evolve_deck", "kind":"follower",
                "advance_follower":true,"name":"楽園の終焉・イツルギ"},"n":1_u8,"may":true}},
            {"move":{"ref":"incoming","to":"field"}}
        ]
    });
    tree["cards"] = json!({"producer":source});
    lowerer().lower(&tree).unwrap().remove("producer").unwrap()
}

fn enter(game: &mut Game) {
    let producer = advance_producer();
    let id = producer.card().abilities[0].node.id;
    game.load_internal_card("producer", &producer).unwrap();
    let mut play = game
        .activate_internal_card("producer", id, PlayerId { seat: 1 })
        .unwrap();
    while let CardPlay::Awaiting(q) = play {
        let answer = match q.domain {
            InputDomain::ResourceEntryPayment { .. } => Answer::ResourceEntryPayment {
                materials: vec![],
                use_ep: false,
            },
            InputDomain::Targets(domain) => Answer::Targets(vec![domain.members[0].handle]),
            other => panic!("unexpected entry question: {other:?}"),
        };
        play = game.answer_internal_card(q.actor, q.token, answer).unwrap();
    }
    game.resume_internal_card().unwrap();
}

#[test]
fn bp14_071_roles_deal_five_to_the_chosen_primary_and_two_to_the_remainder() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"producer","card":"producer"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"source","card":"BP14-071"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"first","card":"BP01-037"}, {"id":"primary","card":"BP01-037"},
        {"id":"unselected","card":"BP01-037"}
    ]);
    let mut game = Game::from_selected_effects(
        producer_selection().clone(),
        &initial,
        &Value::Null,
        &Value::Null,
        "printed-target-role",
    )
    .unwrap();
    enter(&mut game);
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = pending.domain else {
        panic!("fanfare pending")
    };
    let handle = options
        .iter()
        .find(|option| option.card == "BP14-071" && option.line == 2)
        .unwrap()
        .handle;
    let CardPlay::Awaiting(question) = game
        .answer_internal_card(pending.actor, pending.token, Answer::Pending(handle))
        .unwrap()
    else {
        panic!("role question")
    };
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("role targets")
    };
    let primary = domain
        .roles
        .iter()
        .find(|role| matches!(role.cardinality, RoleCardinality::IfNonempty { .. }))
        .unwrap()
        .role;
    let rest = domain
        .roles
        .iter()
        .find(|role| matches!(role.cardinality, RoleCardinality::Remainder {}))
        .unwrap()
        .role;
    resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::TargetsWithRoles {
                targets: vec![0, 1],
                roles: vec![
                    TargetRoleAssignment {
                        role: rest,
                        targets: vec![0],
                    },
                    TargetRoleAssignment {
                        role: primary,
                        targets: vec![1],
                    },
                ],
            },
        )
        .unwrap(),
    );
    game.resume_internal_card().unwrap();
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["cemetery"], json!(["primary"]));
    assert_eq!(state["objects"]["first"]["hp"], 3_u8);
    assert_eq!(state["objects"]["unselected"]["hp"], 5_u8);
    let cemetery_pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(cemetery_options) = cemetery_pending.domain else {
        panic!("cemetery trigger")
    };
    assert_eq!(cemetery_options.len(), 1);
    assert_eq!(cemetery_options[0].line, 1);
    resolved(
        game.answer_internal_card(
            cemetery_pending.actor,
            cemetery_pending.token,
            Answer::Pending(cemetery_options[0].handle),
        )
        .unwrap(),
    );
    let final_state = game.projection(View::Referee).unwrap();
    assert_eq!(final_state["P1"]["leader"]["life"], 21_u8);
    assert_eq!(final_state["P2"]["leader"]["life"], 19_u8);
}
