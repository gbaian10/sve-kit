#![expect(
    clippy::panic,
    reason = "A missing decision invalidates the participant scenarios."
)]
use super::*;
use alloc::collections::BTreeMap;
use sve_engine::game::internal::{DecisionView, InputDomain};

const fn seat_name(seat: u32) -> &'static str {
    if seat == 1 { "P1" } else { "P2" }
}

fn tree(number: &str, head: &Value) -> Value {
    let mut tree = fanfare::expanded::tree().clone();
    let card = &tree["cards"]["BP08-038"];
    let meta = card["meta"].clone();
    let mut entry = card["abilities"][0].clone();
    entry.as_object_mut().unwrap().remove("fanfare");
    for (key, value) in head.as_object().unwrap() {
        entry[key] = value.clone();
    }
    tree["cards"] = json!({ number: {"meta":meta,"abilities":[entry]} });
    tree
}

fn chosen_controller_may(side: &str) -> Value {
    tree(
        "unit-follower",
        &json!({"fanfare":{
            "targets":[{"bind":"victim","from":{"who":side,"zone":"field","kind":"follower"},"n":1}],
            "do":[{"optional":{"who":{"master_of":"victim"},
                "do":[{"draw":{"n":1,"who":{"master_of":"victim"}}}]}}]
        }}),
    )
}

fn opponent_payment(who: &Value, cost: &Value) -> Value {
    tree(
        "unit-amulet",
        &json!({"auto":{
            "on":{"attacked":{"who":"opp","sel":{"kind":"follower"}}},
            "do":[
                {"pay":{"who":who,"cost":cost,"do":[{"destroy":"this"}],"result":"paid"}},
                {"if":{"cond":{"not":{"paid":{"receipt":"paid"}}},
                    "then":[{"damage":{"n":2,"to":"it"}}]}}
            ]
        }}),
    )
}

fn deck(seat: &str) -> Value {
    json!([{"id":format!("{seat}-d0"),"card":"unit-spell"},{"id":format!("{seat}-d1"),"card":"unit-spell"}])
}

fn game(programs: &BTreeMap<String, WellFormedCardProgram>, initial: &Value) -> Game {
    let mut game = Game::new(
        catalog(),
        initial,
        &Value::Null,
        &Value::Null,
        "participant",
    )
    .unwrap();
    for number in ["unit-follower", "unit-amulet", "unit-spell"] {
        let program = programs
            .get(number)
            .cloned()
            .unwrap_or_else(|| card(vec![]));
        game.load_internal_card(number, &program).unwrap();
    }
    game
}

fn awaiting(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(question) = play else {
        panic!("a participant decision must be offered");
    };
    question
}

fn checks(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}

fn pending(game: &mut Game) -> CardPlay {
    checks(game);
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("one pending occurrence");
    };
    game.answer_internal_card(
        pending.actor,
        pending.token,
        Answer::Pending(options[0].handle),
    )
    .unwrap()
}

#[test]
fn optional_and_payment_accept_only_known_participants() {
    lowerer().lower(&chosen_controller_may("opp")).unwrap();
    for who in [json!("self"), json!("opp")] {
        lowerer()
            .lower(&opponent_payment(&who, &json!({"pp":2})))
            .unwrap();
    }
    for (input, case) in [
        (
            opponent_payment(&json!({"master_of":"missing"}), &json!({"pp":2})),
            "an unbound object has no controller",
        ),
        (
            opponent_payment(&json!("opp"), &json!({"act_self":true})),
            "the source's own costs belong to its controller",
        ),
        (
            opponent_payment(&json!("opp"), &json!({"pp":0})),
            "a zero payment is not a decision",
        ),
    ] {
        assert_eq!(
            lowerer().lower(&input).unwrap_err().code,
            "unsupported_construct",
            "{case}"
        );
    }
}

