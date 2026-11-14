#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate these synthetic departures."
)]
//! 「場から墓場に置かれたとき、それを場に出す」 needs a header that guarantees the cemetery.
use super::*;
use sve_engine::game::internal::{InputDomain, ResumeStage};

fn observer(to: &str, gate: bool) -> Value {
    let mut sel = json!({"kind":"follower","other":true});
    if gate {
        sel["where"] =
            json!({"cmp":{"left":{"count":{"who":"self","zone":"cemetery"}},"op":"ge","right":10}});
    }
    same_name::tree(
        &json!({"line":1,"auto":{"on":{"moved":{"from":{"who":"self","zone":"field"},"to":to,"sel":sel}},
            "do":[{"move":{"ref":"it","to":"field"}}]}}),
        "unit-amulet",
    )
}

#[test]
fn only_cemetery_departures_return_their_subject() {
    for gate in [false, true] {
        lowerer().lower(&observer("cemetery", gate)).unwrap();
        assert_eq!(
            lowerer().lower(&observer("hand", gate)).unwrap_err().code,
            "unsupported_construct",
            "a hand departure has no cemetery generation to follow"
        );
    }
}

#[test]
fn zone_count_gates_reject_filtered_or_hidden_zones() {
    for left in [
        json!({"count":{"who":"self","zone":"hand"}}),
        json!({"count":{"who":"self","zone":"cemetery","kind":"follower"}}),
    ] {
        let mut tree = observer("cemetery", true);
        tree["cards"]["unit-amulet"]["abilities"][0]["auto"]["on"]["moved"]["sel"]["where"]["cmp"]
            ["left"] = left;
        assert!(
            lowerer().lower(&tree).is_err(),
            "only whole public zones gate"
        );
    }
}

fn ability(head: &Value, number: &str) -> Value {
    same_name::tree(head, number)["cards"][number]["abilities"][0].clone()
}

fn card_program(heads: &[Value], number: &str) -> WellFormedCardProgram {
    let mut tree = same_name::tree(&heads[0], number);
    for head in &heads[1..] {
        tree["cards"][number]["abilities"]
            .as_array_mut()
            .unwrap()
            .push(ability(head, number));
    }
    lowerer().lower(&tree).unwrap().remove(number).unwrap()
}

fn settle(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}

fn pending_count(game: &mut Game) -> usize {
    game.internal_pending_choice()
        .unwrap()
        .map_or(0, |question| match question.domain {
            InputDomain::Pending(options) => options.len(),
            other => panic!("pending occurrences: {other:?}"),
        })
}

#[test]
fn a_departed_card_returns_to_its_owners_field_not_the_observers() {
    let watch = card_program(
        &[
            json!({"line":1,"activate":{"cost":{"pp":0},
                "targets":[{"bind":"enemy","from":{"who":"opp","zone":"field","kind":"follower"},"n":1}],
                "do":[{"destroy":"enemy"}]}}),
            json!({"line":2,"auto":{"on":{"moved":{"from":{"who":"opp","zone":"field"},"to":"cemetery",
                "sel":{"kind":"follower"}}},"do":[{"move":{"ref":"it","to":"field"}}]}}),
        ],
        "unit-amulet",
    );
    let destroy = watch.card().abilities[0].node.id;
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"watch","card":"unit-amulet"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"unit-follower"}]);
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "owner-return",
    )
    .unwrap();
    game.load_internal_card("unit-amulet", &watch).unwrap();
    game.load_internal_card("unit-follower", &card(vec![]))
        .unwrap();
    let CardPlay::Awaiting(question) = game
        .activate_internal_card("watch", destroy, PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("destroy target");
    };
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("destroy target domain");
    };
    resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Targets(vec![domain.members[0].handle]),
        )
        .unwrap(),
    );
    settle(&mut game);
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("observer occurrence");
    };
    resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap(),
    );
    settle(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        state["P2"]["field"],
        json!(["enemy"]),
        "back on its owner's field"
    );
    assert_eq!(
        state["P1"]["field"],
        json!(["watch"]),
        "not the observer's field"
    );
}

#[test]
fn a_batch_reads_the_gate_before_any_member_moves_and_counts_fillers() {
    let observer = card_program(
        &[
            json!({"line":1,"auto":{"on":{"moved":{"from":{"who":"self","zone":"field"},"to":"cemetery",
            "sel":{"kind":"follower","other":true,"where":{"cmp":{"left":{"count":{"who":"self","zone":"cemetery"}},"op":"ge","right":10}}}}},
            "do":[{"move":{"ref":"it","to":"field"}}]}}),
        ],
        "unit-follower",
    );
    let boom = card_program(
        &[json!({"line":1,"activate":{"cost":{"pp":0},
            "do":[{"destroy":{"ref":{"who":"self","zone":"field","kind":"follower"}}}]}})],
        "unit-amulet",
    );
    let wipe = boom.card().abilities[0].node.id;
    for (cemetery, expected) in [
        // Nine before the batch: the first member's arrival does not open the gate for the second.
        (
            json!(
                (0..9)
                    .map(|n| json!({"id":format!("c{n}"),"card":"unit-spell"}))
                    .collect::<Vec<_>>()
            ),
            0,
        ),
        // Nine fillers and one named card are ten; each observer leaves in the same batch (Q2620).
        (json!([{"filler":9}, {"id":"c","card":"unit-spell"}]), 2),
    ] {
        let mut initial = setup(1, json!([]));
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P2"]["zones"]["field"] = json!([]);
        initial["players"]["P1"]["zones"]["cemetery"] = cemetery;
        initial["players"]["P1"]["zones"]["field"] = json!([
            {"id":"boom","card":"unit-amulet"},
            {"id":"a","card":"unit-follower"},
            {"id":"b","card":"unit-follower"}
        ]);
        let mut game = Game::new(
            catalog(),
            &initial,
            &Value::Null,
            &Value::Null,
            "batch-gate",
        )
        .unwrap();
        game.load_internal_card("unit-amulet", &boom).unwrap();
        game.load_internal_card("unit-follower", &observer).unwrap();
        game.load_internal_card("unit-spell", &card(vec![]))
            .unwrap();
        resolved(
            game.activate_internal_card("boom", wipe, PlayerId { seat: 1 })
                .unwrap(),
        );
        settle(&mut game);
        assert_eq!(
            pending_count(&mut game),
            expected,
            "each follower observes the other only from ten cards"
        );
    }
}
