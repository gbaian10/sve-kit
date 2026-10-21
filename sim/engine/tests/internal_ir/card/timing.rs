use super::*;
use sve_engine::game::internal::{Answer, InputDomain};
use sve_engine::ir::lowering::ExpandedLowerer;

pub(super) fn on_attack(mut ability: Located<Ability>) -> Located<Ability> {
    ability.node.kind = AbilityKind::Triggered;
    ability.node.play = None;
    ability.node.source.origin = AbilityOrigin::Keyword {
        keyword: KeywordId::OnAttack,
        origin: Box::new(ability.node.source.origin.clone()),
    };
    let event = Binding::new(90, ability.node.scope);
    ability.node.trigger = Some(TriggerSpec {
        active_zones: vec![ZoneKind::Field],
        view: EventSnapshotPolicy::AfterEntry,
        aggregation: TriggerAggregation::PerFact {},
        limit: None,
        on: TriggerCondition::Event {
            pattern: EventPattern::Attack {
                binding: event,
                predicate: Expr::Compute(Box::new(Predicate::ObjectEquals {
                    left: Expr::Frozen(Box::new(ObjectFrozen::AttackAttacker { event })),
                    right: Expr::Frozen(Box::new(ObjectFrozen::SourceHolder {})),
                })),
            },
        },
    });
    locate_steps(
        &mut ability.node.body.as_mut().unwrap().steps,
        &ability.source,
        &ability.node.source,
    );
    ability
}

fn next(game: &mut Game, handle: u64) -> Activation {
    let q = game.internal_pending_choice().unwrap().unwrap();
    resolved(
        game.answer_internal_card(q.actor, q.token, Answer::Pending(handle))
            .unwrap(),
    )
}

#[test]
fn bp10_122_attack_pending_precedes_quick_and_keeps_later_departure_information() {
    let lowered = ExpandedLowerer::new()
        .unwrap()
        .lower(fanfare::expanded::tree())
        .unwrap();
    let mut draw = lowered["BP10-122"].card().abilities[0].clone();
    draw.source = source("unit-follower");
    draw.source.fragment = None;
    draw.node.source.origin = AbilityOrigin::Keyword {
        keyword: KeywordId::OnAttack,
        origin: Box::new(AbilityOrigin::Printed {
            face: FaceId("0".into()),
            line: Nat(1),
        }),
    };
    locate_steps(
        &mut draw.node.body.as_mut().unwrap().steps,
        &draw.source,
        &draw.node.source,
    );
    let program = card(vec![
        draw,
        on_attack(ability(
            2,
            vec![
                numeric::buff(2, 2, 0),
                movement::put(
                    3,
                    Selector::Objects {
                        objects: vec![ObjectOperand::SourceHolder {}],
                    },
                    OwnerZone::Cemetery,
                ),
            ],
        )),
        on_attack(ability(
            3,
            vec![source_information::source_hit(
                4,
                source_information::source_read(
                    &ObjectView::SourceInformation {},
                    CharacteristicView::Effective,
                    Some(Stat::Attack),
                ),
            )],
        )),
    ]);
    for seat in [1, 2] {
        let mut game = position_with_state(seat, &program, &json!({"entered_this_turn":false}));
        let opponent = if seat == 1 { "P2" } else { "P1" };
        let before = exact_authority(&game);
        game.internal_rule_action(&json!({"do":"activate","card":"s"}))
            .unwrap_err();
        assert_eq!(before, exact_authority(&game));
        let attack = game
            .internal_rule_action(
                &json!({"do":"attack","attacker":"s","target":format!("{opponent}.leader")}),
            )
            .unwrap();
        assert_eq!(attack.outcome, "resolved");
        let q = game.internal_pending_choice().unwrap().unwrap();
        assert!(
            matches!(&q.domain, InputDomain::Pending(options) if options.len()==3 && options.iter().all(|o| o.event.contains("Attack 1")))
        );
        let before = exact_authority(&game);
        game.internal_rule_action(&json!({"do":"pass"}))
            .unwrap_err();
        assert_eq!(before, exact_authority(&game));
        let sacrificed = next(&mut game, 2);
        game.resume_internal_card().unwrap();
        let hit = next(&mut game, 2);
        assert_eq!(hit.source.holder, sacrificed.source.holder);
        assert!(hit.progress.facts.iter().any(|fact| matches!(fact, Fact::Damage(hit) if hit.amount.0.get()==4 && hit.source.object==sacrificed.source.holder)));
        game.resume_internal_card().unwrap();
        next(&mut game, 1);
        game.resume_internal_card().unwrap();
        assert!(game.internal_pending_choice().unwrap().is_none());
        assert_eq!(
            game.projection(View::Referee).unwrap()[opponent]["leader"]["life"],
            16
        );
        let quick = game.internal_rule_action(&json!({"do":"pass"})).unwrap();
        assert_eq!(quick.outcome, "resolved");
        assert_eq!(
            game.projection(View::Referee).unwrap()[opponent]["leader"]["life"],
            16
        );
    }
}

