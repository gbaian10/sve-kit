#![expect(
    clippy::panic,
    reason = "Synthetic Advance procedures must ask only their declared payment and private choices."
)]
use super::*;
use sve_engine::EngineFailure;
use sve_engine::game::internal::{DecisionView, InputDomain};

fn entry() -> Value {
    json!([
        {"choose":{"bind":"incoming","from":{"who":"self","zone":"evolve_deck",
            "kind":"follower","advance_follower":true,"name":"named advance"},"n":1,"may":true}},
        {"move":{"ref":"incoming","to":"field"}}
    ])
}

fn tree(advance: bool, enter: bool) -> Value {
    let mut tree = fanfare::expanded::tree().clone();
    let mut source = tree["cards"]["BP01-041"].clone();
    let head = &mut source["abilities"][0]["activate"];
    *head = json!({"cost":{"pp":if advance {4_u8} else {3_u8}},
        "do":if enter {entry()} else {json!([{"buff":{"on":"this","power":1}}])}});
    if advance {
        head["advance"] = json!(true);
        if enter {
            head["cost"] = json!({"steps":[{"pp":4},{"banish_self":true}]});
        }
    }
    tree["cards"] = json!({"unit-follower":source});
    tree
}

fn program(advance: bool, enter: bool) -> WellFormedCardProgram {
    let input = tree(advance, enter);
    let mut prefix = input.clone();
    let head = &mut prefix["cards"]["unit-follower"]["abilities"][0]["activate"];
    head.as_object_mut().unwrap().remove("advance");
    if enter {
        head["do"] = json!([{"draw":{"who":"self","n":1}}]);
    }
    lowerer().lower(&prefix).unwrap_or_else(|error| {
        panic!("the ordinary cost/control must lower before testing new entry: {error}")
    });
    let case = if !advance {
        "ordinary_entry"
    } else if enter {
        "advance_entry"
    } else {
        "advance_head"
    };
    lowerer()
        .lower(&input)
        .unwrap_or_else(|error| {
            panic!(
                "ADVANCE_ENTRY_RED case={case}; code={}; path={:?}",
                error.code, error.path
            )
        })
        .remove("unit-follower")
        .unwrap()
}

fn board(program: &WellFormedCardProgram, pp: u8, ep: u8, full: bool, face_up: bool) -> Game {
    board_at(program, pp, ep, full, face_up, None)
}

fn board_at(
    program: &WellFormedCardProgram,
    pp: u8,
    ep: u8,
    full: bool,
    face_up: bool,
    timing: Option<(&str, &str)>,
) -> Game {
    let (mut snapshot, registry, document) = catalog_documents(&json!("1"));
    let mut definitions = serde_json::from_str::<Value>(&document).unwrap();
    for (number, name, kind) in [
        ("incoming", "named advance", "フォロワー・アドバンス"),
        ("wrong-name", "other advance", "フォロワー・アドバンス"),
        ("wrong-kind", "named advance", "フォロワー・エボルヴ"),
        ("normal-form", "unit-follower", "フォロワー・エボルヴ"),
    ] {
        snapshot.push('\n');
        snapshot.push_str(
            &json!({"number":number,"faces":[{"name":name,"card_type":kind,
            "card_class":"ニュートラル","cost":"4","power":"5","hp":"6",
            "traits":[],"text":null,"sections":[]}]})
            .to_string(),
        );
        definitions["cards"][number] = json!({"status":"complete","abilities":[]});
    }
    let catalog = Arc::new(
        Catalog::from_documents(
            &snapshot,
            &registry,
            &[("synthetic.yaml".into(), definitions.to_string())],
        )
        .unwrap(),
    );
    let mut initial = setup(1, json!([]));
    if let Some((active, phase)) = timing {
        initial["turn"]["active"] = json!(active);
        initial["turn"]["phase"] = json!(phase);
    }
    initial["players"]["P1"]["pp"] = json!({"current":pp,"max":pp.max(4)});
    initial["players"]["P1"]["ep"] = json!(ep);
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"source","card":"unit-follower"},{"id":"second","card":"unit-follower"}
    ]);
    if full {
        for n in 0_u8..3 {
            initial["players"]["P1"]["zones"]["field"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":format!("old-{n}"),"card":"unit-follower"}));
        }
    }
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([
        {"id":"incoming","card":"incoming","state":{"face_up":face_up}},
        {"id":"decoy","card":"wrong-name"}, {"id":"evolve","card":"wrong-kind"},
        {"id":"normal-form","card":"normal-form"}
    ]);
    initial["players"]["P2"]["zones"]["evolve_deck"] = json!([
        {"id":"opposing","card":"incoming"}
    ]);
    let mut game = Game::new(
        catalog,
        &initial,
        &Value::Null,
        &Value::Null,
        "advance-entry",
    )
    .unwrap();
    game.load_internal_card("unit-follower", program).unwrap();
    for number in ["incoming", "wrong-name", "wrong-kind", "normal-form"] {
        game.load_internal_card(number, &card(vec![])).unwrap();
    }
    game
}