#[test]
fn chosen_objects_controller_answers_may_from_either_seat_even_when_stolen() {
    // (chooser, target side, stolen) -> the chosen object's controller decides, never its owner.
    for seat in [1_u32, 2] {
        let me = seat_name(seat);
        let them = seat_name(3 - seat);
        for (side, stolen) in [("opp", false), ("opp", true), ("self", false)] {
            let programs = lowerer().lower(&chosen_controller_may(side)).unwrap();
            let mut initial = setup(seat, json!([]));
            initial["turn"]["active"] = json!(me);
            initial["players"][me]["zones"]["hand"] = json!([{"id":"s","card":"unit-follower"}]);
            initial["players"][me]["pp"] = json!({"current":5,"max":5});
            for player in [me, them] {
                initial["players"][player]["zones"]["deck"] = deck(player);
            }
            initial["players"][them]["zones"]["field"] = if stolen {
                json!([{"id":"g","card":"unit-follower","owner":me}])
            } else {
                json!([{"id":"f","card":"unit-follower"}])
            };
            let decider = if side == "opp" { 3 - seat } else { seat };
            let mut game = game(&programs, &initial);
            game.play_internal_card("s", PlayerId { seat }).unwrap();
            let target = awaiting(pending(&mut game));
            let InputDomain::Targets(domain) = &target.domain else {
                panic!("one target");
            };
            let may = awaiting(
                game.answer_internal_card(
                    target.actor,
                    target.token,
                    Answer::Targets(vec![domain.members[0].handle]),
                )
                .unwrap(),
            );
            assert_eq!(
                may.actor,
                PlayerId { seat: decider },
                "seat {seat} {side} stolen={stolen}"
            );
            let before = exact_authority(&game);
            game.answer_internal_card(
                PlayerId { seat: 3 - decider },
                may.token,
                Answer::Integer(Nat(1)),
            )
            .unwrap_err();
            assert_eq!(exact_authority(&game), before, "a wrong seat cannot answer");
            resolved(
                game.answer_internal_card(may.actor, may.token, Answer::Integer(Nat(1)))
                    .unwrap(),
            );
            let state = game.projection(View::Referee).unwrap();
            let drawer = seat_name(decider);
            assert_eq!(state[drawer]["hand"].as_array().unwrap().len(), 1);
            assert_eq!(
                state[seat_name(3 - decider)]["hand"]
                    .as_array()
                    .unwrap()
                    .len(),
                0
            );
        }
    }
}

#[test]
fn opponent_pays_or_declines_and_the_receipt_drives_the_unpaid_branch() {
    // (accept, opponent PP) -> paid destroys the amulet; refusal or shortfall damages the attacker.
    for seat in [1_u32, 2] {
        let me = seat_name(seat);
        let them = seat_name(3 - seat);
        let programs = lowerer()
            .lower(&opponent_payment(&json!("opp"), &json!({"pp":2})))
            .unwrap();
        for (accept, pp) in [(true, 2_i64), (false, 2), (true, 1)] {
            let mut initial = setup(seat, json!([]));
            initial["turn"]["active"] = json!(them);
            initial["players"][me]["zones"]["hand"] = json!([]);
            initial["players"][me]["zones"]["field"] = json!([{"id":"a","card":"unit-amulet"}]);
            initial["players"][them]["zones"]["field"] = json!([{"id":"f","card":"unit-follower"}]);
            initial["players"][them]["pp"] = json!({"current":pp,"max":5});
            let mut game = game(&programs, &initial);
            assert_eq!(
                game.internal_rule_action(
                    &json!({"do":"attack","attacker":"f","target":format!("{me}.leader")})
                )
                .unwrap()
                .outcome,
                "resolved"
            );
            let payment = awaiting(pending(&mut game));
            assert_eq!(payment.domain, InputDomain::OptionalCost);
            assert_eq!(payment.actor, PlayerId { seat: 3 - seat });
            let before = exact_authority(&game);
            game.answer_internal_card(
                PlayerId { seat },
                payment.token,
                Answer::OptionalCost(accept),
            )
            .unwrap_err();
            assert_eq!(
                exact_authority(&game),
                before,
                "the controller cannot pay for them"
            );
            resolved(
                game.answer_internal_card(
                    payment.actor,
                    payment.token,
                    Answer::OptionalCost(accept),
                )
                .unwrap(),
            );
            let state = game.projection(View::Referee).unwrap();
            let paid = accept && pp >= 2;
            assert_eq!(
                state[them]["pp"]["current"],
                if paid { pp - 2 } else { pp },
                "seat {seat} accept={accept} pp={pp}"
            );
            assert_eq!(state[me]["pp"]["current"], 2, "the controller never pays");
            assert_eq!(
                state[me]["cemetery"],
                if paid { json!(["a"]) } else { json!([]) }
            );
            assert_eq!(state["objects"]["f"]["damage"], if paid { 0 } else { 2 });
        }
    }
}

