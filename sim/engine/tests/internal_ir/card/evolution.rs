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

pub(super) fn position(
    seat: u32,
    base: &WellFormedCardProgram,
    form: &WellFormedCardProgram,
) -> Game {
    position_with_decks(seat, base, form, false)
}

fn position_with_decks(
    seat: u32,
    base: &WellFormedCardProgram,
    form: &WellFormedCardProgram,
    turn_decks: bool,
) -> Game {
    position_with_zones(seat, base, form, turn_decks, &Value::Null)
}

fn position_with_zones(
    seat: u32,
    base: &WellFormedCardProgram,
    form: &WellFormedCardProgram,
    turn_decks: bool,
    zones: &Value,
) -> Game {
    let snapshot = [("unit-follower","フォロワー","1","2","3"),("unit-form","フォロワー・エボルヴ","-","4","5"),("unit-spell","スペル","1","-","-"),("unit-amulet","アミュレット","1","-","-"),("unit-other","スペル","2","-","-")]
        .iter().map(|(number,kind,cost,power,hp)| json!({"number":number,"faces":[{"name":if *number=="unit-form" {"unit-follower"} else {number},"card_class":"ニュートラル","card_type":kind,"cost":cost,"power":power,"hp":hp,"traits":[],"text":null,"sections":[]}]}).to_string()).collect::<Vec<_>>().join("\n");
    let registry = json!({"format":1,"kind":"keyword_registry","keywords":{}}).to_string();
    let document=json!({"format":1,"kind":"effect_set","cards":{
        "unit-follower":{"status":"complete","abilities":[]},"unit-form":{"status":"complete","abilities":[]},"unit-spell":{"status":"complete","abilities":[]},"unit-amulet":{"status":"complete","abilities":[]},"unit-other":{"status":"complete","abilities":[]}
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
    for player in ["P1", "P2"] {
        if let Some(zones) = zones[player].as_object() {
            initial["players"][player]["zones"]
                .as_object_mut()
                .unwrap()
                .extend(zones.clone());
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
    game.load_internal_card("unit-amulet", &card(vec![]))
        .unwrap();
    game.load_internal_card("unit-other", &card(vec![]))
        .unwrap();
    game
}

// Schema compilation dominates these tests, so they share one immutable lowered fixture.
fn programs() -> &'static (WellFormedCardProgram, WellFormedCardProgram) {
    static PROGRAMS: OnceLock<(WellFormedCardProgram, WellFormedCardProgram)> = OnceLock::new();
    PROGRAMS.get_or_init(lowered_programs)
}

fn lowered_programs() -> (WellFormedCardProgram, WellFormedCardProgram) {
    let lowered = lowerer().lower(fanfare::expanded::tree()).unwrap();
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

#[test]
fn commander_evolution_condition_lowers_public_field_qualification() {
    let mut tree = fanfare::expanded::tree().clone();
    tree["cards"]["synthetic-evolution"]["abilities"][0]["evolve"]["usable_if"] =
        json!({"exists":{"who":"self","zone":"field","trait":"指揮官"}});
    lowerer().lower(&tree).unwrap();
}

#[test]
fn cemetery_evolution_condition_lowers_original_cost_count() {
    let mut tree = fanfare::expanded::tree().clone();
    tree["cards"]["synthetic-evolution"]["abilities"][0]["evolve"]["usable_if"] = json!({"cmp":{"left":{"count":{"who":"self","zone":"cemetery","orig_cost_eq":2}},"op":"ge","right":10}});
    lowerer().lower(&tree).unwrap();
}

fn cemetery_condition() -> Value {
    json!({"cmp":{"left":{"count":{"who":"self","zone":"cemetery","orig_cost_eq":1}},"op":"ge","right":10}})
}

fn cemetery_program(condition: &Value) -> WellFormedCardProgram {
    let mut tree = fanfare::expanded::tree().clone();
    let abilities = &mut tree["cards"]["synthetic-evolution"]["abilities"];
    abilities[0]["evolve"] = json!({"cost":{"pp":1},"usable_if":condition});
    let mut ordinary = abilities[0].clone();
    ordinary["line"] = json!(2);
    ordinary["evolve"] = json!({"cost":{"pp":1}});
    abilities.as_array_mut().unwrap().push(ordinary);
    let lowered = lowerer().lower(&tree).unwrap();
    card(
        lowered["synthetic-evolution"]
            .card()
            .abilities
            .iter()
            .zip(1_u64..)
            .map(|(ability, line)| relabel(ability.clone(), "unit-follower", line))
            .collect(),
    )
}

#[test]
fn cemetery_evolution_counts_all_card_types_by_original_cost_and_controller() {
    let base = cemetery_program(&cemetery_condition());
    for seat in [1, 2] {
        let actor = if seat == 1 { "P1" } else { "P2" };
        let opponent = if seat == 1 { "P2" } else { "P1" };
        for (owner, zone, count, expected) in [
            (actor, "cemetery", 9, false),
            (actor, "cemetery", 10, true),
            (actor, "cemetery", 11, true),
            (opponent, "cemetery", 10, false),
            (actor, "hand", 10, false),
            (actor, "ex", 10, false),
        ] {
            let mut zones = json!({});
            zones[owner][zone] = json!((0..count).map(|i| json!({"id":format!("c{i}"),"card":(["unit-follower","unit-spell","unit-amulet"][i%3]),"state":{"cost":8}})).collect::<Vec<_>>());
            // A modified effective cost does not turn this original cost 2 card into cost 1.
            if let Some(cards) = zones
                .get_mut(actor)
                .and_then(|zones| zones.get_mut("cemetery"))
                .and_then(Value::as_array_mut)
            {
                cards.push(json!({"id":"wrong-cost","card":"unit-other","state":{"cost":1}}));
            }
            let mut game = position_with_zones(seat, &base, &card(vec![]), false, &zones);
            let before = game.projection(View::Referee).unwrap();
            let result = game
                .internal_rule_action(
                    &json!({"do":"evolve","source":"s","evolve_card":"e","definition":base.card().abilities[0].node.id.0,"pay":{"pp":1}}),
                )
                .unwrap();
            if expected {
                assert_eq!(
                    result.outcome, "resolved",
                    "the live original-cost threshold is met"
                );
                let state = game.projection(View::Referee).unwrap();
                assert_eq!(
                    state[actor]["pp"]["current"],
                    before[actor]["pp"]["current"].as_i64().unwrap() - 1,
                    "only the printed evolution cost is paid"
                );
                assert_eq!(
                    state["objects"]["s"]["power"], 4,
                    "the real form is installed"
                );
                assert_eq!(
                    state[actor]["cemetery"], before[actor]["cemetery"],
                    "the qualification does not pay or move cemetery materials"
                );
            } else {
                assert_eq!(
                    result.outcome, "cannot-evolve",
                    "wrong ownership, zone or count does not qualify"
                );
                assert!(
                    result.events.is_empty(),
                    "a failed prerequisite reveals nothing"
                );
                assert_eq!(
                    game.projection(View::Referee).unwrap(),
                    before,
                    "failure spends no PP, form or evolution allowance"
                );
                game.internal_rule_action(
                    &json!({"do":"evolve","source":"s","evolve_card":"e","definition":base.card().abilities[1].node.id.0,"pay":{"pp":1}}),
                )
                .unwrap();
                assert_eq!(
                    game.projection(View::Referee).unwrap()["objects"]["s"]["power"],
                    4,
                    "a rejected conditional attempt leaves ordinary evolution available"
                );
            }
        }
    }
}

#[test]
fn cemetery_evolution_lowering_rejects_hidden_filters_and_invalid_cost_syntax() {
    let mut cases = vec![];
    for (key, value) in [
        ("zone", json!("hand")),
        ("zone", json!("deck")),
        (
            "orig_cost_eq",
            json!({"resource":{"who":"self","kind":"pp"}}),
        ),
        ("cost_eq", json!(1)),
    ] {
        let mut condition = cemetery_condition();
        if key == "cost_eq" {
            condition["cmp"]["left"]["count"]
                .as_object_mut()
                .unwrap()
                .remove("orig_cost_eq");
        }
        condition["cmp"]["left"]["count"][key] = value;
        cases.push(condition);
    }
    for condition in cases {
        let mut tree = fanfare::expanded::tree().clone();
        tree["cards"]["synthetic-evolution"]["abilities"][0]["evolve"]["usable_if"] = condition;
        assert!(
            lowerer().lower(&tree).is_err(),
            "unimplemented queries must fail closed"
        );
    }
}

fn qualification_game() -> Game {
    Game::new(
        catalog(),
        &setup(1, json!([])),
        &Value::Null,
        &Value::Null,
        "qualification-admission",
    )
    .unwrap()
}

#[test]
fn cemetery_evolution_admission_rejects_filtered_private_zone_reads() {
    let base = cemetery_program(&cemetery_condition());
    qualification_game()
        .load_internal_card("unit-follower", &base)
        .unwrap();
    for hidden in [ZoneKind::Hand, ZoneKind::Deck] {
        let mut candidate = base.card().clone();
        let CostSpec::Ordered { steps } =
            &mut candidate.abilities[0].node.play.as_mut().unwrap().costs
        else {
            panic!("fixed costs")
        };
        let Expr::Compute(operation) = &mut steps[0]
            .instruction
            .precondition
            .as_mut()
            .unwrap()
            .predicate
        else {
            panic!("comparison")
        };
        let Predicate::CompareInt {
            left: Expr::Compute(count),
            ..
        } = operation.as_mut()
        else {
            panic!("count comparison")
        };
        let IntOperation::FromNat {
            value: Expr::Live(read),
        } = count.as_mut()
        else {
            panic!("count conversion")
        };
        let NatRead::ZoneCount {
            selector: Selector::Zone { zone, .. },
        } = read.as_mut()
        else {
            panic!("public selector")
        };
        *zone = hidden;
        assert_cemetery_qualification_rejected(candidate);
    }
}

#[test]
fn evolution_qualification_uses_the_shared_public_condition_grammar() {
    for condition in [
        json!({"awakened":true}),
        json!({"cmp":{"left":{"count":{"who":"self","zone":"cemetery","name":"unit-spell"}},"op":"ge","right":1}}),
        json!({"cmp":{"left":{"count":{"who":"self","zone":"ex","name_contains":"unit","token":true}},"op":"ge","right":1}}),
        json!({"cmp":{"left":{"count":{"who":"self","zone":"cemetery","not_token":true}},"op":"ge","right":1}}),
        json!({"exists":{"who":"opp","zone":"field","trait":"指揮官"}}),
        json!({"cmp":{"left":{"count":{"who":"opp","zone":"cemetery","kind":"spell"}},"op":"ge","right":{"resource":{"who":"self","kind":"pp"}}}}),
    ] {
        let mut tree = fanfare::expanded::tree().clone();
        tree["cards"]["synthetic-evolution"]["abilities"][0]["evolve"]["usable_if"] = condition;
        lowerer().lower(&tree).unwrap();
    }
}

fn assert_cemetery_qualification_rejected(candidate: CardProgram) {
    let checked = WellFormedCardProgram::check(candidate).unwrap();
    let mut game = qualification_game();
    let before = game.projection(View::Referee).unwrap();
    assert!(
        game.load_internal_card("unit-follower", &checked).is_err(),
        "only the exact public original-cost prerequisite is admitted"
    );
    assert_eq!(
        game.projection(View::Referee).unwrap(),
        before,
        "rejected admission does not alter the game"
    );
}

fn conditional_evolution(query: &Value) -> WellFormedCardProgram {
    let mut tree = fanfare::expanded::tree().clone();
    tree["cards"]["synthetic-evolution"]["abilities"][0]["evolve"] =
        json!({"cost":{"pp":0},"usable_if":{"exists":query}});
    let lowered = lowerer().lower(&tree).unwrap();
    card(vec![relabel(
        lowered["synthetic-evolution"].card().abilities[0].clone(),
        "unit-follower",
        1,
    )])
}

#[test]
fn commander_evolution_guard_checks_the_current_field_before_any_cost_or_reveal() {
    for seat in [1, 2] {
        for matched in [false, true] {
            let query = if matched {
                json!({"who":"self","zone":"field","kind":"follower"})
            } else {
                json!({"who":"self","zone":"field","trait":"指揮官"})
            };
            let mut game = position(seat, &conditional_evolution(&query), &card(vec![]));
            let before = game.projection(View::Referee).unwrap();
            let result = game.internal_rule_action(
                &json!({"do":"evolve","source":"s","evolve_card":"e","pay":{"pp":0}}),
            );
            if matched {
                assert_eq!(
                    result.unwrap().outcome,
                    "resolved",
                    "the admitted public condition allows evolution"
                );
                assert_eq!(
                    game.projection(View::Referee).unwrap()["objects"]["s"]["power"],
                    4,
                    "the actual form is installed"
                );
            } else {
                let refused = result.unwrap();
                assert_eq!(
                    refused.outcome, "cannot-evolve",
                    "an unmet printed qualification refuses the action"
                );
                assert!(
                    refused.events.is_empty(),
                    "the failed qualification reveals nothing"
                );
                assert_eq!(
                    game.projection(View::Referee).unwrap(),
                    before,
                    "a failed qualification cannot reveal, pay or evolve"
                );
            }
        }
    }
}

#[test]
fn commander_evolution_qualification_rejects_hidden_zones() {
    for zone in ["hand", "deck"] {
        let query = json!({"who":"self","zone":zone,"trait":"指揮官"});
        let mut tree = fanfare::expanded::tree().clone();
        tree["cards"]["synthetic-evolution"]["abilities"][0]["evolve"]["usable_if"] =
            json!({"exists":query});
        assert!(
            lowerer().lower(&tree).is_err(),
            "an intrinsic qualification cannot inspect private filtered cards"
        );
    }
}

#[test]
fn commander_evolution_admission_rejects_a_guard_on_the_payment_instruction() {
    let mut candidate =
        conditional_evolution(&json!({"who":"self","zone":"field","kind":"follower"}))
            .card()
            .clone();
    let CostSpec::Ordered { steps } = &mut candidate.abilities[0].node.play.as_mut().unwrap().costs
    else {
        panic!("fixed ordered costs");
    };
    let mut guard = steps[0].instruction.precondition.take().unwrap();
    guard.protects = steps[1].instruction.id;
    steps[1].instruction.precondition = Some(guard);
    assert!(
        WellFormedCardProgram::check(candidate).is_err(),
        "a guard on payment cannot be silently ignored or turn a required cost into an optional one"
    );
}

#[test]
fn commander_evolution_keeps_both_printed_evolution_definitions() {
    let mut tree = fanfare::expanded::tree().clone();
    let abilities = &mut tree["cards"]["synthetic-evolution"]["abilities"];
    let mut second = abilities[0].clone();
    second["line"] = json!(2);
    second["evolve"] = json!({"cost":{"pp":0},"usable_if":{"exists":{"who":"self","zone":"field","trait":"指揮官"}}});
    abilities.as_array_mut().unwrap().push(second);
    let programs = lowerer().lower(&tree).unwrap();
    assert_eq!(
        programs["synthetic-evolution"].card().abilities.len(),
        2,
        "ordinary and conditional intrinsic abilities retain distinct scopes"
    );
}
