#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Only the cemetery entry fixture's declared questions may occur."
)]
use super::*;
use sve_engine::game::internal::{DecisionView, InputDomain};

fn tree() -> Value {
    let mut tree = fanfare::expanded::tree().clone();
    let mut source = tree["cards"]["BP08-038"].clone();
    let mut entry = source["abilities"][0].clone();
    entry.as_object_mut().unwrap().remove("fanfare");
    entry["line"] = json!(1);
    entry["auto"] = json!({"zone":"cemetery","on":{"enter_field":{"who":"self","sel":{"name":"unit-follower"}}},
        "cost":{"banish_self":true},"targets":[{"bind":"ally","from":{"who":"self","zone":"field","kind":"follower"},"n":1}],
        "do":[{"damage":{"n":1,"to":"ally"}}]});
    source["abilities"] = json!([entry]);
    tree["cards"] = json!({"unit-spell":source});
    tree
}

fn initial(copies: usize, zone: &str) -> Value {
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"incoming","card":"unit-follower"}]);
    initial["players"]["P1"]["zones"][zone] = json!(
        (0..copies)
            .map(|i| json!({"id":format!("source{i}"),"card":"unit-spell"}))
            .collect::<Vec<_>>()
    );
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"opponent","card":"unit-follower"}]);
    initial
}

fn position(input: &Value, initial: &Value) -> Game {
    let programs = lowerer().lower(input).unwrap();
    let mut game = Game::new(
        catalog(),
        initial,
        &Value::Null,
        &Value::Null,
        "cemetery-self-cost",
    )
    .unwrap();
    for (number, program) in &programs {
        game.load_internal_card(number, program).unwrap();
    }
    if !programs.contains_key("unit-follower") {
        game.load_internal_card("unit-follower", &card(vec![]))
            .unwrap();
    }
    game
}

fn enter(game: &mut Game) {
    assert!(
        matches!(
            game.play_internal_card("incoming", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ),
        "the follower enters before cemetery occurrences are played"
    );
}

fn question(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(question) = play else {
        panic!("declared cemetery payment or target")
    };
    question
}

fn fire(game: &mut Game) -> CardPlay {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("entry occurrences")
    };
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::Pending(options[0].handle),
    )
    .unwrap()
}

fn declare(game: &mut Game, play: CardPlay, accepted: bool) -> CardPlay {
    let question = question(play);
    assert!(
        matches!(question.domain, InputDomain::OptionalCost),
        "self banish is declared before selecting formal targets"
    );
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::OptionalCost(accepted),
    )
    .unwrap()
}

#[test]
fn cemetery_self_cost_selects_before_payment_and_each_copy_banishes_only_itself() {
    let mut game = position(&tree(), &initial(2, "cemetery"));
    enter(&mut game);
    for remaining in [2, 1] {
        let pending = fire(&mut game);
        let targets = question(declare(&mut game, pending, true));
        let InputDomain::Targets(domain) = &targets.domain else {
            panic!("own follower targets")
        };
        assert_eq!(domain.members.len(), 1);
        let before = game.projection(View::Referee).unwrap();
        assert_eq!(
            before["P1"]["cemetery"].as_array().unwrap().len(),
            remaining
        );
        game.answer_internal_card(targets.actor, targets.token, Answer::Targets(vec![99]))
            .unwrap_err();
        assert_eq!(game.projection(View::Referee).unwrap(), before);
        resolved(
            game.answer_internal_card(
                targets.actor,
                targets.token,
                Answer::Targets(vec![domain.members[0].handle]),
            )
            .unwrap(),
        );
        game.resume_internal_card().unwrap();
    }
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["cemetery"], json!([]));
    assert_eq!(state["P1"]["banish"].as_array().unwrap().len(), 2);
    assert_eq!(state["objects"]["incoming"]["hp"], 1);
    assert_eq!(state["objects"]["opponent"]["hp"], 3);
    assert_eq!(state["P1"]["pp"]["current"], 1);
}

#[test]
fn cemetery_self_cost_decline_cancels_without_banish_or_effect() {
    let mut game = position(&tree(), &initial(1, "cemetery"));
    enter(&mut game);
    let pending = fire(&mut game);
    assert!(matches!(
        declare(&mut game, pending, false),
        CardPlay::Cancelled
    ));
    game.resume_internal_card().unwrap();
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["cemetery"], json!(["source0"]));
    assert_eq!(state["objects"]["incoming"]["hp"], 3);
    assert!(game.internal_pending_choice().unwrap().is_none());
}