fn opponent() -> PlayerRef {
    PlayerRef::RequireOne {
        players: Box::new(PlayerSet {
            relation: PlayerRelation::Opponents,
            relative_to: PlayerRef::Controller {},
            cardinality: Cardinality {
                minimum: Nat(1),
                maximum: Some(Nat(1)),
            },
            text_quantifier: TextQuantifier::One,
        }),
    }
}

fn opponent_pp(result: &str, gain: u8) -> Value {
    json!({"pay":{"who":"opp","cost":{"pp":1},"do":[{"life":{"who":"self","delta":gain}}],"result":result}})
}

fn unpaid(result: &str, damage: u8) -> Value {
    json!({"if":{"cond":{"not":{"paid":{"receipt":result}}},"then":[{"damage":{"n":damage,"to":"it"}}]}})
}

fn on_attack(steps: &Value) -> Value {
    tree(
        "unit-amulet",
        &json!({"auto":{"on":{"attacked":{"who":"opp","sel":{"kind":"follower"}}},"do":steps}}),
    )
}

/// P1 controls the amulet; P2 attacks with `f` holding `pp`.
fn attacked(programs: &BTreeMap<String, WellFormedCardProgram>, pp: i64, max: i64) -> Game {
    let mut initial = setup(1, json!([]));
    initial["turn"]["active"] = json!("P2");
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["pp"] = json!({"current":2,"max":max});
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"a","card":"unit-amulet"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"f","card":"unit-follower"}]);
    initial["players"]["P2"]["pp"] = json!({"current":pp,"max":5});
    let mut game = game(programs, &initial);
    game.internal_rule_action(&json!({"do":"attack","attacker":"f","target":"P1.leader"}))
        .unwrap();
    game
}

fn pay(game: &mut Game, question: &DecisionView, accept: bool) -> CardPlay {
    assert_eq!(
        question.domain,
        InputDomain::OptionalCost,
        "a payment question"
    );
    assert_eq!(
        question.actor,
        PlayerId { seat: 2 },
        "the attacker's controller pays"
    );
    game.answer_internal_card(question.actor, question.token, Answer::OptionalCost(accept))
        .unwrap()
}

