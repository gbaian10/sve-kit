use super::*;
use sve_engine::game::internal::{Answer, InputDomain};
use sve_engine::ir::lowering::ExpandedLowerer;

pub(super) fn own_zone(kind: ZoneKind, scope: u64) -> Selector {
    let mut selector = zone(kind, scope);
    if let Selector::Zone {
        players: selected, ..
    } = &mut selector
    {
        *selected = players(PlayerRelation::SelfPlayer);
    }
    selector
}
pub(super) fn enter(id: u64, objects: Selector) -> Located<Node> {
    instruction(
        id,
        Action::Move {
            objects,
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
        },
    )
}
fn seed() -> Located<Ability> {
    let lowered = ExpandedLowerer::new()
        .unwrap()
        .lower(fanfare::expanded::tree())
        .unwrap();
    let original = &lowered["BP20-028"].card().abilities[0].node;
    let mut seed = ability(1, original.body.as_ref().unwrap().steps.clone());
    seed.node.kind = AbilityKind::Triggered;
    seed.node.play = None;
    seed.node.trigger.clone_from(&original.trigger);
    seed
}
fn position(
    base: &WellFormedCardProgram,
    opponent: &WellFormedCardProgram,
    incoming: bool,
) -> Game {
    let snapshot = ["unit-follower", "unit-observer", "unit-spell"].iter().map(|number| json!({"number":number,"faces":[{"name":number,"card_class":"ニュートラル","card_type":if *number=="unit-spell" {"スペル"} else {"フォロワー"},"cost":"1","power":"2","hp":"3","traits":["兵士"],"text":null,"sections":[]}]}).to_string()).collect::<Vec<_>>().join("\n");
    let registry = json!({"format":1,"kind":"keyword_registry","keywords":{}}).to_string();
    let document=json!({"format":1,"kind":"effect_set","cards":{"unit-follower":{"status":"complete","abilities":[]},"unit-observer":{"status":"complete","abilities":[]},"unit-spell":{"status":"complete","abilities":[]}}}).to_string();
    let catalog = Arc::new(
        Catalog::from_documents(&snapshot, &registry, &[("entry.yaml".into(), document)]).unwrap(),
    );
    let mut initial = setup(1, json!([]));
    for seat in ["P1", "P2"] {
        initial["players"][seat]["zones"]["hand"] = json!([]);
        initial["players"][seat]["zones"]["deck"] = json!(
            (1..=12)
                .map(|n| json!({"id":format!("{seat}-d{n}"),"card":"unit-spell"}))
                .collect::<Vec<_>>()
        );
    }
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"o","card":"unit-observer"}]);
    if incoming {
        initial["players"]["P1"]["zones"]["cemetery"] =
            json!([{"id":"a","card":"unit-follower"},{"id":"b","card":"unit-follower"}]);
        initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"c","card":"unit-follower"}]);
    }
    let mut game = Game::new(catalog, &initial, &Value::Null, &Value::Null, "entry-seed").unwrap();
    game.load_internal_card("unit-follower", base).unwrap();
    game.load_internal_card("unit-observer", opponent).unwrap();
    game.load_internal_card("unit-spell", &card(vec![]))
        .unwrap();
    game
}
#[expect(clippy::panic, reason = "The seed has one fixed controller predicate.")]
fn opponent(mut entry: Located<Ability>) -> WellFormedCardProgram {
    entry.source = source("unit-observer");
    entry.source.fragment = None;
    let TriggerCondition::Event {
        pattern: EventPattern::EnterField { predicate, .. },
    } = &mut entry.node.trigger.as_mut().unwrap().on
    else {
        panic!("entry seed")
    };
    let Expr::Compute(predicate) = predicate else {
        panic!("entry predicate")
    };
    let Predicate::And { predicates } = predicate.as_mut() else {
        panic!("entry conjunction")
    };
    let Expr::Compute(controller) = &mut predicates[0] else {
        panic!("entry controller")
    };
    let Predicate::PlayerEquals { right, .. } = controller.as_mut() else {
        panic!("entry player")
    };
    *right = Expr::Literal(PlayerRef::RequireOne {
        players: Box::new(PlayerSet {
            relation: PlayerRelation::Opponents,
            relative_to: PlayerRef::Controller {},
            cardinality: Cardinality {
                minimum: Nat(1),
                maximum: Some(Nat(1)),
            },
            text_quantifier: TextQuantifier::One,
        }),
    });
    locate_steps(
        &mut entry.node.body.as_mut().unwrap().steps,
        &entry.source,
        &entry.node.source,
    );
    card(vec![entry])
}
fn drain(game: &mut Game) -> Vec<Activation> {
    let mut results = Vec::new();
    while let Some(q) = game.internal_pending_choice().unwrap() {
        results.push(resolved(
            game.answer_internal_card(q.actor, q.token, Answer::Pending(1))
                .unwrap(),
        ));
        game.resume_internal_card().unwrap();
    }
    results
}
#[expect(
    clippy::panic,
    reason = "The fixed seed must produce exactly four sequential quota questions."
)]
fn choose_batch(game: &mut Game, mut play: CardPlay) -> Activation {
    for ordinal in 0..4 {
        let CardPlay::Awaiting(q) = play else {
            panic!("missing simultaneous quota question")
        };
        let expected = if ordinal < 3 { 1 } else { 2 };
        assert_eq!(
            q.actor,
            PlayerId { seat: expected },
            "turn player chooses first"
        );
        assert!(
            matches!(&q.domain, InputDomain::TriggerQuota(domain) if domain.candidates.len()==2 && domain.earlier.len()==ordinal),
            "both facts and earlier choices remain public"
        );
        let before = exact_authority(game);
        game.answer_internal_card(
            PlayerId {
                seat: if expected == 1 { 2 } else { 1 },
            },
            q.token,
            Answer::TriggerQuota(1),
        )
        .unwrap_err();
        game.answer_internal_card(q.actor, q.token, Answer::TriggerQuota(9))
            .unwrap_err();
        game.answer_internal_card(q.actor, q.token, Answer::Pending(1))
            .unwrap_err();
        assert_eq!(
            before,
            exact_authority(game),
            "invalid answers preserve the full question"
        );
        game.internal_pending_choice().unwrap_err();
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["s"]["power"],
            2,
            "later body waits for every admission choice"
        );
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["hand"]
                .as_array()
                .unwrap()
                .len(),
            0,
            "no pending body resolves inside admission"
        );
        play = game
            .answer_internal_card(
                q.actor,
                q.token,
                Answer::TriggerQuota(if ordinal == 0 { 2 } else { 1 }),
            )
            .unwrap();
    }
    resolved(play)
}
#[expect(
    clippy::panic,
    reason = "The admitted batch has four quota questions before its deliberate body error."
)]
fn failed_batch(base: &WellFormedCardProgram, other: &WellFormedCardProgram) {
    let mut broken = base.card().clone();
    let helper = broken
        .abilities
        .iter_mut()
        .find(|a| a.node.id == AbilityId(20))
        .unwrap();
    helper.node.body.as_mut().unwrap().steps.push(draw_value(
        25,
        Expr::Compute(Box::new(IntOperation::Arithmetic {
            operation: Arithmetic::Add,
            arguments: vec![Expr::Literal(Int(i64::MAX)), Expr::Literal(Int(1))],
        })),
    ));
    locate_steps(
        &mut helper.node.body.as_mut().unwrap().steps,
        &helper.source,
        &helper.node.source,
    );
    let mut game = position(&card(broken.abilities), other, true);
    let before = [View::P1, View::P2, View::Referee].map(|view| game.projection(view).unwrap());
    let mut play = game
        .activate_internal_card("s", AbilityId(20), PlayerId { seat: 1 })
        .unwrap();
    for _ in 0..3 {
        let CardPlay::Awaiting(q) = play else {
            panic!("quota prefix")
        };
        play = game
            .answer_internal_card(q.actor, q.token, Answer::TriggerQuota(1))
            .unwrap();
    }
    let CardPlay::Awaiting(q) = play else {
        panic!("last quota question")
    };
    game.answer_internal_card(q.actor, q.token, Answer::TriggerQuota(1))
        .unwrap_err();
    assert_eq!(
        before,
        [View::P1, View::P2, View::Referee].map(|view| game.projection(view).unwrap()),
        "body failure restores movement, quota, pending and views to the outer origin"
    );
    assert!(
        game.internal_pending_choice().unwrap().is_none(),
        "failed transaction leaves no admitted occurrence"
    );
    let CardPlay::Awaiting(retry) = game
        .activate_internal_card("s", AbilityId(20), PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("quota must be available on retry")
    };
    let before = exact_authority(&game);
    game.answer_internal_card(q.actor, q.token, Answer::TriggerQuota(1))
        .unwrap_err();
    assert_eq!(
        before,
        exact_authority(&game),
        "rolled-back question cannot answer the fresh attempt"
    );
    game.concede_internal(PlayerId { seat: 2 }).unwrap();
    game.answer_internal_card(retry.actor, retry.token, Answer::TriggerQuota(1))
        .unwrap_err();
    assert!(
        game.internal_decision(View::Referee).is_none(),
        "terminal cleanup discards staged admission"
    );
}

