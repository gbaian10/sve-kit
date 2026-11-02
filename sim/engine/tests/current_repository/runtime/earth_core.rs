#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected questions invalidate the printed Earth Rite scenarios."
)]
use super::*;
use alloc::collections::BTreeMap;
use sve_engine::Result;
use sve_engine::game::internal::InstructionOutcome;

fn initial(card: &str, field: bool, soil: bool) -> Value {
    let mut value = draw_position();
    value["players"]["P1"]["zones"]["field"] = json!([]);
    value["players"]["P1"]["zones"][if field { "field" } else { "hand" }] =
        json!([{"id":"source","card":card}]);
    if soil {
        let cards = value["players"]["P1"]["zones"]["field"]
            .as_array_mut()
            .unwrap();
        cards.push(json!({"id":"soil","card":"BP01-075","state":{"acted":true,"counters":{"stack_counter":1_u8}}}));
    }
    value["players"]["P1"]["zones"]["ex"] =
        json!([{"id":"ex-soil","card":"BP01-075","state":{"counters":{"stack_counter":5_u8}}}]);
    value["players"]["P2"]["zones"]["field"] = json!([
        {"id":"enemy","card":"BP01-097"},
        {"id":"opposing-soil","card":"BP01-075","state":{"counters":{"stack_counter":5_u8}}}
    ]);
    value
}

#[expect(
    clippy::panic_in_result_fn,
    reason = "Malformed fixture questions must not masquerade as an expected payment rejection."
)]
fn finish(game: &mut Game, mut play: CardPlay, accepted: bool) -> Result<CardPlay> {
    while let CardPlay::Awaiting(question) = play {
        let answer = match &question.domain {
            InputDomain::OptionalCost => Answer::OptionalCost(accepted),
            InputDomain::Targets(domain) => {
                assert_eq!(
                    domain.members.len(),
                    1,
                    "the printed target is still required when declining Earth Rite"
                );
                Answer::Targets(vec![domain.members[0].handle])
            }
            InputDomain::CostMaterials(domain) => {
                assert!(accepted, "declining the cost must not select an amulet");
                assert!(
                    domain.members.iter().all(|member| member.player.seat == 1
                        && matches!(member.card.as_deref(), Some("BP01-075" | "BP03-T02"))),
                    "only the own field Stack amulet is eligible"
                );
                Answer::CostMaterials(
                    domain
                        .members
                        .iter()
                        .take(1)
                        .map(|member| member.handle)
                        .collect(),
                )
            }
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            _ => panic!("unexpected Earth Rite decision"),
        };
        play = game.answer_internal_card(question.actor, question.token, answer)?;
    }
    Ok(play)
}

fn settle(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}

fn payments(result: &Activation) -> Vec<&PaymentEvent> {
    result
        .progress
        .facts
        .iter()
        .filter_map(|fact| match fact {
            Fact::Payment { player, event } => {
                assert_eq!(
                    *player,
                    PlayerId { seat: 1 },
                    "the controller pays Earth Rite"
                );
                Some(event)
            }
            _ => None,
        })
        .collect()
}

fn stack_id(card: &str) -> AbilityId {
    let ability = selection().current[card]
        .program
        .card()
        .abilities
        .iter()
        .find(|ability| {
            matches!(
                ability.node.source.origin,
                AbilityOrigin::Keyword {
                    keyword: KeywordId::Stack,
                    ..
                }
            )
        })
        .unwrap();
    assert_eq!(
        ability.node.kind,
        AbilityKind::Activated,
        "Stack grants its common activation"
    );
    ability.node.id
}

