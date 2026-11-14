use super::*;
use core::mem::swap;
use sve_engine::game::player::Viewer;
use sve_engine::game::player::observation::{
    EffectRecipient, InstalledChange, ObservedZone, PreventionChange,
};

fn input(effect: &Value) -> Value {
    equipment_played::tree(&json!({"static":{"effects":[effect]}}), "unit-follower")
}
fn lower_and_load(effect: &Value) {
    let programs = lowerer().lower(&input(effect)).unwrap();
    let mut game = Game::new(
        catalog(),
        &setup(1, json!([])),
        &Value::Null,
        &Value::Null,
        "prevention-controller-preflight",
    )
    .unwrap();
    game.load_internal_card("unit-follower", &programs["unit-follower"])
        .unwrap();
}

fn amulet(mut abilities: Vec<Located<Ability>>) -> WellFormedCardProgram {
    for ability in &mut abilities {
        ability.source.card_id = "unit-amulet".into();
        ability.source.card_no = "unit-amulet".into();
        if let Some(body) = &mut ability.node.body {
            locate_steps(&mut body.steps, &ability.source, &ability.node.source);
        }
    }
    card(abilities)
}

#[test]
fn ordinary_self_ability_damage_prevention_is_a_valid_lower_load_baseline() {
    lower_and_load(&json!({"prevent":{"kind":"ability"}}));
}

fn union_prevention() -> WellFormedCardProgram {
    lowerer().lower(&input(&json!({"prevent":{"kind":"ability","on":{"any_of":[
        {"who":"self","zone":"field","kind":"follower","trait":"academy","not_same_name_as_this":true},
        {"who":"self","zone":"field","kind":"follower","trait":"beast"}
    ]}}}))).unwrap().remove("unit-follower").unwrap()
}