#[test]
fn bp20_028_entry_seed_admits_once_after_controller_batch_choices_and_real_turn_reset() {
    let seed = seed();
    let base = card(vec![
        seed.clone(),
        ability(
            20,
            vec![
                enter(20, own_zone(ZoneKind::Cemetery, 20)),
                numeric::buff(21, 5, 0),
            ],
        ),
        ability(21, vec![enter(22, own_zone(ZoneKind::Ex, 21))]),
        ability(
            22,
            vec![
                movement::put(
                    23,
                    Selector::Objects {
                        objects: vec![ObjectOperand::SourceHolder {}],
                    },
                    OwnerZone::Cemetery,
                ),
                enter(24, own_zone(ZoneKind::Cemetery, 22)),
            ],
        ),
    ]);
    let other = opponent(seed);
    failed_batch(&base, &other);
    let mut game = position(&base, &other, true);
    let play = game
        .activate_internal_card("s", AbilityId(20), PlayerId { seat: 1 })
        .unwrap();
    let original = choose_batch(&mut game, play);
    assert!(original.progress.complete);
    game.resume_internal_card().unwrap();
    let first = drain(&mut game);
    assert_eq!(first.len(), 4);
    assert_eq!(
        first
            .iter()
            .filter(|a| a.source.controller.seat == 1)
            .count(),
        3
    );
    assert!(first.iter().all(|a| {
        a.progress
            .facts
            .iter()
            .any(|fact| matches!(fact, Fact::Draw(_)))
    }));
    resolved(
        game.activate_internal_card("s", AbilityId(21), PlayerId { seat: 1 })
            .unwrap(),
    );
    game.resume_internal_card().unwrap();
    assert_eq!(drain(&mut game).len(), 1);
    for _ in 0..2 {
        assert_eq!(
            game.internal_rule_action(&json!({"do":"end-phase"}))
                .unwrap()
                .outcome,
            "resolved"
        );
        // The Quick pass must really finish this turn; the next active player's draw succeeds.
        assert_eq!(quick::pass(&mut game).outcome, "resolved");
    }
    resolved(
        game.activate_internal_card("s", AbilityId(22), PlayerId { seat: 1 })
            .unwrap(),
    );
    game.resume_internal_card().unwrap();
    let next = drain(&mut game);
    assert_eq!(next.len(), 5);
    assert_ne!(next[0].execution, first[0].execution);
}

