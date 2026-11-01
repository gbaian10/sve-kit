#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate grant lifetime tests."
)]
use super::*;
use sve_engine::game::internal::{Answer, InputDomain};

fn followers(scope: u64) -> Selector {
    let binding = Binding::new(50, ScopeId(scope));
    Selector::Zone {
        players: players(PlayerRelation::SelfPlayer),
        zone: ZoneKind::Field,
        view: SelectionView::Current {},
        binding,
        predicate: Box::new(Expr::Compute(Box::new(Predicate::KindEquals {
            left: Expr::Live(Box::new(KindRead::Kind {
                object: ObjectView::Current {
                    object: Box::new(ObjectOperand::Bound { binding }),
                },
                view: CharacteristicView::Effective,
            })),
            right: Expr::Literal(CardKind::Follower),
        }))),
    }
}

fn grant(end: DurationEnd) -> Located<Node> {
    instruction(
        1,
        Action::Grant {
            subjects: followers(100),
            abilities: vec![GrantedAbility::Keyword {
                keyword: KeywordId::Drain,
            }],
            duration: Duration {
                end,
                anchor: TimeAnchor::Installation {},
                usage: None,
                intervals: vec![],
            },
            provider: TextProvider {
                provider: ObjectView::Current {
                    object: Box::new(ObjectOperand::SourceHolder {}),
                },
                face: FaceId("0".into()),
                ability: AbilityId(1),
            },
        },
    )
}

fn grant_program(mut abilities: Vec<Located<Ability>>) -> WellFormedCardProgram {
    for ability in &mut abilities {
        ability.source.card_id = "unit-amulet".into();
        ability.source.card_no = "unit-amulet".into();
        if let Some(body) = &mut ability.node.body {
            locate_steps(&mut body.steps, &ability.source, &ability.node.source);
        }
    }
    card(abilities)
}

fn position(program: &WellFormedCardProgram) -> Game {
    position_with_silence(program, false, false)
}

fn position_with_silence(
    program: &WellFormedCardProgram,
    provider_silenced: bool,
    recipient_silenced: bool,
) -> Game {
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"s","card":"unit-amulet","state":{"silenced":provider_silenced}}, {"id":"ally","card":"unit-follower","state":{"silenced":recipient_silenced}}, {"id":"second","card":"unit-follower"}
    ]);
    let mut game = Game::new(
        static_keywords::keyword_catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "dynamic-grant",
    )
    .unwrap();
    game.load_internal_card("unit-amulet", program).unwrap();
    game.load_internal_card("unit-follower", &grant_program(vec![]))
        .unwrap();
    game.load_internal_card("unit-spell", &grant_program(vec![]))
        .unwrap();
    game
}

fn activate_grant(game: &mut Game) {
    resolved(
        game.activate_internal_card("s", AbilityId(1), PlayerId { seat: 1 })
            .unwrap(),
    );
    game.resume_internal_card().unwrap();
}

fn strike(game: &mut Game, holder: &str) {
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":holder,"target":"P2.leader"}))
            .unwrap()
            .outcome,
        "resolved",
        "the recipient reaches a real attack window"
    );
    let q = game.internal_quick_choice().unwrap().unwrap();
    game.answer_internal_card(q.actor, q.token, Answer::Quick(None))
        .unwrap();
}

fn heal(game: &mut Game) -> Activation {
    let q = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &q.domain else {
        panic!("Drain")
    };
    assert_eq!(options.len(), 1, "multiple Drain grants recover only once");
    assert_eq!(
        options[0].card, "unit-amulet",
        "Drain keeps the granting card as its text provider"
    );
    let result = resolved(
        game.answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
            .unwrap(),
    );
    game.resume_internal_card().unwrap();
    result
}

#[test]
fn drain_grants_keep_distinct_recipient_instances_and_merge_repeated_grants() {
    let mut game = position(&grant_program(vec![
        ability(1, vec![grant(DurationEnd::NoTemporalExpiry {})]),
        ability(
            2,
            vec![instruction(
                2,
                Action::Posture {
                    objects: followers(101),
                    posture: Posture::Standing,
                },
            )],
        ),
    ]));
    activate_grant(&mut game);
    activate_grant(&mut game);
    strike(&mut game, "ally");
    let first = heal(&mut game);
    strike(&mut game, "second");
    let second = heal(&mut game);
    assert_ne!(first.source.holder.id, second.source.holder.id);
    assert_ne!(first.source.instance, second.source.instance);
    resolved(
        game.activate_internal_card("s", AbilityId(2), PlayerId { seat: 1 })
            .unwrap(),
    );
    game.resume_internal_card().unwrap();
    strike(&mut game, "ally");
    let repeated = heal(&mut game);
    assert_eq!(first.source.instance, repeated.source.instance);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        26_u8
    );
}

