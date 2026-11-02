#![expect(
    clippy::panic,
    reason = "Unexpected questions invalidate these real grant scenarios."
)]
use super::*;

#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "These seed cards use only formal targets and their printed mode choice."
)]
fn settle(game: &mut Game, mut play: CardPlay, target: Option<&str>) {
    loop {
        let CardPlay::Awaiting(question) = play else {
            assert!(
                matches!(play, CardPlay::Resolved(_) | CardPlay::Entered(_)),
                "the complete printed body must resolve"
            );
            return;
        };
        let answer = match &question.domain {
            InputDomain::Targets(domain) => {
                let member = domain
                    .members
                    .iter()
                    .find(|member| {
                        target.is_none_or(|target| member.card.as_deref() == Some(target))
                    })
                    .unwrap();
                Answer::Targets(vec![member.handle])
            }
            InputDomain::Modes { .. } => Answer::Modes(vec![ModeId(2)]),
            _ => panic!("unexpected seed question"),
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
}

fn trigger(game: &mut Game) -> CardPlay {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("pending")
    };
    assert_eq!(
        options.len(),
        1,
        "one matching printed seed trigger is pending"
    );
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::Pending(options[0].handle),
    )
    .unwrap()
}

fn hit() -> Action {
    let mut objects = subjects();
    if let Selector::Zone { players, .. } = &mut objects {
        players.relation = PlayerRelation::SelfPlayer;
        players.cardinality = Cardinality {
            minimum: Nat(1),
            maximum: Some(Nat(1)),
        };
    }
    Action::Damage {
        recipients: RecipientSet::Objects {
            objects: Selector::Difference {
                input: Box::new(objects),
                except: Box::new(Selector::Objects {
                    objects: vec![ObjectOperand::SourceHolder {}],
                }),
            },
        },
        amount: DamageAmount::Uniform {
            amount: Expr::Literal(Int(2)),
        },
        flags: DamageFlags::Ability,
        source: ObjectView::SourceInformation {},
        batch: BatchPolicy::Instruction {},
    }
}

#[test]
fn bp20_036_grants_shield_only_to_the_chosen_receiver_after_the_spell_departs() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"spell","card":"BP20-036"}]);
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"ally","card":"BP01-037"},{"id":"other","card":"BP01-037"}]);
    let mut game = with_producer(&initial, hit());
    let played = game
        .play_internal_card("spell", PlayerId { seat: 1 })
        .unwrap();
    settle(&mut game, played, Some("BP01-037"));
    game.resume_internal_card().unwrap();
    let before = game.projection(View::Referee).unwrap();
    assert_eq!(
        before["P1"]["cemetery"],
        json!(["spell"]),
        "the printed spell has departed before the granted shield is used"
    );
    assert!(
        before["objects"]["ally"]["keywords"]
            .as_array()
            .unwrap()
            .contains(&json!("指定攻撃")),
        "the selected follower also receives permanent Assail"
    );
    resolved(
        game.activate_internal_card("producer", AbilityId(1), PlayerId { seat: 1 })
            .unwrap(),
    );
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(
        after["objects"]["ally"]["hp"], before["objects"]["ally"]["hp"],
        "the granted static definition prevents ability damage"
    );
    assert_eq!(
        after["objects"]["other"]["hp"].as_i64().unwrap(),
        before["objects"]["other"]["hp"].as_i64().unwrap() - 2,
        "an unchosen follower does not receive the shield"
    );
}