fn question(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(question) = play else {
        panic!("the printed optional entry or resource payment needs a decision");
    };
    question
}

fn payment(game: &mut Game, play: CardPlay, use_ep: bool) -> CardPlay {
    let q = question(play);
    let InputDomain::ResourceEntryPayment {
        materials,
        ep_allowed,
    } = &q.domain
    else {
        panic!("Advance shares the evolution-equivalent resource payment question");
    };
    assert!(
        materials.members.is_empty(),
        "Advance has no Meal or Ride resource materials"
    );
    assert_eq!(
        materials.cardinality.maximum,
        Some(Nat(0)),
        "no invented resource is paid"
    );
    assert_eq!(
        *ep_allowed, use_ep,
        "EP substitutes exactly one printed PP when available"
    );
    game.answer_internal_card(
        q.actor,
        q.token,
        Answer::ResourceEntryPayment {
            materials: vec![],
            use_ep,
        },
    )
    .unwrap()
}

fn choose(game: &mut Game, play: CardPlay, accept: bool) -> CardPlay {
    let q = question(play);
    assert!(
        game.internal_decision(View::P2).is_none(),
        "unselected evolve-deck resources stay private"
    );
    let InputDomain::Targets(domain) = &q.domain else {
        panic!("the named Advance entry is a private resolution choice");
    };
    assert_eq!(
        domain.members.len(),
        1,
        "only the own matching Advance is eligible"
    );
    assert_eq!(
        domain.members[0].card.as_deref(),
        Some("incoming"),
        "same-name EvolveFollower and wrong name are excluded"
    );
    assert_eq!(
        domain.cardinality.minimum,
        Nat(0),
        "the player may decline entry"
    );
    let handles = if accept {
        vec![domain.members[0].handle]
    } else {
        vec![]
    };
    game.answer_internal_card(q.actor, q.token, Answer::Targets(handles))
        .unwrap()
}

#[test]
fn advance_activation_substitutes_one_ep_and_shares_the_evolution_quota() {
    let program = program(true, false);
    for (pp, remaining) in [(3_u8, 0_u8), (7, 4)] {
        let mut game = board(&program, pp, 1, false, false);
        let play = game
            .activate_internal_card("source", AbilityId(1), PlayerId { seat: 1 })
            .unwrap();
        resolved(payment(&mut game, play, true));
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(
            view["P1"]["pp"]["current"], remaining,
            "one EP substitutes exactly one printed PP, including an otherwise unaffordable play"
        );
        assert_eq!(view["P1"]["ep"], 0, "one EP is consumed");
        assert_eq!(
            view["objects"]["source"]["power"], 3,
            "the ordinary typed body resolves"
        );
        if remaining == 4 {
            if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
                game.resume_internal_card().unwrap();
            }
            let before = exact_authority(&game);
            let error = game
                .activate_internal_card("second", AbilityId(1), PlayerId { seat: 1 })
                .unwrap_err();
            assert!(
                matches!(error, EngineFailure::IllegalPlay(_)),
                "the second declaration is illegal, rather than an unsupported runtime shape"
            );
            assert_eq!(
                exact_authority(&game),
                before,
                "another Advance cannot be declared despite enough PP for its complete cost"
            );
        }
    }
}

#[test]
fn ordinary_activation_privately_selects_only_the_named_advance_and_respects_capacity() {
    let program = program(false, true);
    for (accept, full) in [(true, false), (false, false), (true, true)] {
        let mut game = board(&program, 3, 1, full, false);
        let play = game
            .activate_internal_card("source", AbilityId(1), PlayerId { seat: 1 })
            .unwrap();
        resolved(choose(&mut game, play, accept));
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(
            view["P1"]["pp"]["current"], 0,
            "ordinary activation pays only its printed PP"
        );
        assert_eq!(view["P1"]["ep"], 1, "ordinary activation cannot spend EP");
        assert_eq!(
            view["P1"]["field"]
                .as_array()
                .unwrap()
                .contains(&json!("incoming")),
            accept && !full,
            "decline or a full field cannot create an entry"
        );
        assert_eq!(
            view["objects"]["incoming"]["generation"],
            u8::from(accept && !full),
            "only a successful movement changes generation"
        );
        assert_eq!(
            view["P2"]["evolve_deck"],
            json!(["opposing"]),
            "the opponent's matching resource never moves"
        );
    }
}