fn union_catalog() -> Arc<Catalog> {
    let (snapshot, registry, document) = catalog_documents(&json!("1"));
    let mut rows = snapshot
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    let template = rows
        .iter_mut()
        .find(|row| row["number"] == "unit-follower")
        .unwrap();
    template["faces"][0]["traits"] = json!(["academy"]);
    let template = template.clone();
    let mut document: Value = serde_json::from_str(&document).unwrap();
    for (number, name, traits) in [
        ("academy-peer", "other-name", json!(["academy"])),
        ("beast-peer", "unit-follower", json!(["beast"])),
        ("ordinary-peer", "ordinary-name", json!([])),
    ] {
        let mut row = template.clone();
        row["number"] = json!(number);
        row["faces"][0]["name"] = json!(name);
        row["faces"][0]["traits"] = traits;
        rows.push(row);
        document["cards"][number] =
            json!({"status":"complete","review":"synthetic","abilities":[]});
    }
    let snapshot = rows
        .iter()
        .map(Value::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    Arc::new(
        Catalog::from_documents(
            &snapshot,
            &registry,
            &[("unit.yaml".into(), document.to_string())],
        )
        .unwrap(),
    )
}

fn union_board(silenced: bool) -> Game {
    let mut position = setup(1, json!([]));
    position["players"]["P1"]["zones"]["field"] = json!([
        {"id":"provider","card":"unit-follower","state":{"silenced":silenced}},
        {"id":"same-name","card":"unit-follower","state":{"silenced":true}},
        {"id":"academy","card":"academy-peer"},
        {"id":"beast","card":"beast-peer"},
        {"id":"ordinary","card":"ordinary-peer"}
    ]);
    position["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"academy-peer"}]);
    let mut game = Game::new(
        union_catalog(),
        &position,
        &Value::Null,
        &Value::Null,
        "prevention-union",
    )
    .unwrap();
    game.load_internal_card("unit-follower", &union_prevention())
        .unwrap();
    for number in ["unit-spell", "academy-peer", "beast-peer", "ordinary-peer"] {
        game.load_internal_card(number, &card(vec![])).unwrap();
    }
    game
}

#[test]
fn prevention_union_keeps_name_exclusion_in_its_own_branch_and_is_live() {
    for silenced in [false, true] {
        let mut game = union_board(silenced);
        game.execute_internal(
            &checked(vec![damage(
                1,
                RecipientSet::Objects {
                    objects: zone(ZoneKind::Field, 99),
                },
                2,
            )]),
            &source("synthetic"),
            context(1),
        )
        .unwrap();
        let view = game.projection(View::Referee).unwrap();
        for id in ["provider", "same-name", "ordinary", "enemy"] {
            assert_eq!(view["objects"][id]["damage"], 2, "unprotected {id}");
        }
        for id in ["academy", "beast"] {
            assert_eq!(
                view["objects"][id]["damage"],
                if silenced { 2 } else { 0 },
                "protected {id}"
            );
        }
    }
}

#[test]
fn prevention_union_does_not_authorize_a_private_branch() {
    let mut raw = union_prevention().card().clone();
    let Effect::Replacement {
        pattern:
            ProposedEventPattern::Damage {
                recipient: Some(recipient),
                ..
            },
        ..
    } = &mut raw.abilities[0].node.declarations[0]
    else {
        panic!("replacement")
    };
    let Expr::Compute(predicate) = &mut recipient.condition else {
        panic!("recipient")
    };
    let Predicate::RecipientIsFollower {
        objects: Selector::Union { sets },
        ..
    } = predicate.as_mut()
    else {
        panic!("union")
    };
    let Selector::Zone { zone, .. } = &mut sets[1] else {
        panic!("zone")
    };
    *zone = ZoneKind::Hand;
    let program = WellFormedCardProgram::check(raw).unwrap();
    let mut game = Game::new(
        union_catalog(),
        &setup(1, json!([])),
        &Value::Null,
        &Value::Null,
        "private-prevention-branch",
    )
    .unwrap();
    let mut control = game.clone();
    control
        .load_internal_card("unit-follower", &union_prevention())
        .unwrap();
    let error = game
        .load_internal_card("unit-follower", &program)
        .unwrap_err();
    assert!(
        error.to_string().contains("private observation"),
        "the same catalog accepts the original program and rejects the private branch at its privacy boundary: {error}"
    );
}
#[test]
fn ordinary_own_followers_ability_damage_reduction_is_a_valid_lower_load_baseline() {
    lower_and_load(
        &json!({"damage_mod":{"on":{"who":"self","zone":"field","kind":"follower"},"kind":"ability","delta":-1}}),
    );
}
#[test]
fn existing_field_source_filter_is_a_valid_but_narrower_lower_load_baseline() {
    lower_and_load(&json!({"prevent":{"kind":"ability","source":{"who":"opp","zone":"field"}}}));
}
#[test]
fn opponent_only_ability_damage_prevention_requires_a_controller_only_source_query() {
    lower_and_load(&json!({"prevent":{"kind":"ability","source":{"who":"opp"}}}));
}
#[test]
fn opponent_only_ability_damage_reduction_requires_a_controller_only_source_query() {
    lower_and_load(
        &json!({"damage_mod":{"on":{"who":"self","zone":"field","kind":"follower"},"kind":"ability","source":{"who":"opp"},"delta":-1}}),
    );
}

fn controller_program() -> WellFormedCardProgram {
    lowerer()
        .lower(&input(
            &json!({"prevent":{"kind":"ability","source":{"who":"opp"}}}),
        ))
        .unwrap()
        .remove("unit-follower")
        .unwrap()
}

#[test]
fn controller_only_source_keeps_other_author_shapes_closed() {
    for shape in [
        json!({"prevent":{"kind":"ability","source":{"who":"self"}}}),
        json!({"prevent":{"kind":"combat","source":{"who":"opp"}}}),
        json!({"prevent":{"kind":"any","source":{"who":"opp"}}}),
        json!({"prevent":{"kind":"ability","source":{"who":"opp","kind":"follower"}}}),
        json!({"prevent":{"kind":"ability","on":{"leader":"self"},"source":{"who":"opp"}}}),
    ] {
        lowerer().lower(&input(&shape)).unwrap_err();
    }
    let mut equipment = input(&json!({"prevent":{"kind":"ability","source":{"who":"opp"}}}));
    equipment["cards"]["unit-follower"]["abilities"][0]["static"]["zone"] = json!("equipment");
    lowerer().lower(&equipment).unwrap_err();
    let applied = equipment_played::tree(
        &json!({"activate":{"cost":{"pp":0},"do":[{"apply":{"on":"this","effect":{"prevent":{"kind":"ability","source":{"who":"opp"}}},"until":"turn"}}]}}),
        "unit-follower",
    );
    lowerer().lower(&applied).unwrap_err();
}

#[test]
fn raw_source_predicates_cannot_reverse_or_replace_the_canonical_players() {
    for reverse in [false, true] {
        let mut raw = controller_program().card().clone();
        let Effect::Replacement {
            pattern:
                ProposedEventPattern::Damage {
                    source: Some(source),
                    ..
                },
            ..
        } = &mut raw.abilities[0].node.declarations[0]
        else {
            panic!("expected the tested IR shape")
        };
        let Expr::Compute(predicate) = &mut source.condition else {
            panic!("expected the tested IR shape")
        };
        let Predicate::PlayerEquals { left, right } = predicate.as_mut() else {
            panic!("expected the tested IR shape")
        };
        if reverse {
            swap(left, right);
        } else {
            *right = Expr::Literal(PlayerRef::Controller {});
        }
        WellFormedCardProgram::check(raw).unwrap_err();
    }
}

#[test]
fn contextual_damage_controller_is_rejected_in_ordinary_values_and_conditions() {
    let contextual = PlayerRef::ProposedDamageController {};
    let comparison = Expr::Compute(Box::new(Predicate::PlayerEquals {
        left: Expr::Literal(contextual.clone()),
        right: Expr::Literal(PlayerRef::Controller {}),
    }));
    for action in [
        Action::Draw {
            player: contextual.clone(),
            count: Expr::Literal(Int(1)),
            mode: StepwiseMode::Exact,
        },
        Action::Life {
            player: contextual,
            change: NumericChange::Add {
                amount: Expr::Literal(Int(1)),
            },
        },
    ] {
        WellFormedProgram::check(body(1, vec![instruction(1, action)])).unwrap_err();
    }
    let mut step = instruction(
        1,
        Action::Draw {
            player: PlayerRef::Controller {},
            count: Expr::Literal(Int(0)),
            mode: StepwiseMode::Exact,
        },
    );
    let Node::Instruction { instruction } = &mut step.node else {
        panic!("expected the tested IR shape")
    };
    instruction.precondition = Some(Precondition {
        kind: PreconditionKind::CardTextPrerequisite,
        predicate: comparison.clone(),
        protects: InstructionId(1),
        reason: PreconditionReason::CardText {},
    });
    WellFormedProgram::check(body(1, vec![step])).unwrap_err();
    let mut raw = controller_program().card().clone();
    let Effect::Replacement { condition, .. } = &mut raw.abilities[0].node.declarations[0] else {
        panic!("expected the tested IR shape")
    };
    *condition = comparison;
    WellFormedCardProgram::check(raw).unwrap_err();
}

#[test]
fn native_damage_uses_the_ability_controller_even_when_the_object_has_another_owner() {
    for actor in [1, 2] {
        for owner in ["P1", "P2"] {
            let mut position = setup(1, json!([]));
            position["players"]["P1"]["zones"]["hand"][0]["owner"] = json!(owner);
            if actor == 2 {
                position["players"]["P2"]["zones"]["hand"] =
                    position["players"]["P1"]["zones"]["hand"].take();
            }
            let mut board = Game::new(
                catalog(),
                &position,
                &Value::Null,
                &Value::Null,
                "controller-owner",
            )
            .unwrap();
            board
                .load_internal_card("unit-follower", &controller_program())
                .unwrap();
            board
                .load_internal_card("unit-spell", &card(vec![]))
                .unwrap();
            let result = board
                .execute_internal(
                    &checked(vec![damage(
                        1,
                        RecipientSet::Objects {
                            objects: zone(ZoneKind::Field, 99),
                        },
                        2,
                    )]),
                    &source("synthetic"),
                    context(actor),
                )
                .unwrap();
            assert_eq!(
                result
                    .facts
                    .iter()
                    .filter(|f| matches!(f, Fact::Damage(_)))
                    .count(),
                usize::from(actor == 2)
            );
            assert_eq!(
                board.projection(View::Referee).unwrap()["objects"]["f"]["damage"],
                if actor == 2 { 2 } else { 0 }
            );
        }
    }
}

#[test]
fn a_departed_stolen_source_keeps_its_ability_controller_and_old_generation() {
    for (destination, zone_name) in [(OwnerZone::Cemetery, "cemetery"), (OwnerZone::Hand, "hand")] {
        let mut position = setup(1, json!([]));
        position["players"]["P1"]["zones"]["hand"] = json!([]);
        position["players"]["P1"]["zones"]["field"] =
            json!([{"id":"s","card":"unit-amulet","owner":"P2"}]);
        let mut board = Game::new(
            catalog(),
            &position,
            &Value::Null,
            &Value::Null,
            "departed-stolen-source",
        )
        .unwrap();
        board
            .load_internal_card("unit-follower", &controller_program())
            .unwrap();
        let departure = movement::put(
            1,
            Selector::Objects {
                objects: vec![ObjectOperand::SourceHolder {}],
            },
            destination,
        );
        let mut hit = damage(
            2,
            RecipientSet::Objects {
                objects: zone(ZoneKind::Field, 99),
            },
            2,
        );
        let Node::Instruction { instruction } = &mut hit.node else {
            panic!("expected the tested IR shape")
        };
        let Action::Damage {
            source: damage_source,
            ..
        } = &mut instruction.action
        else {
            panic!("expected the tested IR shape")
        };
        *damage_source = ObjectView::SourceInformation {};
        board
            .load_internal_card(
                "unit-amulet",
                &amulet(vec![ability(1, vec![departure, hit])]),
            )
            .unwrap();
        let result = activate(&mut board, 1).progress;
        assert!(!result.facts.iter().any(|f| matches!(f, Fact::Damage(_))));
        let state = board.projection(View::Referee).unwrap();
        assert!(
            state["P2"][zone_name]
                .as_array()
                .unwrap()
                .contains(&json!("s"))
        );
        assert_eq!(state["objects"]["f"]["damage"], 0);
    }
}

#[test]
fn a_stolen_sources_real_lastword_uses_its_pre_departure_ability_controller() {
    use sve_engine::game::internal::{Answer, InputDomain};
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"s","card":"unit-amulet","owner":"P2"}]);
    let destroy = instruction(
        1,
        Action::Destroy {
            objects: Selector::Objects {
                objects: vec![ObjectOperand::SourceHolder {}],
            },
        },
    );
    let mut hit = damage(
        2,
        RecipientSet::Objects {
            objects: zone(ZoneKind::Field, 99),
        },
        2,
    );
    let Node::Instruction { instruction } = &mut hit.node else {
        panic!("expected the tested IR shape")
    };
    let Action::Damage {
        source: damage_source,
        ..
    } = &mut instruction.action
    else {
        panic!("expected the tested IR shape")
    };
    *damage_source = ObjectView::SourceInformation {};
    let program = amulet(vec![
        ability(1, vec![destroy]),
        triggers::lastword(ability(2, vec![hit])),
    ]);
    let mut board = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "stolen-lastword-controller",
    )
    .unwrap();
    board.load_internal_card("unit-amulet", &program).unwrap();
    board
        .load_internal_card("unit-follower", &controller_program())
        .unwrap();
    activate(&mut board, 1);
    while board.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        board.resume_internal_card().unwrap();
    }
    assert_eq!(
        board.projection(View::Referee).unwrap()["P2"]["cemetery"],
        json!(["s"])
    );
    let question = board.internal_pending_choice().unwrap().unwrap();
    assert_eq!(question.actor, PlayerId { seat: 1 });
    let InputDomain::Pending(options) = question.domain else {
        panic!("expected the tested IR shape")
    };
    assert_eq!(options.len(), 1);
    let result = resolved(
        board
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap(),
    );
    assert!(
        !result
            .progress
            .facts
            .iter()
            .any(|fact| matches!(fact, Fact::Damage(_)))
    );
    assert_eq!(
        board.projection(View::Referee).unwrap()["objects"]["f"]["damage"],
        0
    );
}