#[test]
fn play_time_declarations_keep_the_controller_as_payer_at_load() {
    let input = tree(
        "unit-follower",
        &json!({"fanfare":{"optional_cost":{"pp":2,"result":"paid"},
            "do":[{"if":{"cond":{"paid":{"receipt":"paid"}},"then":[{"draw":{"n":1}}]}}]}}),
    );
    let mut program = lowerer().lower(&input).unwrap()["unit-follower"]
        .card()
        .clone();
    let Node::Control {
        control:
            Control::Pay {
                at: PaymentTime::PlayParameter,
                costs: CostSpec::Ordered { steps },
                ..
            },
    } = &mut program.abilities[0].node.body.as_mut().unwrap().steps[0].node
    else {
        panic!("play-time declaration");
    };
    let instruction = &mut steps[0].instruction;
    instruction.actor = opponent();
    let Action::Resource { player, .. } = &mut instruction.action else {
        panic!("PP declaration");
    };
    *player = opponent();
    let checked = WellFormedCardProgram::check(program).unwrap();
    let mut game = Game::new(
        catalog(),
        &setup(1, json!([])),
        &Value::Null,
        &Value::Null,
        "foreign-declaration",
    )
    .unwrap();
    let before = format!("{game:?}");
    game.load_internal_card("unit-follower", &checked)
        .unwrap_err();
    assert_eq!(
        format!("{game:?}"),
        before,
        "the controller would answer for them"
    );
}

#[test]
fn each_named_payment_result_reads_its_own_receipt() {
    // (first accepted, second accepted) with one opponent PP -> exactly one payment succeeds.
    let programs = lowerer()
        .lower(&on_attack(&json!([
            opponent_pp("paid0", 1),
            opponent_pp("paid1", 1),
            unpaid("paid0", 1),
            unpaid("paid1", 2)
        ])))
        .unwrap();
    for (first, damage) in [(true, 2), (false, 1)] {
        let mut game = attacked(&programs, 1, 2);
        let question = awaiting(pending(&mut game));
        let second = awaiting(pay(&mut game, &question, first));
        resolved(pay(&mut game, &second, true));
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P2"]["pp"]["current"], 0, "first={first}");
        assert_eq!(state["P1"]["leader"]["life"], 21);
        assert_eq!(state["objects"]["f"]["damage"], damage, "first={first}");
    }
}

#[test]
fn exclusive_branches_keep_their_payment_results_inside_the_branch() {
    let branches = |after: Option<Value>| {
        let mut steps = vec![json!({"if":{
            "cond":{"cmp":{"left":{"resource":{"who":"self","kind":"pp_max"}},"op":"ge","right":7}},
            "then":[opponent_pp("high", 1), unpaid("high", 1)],
            "else":[opponent_pp("low", 1), unpaid("low", 2)]
        }})];
        steps.extend(after);
        on_attack(&Value::Array(steps))
    };
    assert_eq!(
        lowerer()
            .lower(&branches(Some(unpaid("high", 3))))
            .unwrap_err()
            .code,
        "unsupported_construct",
        "a branch payment is not readable after the branch"
    );
    let programs = lowerer().lower(&branches(None)).unwrap();
    for (max, damage) in [(7, 1), (2, 2)] {
        let mut game = attacked(&programs, 1, max);
        let question = awaiting(pending(&mut game));
        resolved(pay(&mut game, &question, false));
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P2"]["pp"]["current"], 1);
        assert_eq!(state["objects"]["f"]["damage"], damage, "max={max}");
    }
}

#[test]
fn a_chosen_object_keeps_its_selection_time_controller_after_returning_to_its_owner() {
    // Q1012: the stolen card leaves for its owner's deck, yet its former controller decides.
    let programs = lowerer()
        .lower(&tree(
            "unit-follower",
            &json!({"fanfare":{
                "targets":[{"bind":"victim","from":{"who":"opp","zone":"field","kind":"follower"},"n":1}],
                "do":[
                    {"move":{"ref":"victim","to":"deck","position":"bottom"}},
                    {"optional":{"who":{"master_of":"victim"},
                        "do":[{"draw":{"n":1,"who":{"master_of":"victim"}}}]}}
                ]
            }}),
        ))
        .unwrap();
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"s","card":"unit-follower"}]);
    initial["players"]["P1"]["pp"] = json!({"current":5,"max":5});
    initial["players"]["P2"]["zones"]["deck"] = deck("P2");
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"g","card":"unit-follower","owner":"P1"}]);
    let mut game = game(&programs, &initial);
    game.play_internal_card("s", PlayerId { seat: 1 }).unwrap();
    let target = awaiting(pending(&mut game));
    let InputDomain::Targets(domain) = &target.domain else {
        panic!("one target");
    };
    let may = awaiting(
        game.answer_internal_card(
            target.actor,
            target.token,
            Answer::Targets(vec![domain.members[0].handle]),
        )
        .unwrap(),
    );
    assert_eq!(may.actor, PlayerId { seat: 2 });
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        state["P1"]["deck"],
        json!(["g"]),
        "the card returned to its owner"
    );
    resolved(
        game.answer_internal_card(may.actor, may.token, Answer::Integer(Nat(1)))
            .unwrap(),
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["hand"].as_array().unwrap().len(), 1);
    assert_eq!(state["P1"]["hand"], json!([]));
}

