#![expect(
    clippy::panic,
    reason = "Unexpected questions invalidate the printed discard scenarios."
)]
use super::event_cemetery::finish_checks;
use super::event_subject_capture::prepared_program;
use super::*;

fn discard_all() -> WellFormedCardProgram {
    producer(Action::Discard {
        player: PlayerRef::Controller {},
        mode: DiscardMode::All {
            eligible: Selector::Zone {
                players: PlayerSet {
                    relation: PlayerRelation::SelfPlayer,
                    relative_to: PlayerRef::Controller {},
                    cardinality: Cardinality {
                        minimum: Nat(1),
                        maximum: Some(Nat(1)),
                    },
                    text_quantifier: TextQuantifier::All,
                },
                zone: ZoneKind::Hand,
                view: SelectionView::Current {},
                binding: Binding::new(400, ScopeId(1)),
                predicate: Box::new(Expr::Literal(Bool(true))),
            },
        },
    })
}

fn initial(card: &str, hand: bool) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = if hand {
        json!([])
    } else {
        json!([{"id":"source","card":card}])
    };
    initial["players"]["P1"]["zones"]["hand"] = if hand {
        json!([{"id":"source","card":card}])
    } else {
        json!([{"id":"h0","card":"subject-draw"},{"id":"h1","card":"subject-low"}])
    };
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([]);
    initial["players"]["P1"]["zones"]["deck"] =
        json!([{"id":"drawn","card":"subject-draw"},{"id":"spare","card":"subject-draw"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([]);
    initial["players"]["P2"]["zones"]["hand"] = json!([]);
    initial
}

fn pending(game: &mut Game, card: &str, count: usize) -> CardPlay {
    finish_checks(game);
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("printed trigger order")
    };
    assert_eq!(
        options.len(),
        count,
        "each committed discard supplies one occurrence"
    );
    let option = options.iter().find(|option| option.card == card).unwrap();
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::Pending(option.handle),
    )
    .unwrap()
}

fn pay(game: &mut Game, play: CardPlay) -> CardPlay {
    if let CardPlay::Awaiting(question) = play {
        assert_eq!(
            question.domain,
            InputDomain::OptionalCost,
            "only the resolution payment can intervene"
        );
        game.answer_internal_card(question.actor, question.token, Answer::OptionalCost(true))
            .unwrap()
    } else {
        play
    }
}

fn discard(game: &mut Game, card: &str, count: usize) {
    finish_checks(game);
    resolved(
        game.activate_internal_card("producer", AbilityId(1), PlayerId { seat: 1 })
            .unwrap(),
    );
    for remaining in (1..=count).rev() {
        let play = pending(game, card, remaining);
        let play = pay(game, play);
        if !matches!(play, CardPlay::Cancelled) {
            resolved(play);
        }
    }
    finish_checks(game);
    assert!(
        game.internal_pending_choice().unwrap().is_none(),
        "all discard occurrences are consumed"
    );
}

#[test]
fn light_path_all_printings_draw_after_self_discard_and_keep_their_normal_spell() {
    for card in ["BP01-170", "PR-016", "PR-210"] {
        for discarded in [false, true] {
            let mut game = prepared_program(&initial(card, true), &discard_all());
            if discarded {
                discard(&mut game, card, 1);
            } else {
                let activation = resolved(
                    game.play_internal_card("source", PlayerId { seat: 1 })
                        .unwrap(),
                );
                assert!(
                    !activation
                        .progress
                        .facts
                        .iter()
                        .any(|fact| matches!(fact, Fact::Discard(_)))
                );
                finish_checks(&mut game);
                assert!(game.internal_pending_choice().unwrap().is_none());
            }
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["P1"]["hand"], json!(["drawn"]));
            assert_eq!(state["P1"]["cemetery"], json!(["source"]));
            assert_eq!(state["P1"]["deck"], json!(["spare"]));
        }
    }
}

