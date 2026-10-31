#![expect(
    clippy::panic,
    reason = "Unexpected questions invalidate the printed meal scenarios."
)]
use super::*;

fn position(card: &str, carrots: u8) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":card}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!(
        (0..carrots)
            .map(|i| json!({"id":format!("carrot-{i}"),"card":"CSD01-032"}))
            .collect::<Vec<_>>()
    );
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    initial
}

fn from_position(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "meal-race",
    )
    .unwrap()
}

fn definition(card: &str, index: usize) -> AbilityId {
    selection_with_rules().current[card]
        .program
        .card()
        .abilities
        .iter()
        .filter(|ability| {
            matches!(
                ability.node.source.origin,
                AbilityOrigin::Keyword {
                    keyword: KeywordId::Meal,
                    ..
                }
            )
        })
        .nth(index)
        .unwrap()
        .node
        .id
}

pub(super) fn feed(game: &mut Game, card: &str, index: usize, ep: bool) -> Activation {
    let CardPlay::Awaiting(question) = game
        .activate_internal_card("source", definition(card, index), PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("meal payment");
    };
    let InputDomain::MealPayment {
        materials,
        ep_allowed,
    } = &question.domain
    else {
        panic!("meal resources");
    };
    assert!(
        !ep || *ep_allowed,
        "the printed meal permits this EP substitution"
    );
    let count = usize::try_from(materials.cardinality.minimum.0).unwrap();
    let handles = materials
        .members
        .iter()
        .take(count)
        .map(|member| member.handle)
        .collect();
    resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::MealPayment {
                materials: handles,
                use_ep: ep,
            },
        )
        .unwrap(),
    )
}

pub(super) fn settle(game: &mut Game) {
    loop {
        if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
        let Some(pending) = game.internal_pending_choice().unwrap() else {
            break;
        };
        let InputDomain::Pending(options) = &pending.domain else {
            panic!("race pending");
        };
        let mut play = game
            .answer_internal_card(
                pending.actor,
                pending.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap();
        while let CardPlay::Awaiting(question) = play {
            let answer = match &question.domain {
                InputDomain::Targets(domain) => Answer::Targets(
                    domain
                        .members
                        .first()
                        .map(|member| vec![member.handle])
                        .unwrap_or_default(),
                ),
                InputDomain::OptionalCost => Answer::OptionalCost(false),
                InputDomain::DriveTrigger { .. }
                | InputDomain::TokenCapacity { .. }
                | InputDomain::MealPayment { .. }
                | InputDomain::WardEntry { .. }
                | InputDomain::Quick { .. }
                | InputDomain::CostMaterials(_)
                | InputDomain::BeforeDraw
                | InputDomain::Pending(_)
                | InputDomain::TriggerQuota(_)
                | InputDomain::Integer(_)
                | InputDomain::Order(_)
                | InputDomain::Search(_)
                | InputDomain::SearchPartition(_)
                | InputDomain::Distribution(_)
                | InputDomain::SearchReplacement
                | InputDomain::Modes { .. } => panic!("printed race target"),
            };
            play = game
                .answer_internal_card(question.actor, question.token, answer)
                .unwrap();
        }
        resolved(play);
    }
}

#[test]
fn cp01_022_printings_pay_one_carrot_and_gain_rush_without_evolving() {
    for card in ["CP01-022", "CP01-P16"] {
        let mut game = from_position(&position(card, 1));
        let result = feed(&mut game, card, 0, false);
        assert_eq!(
            result
                .progress
                .facts
                .iter()
                .filter(|fact| matches!(fact, Fact::Raced(_)))
                .count(),
            1
        );
        assert!(
            !result
                .progress
                .facts
                .iter()
                .any(|fact| matches!(fact, Fact::Evolved(_)))
        );
        settle(&mut game);
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["P1"]["pp"]["current"], 9_u8);
        assert_eq!(view["P1"]["race"], json!(["carrot-0"]));
        assert_eq!(
            view["objects"]["source"]["keywords"],
            json!(["必殺", "突進"])
        );
        game.activate_internal_card("source", definition(card, 0), PlayerId { seat: 1 })
            .unwrap_err();
    }
}

#[test]
fn cp01_025_printings_use_ep_and_choose_the_race_target_before_acting_it() {
    for card in ["CP01-025", "CP01-P19", "CSD01-018"] {
        let mut initial = position(card, 1);
        initial["players"]["P1"]["ep"] = json!(1_u8);
        initial["players"]["P1"]["pp"]["current"] = json!(0_u8);
        let mut game = from_position(&initial);
        feed(&mut game, card, 0, true);
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["enemy"]["acted"],
            false
        );
        settle(&mut game);
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["P1"]["ep"], 0_u8);
        assert_eq!(view["P1"]["pp"]["current"], 0_u8);
        assert_eq!(view["objects"]["enemy"]["acted"], true);
    }
}

#[test]
fn cp01_042_three_carrots_produce_three_independently_resolved_stat_gains() {
    for card in ["CP01-042", "CP01-SL12", "CP01-SP12"] {
        let mut game = from_position(&position(card, 3));
        let result = feed(&mut game, card, 2, false);
        assert_eq!(
            result
                .progress
                .facts
                .iter()
                .filter(|fact| matches!(fact, Fact::Raced(_)))
                .count(),
            3
        );
        settle(&mut game);
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["objects"]["source"]["power"], 10_u8);
        assert_eq!(view["objects"]["source"]["hp"], 7_u8);
        assert_eq!(view["P1"]["pp"]["current"], 7_u8);
    }
}