#[test]
fn a_returned_stolen_target_is_still_paid_for_by_its_selection_time_controller() {
    let programs = lowerer()
        .lower(&tree(
            "unit-amulet",
            &json!({"auto":{
                "on":{"attacked":{"who":"opp","sel":{"kind":"follower"}}},
                "targets":[{"bind":"victim","from":{"who":"opp","zone":"field","kind":"follower"},"n":1}],
                "do":[
                    {"move":{"ref":"victim","to":"deck","position":"bottom"}},
                    {"pay":{"who":{"master_of":"victim"},"cost":{"pp":1},
                        "do":[{"life":{"who":"self","delta":1}}]}}
                ]
            }}),
        ))
        .unwrap();
    let mut initial = setup(1, json!([]));
    initial["turn"]["active"] = json!("P2");
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"a","card":"unit-amulet"}]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"g","card":"unit-follower","owner":"P1"}]);
    initial["players"]["P2"]["pp"] = json!({"current":3,"max":5});
    let mut game = game(&programs, &initial);
    game.internal_rule_action(&json!({"do":"attack","attacker":"g","target":"P1.leader"}))
        .unwrap();
    let target = awaiting(pending(&mut game));
    let InputDomain::Targets(domain) = &target.domain else {
        panic!("one target");
    };
    let payment = awaiting(
        game.answer_internal_card(
            target.actor,
            target.token,
            Answer::Targets(vec![domain.members[0].handle]),
        )
        .unwrap(),
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["deck"],
        json!(["g"])
    );
    resolved(pay(&mut game, &payment, true));
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        state["P2"]["pp"]["current"], 2,
        "the former controller paid"
    );
    assert_eq!(
        state["P1"]["pp"]["current"], 2,
        "the owner and effect controller did not"
    );
    assert_eq!(state["P1"]["leader"]["life"], 21);
}

#[test]
fn an_optional_then_reads_the_payment_declared_in_its_do() {
    let optional = |after: Option<Value>| {
        let mut steps = vec![json!({"optional":{
            "do":[opponent_pp("paid", 1)],
            "then":[unpaid("paid", 2)]
        }})];
        steps.extend(after);
        on_attack(&Value::Array(steps))
    };
    assert_eq!(
        lowerer()
            .lower(&optional(Some(unpaid("paid", 1))))
            .unwrap_err()
            .code,
        "unsupported_construct",
        "the payment result ends with its optional"
    );
    let programs = lowerer().lower(&optional(None)).unwrap();
    for (accept, pp, damage) in [(true, 0, 0), (false, 1, 2)] {
        let mut game = attacked(&programs, 1, 2);
        let may = awaiting(pending(&mut game));
        assert_eq!(may.actor, PlayerId { seat: 1 }, "the controller opts in");
        let payment = awaiting(
            game.answer_internal_card(may.actor, may.token, Answer::Integer(Nat(1)))
                .unwrap(),
        );
        resolved(pay(&mut game, &payment, accept));
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P2"]["pp"]["current"], pp, "accept={accept}");
        assert_eq!(state["objects"]["f"]["damage"], damage, "accept={accept}");
    }
}