#[expect(
    clippy::panic,
    reason = "This helper alters only a checked synthetic self-entry header."
)]
fn self_entry(mut entry: Located<Ability>) -> Located<Ability> {
    entry.node.id = AbilityId(2);
    entry.node.scope = ScopeId(2);
    entry.node.body.as_mut().unwrap().scope = ScopeId(2);
    entry.source.line = 2;
    entry.node.source.origin = AbilityOrigin::Printed {
        face: FaceId("0".into()),
        line: Nat(2),
    };
    let TriggerCondition::Event {
        pattern: EventPattern::EnterField { binding, predicate },
    } = &mut entry.node.trigger.as_mut().unwrap().on
    else {
        panic!("entry")
    };
    *binding = Binding::new(90, ScopeId(2));
    *predicate = Expr::Compute(Box::new(Predicate::ObjectEquals {
        left: Expr::Frozen(Box::new(ObjectFrozen::EnterFieldObject { event: *binding })),
        right: Expr::Frozen(Box::new(ObjectFrozen::SourceHolder {})),
    }));
    entry.node.trigger.as_mut().unwrap().limit = None;
    let entry_subject = Selector::Objects {
        objects: vec![ObjectOperand::FromExpression {
            value: Box::new(Expr::Frozen(Box::new(ObjectFrozen::EnterFieldObject {
                event: Binding::new(90, ScopeId(2)),
            }))),
        }],
    };
    entry.node.body.as_mut().unwrap().steps = vec![source_information::source_hit(
        10,
        source_information::source_read(
            &ObjectView::SourceInformation {},
            CharacteristicView::Effective,
            Some(Stat::Attack),
        ),
    )];
    entry.node.body.as_mut().unwrap().steps.insert(
        0,
        instruction(
            9,
            Action::Modify {
                subjects: entry_subject,
                patch: CharacteristicPatch::Numeric {
                    stat: Stat::Attack,
                    change: NumericChange::Add {
                        amount: Expr::Literal(Int(1)),
                    },
                },
                expires: Duration {
                    end: DurationEnd::NoTemporalExpiry {},
                    anchor: TimeAnchor::Installation {},
                    usage: None,
                    intervals: vec![],
                },
                source: EffectReference::CurrentInstallation {},
            },
        ),
    );
    locate_steps(
        &mut entry.node.body.as_mut().unwrap().steps,
        &entry.source,
        &entry.node.source,
    );
    entry
}
#[test]
fn returning_source_keeps_old_pending_lki_and_assigns_new_entry_generation() {
    let seed = self_entry(seed());
    let hit = timing::on_attack(ability(
        1,
        vec![source_information::source_hit(
            1,
            source_information::source_read(
                &ObjectView::SourceInformation {},
                CharacteristicView::Effective,
                Some(Stat::Attack),
            ),
        )],
    ));
    let transform = timing::on_attack(ability(
        3,
        vec![
            numeric::buff(2, 2, 0),
            movement::put(
                3,
                Selector::Objects {
                    objects: vec![ObjectOperand::SourceHolder {}],
                },
                OwnerZone::Cemetery,
            ),
            enter(4, own_zone(ZoneKind::Cemetery, 33)),
            numeric::buff(5, 5, 0),
        ],
    ));
    let again = timing::on_attack(ability(
        4,
        vec![
            movement::put(6, own_zone(ZoneKind::Field, 44), OwnerZone::Cemetery),
            enter(7, own_zone(ZoneKind::Cemetery, 45)),
            numeric::buff(8, 5, 0),
        ],
    ));
    let mut game = position(
        &card(vec![hit, seed, transform, again]),
        &card(vec![]),
        false,
    );
    game.internal_rule_action(&json!({"do":"attack","attacker":"s","target":"P2.leader"}))
        .unwrap();
    let q = game.internal_pending_choice().unwrap().unwrap();
    let transformed = resolved(
        game.answer_internal_card(q.actor, q.token, Answer::Pending(2))
            .unwrap(),
    );
    game.resume_internal_card().unwrap();
    let q = game.internal_pending_choice().unwrap().unwrap();
    resolved(
        game.answer_internal_card(q.actor, q.token, Answer::Pending(2))
            .unwrap(),
    );
    game.resume_internal_card().unwrap();
    let results = drain(&mut game);
    assert_eq!(results.len(), 3);
    assert_eq!(results[0].source.holder, transformed.source.holder);
    assert_eq!(results[1].source.holder.id, transformed.source.holder.id);
    assert_eq!(results[1].source.holder.generation, Generation(2));
    assert_ne!(results[0].source.instance, results[1].source.instance);
    assert_eq!(results[2].source.holder.generation, Generation(4));
    assert!(
        !results[1]
            .progress
            .facts
            .iter()
            .any(|fact| matches!(fact, Fact::StatChange(_)))
    );
    assert!(
        results[2]
            .progress
            .facts
            .iter()
            .any(|fact| matches!(fact, Fact::StatChange(_)))
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
        1
    );
}