#[test]
fn three_races_cannot_be_paid_with_two_unused_carrots() {
    let mut initial = position("CP01-042", 3);
    initial["players"]["P1"]["zones"]["evolve_deck"][2]["face_up"] = json!(true);
    let mut game = from_position(&initial);
    let before = game.projection(View::Referee).unwrap();
    game.activate_internal_card("source", definition("CP01-042", 2), PlayerId { seat: 1 })
        .unwrap_err();
    assert_eq!(game.projection(View::Referee).unwrap(), before);
    let mut fewer_carrots = from_position(&position("CP01-042", 2));
    fewer_carrots
        .activate_internal_card("source", definition("CP01-042", 2), PlayerId { seat: 1 })
        .unwrap_err();
}

#[test]
fn meal_resource_answers_reject_duplicates_and_wrong_actors_without_spending() {
    let mut game = from_position(&position("CP01-042", 3));
    let CardPlay::Awaiting(question) = game
        .activate_internal_card("source", definition("CP01-042", 2), PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("meal payment");
    };
    let before = game.projection(View::Referee).unwrap();
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::MealPayment {
            materials: vec![0, 1, 2],
            use_ep: true,
        },
    )
    .unwrap_err();
    assert_eq!(game.projection(View::Referee).unwrap(), before);
    for (actor, materials) in [
        (PlayerId { seat: 2 }, vec![0, 1, 2]),
        (PlayerId { seat: 1 }, vec![0, 0, 2]),
        (PlayerId { seat: 1 }, vec![0, 1, 99]),
    ] {
        game.answer_internal_card(
            actor,
            question.token,
            Answer::MealPayment {
                materials,
                use_ep: false,
            },
        )
        .unwrap_err();
        assert_eq!(game.projection(View::Referee).unwrap(), before);
    }
    assert!(game.internal_decision(View::P1).is_some());
    assert!(game.internal_decision(View::P2).is_none());
    resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::MealPayment {
                materials: vec![0, 1, 2],
                use_ep: false,
            },
        )
        .unwrap(),
    );
    settle(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        7_u8
    );
}

#[test]
fn leaving_the_field_returns_all_meal_resources_face_up() {
    let mut initial = position("CP01-042", 3);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"offering","card":"BP01-140"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"drawn","card":"BP01-171"}]);
    let mut game = from_position(&initial);
    feed(&mut game, "CP01-042", 2, false);
    settle(&mut game);
    let CardPlay::Awaiting(question) = game
        .play_internal_card("offering", PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("offering target");
    };
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("public offering");
    };
    let source = domain
        .members
        .iter()
        .find(|member| member.card.as_deref() == Some("CP01-042"))
        .unwrap();
    resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Targets(vec![source.handle]),
        )
        .unwrap(),
    );
    settle(&mut game);
    let view = game.projection(View::Referee).unwrap();
    assert!(
        view["P1"]["cemetery"]
            .as_array()
            .unwrap()
            .contains(&json!("source"))
    );
    assert_eq!(view["P1"]["evolve_deck"].as_array().unwrap().len(), 3);
    for i in 0_u8..3 {
        assert!(
            view["P1"]["evolve_deck"]
                .as_array()
                .unwrap()
                .contains(&json!(format!("carrot-{i}")))
        );
        assert_eq!(view["objects"][format!("carrot-{i}")]["face_up"], true);
    }
}

#[test]
fn meal_and_evolution_share_the_same_turn_quota() {
    let mut initial = position("CP01-022", 1);
    initial["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"evolving","card":"BP02-118"}));
    initial["players"]["P1"]["zones"]["evolve_deck"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"form","card":"BP02-119"}));
    let mut meal_first = from_position(&initial);
    feed(&mut meal_first, "CP01-022", 0, false);
    settle(&mut meal_first);
    let evolution =
        json!({"do":"evolve","source":"evolving","evolve_card":"form","pay":{"pp":3_u8}});
    assert_eq!(
        meal_first.internal_rule_action(&evolution).unwrap().outcome,
        "cannot-evolve"
    );
    let mut evolution_first = from_position(&initial);
    assert_eq!(
        evolution_first
            .internal_rule_action(&evolution)
            .unwrap()
            .outcome,
        "resolved"
    );
    settle(&mut evolution_first);
    evolution_first
        .activate_internal_card("source", definition("CP01-022", 0), PlayerId { seat: 1 })
        .unwrap_err();
}

#[test]
fn zero_races_do_not_require_a_meal_link_or_grant_rush() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    let mut game = with_producer(
        &initial,
        Action::MealRace {
            subject: ObjectOperand::SourceHolder {},
            operation: MealRaceOperation::Race,
            amount: Expr::Literal(Nat(0)),
            resource_links: vec![],
        },
    );
    let result = resolved(
        game.activate_internal_card("producer", AbilityId(1), PlayerId { seat: 1 })
            .unwrap(),
    );
    assert!(
        !result
            .progress
            .facts
            .iter()
            .any(|fact| matches!(fact, Fact::Raced(_)))
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["producer"]["keywords"],
        json!([])
    );
}
