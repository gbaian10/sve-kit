#![expect(
    clippy::panic,
    reason = "The named private spell procedures must expose their declared choices."
)]
use super::*;
use alloc::collections::BTreeMap;
use sve_engine::game::internal::{DecisionView, InputDomain};

fn input() -> Value {
    let mut document = fanfare::expanded::tree().clone();
    let mut parent = document["cards"]["BP01-041"].clone();
    parent["abilities"][0]["activate"] = json!({"cost":{"act_self":true},"do":[
        {"choose":{"bind":"incoming","from":{"who":"self","zone":"evolve_deck","kind":"spell","advance_spell":true,"name":"unit-spell"},"n":1,"may":true}},
        {"play_card":{"ref":"incoming"}},
        {"life":{"who":"self","delta":1}}
    ]});
    let mut child = parent.clone();
    child["abilities"][0]
        .as_object_mut()
        .unwrap()
        .remove("activate");
    child["abilities"][0]["spell"] = json!({"do":[{"life":{"who":"self","delta":2}}]});
    document["cards"] = json!({"unit-follower":parent,"unit-spell":child,"other-spell":child});
    document
}

fn board(pp: u8, face_up: bool) -> Game {
    let programs = lowerer()
        .lower_with_printed_costs(
            &input(),
            &BTreeMap::from([
                ("unit-spell".into(), Nat(3)),
                ("other-spell".into(), Nat(3)),
            ]),
        )
        .unwrap();
    let (snapshot, keywords, document) = catalog_documents(&json!("3"));
    let mut rows = snapshot
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    let spell = rows
        .iter_mut()
        .find(|row| row["number"] == "unit-spell")
        .unwrap();
    spell["faces"][0]["card_type"] = json!("スペル・アドバンス");
    spell["faces"][0]["power"] = json!("-");
    spell["faces"][0]["hp"] = json!("-");
    let mut other = spell.clone();
    other["number"] = json!("other-spell");
    other["faces"][0]["name"] = json!("other-spell");
    let mut wrong = spell.clone();
    wrong["number"] = json!("wrong-kind");
    wrong["faces"][0]["card_type"] = json!("スペル・エボルヴ");
    rows.extend([other, wrong]);
    let extra=json!({"format":1,"kind":"effect_set","cards":{"other-spell":{"status":"complete","abilities":[]},"wrong-kind":{"status":"complete","abilities":[]}}}).to_string();
    let catalog = Arc::new(
        Catalog::from_documents(
            &rows
                .iter()
                .map(Value::to_string)
                .collect::<Vec<_>>()
                .join("\n"),
            &keywords,
            &[("unit.yaml".into(), document), ("extra.yaml".into(), extra)],
        )
        .unwrap(),
    );
    let mut position = setup(1, json!([]));
    position["players"]["P1"]["pp"] = json!({"current":pp,"max":6});
    position["players"]["P1"]["zones"]["hand"] = json!([]);
    position["players"]["P1"]["zones"]["field"] =
        json!([{"id":"source","card":"unit-follower"},{"id":"second","card":"unit-follower"}]);
    position["players"]["P1"]["zones"]["evolve_deck"] = json!([
        {"id":"child","card":"unit-spell","face_up":face_up},
        {"id":"decoy","card":"other-spell"},{"id":"wrong","card":"wrong-kind"}
    ]);
    position["players"]["P2"]["zones"]["evolve_deck"] =
        json!([{"id":"foreign","card":"unit-spell"}]);
    let mut game = Game::new(
        catalog,
        &position,
        &Value::Null,
        &Value::Null,
        "advance-spell",
    )
    .unwrap();
    for (number, program) in programs {
        game.load_internal_card(&number, &program).unwrap();
    }
    game.load_internal_card("wrong-kind", &card(vec![]))
        .unwrap();
    game
}

fn question(game: &mut Game, source: &str) -> DecisionView {
    let CardPlay::Awaiting(q) = game
        .activate_internal_card(source, AbilityId(1), PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("private named Advance spell choice")
    };
    assert!(
        matches!(q.domain, InputDomain::Targets(_)),
        "the selector exposes only scoped target handles"
    );
    assert!(
        game.internal_decision(View::P2).is_none(),
        "the opponent cannot see unused evolve-deck cards"
    );
    q
}

fn choose(game: &mut Game, q: &DecisionView, accept: bool) -> Activation {
    let InputDomain::Targets(domain) = &q.domain else {
        panic!("named spell members")
    };
    let handles = if accept {
        domain
            .members
            .iter()
            .take(1)
            .map(|member| member.handle)
            .collect()
    } else {
        vec![]
    };
    resolved(
        game.answer_internal_card(q.actor, q.token, Answer::Targets(handles))
            .unwrap(),
    )
}

#[test]
fn named_advance_spell_uses_normal_cost_and_keeps_the_parent_when_declined_or_unaffordable() {
    for (pp, accept) in [(2_u8, true), (3, true), (3, false)] {
        let mut game = board(pp, false);
        let q = question(&mut game, "source");
        let result = choose(&mut game, &q, accept);
        let played = accept && pp >= 3;
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P1"]["pp"]["current"],
            if played { pp - 3 } else { pp }
        );
        assert_eq!(state["P1"]["leader"]["life"], if played { 23 } else { 21 });
        assert_eq!(state["objects"]["source"]["acted"], true);
        assert_eq!(state["objects"]["child"]["face_up"], played);
        assert_eq!(
            state["objects"]["child"]["generation"],
            if played { 3 } else { 0 }
        );
        assert_eq!(state["P1"]["cemetery"], json!([]));
        assert_eq!(
            result
                .progress
                .facts
                .iter()
                .filter(|fact| matches!(fact, Fact::CardPlayed { .. }))
                .count(),
            usize::from(played)
        );
    }
}