#[test]
fn native_hit_uses_the_instruction_source_controller_instead_of_the_object_role() {
    let mut board = Game::new(
        catalog(),
        &setup(1, json!([])),
        &Value::Null,
        &Value::Null,
        "instruction-source-controller",
    )
    .unwrap();
    board
        .load_internal_card("unit-spell", &card(vec![]))
        .unwrap();
    board
        .load_internal_card("unit-follower", &controller_program())
        .unwrap();
    let mut hit = damage(
        1,
        RecipientSet::Objects {
            objects: zone(ZoneKind::Field, 99),
        },
        2,
    );
    let Node::Instruction { instruction } = &mut hit.node else {
        panic!("expected the tested IR shape")
    };
    instruction.source.controller = PlayerRef::RequireOne {
        players: Box::new(PlayerSet {
            relation: PlayerRelation::Opponents,
            relative_to: PlayerRef::Controller {},
            cardinality: Cardinality {
                minimum: Nat(1),
                maximum: Some(Nat(1)),
            },
            text_quantifier: TextQuantifier::One,
        }),
    };
    let result = board
        .execute_internal(&checked(vec![hit]), &source("synthetic"), context(1))
        .unwrap();
    let hits = result
        .facts
        .iter()
        .filter_map(|fact| {
            if let Fact::Damage(hit) = fact {
                Some(hit)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].controller, PlayerId { seat: 2 });
    assert_eq!(
        board.projection(View::Referee).unwrap()["objects"]["f"]["damage"],
        2
    );
}

#[test]
fn raw_controller_prevention_cannot_target_leaders_or_install_as_an_applied_effect() {
    let original = controller_program().card().clone();
    let mut leader = original.clone();
    let Effect::Replacement {
        pattern:
            ProposedEventPattern::Damage {
                recipient: Some(recipient),
                ..
            },
        ..
    } = &mut leader.abilities[0].node.declarations[0]
    else {
        panic!("expected the tested IR shape")
    };
    recipient.condition = Expr::Compute(Box::new(Predicate::RecipientIsLeader {
        recipient: DamageRecipient::Bound {
            binding: recipient.binding,
        },
        players: players(PlayerRelation::SelfPlayer),
    }));
    WellFormedCardProgram::check(leader).unwrap_err();
    let mut installed=lowerer().lower(&equipment_played::tree(&json!({"activate":{"cost":{"pp":0},"do":[{"apply":{"on":"this","effect":{"prevent":{"kind":"ability"}},"until":"turn"}}]}}),"unit-follower")).unwrap().remove("unit-follower").unwrap().card().clone();
    let Node::Instruction { instruction } =
        &mut installed.abilities[0].node.body.as_mut().unwrap().steps[0].node
    else {
        panic!("expected the tested IR shape")
    };
    let Action::InstallEffect { effect, .. } = &mut instruction.action else {
        panic!("expected the tested IR shape")
    };
    let Effect::Replacement {
        pattern: ProposedEventPattern::Damage { source, .. },
        ..
    } = effect.as_mut()
    else {
        panic!("expected the tested IR shape")
    };
    let Effect::Replacement {
        pattern: ProposedEventPattern::Damage {
            source: canonical, ..
        },
        ..
    } = &original.abilities[0].node.declarations[0]
    else {
        panic!("expected the tested IR shape")
    };
    *source = canonical.clone();
    WellFormedCardProgram::check(installed).unwrap_err();
}

fn installed_object_prevention() -> Game {
    let tree = equipment_played::tree(
        &json!({"activate":{"cost":{"pp":0},"do":[{"apply":{"on":"this","effect":{"prevent":{"kind":"ability"}},"until":"turn"}}]}}),
        "unit-follower",
    );
    let program = lowerer()
        .lower(&tree)
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    let mut initial = setup(1, json!([{"id":"d0","card":"unit-follower"}]));
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
    let game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "object-prevention-observation",
    )
    .unwrap();
    let mut saved = serde_json::to_value(&game).unwrap();
    saved["state"]["objects"]["d0"]["generation"] = json!(u64::MAX);
    let mut game: Game = serde_json::from_value(saved).unwrap();
    game.load_internal_card("unit-follower", &program).unwrap();
    resolved(
        game.activate_internal_card(
            "s",
            program.card().abilities[0].node.id,
            PlayerId { seat: 1 },
        )
        .unwrap(),
    );
    game.resume_internal_card().unwrap();
    game
}