#[test]
fn declining_advance_entry_still_pays_self_banish_pp_ep_and_the_shared_quota() {
    let program = program(true, true);
    let mut game = board(&program, 7, 1, false, false);
    let play = game
        .activate_internal_card("source", AbilityId(1), PlayerId { seat: 1 })
        .unwrap();
    let play = payment(&mut game, play, true);
    resolved(choose(&mut game, play, false));
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(
        view["P1"]["pp"]["current"], 4,
        "PP payment is not refunded when entry is declined"
    );
    assert_eq!(
        view["P1"]["ep"], 0,
        "EP payment is not refunded when entry is declined"
    );
    assert_eq!(
        view["P1"]["banish"],
        json!(["source"]),
        "self-banish is a cost before the private entry decision"
    );
    assert_eq!(
        view["objects"]["incoming"]["generation"], 0,
        "declining leaves the Advance in its original generation"
    );
    if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
    let before = exact_authority(&game);
    let error = game
        .activate_internal_card("second", AbilityId(1), PlayerId { seat: 1 })
        .unwrap_err();
    assert!(
        matches!(error, EngineFailure::IllegalPlay(_)),
        "declining keeps the legal-play quota exhausted"
    );
    assert_eq!(
        exact_authority(&game),
        before,
        "declining does not restore the quota despite enough PP for another full payment"
    );
}

#[test]
fn entry_rejects_other_players_general_evolve_followers_and_unbounded_or_unnamed_choices() {
    program(false, true);
    for (label, key, value) in [
        ("opponent", "who", json!("opp")),
        ("ordinary evolve follower", "kind", json!("evolve_follower")),
        ("no Advance restriction", "advance_follower", Value::Null),
        ("no printed name", "name", Value::Null),
    ] {
        let mut input = tree(false, true);
        let from =
            input["cards"]["unit-follower"]["abilities"][0]["activate"]["do"][0]["choose"]["from"]
                .as_object_mut()
                .unwrap();
        if value.is_null() {
            from.remove(key);
        } else {
            from.insert(key.into(), value);
        }
        let error = lowerer().lower(&input).unwrap_err();
        assert_eq!(
            error.code, "unsupported_construct",
            "{label} fails the closed capability: {error}"
        );
        assert!(
            error.path == "/cards/unit-follower/abilities/0" || error.path.ends_with("/do/0"),
            "{label} is rejected during decode or at the private choice: {error}"
        );
    }
    let mut input = tree(false, true);
    input["cards"]["unit-follower"]["abilities"][0]["activate"]["do"][0]["choose"]["n"] = json!(2);
    let error = lowerer().lower(&input).unwrap_err();
    assert_eq!(
        error.code, "unsupported_construct",
        "the capability cannot select multiple resources"
    );
    assert!(
        error.path.ends_with("/do/0"),
        "the valid producer's changed bounds are rejected at choice: {error}"
    );
}

fn checked_tree(input: &Value) -> WellFormedCardProgram {
    lowerer()
        .lower(input)
        .unwrap()
        .remove("unit-follower")
        .unwrap()
}

fn second_printed_ability(mut ability: Value) -> Value {
    ability["line"] = json!(4);
    ability["sources"][0]["ability_scope"] = json!("ability_1");
    ability["sources"][0]["occurrence"]["line_ordinal"] = json!(3);
    ability
}

fn with_ordinary_evolution() -> WellFormedCardProgram {
    let mut input = tree(true, false);
    let mut evolution =
        second_printed_ability(input["cards"]["unit-follower"]["abilities"][0].clone());
    evolution.as_object_mut().unwrap().remove("activate");
    evolution["evolve"] = json!({"cost":{"pp":1}});
    input["cards"]["unit-follower"]["abilities"]
        .as_array_mut()
        .unwrap()
        .push(evolution);
    checked_tree(&input)
}

fn check_timing(game: &mut Game) {
    if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}