#[test]
fn bp16_103_all_printings_gain_shield_and_assail_only_after_ability_entry() {
    for number in ["BP16-103", "BP16-P23"] {
        for ability in [false, true] {
            let mut initial = draw_position();
            initial["players"]["P1"]["zones"]["field"] = json!([]);
            initial["players"]["P1"]["zones"][if ability { "cemetery" } else { "hand" }] =
                json!([{"id":"source","card":number}]);
            initial["players"]["P2"]["zones"]["field"] =
                json!([{"id":"enemy","card":"BP01-037","state":{"acted":true}}]);
            let movement = Action::Move {
                objects: subjects_in(ZoneKind::Cemetery),
                movement: MoveSpec {
                    to: MoveDestination::Shared {
                        zone: ZoneRef {
                            player: PlayerRef::Controller {},
                            kind: ZoneKind::Field,
                        },
                    },
                    position: None,
                    order: None,
                    initial_posture: None,
                    entry_effects: vec![],
                },
                reason: MoveReason::Put,
            };
            let mut game = with_producer(&initial, if ability { movement } else { hit() });
            let entered = if ability {
                game.activate_internal_card("producer", AbilityId(1), PlayerId { seat: 1 })
                    .unwrap()
            } else {
                game.play_internal_card("source", PlayerId { seat: 1 })
                    .unwrap()
            };
            settle(&mut game, entered, None);
            if ability {
                game.resume_internal_card().unwrap();
            }
            let pending = trigger(&mut game);
            settle(&mut game, pending, None);
            let view = game.projection(View::Referee).unwrap();
            assert_eq!(
                view["objects"]["source"]["keywords"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("指定攻撃")),
                ability,
                "the real entry method gates the permanent keyword together with the shield"
            );
            assert!(
                view["objects"]["source"]["keywords"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("突進")),
                "the printed Rush remains present in every entry path"
            );
            game.resume_internal_card().unwrap();
            game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"enemy"}))
                .unwrap();
            let attacking = trigger(&mut game);
            settle(&mut game, attacking, None);
            assert_eq!(
                game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
                19_u8,
                "the printed Attack ability remains active beside the granted static definition"
            );
            game.resume_internal_card().unwrap();
            let quick = game.internal_quick_choice().unwrap().unwrap();
            game.answer_internal_card(quick.actor, quick.token, Answer::Quick(None))
                .unwrap();
            let after = game.projection(View::Referee).unwrap();
            assert_eq!(
                after["P1"]["cemetery"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("source")),
                !ability,
                "only the granted shield prevents lethal combat damage after ability entry"
            );
        }
    }
}

#[test]
fn bp12_005_all_printings_discount_existing_and_new_same_name_ex_tokens_through_play() {
    for form in ["BP12-005", "BP12-P02"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP12-004"}]);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
        initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"other","card":"BP03-083"}]);
        let mut game = game(&initial);
        let entered = game
            .play_internal_card("source", PlayerId { seat: 1 })
            .unwrap();
        settle(&mut game, entered, None);
        let fanfare = trigger(&mut game);
        settle(&mut game, fanfare, None);
        game.resume_internal_card().unwrap();
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":2_u8}}),
        )
        .unwrap();
        let evolved = trigger(&mut game);
        settle(&mut game, evolved, None);
        game.resume_internal_card().unwrap();
        let before = game.projection(View::Referee).unwrap();
        let tokens = before["P1"]["ex"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(Value::as_str)
            .filter(|id| *id != "other")
            .map(str::to_owned)
            .collect::<Vec<_>>();
        assert_eq!(
            tokens.len(),
            2,
            "the existing fanfare token and the new evolution token both remain in EX"
        );
        for token in tokens {
            let old = game.projection(View::Referee).unwrap()["P1"]["pp"]["current"].clone();
            let played = game
                .play_internal_card(&token, PlayerId { seat: 1 })
                .unwrap();
            settle(&mut game, played, None);
            game.resume_internal_card().unwrap();
            assert_eq!(
                game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
                old,
                "each granted self cost declaration survives the EX to Resolution rule movement and makes the token free"
            );
        }
        let old = game.projection(View::Referee).unwrap()["P1"]["pp"]["current"]
            .as_i64()
            .unwrap();
        let played = game
            .play_internal_card("other", PlayerId { seat: 1 })
            .unwrap();
        settle(&mut game, played, None);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
            old - 1,
            "a different name in EX keeps its ordinary printed cost"
        );
    }
}