#[test]
fn returned_face_up_advance_spell_cannot_be_chosen_by_another_ready_source() {
    let mut game = board(6, false);
    let first = question(&mut game, "source");
    let InputDomain::Targets(domain) = &first.domain else {
        panic!("private spell choice")
    };
    assert_eq!(domain.members.len(), 1);
    choose(&mut game, &first, true);
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
    let second = question(&mut game, "second");
    let InputDomain::Targets(domain) = &second.domain else {
        panic!("second spell choice")
    };
    assert!(
        domain.members.is_empty(),
        "CR4.6.3 excludes the returned face-up spell even with enough PP"
    );
    choose(&mut game, &second, false);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["pp"]["current"], 3);
    assert_eq!(state["P1"]["leader"]["life"], 24);
}

#[test]
fn advance_spell_authoring_does_not_open_other_zones_cost_overrides_or_consumers() {
    let costs = BTreeMap::from([
        ("unit-spell".into(), Nat(3)),
        ("other-spell".into(), Nat(3)),
    ]);
    for mutation in 0..6 {
        let mut document = input();
        let body = &mut document["cards"]["unit-follower"]["abilities"][0]["activate"]["do"];
        match mutation {
            0 => body[0]["choose"]["from"]["who"] = json!("opp"),
            1 => body[0]["choose"]["from"]["kind"] = json!("follower"),
            2 => body[1]["play_card"]["cost"] = json!(0),
            3 => body[1] = json!({"move":{"ref":"incoming","to":"ex"}}),
            4 => body[0]["choose"]["n"] = json!(2),
            _ => {
                body[0]["choose"]["from"]
                    .as_object_mut()
                    .unwrap()
                    .remove("name");
                body[0]["choose"]["from"]["name_contains"] = json!("spell");
            }
        }
        lowerer()
            .lower_with_printed_costs(&document, &costs)
            .unwrap_err();
    }
    let mut document = input();
    let head = &mut document["cards"]["unit-follower"]["abilities"][0]["activate"];
    head["targets"] = json!([{"bind":"incoming","from":{"who":"self","zone":"cemetery","name":"unit-spell"},"n":1}]);
    head["do"] = json!([{"play_card":{"ref":"incoming"}}]);
    lowerer()
        .lower_with_printed_costs(&document, &costs)
        .unwrap_err();
    document["cards"]["unit-follower"]["abilities"][0]["activate"]["do"][0]["play_card"]["cost"] =
        json!(0);
    lowerer()
        .lower_with_printed_costs(&document, &costs)
        .unwrap();
}

#[test]
fn a_raw_normal_spell_play_cannot_escape_the_exact_private_pair() {
    let programs = lowerer()
        .lower_with_printed_costs(
            &input(),
            &BTreeMap::from([
                ("unit-spell".into(), Nat(3)),
                ("other-spell".into(), Nat(3)),
            ]),
        )
        .unwrap();
    let mut raw = programs["unit-follower"].card().clone();
    let steps = &mut raw.abilities[0].node.body.as_mut().unwrap().steps;
    let Node::Control {
        control: Control::Sequence { body },
    } = &mut steps[0].node
    else {
        panic!("closed named Advance pair")
    };
    let mut extra = body.steps[1].clone();
    let Node::Instruction { instruction } = &mut extra.node else {
        panic!("normal spell play")
    };
    instruction.id = InstructionId(900);
    instruction.receipt = ReceiptId(900);
    instruction.action = Action::Life {
        player: PlayerRef::Controller {},
        change: NumericChange::Add {
            amount: Expr::Literal(Int(1)),
        },
    };
    body.steps.push(extra);
    let error = WellFormedCardProgram::check(raw).unwrap_err();
    assert_eq!(
        error.reason,
        WellFormedReason::UnsupportedShape,
        "an otherwise valid third step removes the compound's normal-play authority"
    );
    let mut grave = input();
    let head = &mut grave["cards"]["unit-follower"]["abilities"][0]["activate"];
    head["targets"] = json!([{"bind":"incoming","from":{"who":"self","zone":"cemetery","name":"unit-spell"},"n":1}]);
    head["do"] = json!([{"play_card":{"ref":"incoming","cost":0}}]);
    let programs = lowerer()
        .lower_with_printed_costs(
            &grave,
            &BTreeMap::from([
                ("unit-spell".into(), Nat(3)),
                ("other-spell".into(), Nat(3)),
            ]),
        )
        .unwrap();
    let mut ordinary = programs["unit-follower"].card().clone();
    let Node::Instruction { instruction } =
        &mut ordinary.abilities[0].node.body.as_mut().unwrap().steps[0].node
    else {
        panic!("ordinary grave play instruction")
    };
    let Action::PlayCard { cost_override, .. } = &mut instruction.action else {
        panic!("ordinary zero-cost card play")
    };
    *cost_override = None;
    let error = WellFormedCardProgram::check(ordinary).unwrap_err();
    assert_eq!(
        error.reason,
        WellFormedReason::UnsupportedShape,
        "the unchanged valid cemetery target cannot authorize an omitted cost in raw IR"
    );
}