#[test]
fn advance_and_ordinary_evolution_exclude_each_other_in_both_directions() {
    let program = with_ordinary_evolution();
    for advance_first in [false, true] {
        let mut game = board(&program, 7, 1, false, false);
        let normal =
            json!({"do":"evolve","source":"second","evolve_card":"normal-form","pay":{"pp":1}});
        if advance_first {
            let play = game
                .activate_internal_card("source", AbilityId(1), PlayerId { seat: 1 })
                .unwrap();
            resolved(payment(&mut game, play, true));
            check_timing(&mut game);
            let before = game.projection(View::Referee).unwrap();
            let refused = game.internal_rule_action(&normal).unwrap();
            assert_eq!(
                refused.outcome, "cannot-evolve",
                "Advance used the ordinary evolution quota"
            );
            assert!(
                refused.events.is_empty(),
                "a rejected evolution produces no events"
            );
            assert_eq!(
                game.projection(View::Referee).unwrap(),
                before,
                "the failed evolution has no public state changes"
            );
        } else {
            let evolved = game.internal_rule_action(&normal).unwrap();
            assert_eq!(
                evolved.outcome, "resolved",
                "the ordinary evolution control is valid"
            );
            let view = game.projection(View::Referee).unwrap();
            assert_eq!(
                view["objects"]["second"]["evolved_with"], "normal-form",
                "the printed ordinary evolution attaches its same-name EvolveFollower"
            );
            assert_eq!(
                view["P1"]["pp"]["current"], 6,
                "ordinary evolution paid its printed one PP"
            );
            assert_eq!(
                view["P1"]["ep"], 1,
                "this ordinary evolution control did not pay EP"
            );
            check_timing(&mut game);
            let before = exact_authority(&game);
            let error = game
                .activate_internal_card("source", AbilityId(1), PlayerId { seat: 1 })
                .unwrap_err();
            assert!(
                matches!(error, EngineFailure::IllegalPlay(_)),
                "ordinary evolution prevents Advance declaration"
            );
            assert_eq!(
                exact_authority(&game),
                before,
                "Advance rejection is atomic despite enough PP and EP"
            );
        }
    }
}

#[test]
fn ordinary_activation_entry_preserves_the_evolution_equivalent_quota() {
    let mut input = tree(false, true);
    input["cards"]["unit-follower"]["abilities"]
        .as_array_mut()
        .unwrap()
        .push(second_printed_ability(
            tree(true, false)["cards"]["unit-follower"]["abilities"][0].clone(),
        ));
    let program = checked_tree(&input);
    let advance = program
        .card()
        .abilities
        .iter()
        .find(|ability| {
            matches!(
                ability.node.source.origin,
                AbilityOrigin::Keyword {
                    keyword: KeywordId::AdvanceActivate,
                    ..
                }
            )
        })
        .unwrap()
        .node
        .id;
    let mut game = board(&program, 7, 0, false, false);
    let play = game
        .activate_internal_card("source", AbilityId(1), PlayerId { seat: 1 })
        .unwrap();
    resolved(choose(&mut game, play, true));
    check_timing(&mut game);
    let play = game
        .activate_internal_card("second", advance, PlayerId { seat: 1 })
        .unwrap();
    resolved(payment(&mut game, play, false));
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(
        view["P1"]["pp"]["current"], 0,
        "both independent printed costs were paid"
    );
    assert_eq!(
        view["objects"]["second"]["power"], 3,
        "Advance can resolve after ordinary entry"
    );
}

#[test]
fn insufficient_advance_pp_and_forged_resource_materials_do_not_change_authority() {
    let program = program(true, false);
    let mut poor = board(&program, 3, 0, false, false);
    let before = exact_authority(&poor);
    let error = poor
        .activate_internal_card("source", AbilityId(1), PlayerId { seat: 1 })
        .unwrap_err();
    assert!(
        matches!(error, EngineFailure::IllegalPlay(_)),
        "EP-free payment cannot invent the fourth PP"
    );
    assert_eq!(
        exact_authority(&poor),
        before,
        "unaffordable declaration rolls back its identities and state"
    );
    let mut game = board(&program, 3, 1, false, false);
    let q = question(
        game.activate_internal_card("source", AbilityId(1), PlayerId { seat: 1 })
            .unwrap(),
    );
    let before = exact_authority(&game);
    let error = game
        .answer_internal_card(
            q.actor,
            q.token,
            Answer::ResourceEntryPayment {
                materials: vec![0],
                use_ep: true,
            },
        )
        .unwrap_err();
    assert!(
        matches!(error, EngineFailure::Invalid(_)),
        "Advance cannot pay a fabricated Meal or Ride item"
    );
    assert_eq!(
        exact_authority(&game),
        before,
        "invalid payment preserves the original question"
    );
    resolved(
        game.answer_internal_card(
            q.actor,
            q.token,
            Answer::ResourceEntryPayment {
                materials: vec![],
                use_ep: true,
            },
        )
        .unwrap(),
    );
}