#[test]
fn installed_drain_survives_its_provider_leaving_before_or_after_installation() {
    for before in [false, true] {
        let leave = movement::put(
            2,
            Selector::Objects {
                objects: vec![ObjectOperand::SourceHolder {}],
            },
            OwnerZone::Hand,
        );
        let steps = if before {
            vec![leave, grant(DurationEnd::NoTemporalExpiry {})]
        } else {
            vec![grant(DurationEnd::NoTemporalExpiry {}), leave]
        };
        let mut game = position(&grant_program(vec![ability(1, steps)]));
        activate_grant(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["hand"],
            json!(["s"])
        );
        strike(&mut game, "ally");
        heal(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
            22_u8
        );
    }
}

#[test]
fn drain_does_not_follow_a_returning_recipient_generation() {
    let move_out = movement::put(2, followers(101), OwnerZone::Hand);
    let mut game = position(&grant_program(vec![ability(
        1,
        vec![grant(DurationEnd::NoTemporalExpiry {}), move_out],
    )]));
    activate_grant(&mut game);
    assert!(matches!(
        game.play_internal_card("ally", PlayerId { seat: 1 })
            .unwrap(),
        CardPlay::Entered(_)
    ));
    game.resume_internal_card().unwrap();
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["ally"]["keywords"],
        json!([])
    );
}

fn aura() -> Located<Ability> {
    let mut aura = static_keywords::conditional_keyword();
    if let Effect::Continuous {
        target,
        condition,
        patch,
        ..
    } = &mut aura.node.declarations[0]
    {
        *target = EffectTarget::Dynamic {
            selector: followers(200),
        };
        *condition = None;
        *patch = CharacteristicPatch::GrantAbility {
            abilities: vec![GrantedAbility::Keyword {
                keyword: KeywordId::Drain,
            }],
        };
    }
    aura
}

#[test]
fn continuous_drain_ends_when_the_provider_leaves() {
    for leave in [false, true] {
        let program = grant_program(vec![
            ability(
                1,
                vec![movement::put(
                    1,
                    Selector::Objects {
                        objects: vec![ObjectOperand::SourceHolder {}],
                    },
                    OwnerZone::Hand,
                )],
            ),
            aura(),
        ]);
        let mut game = position(&program);
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["ally"]["keywords"],
            json!(["ドレイン"])
        );
        if leave {
            activate_grant(&mut game);
        }
        strike(&mut game, "ally");
        if leave {
            assert!(game.internal_pending_choice().unwrap().is_none());
            assert_eq!(
                game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
                20_u8
            );
        } else {
            heal(&mut game);
        }
    }
}

#[test]
fn turn_limited_drain_expires_at_the_actual_turn_boundary() {
    let mut game = position(&grant_program(vec![ability(
        1,
        vec![grant(DurationEnd::CurrentTurnEnd {})],
    )]));
    activate_grant(&mut game);
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    quick::pass(&mut game);
    game.internal_rule_action(&json!({"do":"end-discard","select":[]}))
        .unwrap();
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["ally"]["keywords"],
        json!([])
    );
}

#[test]
fn silencing_the_provider_removes_continuous_drain() {
    let mut game = position_with_silence(&grant_program(vec![aura()]), true, false);
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["ally"]["keywords"],
        json!([])
    );
    strike(&mut game, "ally");
    assert!(game.internal_pending_choice().unwrap().is_none());
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        20_u8
    );
}

#[test]
fn a_later_grant_restores_drain_and_basic_keywords_after_printed_ability_loss() {
    let mut applied = grant(DurationEnd::NoTemporalExpiry {});
    if let Node::Instruction { instruction } = &mut applied.node
        && let Action::Grant { abilities, .. } = &mut instruction.action
    {
        abilities.push(GrantedAbility::Keyword {
            keyword: KeywordId::Rush,
        });
    }
    let mut game =
        position_with_silence(&grant_program(vec![ability(1, vec![applied])]), false, true);
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["ally"]["keywords"],
        json!([])
    );
    activate_grant(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["ally"]["keywords"],
        json!(["ドレイン", "突進"])
    );
    strike(&mut game, "ally");
    heal(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        22_u8
    );
}