fn interrupted_object_prevention() -> WellFormedProgram {
    checked(vec![
        movement::put(
            1,
            Selector::Objects {
                objects: vec![ObjectOperand::SourceHolder {}],
            },
            OwnerZone::Hand,
        ),
        ordering::look(2, 1),
        ordering::order(3, ordering::looked(2), Position::Top {}),
        draw(4, 1),
    ])
}

#[test]
fn installed_object_prevention_observation_matches_the_real_damage_replacement() {
    let mut game = installed_object_prevention();
    let before = rule_observation::field(&game, Viewer::P1, 0);
    let effect = before
        .effects
        .iter()
        .find(|effect| matches!(effect.change, InstalledChange::Prevention { .. }))
        .unwrap();
    assert!(
        matches!(&effect.change,InstalledChange::Prevention { kind: DamageKind::Ability, change: PreventionChange::Prevent, remaining: None, recipients: Some(targets), sources: None } if matches!(targets.as_slice(),[EffectRecipient::Card(_)]))
    );
    let hit = checked(vec![instruction(
        1,
        Action::Damage {
            recipients: RecipientSet::Objects {
                objects: zone(ZoneKind::Field, 91),
            },
            amount: DamageAmount::Uniform {
                amount: Expr::Literal(Int(2)),
            },
            flags: DamageFlags::Ability,
            source: ObjectView::Current {
                object: Box::new(ObjectOperand::SourceHolder {}),
            },
            batch: BatchPolicy::Instruction {},
        },
    )]);
    let mut opposing = context(2);
    opposing.source = "f".into();
    assert!(
        game.execute_internal(&hit, &source("synthetic"), opposing)
            .unwrap()
            .complete
    );
    assert_eq!(
        rule_observation::field(&game, Viewer::P1, 0).stats,
        before.stats
    );
    let own = game
        .observe(Viewer::P1)
        .unwrap()
        .players
        .iter()
        .find(|player| player.player == Viewer::P1)
        .unwrap()
        .zones
        .iter()
        .find(|zone| zone.kind == ObservedZone::Field)
        .unwrap()
        .cards[0]
        .card
        .handle
        .clone();
    let interrupted = interrupted_object_prevention();
    assert!(
        !game
            .execute_internal(&interrupted, &source("synthetic"), context(1))
            .unwrap()
            .complete
    );
    let question = game.internal_decision(View::P1).unwrap();
    game.observe(Viewer::P2).unwrap();
    game.answer_internal(question.actor, question.token, Answer::Order(vec![0]))
        .unwrap_err();
    let restored = game.observe(Viewer::P2).unwrap();
    let opposing = restored
        .players
        .iter()
        .find(|player| player.player == Viewer::P1)
        .unwrap()
        .zones
        .iter()
        .find(|zone| zone.kind == ObservedZone::Field)
        .unwrap()
        .cards[0]
        .card
        .properties
        .state
        .as_ref()
        .unwrap();
    let effect = opposing
        .effects
        .iter()
        .find(|effect| matches!(effect.change, InstalledChange::Prevention { .. }))
        .unwrap();
    assert!(effect.source.as_ref().unwrap().holder.is_none());
    assert!(
        matches!(&effect.change,InstalledChange::Prevention { recipients: Some(links), .. } if links.is_empty()),
        "frozen scope links must not bypass the retired source identity"
    );
    assert!(rule_observation::field(&game,Viewer::P1,0).effects.iter().any(|effect| matches!(&effect.change,InstalledChange::Prevention { recipients: Some(links), .. } if links == &[EffectRecipient::Card(own.clone())])),"continuous owner knowledge keeps its authorized scope link");
}