#[test]
fn face_up_advance_resources_remain_eligible_for_named_reentry() {
    let program = program(false, true);
    let mut game = board(&program, 3, 0, false, true);
    let play = game
        .activate_internal_card("source", AbilityId(1), PlayerId { seat: 1 })
        .unwrap();
    resolved(choose(&mut game, play, true));
    let view = game.projection(View::Referee).unwrap();
    assert!(
        view["P1"]["field"]
            .as_array()
            .unwrap()
            .contains(&json!("incoming")),
        "CR9.2.2 returned face-up Advance cards are reusable resources"
    );
    assert_eq!(
        view["objects"]["incoming"]["generation"], 1,
        "the new entry gets a fresh field generation"
    );
}

#[test]
fn fabricated_and_replayed_private_entry_answers_preserve_authority() {
    let program = program(false, true);
    let mut game = board(&program, 3, 0, false, false);
    let q = question(
        game.activate_internal_card("source", AbilityId(1), PlayerId { seat: 1 })
            .unwrap(),
    );
    let before = exact_authority(&game);
    let error = game
        .answer_internal_card(q.actor, q.token, Answer::Targets(vec![u64::MAX]))
        .unwrap_err();
    assert!(
        matches!(error, EngineFailure::Invalid(_)),
        "a handle outside this private question is invalid"
    );
    assert_eq!(
        exact_authority(&game),
        before,
        "an invalid private answer cannot consume the selection"
    );
    let InputDomain::Targets(domain) = &q.domain else {
        panic!("private entry domain");
    };
    resolved(
        game.answer_internal_card(
            q.actor,
            q.token,
            Answer::Targets(vec![domain.members[0].handle]),
        )
        .unwrap(),
    );
    let before = exact_authority(&game);
    let error = game
        .answer_internal_card(
            q.actor,
            q.token,
            Answer::Targets(vec![domain.members[0].handle]),
        )
        .unwrap_err();
    assert!(
        matches!(error, EngineFailure::Invalid(_)),
        "the consumed private token is invalid"
    );
    assert_eq!(
        exact_authority(&game),
        before,
        "the old decision token cannot move the resource twice"
    );
}

#[test]
fn a_private_evolve_deck_binding_cannot_escape_its_adjacent_field_move() {
    program(false, true);
    for to in ["hand", "cemetery", "ex"] {
        let mut input = tree(false, true);
        input["cards"]["unit-follower"]["abilities"][0]["activate"]["do"][1]["move"]["to"] =
            json!(to);
        let error = lowerer().lower(&input).unwrap_err();
        assert_eq!(
            error.code, "unsupported_construct",
            "the closed entry cannot grant another hidden-zone operation"
        );
        assert!(
            error.path.ends_with("/do/1"),
            "the changed consumer is rejected: {error}"
        );
    }
    let mut input = tree(false, true);
    input["cards"]["unit-follower"]["abilities"][0]["activate"]["do"]
        .as_array_mut()
        .unwrap()
        .push(json!({"buff":{"on":"incoming","power":1}}));
    let error = lowerer().lower(&input).unwrap_err();
    assert_eq!(
        error.code, "unsupported_construct",
        "the local private binding has no later consumer"
    );
    assert!(
        error.path.ends_with("/do/2"),
        "the escaping consumer is rejected: {error}"
    );
}

#[test]
fn advance_use_prerequisites_are_checked_before_any_payment_question() {
    for (threshold, allowed) in [(2_u8, true), (3, false)] {
        let mut input = tree(true, false);
        input["cards"]["unit-follower"]["abilities"][0]["activate"]["usable_if"] = json!({"cmp":{
            "left":{"count":{"who":"self","zone":"field","kind":"follower"}},"op":"ge","right":threshold
        }});
        let program = checked_tree(&input);
        let mut game = board(&program, 7, 0, false, false);
        if allowed {
            let play = game
                .activate_internal_card("source", AbilityId(1), PlayerId { seat: 1 })
                .unwrap();
            resolved(payment(&mut game, play, false));
            assert_eq!(
                game.projection(View::Referee).unwrap()["objects"]["source"]["power"],
                3,
                "the satisfied prerequisite has a valid ordinary-body control"
            );
        } else {
            let before = exact_authority(&game);
            let error = game
                .activate_internal_card("source", AbilityId(1), PlayerId { seat: 1 })
                .unwrap_err();
            assert!(
                matches!(error, EngineFailure::IllegalPlay(_)),
                "an unsatisfied printed prerequisite prevents declaration"
            );
            assert_eq!(
                exact_authority(&game),
                before,
                "no payment question or cost is committed"
            );
        }
    }
}

