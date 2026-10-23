use super::*;
use std::sync::OnceLock;
use sve_engine::game::internal::InputDomain;

#[expect(
    clippy::panic,
    reason = "Unexpected intrinsic fixture shapes must fail the test."
)]
fn map_steps(nodes: &mut [Located<Node>], span: &SourceSpan, origin: &AbilitySource) {
    for step in nodes {
        step.source = span.clone();
        match &mut step.node {
            Node::Instruction { instruction } => instruction.source = origin.clone(),
            Node::Control {
                control: Control::EachObject { body, .. },
            } => map_steps(&mut body.steps, span, origin),
            Node::Control { .. } | Node::Capture { .. } | Node::Decision { .. } => {
                panic!("unexpected intrinsic projection")
            }
        }
    }
}

#[expect(
    clippy::panic,
    reason = "Unexpected intrinsic costs must fail the test."
)]
fn relabel(mut ability: Located<Ability>, card: &str, line: u64) -> Located<Ability> {
    let mut span = source(card);
    span.fragment = None;
    span.line = u32::try_from(line).unwrap();
    let printed = AbilityOrigin::Printed {
        face: FaceId("0".into()),
        line: Nat(line),
    };
    match &mut ability.node.source.origin {
        AbilityOrigin::Keyword { origin, .. } => **origin = printed,
        origin @ (AbilityOrigin::Printed { .. } | AbilityOrigin::Granted { .. }) => {
            *origin = printed;
        }
    }
    ability.source = span.clone();
    let origin = ability.node.source.clone();
    if let Some(play) = &mut ability.node.play {
        let CostSpec::Ordered { steps } = &mut play.costs else {
            panic!("intrinsic costs")
        };
        for step in steps {
            step.instruction.source = origin.clone();
        }
    }
    map_steps(
        &mut ability.node.body.as_mut().unwrap().steps,
        &span,
        &origin,
    );
    ability
}

fn position(seat: u32, base: &WellFormedCardProgram, form: &WellFormedCardProgram) -> Game {
    position_with_decks(seat, base, form, false)
}

fn position_with_decks(
    seat: u32,
    base: &WellFormedCardProgram,
    form: &WellFormedCardProgram,
    turn_decks: bool,
) -> Game {
    let snapshot = [("unit-follower","フォロワー","1","2","3"),("unit-form","フォロワー・エボルヴ","-","4","5"),("unit-spell","スペル","1","-","-")]
        .iter().map(|(number,kind,cost,power,hp)| json!({"number":number,"faces":[{"name":if *number=="unit-form" {"unit-follower"} else {number},"card_class":"ニュートラル","card_type":kind,"cost":cost,"power":power,"hp":hp,"traits":[],"text":null,"sections":[]}]}).to_string()).collect::<Vec<_>>().join("\n");
    let registry = json!({"format":1,"kind":"keyword_registry","keywords":{}}).to_string();
    let document=json!({"format":1,"kind":"effect_set","cards":{
        "unit-follower":{"status":"complete","abilities":[]},"unit-form":{"status":"complete","abilities":[]},"unit-spell":{"status":"complete","abilities":[]}
    }}).to_string();
    let catalog = Arc::new(
        Catalog::from_documents(&snapshot, &registry, &[("evolution.yaml".into(), document)])
            .unwrap(),
    );
    let actor = if seat == 1 { "P1" } else { "P2" };
    let mut initial = setup(
        seat,
        json!([{"id":"d","card":"unit-spell"},{"id":"d2","card":"unit-spell"},{"id":"d3","card":"unit-spell"}]),
    );
    initial["turn"]["active"] = json!(actor);
    initial["turn"]["elapsed_turns"][actor] = json!(7);
    initial["players"][actor]["sep"] = json!(1);
    initial["players"][actor]["zones"]["hand"] = json!([]);
    initial["players"][actor]["zones"]["field"] =
        json!([{"id":"s","card":"unit-follower"},{"id":"u","card":"unit-follower"}]);
    initial["players"][actor]["zones"]["evolve_deck"] = json!([{"id":"e","card":"unit-form"}]);
    if turn_decks {
        for player in ["P1", "P2"] {
            initial["players"][player]["zones"]["deck"] = json!(
                (0..3)
                    .map(|i| json!({"id":format!("{player}-d{i}"),"card":"unit-spell"}))
                    .collect::<Vec<_>>()
            );
        }
    }
    let mut game = Game::new(
        catalog,
        &initial,
        &Value::Null,
        &Value::Null,
        "evolution-seed",
    )
    .unwrap();
    game.load_internal_card("unit-follower", base).unwrap();
    game.load_internal_card("unit-form", form).unwrap();
    game.load_internal_card("unit-spell", &card(vec![]))
        .unwrap();
    game
}