#[test]
fn every_admitted_stack_seed_and_reprint_can_transfer_all_counters_to_another_acted_stack() {
    for card in [
        "BP01-074",
        "PR-007",
        "BP01-075",
        "BP02-051",
        "PR-022",
        "BP01-T10",
        "BP02-T15",
        "BP03-T02",
        "BP06-T06",
        "BP07-T06",
        "BP08-T06",
        "BP09-T10",
        "BP10-T07",
        "BP11-T08",
        "BP22-T10",
        "EBD02-T01",
    ] {
        let mut value = initial(card, true, false);
        value["players"]["P1"]["zones"]["field"] = json!([
            {"id":"source","card":card,"state":{"counters":{"stack_counter":3_u8}}},
            {"id":"recipient","card":"BP03-T02","state":{"acted":true,"counters":{"stack_counter":2_u8}}},
            {"id":"plain","card":"BP01-041","state":{"counters":{"stack_counter":8_u8}}}
        ]);
        let mut game = game(&value);
        let before = game.projection(View::Referee).unwrap();
        let CardPlay::Awaiting(question) = game
            .activate_internal_card("source", stack_id(card), PlayerId { seat: 1 })
            .unwrap()
        else {
            panic!("Stack recipient target");
        };
        let InputDomain::Targets(domain) = &question.domain else {
            panic!("Stack uses the ordinary target decision");
        };
        assert_eq!(domain.members.len(), 1);
        assert_eq!(domain.members[0].card.as_deref(), Some("BP03-T02"));
        let result = resolved(
            game.answer_internal_card(
                question.actor,
                question.token,
                Answer::Targets(vec![domain.members[0].handle]),
            )
            .unwrap(),
        );
        let transfer = result
            .progress
            .receipts
            .values()
            .find_map(|receipt| {
                if let InstructionOutcome::CounterMove { from, to } = receipt {
                    Some((from, to))
                } else {
                    None
                }
            })
            .unwrap();
        assert_eq!(transfer.0.before, Nat(3));
        assert_eq!(transfer.0.after, Nat(0));
        assert_eq!(transfer.1.before, Nat(2));
        assert_eq!(transfer.1.after, Nat(5));
        assert!(
            payments(&result).is_empty(),
            "counter transfer is not Earth Rite payment"
        );
        settle(&mut game);
        let after = game.projection(View::Referee).unwrap();
        if card.contains("-T") {
            assert_eq!(after["P1"]["cemetery"], json!([]));
            assert!(
                after["objects"]["source"].is_null(),
                "a departed token ceases to exist"
            );
        } else {
            assert_eq!(after["P1"]["cemetery"], json!(["source"]));
        }
        assert_eq!(
            after["objects"]["recipient"]["counters"]["スタックカウンター"],
            5_u8
        );
        assert_eq!(
            after["objects"]["recipient"]["generation"],
            before["objects"]["recipient"]["generation"]
        );
        assert_eq!(after["objects"]["recipient"]["acted"], true);
        assert_eq!(
            after["objects"]["plain"]["counters"]["スタックカウンター"],
            8_u8
        );
        assert_eq!(
            after["objects"]["opposing-soil"]["counters"]["スタックカウンター"],
            5_u8
        );
        assert_eq!(after["P1"]["pp"]["current"], 10_u8);
    }
}

#[test]
fn stack_activation_without_another_own_field_stack_does_not_act_or_remove_counters() {
    let mut value = initial("BP01-075", true, false);
    value["players"]["P1"]["zones"]["field"][0]["state"] =
        json!({"counters":{"stack_counter":3_u8}});
    let mut game = game(&value);
    let before = game.projection(View::Referee).unwrap();
    game.activate_internal_card("source", stack_id("BP01-075"), PlayerId { seat: 1 })
        .expect_err("the source, opposing Stack and own EX cannot be the recipient");
    assert_eq!(game.projection(View::Referee).unwrap(), before);
}