#[test]
fn advance_activation_requires_its_controllers_own_main_timing() {
    let program = program(true, false);
    for timing in [("P2", "main"), ("P1", "end")] {
        let mut game = board_at(&program, 7, 1, false, false, Some(timing));
        let before = exact_authority(&game);
        let error = game
            .activate_internal_card("source", AbilityId(1), PlayerId { seat: 1 })
            .unwrap_err();
        assert!(
            matches!(error, EngineFailure::Unsupported(_)),
            "the existing authority timing gate rejects this declaration"
        );
        assert_eq!(
            exact_authority(&game),
            before,
            "wrong timing cannot spend resources or allocate a live question"
        );
    }
}

#[test]
fn forged_ep_substitution_without_ep_preserves_the_payment_question() {
    let program = program(true, false);
    let mut game = board(&program, 4, 0, false, false);
    let q = question(
        game.activate_internal_card("source", AbilityId(1), PlayerId { seat: 1 })
            .unwrap(),
    );
    let before = exact_authority(&game);
    let error = game
        .answer_internal_card(
            q.actor,
            q.token,
            Answer::ResourceEntryPayment {
                materials: vec![],
                use_ep: true,
            },
        )
        .unwrap_err();
    assert!(
        matches!(error, EngineFailure::Invalid(_)),
        "an unavailable EP option cannot be forged"
    );
    assert_eq!(
        exact_authority(&game),
        before,
        "the rejected answer preserves the original PP-only question"
    );
    resolved(
        game.answer_internal_card(
            q.actor,
            q.token,
            Answer::ResourceEntryPayment {
                materials: vec![],
                use_ep: false,
            },
        )
        .unwrap(),
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        0,
        "the original legal answer still pays the full PP cost"
    );
}

#[test]
fn named_entry_rejects_an_unused_movement_result_instead_of_silently_discarding_it() {
    program(false, true);
    let mut input = tree(false, true);
    input["cards"]["unit-follower"]["abilities"][0]["activate"]["do"][1]["move"]["result"] =
        json!("ignored");
    let error = lowerer().lower(&input).unwrap_err();
    assert_eq!(
        error.code, "unsupported_construct",
        "unsupported receipt exports are never silently accepted"
    );
    assert!(
        error.path.ends_with("/do/1"),
        "the changed valid consumer is rejected: {error}"
    );
}

#[test]
fn activation_prohibition_also_blocks_advance_despite_an_evolution_exception() {
    for except_evolution in [false, true] {
        let mut input = tree(true, false);
        let mut predicate = json!({"kind":"activated"});
        if except_evolution {
            predicate["except"] = json!(["evolution"]);
        }
        let mut lock =
            second_printed_ability(input["cards"]["unit-follower"]["abilities"][0].clone());
        lock["activate"] = json!({"cost":{"pp":0},"do":[{"apply":{"on":"this",
            "effect":{"cannot":{"what":"play_ability","ability":predicate}},"until":"turn"}}]});
        input["cards"]["unit-follower"]["abilities"]
            .as_array_mut()
            .unwrap()
            .push(lock);
        let program = lowerer()
            .lower(&input)
            .unwrap()
            .remove("unit-follower")
            .unwrap();
        let actor = PlayerId { seat: 1 };
        let mut game = board(&program, 7, 1, false, false);
        resolved(
            game.activate_internal_card("source", program.card().abilities[1].node.id, actor)
                .unwrap(),
        );
        game.resume_internal_card().unwrap();
        let before = exact_authority(&game);
        let error = game
            .activate_internal_card("source", program.card().abilities[0].node.id, actor)
            .unwrap_err();
        assert!(
            matches!(error, EngineFailure::IllegalPlay(_)),
            "even an evolution exception forbids Advance activation"
        );
        assert_eq!(
            exact_authority(&game),
            before,
            "forbidden Advance pays nothing and asks no question"
        );
        let play = game
            .activate_internal_card("second", program.card().abilities[0].node.id, actor)
            .unwrap();
        resolved(payment(&mut game, play, true));
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
            4_u8,
            "the other unlocked copy can still substitute EP for one PP"
        );
    }
}