// Schema compilation dominates these tests, so they share one immutable lowered fixture.
fn programs() -> &'static (WellFormedCardProgram, WellFormedCardProgram) {
    static PROGRAMS: OnceLock<(WellFormedCardProgram, WellFormedCardProgram)> = OnceLock::new();
    PROGRAMS.get_or_init(lowered_programs)
}

fn lowered_programs() -> (WellFormedCardProgram, WellFormedCardProgram) {
    let lowered = second::lowerer().lower(fanfare::expanded::tree()).unwrap();
    let intrinsic = relabel(
        lowered["synthetic-evolution"].card().abilities[0].clone(),
        "unit-follower",
        1,
    );
    let sink = damage(
        8,
        RecipientSet::Objects {
            objects: zone(ZoneKind::Field, 80),
        },
        99,
    );
    let base = card(vec![
        intrinsic,
        ability(7, vec![draw(7, 0)]),
        ability(8, vec![sink]),
    ]);
    let mut evolved = relabel(
        lowered["BP20-R14"].card().abilities[0].clone(),
        "unit-form",
        1,
    );
    evolved.node.id = AbilityId(7);
    evolved.node.body.as_mut().unwrap().steps.extend([
        source_information::source_hit(
            10,
            source_information::source_read(
                &ObjectView::SourceInformation {},
                CharacteristicView::Original,
                None,
            ),
        ),
        source_information::source_hit(
            11,
            source_information::source_read(
                &ObjectView::SourceInformation {},
                CharacteristicView::Effective,
                Some(Stat::Attack),
            ),
        ),
    ]);
    map_steps(
        &mut evolved.node.body.as_mut().unwrap().steps,
        &evolved.source,
        &evolved.node.source,
    );
    let lastword = relabel(
        triggers::lastword(ability(
            8,
            vec![source_information::source_hit(12, Expr::Literal(Int(2)))],
        )),
        "unit-form",
        8,
    );
    (base, card(vec![evolved, lastword]))
}

#[test]
fn bp20_r14_evolve_draw_fragment_uses_new_print_instance_and_real_super_facet() {
    let (base, form) = programs();
    for seat in [1, 2] {
        let mut game = position(seat, base, form);
        let prior = resolved(
            game.activate_internal_card("s", AbilityId(7), PlayerId { seat })
                .unwrap(),
        );
        game.resume_internal_card().unwrap();
        let super_evolution = u64::from(seat == 2);
        let step=game.internal_rule_action(&json!({"do":"evolve","source":"s","evolve_card":"e","pay":{"pp":1,"ep":0,"sep":super_evolution},"kind":"forged-unused"})).unwrap();
        assert_eq!(step.outcome, "resolved");
        assert!(
            step.events
                .iter()
                .any(|event| event["kind"] == "公開" && event["object"] == "e")
        );
        let question = game.internal_pending_choice().unwrap().unwrap();
        assert!(
            matches!(&question.domain,InputDomain::Pending(options) if options.len()==1 && options[0].card=="unit-form" && options[0].event.contains(if seat==1 {"Evolved"} else {"SuperEvolved"}))
        );
        let evolved = resolved(
            game.answer_internal_card(question.actor, question.token, Answer::Pending(1))
                .unwrap(),
        );
        assert_eq!(evolved.source.holder, prior.source.holder);
        assert_eq!(evolved.source.definition, prior.source.definition);
        assert_ne!(evolved.source.instance, prior.source.instance);
        assert_eq!(evolved.source.holder.generation, Generation(0));
        let opponent = if seat == 1 { "P2" } else { "P1" };
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view[opponent]["leader"]["life"], 15 - super_evolution);
        assert_eq!(
            view[if seat == 1 { "P1" } else { "P2" }]["pp"]["current"],
            1
        );
        assert!(
            evolved
                .progress
                .facts
                .iter()
                .any(|fact| matches!(fact, Fact::Draw(_)))
        );
        game.resume_internal_card().unwrap();
        resolved(
            game.activate_internal_card("u", AbilityId(8), PlayerId { seat })
                .unwrap(),
        );
        game.resume_internal_card().unwrap();
        let lastword = game.internal_pending_choice().unwrap().unwrap();
        let lastword = resolved(
            game.answer_internal_card(lastword.actor, lastword.token, Answer::Pending(1))
                .unwrap(),
        );
        assert_eq!(lastword.source.holder, evolved.source.holder);
        assert_eq!(lastword.source.definition, AbilityId(8));
        assert_eq!(
            game.projection(View::Referee).unwrap()[opponent]["leader"]["life"],
            13 - super_evolution
        );
        game.resume_internal_card().unwrap();
        assert!(game.internal_pending_choice().unwrap().is_none());
    }
}