#[test]
fn stack_transfer_overflow_rolls_back_the_act_cost_and_both_counter_holders() {
    let mut value = initial("BP01-075", true, false);
    value["players"]["P1"]["zones"]["field"] = json!([
        {"id":"source","card":"BP01-075","state":{"counters":{"stack_counter":3_u8}}},
        {"id":"recipient","card":"BP01-075","state":{"counters":{"stack_counter":i64::MAX}}}
    ]);
    let mut game = game(&value);
    let before = game.projection(View::Referee).unwrap();
    let CardPlay::Awaiting(question) = game
        .activate_internal_card("source", stack_id("BP01-075"), PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("recipient target");
    };
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("public target");
    };
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::Targets(vec![domain.members[0].handle]),
    )
    .expect_err("counter overflow must abort the whole activation");
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(
        after["objects"]["source"]["counters"],
        before["objects"]["source"]["counters"]
    );
    assert_eq!(after["objects"]["source"]["acted"], false);
    assert_eq!(
        after["objects"]["recipient"]["counters"],
        before["objects"]["recipient"]["counters"]
    );
}

#[test]
fn bp01_051_fanfare_selects_its_target_even_when_earth_rite_is_declined() {
    for card in ["BP01-051", "BP01-SL07"] {
        for pay in [false, true] {
            let mut game = game(&initial(card, false, true));
            let CardPlay::Entered(entry) = game
                .play_internal_card("source", PlayerId { seat: 1 })
                .unwrap()
            else {
                panic!("real follower entry");
            };
            if entry.resume == ResumeStage::CheckTiming {
                settle(&mut game);
            }
            let pending = game.internal_pending_choice().unwrap().unwrap();
            let InputDomain::Pending(options) = &pending.domain else {
                panic!("real Fanfare");
            };
            let play = game
                .answer_internal_card(
                    pending.actor,
                    pending.token,
                    Answer::Pending(options[0].handle),
                )
                .unwrap();
            let result = resolved(finish(&mut game, play, pay).unwrap());
            settle(&mut game);
            let view = game.projection(View::Referee).unwrap();
            assert_eq!(
                view["objects"]["enemy"]["damage"],
                if pay { 6_u8 } else { 0_u8 }
            );
            assert_eq!(
                view["P2"]["leader"]["life"],
                if pay { 18_u8 } else { 20_u8 }
            );
            assert_eq!(
                view["P1"]["cemetery"],
                if pay { json!(["soil"]) } else { json!([]) }
            );
            assert_eq!(payments(&result).len(), usize::from(pay));
            if pay {
                assert!(matches!(
                    payments(&result)[0].component,
                    PaidComponent::EarthRite {
                        paid: EarthRitePaidComponent {
                            removed_stack: Nat(1)
                        }
                    }
                ));
                assert!(result.progress.facts.iter().any(|fact| matches!(fact, Fact::Movement(m) if m.method == MoveMethod::Put && m.movement.requested_to.kind == ZoneKind::Cemetery)));
                assert!(!result.progress.facts.iter().any(
                    |fact| matches!(fact, Fact::Movement(m) if m.method == MoveMethod::Destroy)
                ));
            }
        }
    }
}

#[test]
fn bp01_051_required_earth_cost_never_uses_opposing_or_ex_stack_and_rolls_back_act() {
    for soil in [false, true] {
        let mut game = game(&initial("BP01-051", true, soil));
        let id = selection().current["BP01-051"]
            .program
            .card()
            .abilities
            .iter()
            .find(|ability| ability.node.kind == AbilityKind::Activated)
            .unwrap()
            .node
            .id;
        let first = game.activate_internal_card("source", id, PlayerId { seat: 1 });
        let outcome = first.and_then(|play| finish(&mut game, play, true));
        if soil {
            let result = resolved(outcome.unwrap());
            assert_eq!(payments(&result).len(), 1);
            settle(&mut game);
        } else {
            outcome.expect_err("there is no own field Stack to pay");
        }
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["objects"]["source"]["acted"], soil);
        assert_eq!(
            view["objects"]["enemy"]["damage"],
            if soil { 6_u8 } else { 0_u8 }
        );
        assert_eq!(
            view["objects"]["ex-soil"]["counters"]["スタックカウンター"],
            5_u8
        );
        assert_eq!(
            view["objects"]["opposing-soil"]["counters"]["スタックカウンター"],
            5_u8
        );
    }
}