#[test]
fn prevention_union_name_exclusion_also_keeps_extremum_populations_consistent() {
    let population =
        json!({"who":"self","zone":"field","kind":"follower","not_same_name_as_this":true});
    let mut query = population.clone();
    query["where"] = json!({"cmp":{"left":{"power_of":"item"},"op":"eq","right":{"agg":{"of":population,"attr":"power","fn":"max"}}}});
    let tree = equipment_played::tree(
        &json!({"activate":{"cost":{"pp":0},"do":[{"destroy":{"ref":query}}]}}),
        "unit-follower",
    );
    let program = lowerer()
        .lower(&tree)
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"source","card":"unit-follower","state":{"power":10}},
        {"id":"same-name","card":"beast-peer","state":{"power":8}},
        {"id":"high","card":"academy-peer","state":{"power":5}},
        {"id":"low","card":"ordinary-peer","state":{"power":2}}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([]);
    let mut game = Game::new(
        union_catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "excluded-extremum",
    )
    .unwrap();
    game.load_internal_card("unit-follower", &program).unwrap();
    for number in ["unit-spell", "academy-peer", "beast-peer", "ordinary-peer"] {
        game.load_internal_card(number, &card(vec![])).unwrap();
    }
    resolved(
        game.activate_internal_card(
            "source",
            program.card().abilities[0].node.id,
            PlayerId { seat: 1 },
        )
        .unwrap(),
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["cemetery"],
        json!(["high"]),
        "the highest candidate is measured after excluding all source names from both populations"
    );
    let mut inconsistent = tree;
    inconsistent["cards"]["unit-follower"]["abilities"][0]["activate"]["do"][0]["destroy"]["ref"]["where"]["cmp"]["right"]["agg"]["of"].as_object_mut().unwrap().remove("not_same_name_as_this");
    assert!(
        lowerer().lower(&inconsistent).is_err(),
        "the existing same-population check detects a different aggregate population"
    );
}