#[test]
fn cemetery_entry_cost_observes_only_sources_already_in_cemetery() {
    for zone in ["hand", "ex", "banish"] {
        let mut setup = initial(1, zone);
        if zone == "hand" {
            setup["players"]["P1"]["zones"]["hand"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":"incoming","card":"unit-follower"}));
        }
        let mut game = position(&tree(), &setup);
        enter(&mut game);
        game.resume_internal_card().unwrap();
        assert!(game.internal_pending_choice().unwrap().is_none());
    }
}

#[test]
fn cemetery_self_cost_does_not_open_other_headers_or_payment_shapes() {
    position(&tree(), &initial(1, "cemetery"));
    for (field, value) in [
        ("zone", json!("field")),
        ("zone", json!("ex")),
        (
            "on",
            json!({"played":{"who":"self","sel":{"kind":"follower"}}}),
        ),
        ("cost", json!({"steps":[{"pp":1},{"banish_self":true}]})),
    ] {
        let mut input = tree();
        input["cards"]["unit-spell"]["abilities"][0]["auto"][field] = value;
        let programs = match lowerer().lower(&input) {
            Ok(programs) => programs,
            Err(error) => {
                assert_eq!(error.code, "unsupported_construct");
                continue;
            }
        };
        let mut game = Game::new(
            catalog(),
            &initial(1, "cemetery"),
            &Value::Null,
            &Value::Null,
            "closed-cemetery-self-cost",
        )
        .unwrap();
        game.load_internal_card("unit-spell", &programs["unit-spell"])
            .unwrap_err();
    }
    let mut input = tree();
    let auto = &mut input["cards"]["unit-spell"]["abilities"][0]["auto"];
    auto.as_object_mut().unwrap().remove("cost");
    auto["do"] = json!([{"banish":"this"}]);
    let programs = lowerer().lower(&input).unwrap();
    let mut game = Game::new(
        catalog(),
        &initial(1, "cemetery"),
        &Value::Null,
        &Value::Null,
        "closed-cemetery-body",
    )
    .unwrap();
    game.load_internal_card("unit-spell", &programs["unit-spell"])
        .unwrap_err();
}

fn incoming_with(input: &mut Value, head: &Value) {
    let mut incoming = input["cards"]["unit-spell"].clone();
    let entry = &mut incoming["abilities"][0];
    entry.as_object_mut().unwrap().remove("auto");
    entry["fanfare"] = head.clone();
    input["cards"]["unit-follower"] = incoming;
}

fn fire_card(game: &mut Game, card: &str) -> CardPlay {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("printed entry occurrence")
    };
    let handle = options
        .iter()
        .find(|option| option.card == card)
        .unwrap()
        .handle;
    game.answer_internal_card(question.actor, question.token, Answer::Pending(handle))
        .unwrap()
}

fn accept_choices(game: &mut Game, mut play: CardPlay) -> CardPlay {
    while let CardPlay::Awaiting(question) = play {
        let answer = match &question.domain {
            InputDomain::OptionalCost => Answer::OptionalCost(true),
            InputDomain::Targets(domain) => Answer::Targets(
                domain
                    .members
                    .first()
                    .map(|member| member.handle)
                    .into_iter()
                    .collect(),
            ),
            other => panic!("unexpected cemetery fixture choice {other:?}"),
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    play
}

#[test]
fn a_fanfare_discard_does_not_retroactively_create_a_cemetery_entry_occurrence() {
    let mut input = tree();
    incoming_with(
        &mut input,
        &json!({"do":[{"discard":{"all":true,"who":"self"}}]}),
    );
    let mut setup = initial(1, "hand");
    setup["players"]["P1"]["zones"]["hand"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"incoming","card":"unit-follower"}));
    let mut game = position(&input, &setup);
    enter(&mut game);
    let fanfare = fire_card(&mut game, "unit-follower");
    resolved(fanfare);
    game.resume_internal_card().unwrap();
    assert!(game.internal_pending_choice().unwrap().is_none());
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["cemetery"],
        json!(["source0"])
    );
}

#[test]
fn a_required_target_missing_at_play_never_banishes_the_cemetery_source() {
    let mut input = tree();
    incoming_with(&mut input, &json!({"do":[{"destroy":"this"}]}));
    let mut game = position(&input, &initial(1, "cemetery"));
    enter(&mut game);
    resolved(fire_card(&mut game, "unit-follower"));
    game.resume_internal_card().unwrap();
    let pending = fire_card(&mut game, "unit-spell");
    assert!(matches!(
        accept_choices(&mut game, pending),
        CardPlay::Cancelled
    ));
    let state = game.projection(View::Referee).unwrap();
    assert!(
        state["P1"]["cemetery"]
            .as_array()
            .unwrap()
            .contains(&json!("source0"))
    );
    assert_eq!(state["objects"]["source0"]["generation"], 0);
}

#[test]
fn another_occurrence_moving_the_source_cannot_make_its_new_generation_pay() {
    let mut input = tree();
    let mut observer = input["cards"]["unit-spell"].clone();
    observer["abilities"][0]["auto"] = json!({"zone":"field","on":{"enter_field":{"who":"self","sel":{"name":"unit-follower"}}},
        "targets":[{"bind":"old","from":{"who":"self","zone":"cemetery","kind":"spell"},"n":1}],
        "do":[{"move":{"ref":"old","to":"ex"}}]});
    input["cards"]["unit-amulet"] = observer;
    let mut setup = initial(1, "cemetery");
    setup["players"]["P1"]["zones"]["field"] = json!([{"id":"helper","card":"unit-amulet"}]);
    let mut game = position(&input, &setup);
    enter(&mut game);
    let moving = fire_card(&mut game, "unit-amulet");
    resolved(accept_choices(&mut game, moving));
    game.resume_internal_card().unwrap();
    let pending = fire_card(&mut game, "unit-spell");
    assert!(matches!(
        accept_choices(&mut game, pending),
        CardPlay::Cancelled
    ));
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["ex"], json!(["source0"]));
    assert_eq!(state["objects"]["source0"]["generation"], 1);
    assert_eq!(state["objects"]["incoming"]["hp"], 3);
}

#[test]
fn cemetery_self_cost_can_recover_ep_without_a_formal_target_and_above_three() {
    let mut input = tree();
    let auto = &mut input["cards"]["unit-spell"]["abilities"][0]["auto"];
    auto.as_object_mut().unwrap().remove("targets");
    auto["do"] = json!([{"ep":{"who":"self","delta":1}}]);
    let mut setup = initial(1, "cemetery");
    setup["players"]["P1"]["ep"] = json!(3);
    let mut game = position(&input, &setup);
    enter(&mut game);
    let pending = fire(&mut game);
    resolved(declare(&mut game, pending, true));
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["ep"], 4);
    assert_eq!(state["P1"]["banish"], json!(["source0"]));
}