#[test]
fn bp09_p17_end_begin_occurs_once_before_guard_quick_and_expiry() {
    let lowered = ExpandedLowerer::new()
        .unwrap()
        .lower(fanfare::expanded::tree())
        .unwrap();
    let mut end = lowered["BP09-P17"].card().abilities[0].clone();
    end.source = source("unit-follower");
    end.source.fragment = None;
    end.node.source.origin = AbilityOrigin::Printed {
        face: FaceId("0".into()),
        line: Nat(1),
    };
    locate_steps(
        &mut end.node.body.as_mut().unwrap().steps,
        &end.source,
        &end.node.source,
    );
    for seat in [1, 2] {
        let actor = if seat == 1 { "P1" } else { "P2" };
        let mut initial = setup(
            seat,
            json!([{"id":"d1","card":"unit-spell"},{"id":"d2","card":"unit-spell"},{"id":"d3","card":"unit-spell"}]),
        );
        initial["turn"]["active"] = json!(actor);
        // Five cards plus the three drawn leave exactly one excess card for the discard stage.
        initial["players"][actor]["zones"]["hand"] = json!(
            (1..=5)
                .map(|n| json!({"id":format!("h{n}"),"card":"unit-spell"}))
                .collect::<Vec<_>>()
        );
        initial["players"][actor]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
        // The next turn's draw must succeed, so finishing this turn is not a deck-out loss.
        initial["players"][if seat == 1 { "P2" } else { "P1" }]["zones"]["deck"] =
            json!([{"id":"o1","card":"unit-spell"}]);
        let mut game =
            Game::new(catalog(), &initial, &Value::Null, &Value::Null, "end-seed").unwrap();
        game.load_internal_card("unit-follower", &card(vec![end.clone()]))
            .unwrap();
        game.load_internal_card("unit-spell", &card(vec![]))
            .unwrap();
        let end_phase = game
            .internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap();
        assert_eq!(end_phase.outcome, "resolved");
        let q = game.internal_pending_choice().unwrap().unwrap();
        assert_eq!(q.actor, PlayerId { seat });
        assert!(
            matches!(&q.domain, InputDomain::Pending(options) if options.len()==1 && options[0].event.contains("End Begin"))
        );
        let before = exact_authority(&game);
        game.internal_rule_action(&json!({"do":"pass"}))
            .unwrap_err();
        assert_eq!(before, exact_authority(&game));
        let result = next(&mut game, 1);
        assert!(
            result
                .progress
                .facts
                .iter()
                .any(|fact| matches!(fact, Fact::Draw(_)))
        );
        game.resume_internal_card().unwrap();
        assert!(game.internal_pending_choice().unwrap().is_none());
        assert!(game.internal_pending_choice().unwrap().is_none());
        game.answer_internal_card(q.actor, q.token, Answer::Pending(1))
            .unwrap_err();
        // No guard follower can be loaded, so the rule subset has no guard-act entry.
        let before = exact_authority(&game);
        game.internal_rule_action(&json!({"do":"guard-act","select":[]}))
            .unwrap_err();
        assert_eq!(before, exact_authority(&game));
        let quick = game.internal_rule_action(&json!({"do":"pass"})).unwrap();
        assert_eq!(quick.outcome, "resolved");
        let active =
            |game: &Game| game.projection(View::Referee).unwrap()["turn"]["active"].clone();
        assert_eq!(active(&game), actor);
        let short = game
            .internal_rule_action(&json!({"do":"end-discard","select":[]}))
            .unwrap();
        assert_eq!(short.outcome, "cannot-play");
        let discard = game
            .internal_rule_action(&json!({"do":"end-discard","select":["h1"]}))
            .unwrap();
        assert_eq!(discard.outcome, "resolved");
        assert_eq!(active(&game), if seat == 1 { "P2" } else { "P1" });
    }
}