fn limited_activation(id: u64, maximum: u64, cost: i64) -> Located<Ability> {
    let mut a = ability(id, vec![draw(id, 0)]);
    let mut paid = payment::paid_ability(vec![payment::pp(cost)], vec![]);
    let play = paid.node.play.as_mut().unwrap();
    if let CostSpec::Ordered { steps } = &mut play.costs {
        for step in steps {
            step.instruction.source = a.node.source.clone();
            let slot = 1_000_u64.checked_add(id).unwrap();
            step.instruction.id = InstructionId(slot);
            step.instruction.receipt = ReceiptId(slot);
        }
    }
    play.limit = Some(LimitSpec {
        maximum: Nat(maximum),
        window: LimitWindow::Turn,
        scope: LimitScope::AbilityInstance,
        consume_at: ConsumptionPoint::PlayCommitted,
    });
    a.node.play = paid.node.play;
    a
}

fn quota_programs() -> (WellFormedCardProgram, WellFormedCardProgram) {
    let (original, _) = programs();
    let base = card(vec![
        original.card().abilities[0].clone(),
        limited_activation(7, 1, 1),
        limited_activation(8, 1, 2),
        ability(
            9,
            vec![movement::put(
                9,
                Selector::Objects {
                    objects: vec![ObjectOperand::SourceHolder {}],
                },
                OwnerZone::Cemetery,
            )],
        ),
        ability(
            10,
            vec![instruction(
                10,
                Action::Move {
                    objects: zone(ZoneKind::Cemetery, 70),
                    movement: MoveSpec {
                        to: MoveDestination::Shared {
                            zone: ZoneRef {
                                player: PlayerRef::Controller {},
                                kind: ZoneKind::Field,
                            },
                        },
                        initial_posture: None,
                        entry_effects: vec![],
                        position: None,
                        order: None,
                    },
                    reason: MoveReason::Put,
                },
            )],
        ),
        limited_activation(11, 2, 0),
    ]);
    let form = card(vec![relabel(limited_activation(7, 1, 0), "unit-form", 7)]);
    (base, form)
}