#[test]
fn heavenly_dragoon_all_base_printings_trigger_for_both_discards_but_pay_only_once() {
    for card in ["BP15-060", "BP15-SL18", "BP22-R07"] {
        let mut game = prepared_program(&initial(card, false), &discard_all());
        discard(&mut game, card, 2);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["hand"], json!(["drawn"]));
        assert_eq!(state["objects"]["source"]["acted"], true);
        assert_eq!(state["P1"]["cemetery"], json!(["h0", "h1"]));
        assert_eq!(state["P1"]["deck"], json!(["spare"]));
    }
}

fn evolved_position(base: &str, form: &str) -> Value {
    let mut initial = initial(base, false);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"eligible","card":"BP15-074","state":{"cost":0_u8}},
        {"id":"low","card":"BP15-060","state":{"cost":8_u8}},
        {"id":"wrong-class","card":"capture-base"},
        {"id":"other","card":"subject-draw"},
        {"id":"last-seen","card":"subject-low"},
        {"id":"drawn","card":"subject-draw"},{"id":"spare","card":"subject-draw"}
    ]);
    initial
}

fn evolve_and_take_high_dragon(game: &mut Game, form: &str) {
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
        )
        .unwrap()
        .outcome,
        "resolved",
        "the current base performs its real printed evolution"
    );
    let CardPlay::Awaiting(question) = pending(game, form, 1) else {
        panic!("printed evolution Look")
    };
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("private observed choice")
    };
    assert_eq!(
        domain.cardinality.minimum,
        Nat(0),
        "the printed Look choice is optional"
    );
    assert_eq!(
        domain.cardinality.maximum,
        Some(Nat(1)),
        "at most one observed card is chosen"
    );
    assert_eq!(
        domain.members.len(),
        1,
        "original cost and Dragon class leave only one candidate"
    );
    assert_eq!(
        domain.members[0].card.as_deref(),
        Some("BP15-074"),
        "a low current cost does not hide the originally eight-cost Dragon"
    );
    assert!(
        game.internal_decision(View::P2).is_none(),
        "only the controller sees the Look candidates"
    );
    let CardPlay::Awaiting(order) = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Targets(vec![domain.members[0].handle]),
        )
        .unwrap()
    else {
        panic!("Look remainder order")
    };
    let InputDomain::Order(cards) = &order.domain else {
        panic!("private remainder")
    };
    assert_eq!(
        cards.len(),
        4,
        "only the four unchosen observed cards are ordered"
    );
    resolved(
        game.answer_internal_card(
            order.actor,
            order.token,
            Answer::Order(cards.iter().rev().map(|card| card.handle).collect()),
        )
        .unwrap(),
    );
    finish_checks(game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["hand"],
        json!(["h0", "h1", "eligible"]),
        "the chosen Dragon joins the original discard materials"
    );
}

#[test]
fn heavenly_dragoon_all_evolved_printings_keep_look_filters_and_per_card_discard_triggers() {
    for (base, form) in [
        ("BP15-060", "BP15-061"),
        ("BP15-SL18", "BP15-SL19"),
        ("BP22-R07", "BP22-R08"),
    ] {
        let mut game = prepared_program(&evolved_position(base, form), &discard_all());
        evolve_and_take_high_dragon(&mut game, form);
        discard(&mut game, form, 3);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["hand"], json!(["drawn"]));
        assert_eq!(state["objects"]["source"]["acted"], true);
        assert_eq!(state["P1"]["cemetery"], json!(["h0", "h1", "eligible"]));
        assert_eq!(
            state["P1"]["deck"],
            json!(["spare", "last-seen", "other", "wrong-class", "low"])
        );
    }
}