#[test]
fn bp01_063_creates_two_defenders_before_buffing_all_own_golems_only_when_paid() {
    for pay in [false, true] {
        let mut value = initial("BP01-063", false, true);
        value["players"]["P1"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"old","card":"BP01-T08"}));
        value["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-T09"}]);
        let mut game = game(&value);
        let play = game
            .play_internal_card("source", PlayerId { seat: 1 })
            .unwrap();
        let result = resolved(finish(&mut game, play, pay).unwrap());
        settle(&mut game);
        let view = game.projection(View::Referee).unwrap();
        let defenders = view["P1"]["field"]
            .as_array()
            .unwrap()
            .iter()
            .map(|id| &view["objects"][id.as_str().unwrap()])
            .filter(|object| object["name"] == "防御型ゴーレム")
            .collect::<Vec<_>>();
        assert_eq!(defenders.len(), 2);
        for defender in defenders {
            assert_eq!(defender["power"], if pay { 3_u8 } else { 2_u8 });
            assert_eq!(defender["hp"], if pay { 4_u8 } else { 3_u8 });
        }
        assert_eq!(
            view["objects"]["old"]["power"],
            if pay { 4_u8 } else { 3_u8 }
        );
        assert_eq!(view["objects"]["enemy"]["power"], 2_u8);
        assert_eq!(view["objects"]["enemy"]["hp"], 3_u8);
        assert_eq!(payments(&result).len(), usize::from(pay));
    }
}

fn increase_program(amount: i64) -> WellFormedCardProgram {
    let mut tree = expanded::tree().clone();
    let mut entry = tree["cards"]["BP08-038"]["abilities"][0].clone();
    entry.as_object_mut().unwrap().remove("fanfare");
    entry["activate"] = json!({"cost":{"pp":0_u8},"do":[{"stack_plus":amount}]});
    let meta = tree["cards"]["BP08-038"]["meta"].clone();
    tree["cards"] = json!({"producer":{"meta":meta,"abilities":[entry]}});
    lowering::ExpandedLowerer::new()
        .unwrap()
        .lower_with_printed_costs(&tree, &BTreeMap::from([("producer".into(), Nat(1))]))
        .unwrap()["producer"]
        .clone()
}

#[test]
fn stack_increase_asks_even_one_recipient_and_creates_exact_initial_amount_without_one() {
    let program = increase_program(2);
    for has_stack in [false, true] {
        let mut value = draw_position();
        value["players"]["P1"]["zones"]["field"] = if has_stack {
            json!([{"id":"soil","card":"BP01-075","state":{"counters":{"stack_counter":3_u8}}}])
        } else {
            json!([])
        };
        let mut game = increase_game(&value, &program);
        let play = game
            .activate_internal_card(
                "producer",
                program.card().abilities[0].node.id,
                PlayerId { seat: 1 },
            )
            .unwrap();
        if has_stack {
            let CardPlay::Awaiting(question) = play else {
                panic!("singleton Stack still needs a rule choice");
            };
            let InputDomain::Targets(domain) = &question.domain else {
                panic!("Stack recipient");
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
        } else {
            resolved(play);
        }
        settle(&mut game);
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["P1"]["field"].as_array().unwrap().len(), 2);
        let soil = view["P1"]["field"]
            .as_array()
            .unwrap()
            .iter()
            .find(|id| id.as_str() != Some("producer"))
            .unwrap()
            .as_str()
            .unwrap();
        assert_eq!(
            view["objects"][soil]["counters"]["スタックカウンター"],
            if has_stack { 5_u8 } else { 2_u8 }
        );
        if !has_stack {
            assert_eq!(view["objects"][soil]["name"], "大地の魔片");
        }
    }
}