#[test]
fn printed_activation_quota_tracks_turn_reentry_and_evolved_definition() {
    let (base, form) = quota_programs();
    let mut game = position_with_decks(1, &base, &form, true);
    let actor = PlayerId { seat: 1 };
    resolved(
        game.activate_internal_card("u", AbilityId(8), actor)
            .unwrap(),
    );
    game.resume_internal_card().unwrap();
    let before = game.clone();
    assert!(matches!(
        game.activate_internal_card("s", AbilityId(7), actor),
        Err(sve_engine::EngineFailure::IllegalPlay(_))
    ));
    transactions::assert_restored(&game, &before);
    play::next_turn(&mut game);
    play::next_turn(&mut game);
    let first = resolved(
        game.activate_internal_card("s", AbilityId(7), actor)
            .unwrap(),
    );
    game.resume_internal_card().unwrap();
    assert!(matches!(
        game.activate_internal_card("s", AbilityId(7), actor),
        Err(sve_engine::EngineFailure::IllegalPlay(_))
    ));
    resolved(
        game.activate_internal_card("s", AbilityId(9), actor)
            .unwrap(),
    );
    game.resume_internal_card().unwrap();
    resolved(
        game.activate_internal_card("u", AbilityId(10), actor)
            .unwrap(),
    );
    game.resume_internal_card().unwrap();
    let returned = resolved(
        game.activate_internal_card("s", AbilityId(7), actor)
            .unwrap(),
    );
    assert_ne!(first.source.instance, returned.source.instance);
    assert_eq!(returned.source.holder.generation, Generation(2));
    game.resume_internal_card().unwrap();
    game.internal_rule_action(
        &json!({"do":"evolve","source":"s","evolve_card":"e","pay":{"pp":1,"ep":0,"sep":0}}),
    )
    .unwrap();
    let evolved = resolved(
        game.activate_internal_card("s", AbilityId(7), actor)
            .unwrap(),
    );
    assert_eq!(evolved.source.holder, returned.source.holder);
    assert_ne!(evolved.source.instance, returned.source.instance);
    game.resume_internal_card().unwrap();
    assert!(matches!(
        game.activate_internal_card("s", AbilityId(7), actor),
        Err(sve_engine::EngineFailure::IllegalPlay(_))
    ));
    play::next_turn(&mut game);
    play::next_turn(&mut game);
    let next = resolved(
        game.activate_internal_card("s", AbilityId(7), actor)
            .unwrap(),
    );
    assert_eq!(next.source.instance, evolved.source.instance);
    game.resume_internal_card().unwrap();
    resolved(
        game.activate_internal_card("u", AbilityId(11), actor)
            .unwrap(),
    );
    game.resume_internal_card().unwrap();
    resolved(
        game.activate_internal_card("u", AbilityId(11), actor)
            .unwrap(),
    );
    game.resume_internal_card().unwrap();
    assert!(matches!(
        game.activate_internal_card("u", AbilityId(11), actor),
        Err(sve_engine::EngineFailure::IllegalPlay(_))
    ));
}

#[test]
fn super_evolve_pending_is_kind_checked_and_controller_orders_both_facets() {
    let (base, _) = programs();
    let mut triggers = second::evolve_triggers().into_iter();
    let mut evolve = relabel(triggers.next().unwrap(), "unit-form", 1);
    evolve.node.id = AbilityId(1);
    let mut super_evolve = relabel(triggers.next().unwrap(), "unit-form", 2);
    super_evolve.node.id = AbilityId(2);
    let mut invalid = super_evolve.clone();
    let TriggerCondition::Event {
        pattern: EventPattern::Evolve { predicate, .. },
    } = &mut invalid.node.trigger.as_mut().unwrap().on
    else {
        panic!("evolve");
    };
    let Expr::Compute(and) = predicate else {
        panic!("and");
    };
    let Predicate::And { predicates } = and.as_mut() else {
        panic!("and");
    };
    *predicate = predicates[0].clone();
    WellFormedCardProgram::check(CardProgram {
        abilities: vec![invalid],
        card_play_costs: vec![],
    })
    .unwrap_err();
    let form = card(vec![evolve, super_evolve]);
    for super_payment in [0, 1] {
        let mut game = position(1, base, &form);
        game.internal_rule_action(&json!({"do":"evolve","source":"s","evolve_card":"e","pay":{"pp":1,"ep":0,"sep":super_payment}})).unwrap();
        let q = game.internal_pending_choice().unwrap().unwrap();
        assert!(
            matches!(&q.domain, InputDomain::Pending(options) if options.len() == if super_payment==1 {2} else {1})
        );
        let handle = if super_payment == 1 { 2 } else { 1 };
        let first = resolved(
            game.answer_internal_card(q.actor, q.token, Answer::Pending(handle))
                .unwrap(),
        );
        assert_eq!(first.source.definition, AbilityId(handle));
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["sep"],
            1 - super_payment
        );
        game.resume_internal_card().unwrap();
        if super_payment == 1 {
            let q = game.internal_pending_choice().unwrap().unwrap();
            assert!(matches!(&q.domain, InputDomain::Pending(options) if options.len()==1));
            game.answer_internal_card(q.actor, q.token, Answer::Pending(1))
                .unwrap();
            game.resume_internal_card().unwrap();
        }
        assert!(game.internal_pending_choice().unwrap().is_none());
    }
}