#[test]
fn real_hand_activation_costs_publish_both_dragoon_occurrences_after_the_outer_body() {
    let mut initial_state = initial("BP15-060", false);
    initial_state["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"payer","card":"BP15-074"},{"id":"material","card":"BP01-080"}
    ]);
    initial_state["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"d0","card":"subject-draw"},{"id":"d1","card":"subject-draw"},
        {"id":"d2","card":"subject-draw"},{"id":"d3","card":"subject-draw"}
    ]);
    let mut game = prepared_program(&initial_state, &discard_all());
    let ability = selection().current["BP15-074"]
        .program
        .card()
        .abilities
        .iter()
        .find(|ability| {
            ability.node.kind == AbilityKind::Activated
                && ability.node.active_zones == [ZoneKind::Hand]
        })
        .unwrap()
        .node
        .id;
    let mut outer_play = game
        .activate_internal_card("payer", ability, PlayerId { seat: 1 })
        .unwrap();
    while let CardPlay::Awaiting(question) = outer_play {
        let (InputDomain::CostMaterials(domain) | InputDomain::Targets(domain)) = &question.domain
        else {
            panic!("the real hand activation only selects its second discard material");
        };
        assert_eq!(domain.members.len(), 1);
        assert_eq!(domain.members[0].card.as_deref(), Some("BP01-080"));
        let answer = if matches!(&question.domain, InputDomain::CostMaterials(_)) {
            Answer::CostMaterials(vec![domain.members[0].handle])
        } else {
            Answer::Targets(vec![domain.members[0].handle])
        };
        outer_play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    let activation = resolved(outer_play);
    assert_eq!(
        activation
            .progress
            .facts
            .iter()
            .filter(|fact| matches!(fact, Fact::Discard(_)))
            .count(),
        2
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["hand"],
        json!(["d0", "d1"])
    );
    for remaining in [2_usize, 1_usize] {
        let play = pending(&mut game, "BP15-060", remaining);
        let play = pay(&mut game, play);
        if !matches!(play, CardPlay::Cancelled) {
            resolved(play);
        }
    }
    finish_checks(&mut game);
    let projection = game.projection(View::Referee).unwrap();
    assert_eq!(projection["P1"]["hand"], json!(["d0", "d1", "d2"]));
    assert_eq!(projection["P1"]["deck"], json!(["d3"]));
    assert_eq!(projection["P1"]["cemetery"], json!(["payer", "material"]));
    assert_eq!(projection["objects"]["source"]["acted"], true);
    assert_eq!(projection["P1"]["pp"]["current"], 9_u8);
}

#[test]
fn light_path_end_limit_discard_waits_for_its_trigger_and_rechecks_the_hand_limit() {
    let mut state = initial("BP01-170", true);
    for i in 0_u8..7 {
        state["players"]["P1"]["zones"]["hand"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":format!("keep{i}"),"card":"subject-draw"}));
    }
    state["players"]["P2"]["zones"]["deck"] = json!([{"id":"enemy-draw","card":"subject-draw"}]);
    let mut game = prepared_program(&state, &discard_all());
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    while let Some(question) = game.internal_quick_choice().unwrap() {
        game.answer_internal_card(question.actor, question.token, Answer::Quick(None))
            .unwrap();
        finish_checks(&mut game);
    }
    game.internal_rule_action(&json!({"do":"end-discard","select":["source"]}))
        .unwrap();
    assert_eq!(
        game.projection(View::Referee).unwrap()["turn"]["active"],
        "P1",
        "the pending discard trigger keeps the ending turn open"
    );
    resolved(pending(&mut game, "BP01-170", 1));
    finish_checks(&mut game);
    let projection = game.projection(View::Referee).unwrap();
    assert_eq!(
        projection["P1"]["hand"].as_array().unwrap().len(),
        8,
        "the triggered draw exceeds the hand limit again"
    );
    assert_eq!(projection["P1"]["cemetery"], json!(["source"]));
    game.internal_rule_action(&json!({"do":"end-discard","select":["drawn"]}))
        .unwrap();
    assert_eq!(
        game.projection(View::Referee).unwrap()["turn"]["active"],
        "P2",
        "the turn advances after a second ordinary limit discard"
    );
}