#[test]
fn zero_stack_increase_does_not_select_or_create_even_on_a_full_field() {
    let program = increase_program(0);
    let mut value = draw_position();
    value["players"]["P1"]["zones"]["field"] = json!([
        {"id":"a","card":"BP01-037"},{"id":"b","card":"BP01-037"},
        {"id":"c","card":"BP01-037"},{"id":"d","card":"BP01-037"}
    ]);
    let mut game = increase_game(&value, &program);
    resolved(
        game.activate_internal_card(
            "producer",
            program.card().abilities[0].node.id,
            PlayerId { seat: 1 },
        )
        .unwrap(),
    );
    settle(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["field"]
            .as_array()
            .unwrap()
            .len(),
        5
    );
}

#[test]
fn bp01_063_earth_payment_frees_the_fourth_slot_before_creating_both_defenders() {
    for soil in ["BP01-075", "BP03-T02"] {
        let mut value = initial("BP01-063", false, false);
        value["players"]["P1"]["zones"]["field"] = json!([
            {"id":"soil","card":soil,"state":{"counters":{"stack_counter":1_u8}}},
            {"id":"old","card":"BP01-T08"},
            {"id":"a","card":"BP01-037"},{"id":"b","card":"BP01-037"}
        ]);
        let mut game = game(&value);
        let play = game
            .play_internal_card("source", PlayerId { seat: 1 })
            .unwrap();
        let result = resolved(finish(&mut game, play, true).unwrap());
        assert_eq!(payments(&result).len(), 1);
        settle(&mut game);
        let view = game.projection(View::Referee).unwrap();
        let field = view["P1"]["field"].as_array().unwrap();
        assert_eq!(field.len(), 5);
        assert_eq!(
            field
                .iter()
                .filter(|id| view["objects"][id.as_str().unwrap()]["name"] == "防御型ゴーレム")
                .count(),
            2
        );
        assert!(
            !field.contains(&json!("soil")),
            "payment removes the zero Stack before creation"
        );
        if soil == "BP03-T02" {
            assert!(
                view["objects"]["soil"].is_null(),
                "departed tokens cease to exist"
            );
        } else {
            assert!(
                view["P1"]["cemetery"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("soil"))
            );
        }
        assert_eq!(view["objects"]["old"]["power"], 4_u8);
    }
}

#[test]
fn unaffordable_optional_earth_rite_restores_the_fanfare_and_allows_declining_on_retry() {
    let mut game = game(&initial("BP01-051", false, false));
    assert!(matches!(
        game.play_internal_card("source", PlayerId { seat: 1 })
            .unwrap(),
        CardPlay::Entered(_)
    ));
    settle(&mut game);
    let before = game.projection(View::Referee).unwrap();
    for pay in [true, false] {
        let pending = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = &pending.domain else {
            panic!("the unpaid attempt must restore the pending Fanfare");
        };
        let play = game
            .answer_internal_card(
                pending.actor,
                pending.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap();
        let outcome = finish(&mut game, play, pay);
        if pay {
            outcome.expect_err("the opponent's and EX Stack cannot pay optional Earth Rite");
        } else {
            resolved(outcome.unwrap());
            settle(&mut game);
        }
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["P1"]["pp"], before["P1"]["pp"]);
        assert_eq!(view["P1"]["field"], before["P1"]["field"]);
        assert_eq!(view["objects"]["enemy"]["damage"], 0_u8);
        assert_eq!(
            view["objects"]["opposing-soil"]["counters"],
            before["objects"]["opposing-soil"]["counters"]
        );
        assert_eq!(
            view["objects"]["ex-soil"]["counters"],
            before["objects"]["ex-soil"]["counters"]
        );
    }
}

fn increase_game(initial: &Value, program: &WellFormedCardProgram) -> Game {
    let mut value = initial.clone();
    value["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"producer","card":"producer"}));
    let mut game = Game::from_selected_effects(
        producer_selection().clone(),
        &value,
        &Value::Null,
        &Value::Null,
        "stack-increase",
    )
    .unwrap();
    game.load_internal_card("producer", program).unwrap();
    game
}
