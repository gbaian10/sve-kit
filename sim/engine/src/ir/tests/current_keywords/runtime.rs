//! Real current selections exercise rules; external instruction producers are synthetic.
use super::{current_inputs, snapshot};
use crate::catalog::Catalog;
use crate::game::internal::{Activation, Answer, CardPlay, DecisionView, Fact, InputDomain};
use crate::game::{Game, View};
use crate::ir::*;
use serde_json::{Value, json};
use std::{fs::read_to_string, path::Path};

const NUMBER: &str = "BP01-138";

#[path = "runtime/unloaded.rs"]
mod unloaded;

#[path = "runtime/combat.rs"]
mod combat;

fn opponent(seat: &str) -> &str {
    if seat == "P1" { "P2" } else { "P1" }
}

fn actor(seat: &str) -> PlayerId {
    PlayerId {
        seat: if seat == "P1" { 1 } else { 2 },
    }
}

fn setup(seat: &str) -> Value {
    json!({"history":"explicit", "turn":{"active":seat,"first_player":"P1","phase":"main",
        "elapsed_turns":{"P1":3_u8,"P2":3_u8}},"players":{
        "P1":{"leader":{"class":"ニュートラル","life":20_u8},"pp":{"current":10_u8,"max":10_u8},"zones":{}},
        "P2":{"leader":{"class":"ニュートラル","life":20_u8},"pp":{"current":10_u8,"max":10_u8},"zones":{}}
    }})
}

fn field(player: PlayerRelation) -> Selector {
    let seats = if player == PlayerRelation::All { 2 } else { 1 };
    Selector::Zone {
        players: PlayerSet {
            relation: player,
            relative_to: PlayerRef::Controller {},
            text_quantifier: TextQuantifier::All,
            cardinality: Cardinality {
                minimum: Nat(seats),
                maximum: Some(Nat(seats)),
            },
        },
        zone: ZoneKind::Field,
        view: SelectionView::Current {},
        binding: Binding::new(50, ScopeId(1)),
        predicate: Box::new(Expr::Literal(Bool(true))),
    }
}

fn entry(zone: ZoneKind) -> Action {
    let mut objects = field(PlayerRelation::SelfPlayer);
    if let Selector::Zone { zone: from, .. } = &mut objects {
        *from = zone;
    }
    Action::Move {
        objects,
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
    }
}

fn producer(actions: Vec<Action>, targets: Option<PlayerRelation>) -> WellFormedCardProgram {
    let source = SourceSpan {
        card_id: "producer".into(),
        card_no: "producer".into(),
        owner: "card".into(),
        scope: "producer".into(),
        face: "0".into(),
        field: "text".into(),
        section: None,
        line: 1,
        fragment: None,
        author_path: "/producer".into(),
    };
    let origin = AbilitySource {
        holder: ObjectOperand::SourceHolder {},
        controller: PlayerRef::Controller {},
        origin: AbilityOrigin::Printed {
            face: FaceId("0".into()),
            line: Nat(1),
        },
    };
    let cardinality = Cardinality {
        minimum: Nat(1),
        maximum: Some(Nat(1)),
    };
    let parameters = targets
        .map(|relation| Decision {
            id: DecisionId(10),
            captures: vec![],
            actor: PlayerRef::Controller {},
            visibility: Visibility::Public {},
            kind: DecisionKind::PlayParameter {
                slot: PlaySlot::Targets,
                selection_snapshot: SelectionSnapshot::AtDecision,
                play_scope: ExecutionRef::Current {},
                input: InputMode::Inline,
            },
            domain: ChoiceDomain::Objects {
                binding: Binding::new(10, ScopeId(1)),
                target_roles: None,
                constraint: SelectionConstraint {
                    groups: vec![SelectionGroup {
                        selector: field(relation),
                        cardinality,
                    }],
                    cardinality,
                    distinct_by: None,
                    disjoint: Disjointness::Disjoint,
                    aggregate_bounds: vec![],
                },
            },
        })
        .into_iter()
        .collect();
    WellFormedCardProgram::check(CardProgram {
        card_play_costs: vec![],
        abilities: vec![Located {
            source: source.clone(),
            node: Ability {
                id: AbilityId(1),
                scope: ScopeId(1),
                source: origin.clone(),
                kind: AbilityKind::Activated,
                qualification: AbilityQualification::Ordinary,
                active_zones: vec![ZoneKind::Field],
                event_captures: vec![],
                declarations: vec![],
                trigger: None,
                play: Some(PlaySpec {
                    limit: None,
                    captures: vec![],
                    parameters,
                    costs: CostSpec::Ordered { steps: vec![] },
                }),
                body: Some(Program {
                    scope: ScopeId(1),
                    steps: actions
                        .into_iter()
                        .zip(1_u64..)
                        .map(|(action, id)| Located {
                            source: source.clone(),
                            node: Node::Instruction {
                                instruction: Box::new(Instruction {
                                    id: InstructionId(id),
                                    receipt: ReceiptId(id),
                                    actor: PlayerRef::Controller {},
                                    source: origin.clone(),
                                    precondition: None,
                                    context: InstructionContext::Effect,
                                    action,
                                }),
                            },
                        })
                        .collect(),
                }),
            },
        }],
    })
    .unwrap()
}

fn game(initial: &Value, number: &str, program: &WellFormedCardProgram) -> Game {
    let mut selection = if number == NUMBER {
        current_inputs()["keywords"].clone()
    } else {
        current_inputs()["keyword_fanfare"][number].clone()
    };
    if number != NUMBER {
        selection["authors"]
            .as_array_mut()
            .unwrap()
            .push(current_inputs()["keywords"]["authors"][0].clone());
    }
    selected_game(initial, selection, &snapshot(), program)
}

fn selected_game(
    initial: &Value,
    mut selection: Value,
    snapshot: &str,
    program: &WellFormedCardProgram,
) -> Game {
    let empty = json!({"status":"complete","abilities":[]});
    selection["documents"] = json!([[
        "synthetic.yaml",
        json!({"format":1_u8,"kind":"effect_set","cards":{
            "producer":empty,"plain":empty,"amulet":empty
        }})
        .to_string()
    ]]);
    let extra = [("producer","フォロワー"),("plain","フォロワー"),("amulet","アミュレット")]
        .map(|(number,kind)|json!({"number":number,"faces":[{"name":number,"card_class":"ニュートラル","card_type":kind,
            "cost":"1","power":if kind == "アミュレット" {"-"} else {"2"},"hp":if kind == "アミュレット" {"-"} else {"3"},
            "text":"","sections":[],"traits":[]}]}).to_string()).join("\n");
    let snapshot = format!("{}\n{extra}", snapshot.trim_end());
    let registry = read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../authored/rules/keywords.yaml"),
    )
    .unwrap();
    let selected = Catalog::from_current_selection(&snapshot, &registry, &selection).unwrap();
    let mut game = Game::from_selected_effects(
        selected,
        initial,
        &Value::Null,
        &Value::Null,
        "keyword-rules",
    )
    .unwrap();
    game.load_internal_card("producer", program).unwrap();
    for number in ["plain", "amulet"] {
        game.load_internal_card(
            number,
            &WellFormedCardProgram::check(CardProgram {
                abilities: vec![],
                card_play_costs: vec![],
            })
            .unwrap(),
        )
        .unwrap();
    }
    game
}

fn synthetic_reverse() -> (Value, String) {
    // BP01-138 itself is not a double-faced card.
    synthetic_reverse_for(NUMBER, current_inputs()["keywords"].clone())
}

fn synthetic_reverse_for(number: &str, mut selection: Value) -> (Value, String) {
    let input = &mut selection["authors"][0]["input"];
    let mut back = input["source_payload"]["faces"][0].clone();
    back["face_id"] = json!(format!("f:{}", "e".repeat(32)));
    back["revision_id"] = json!("synthetic-keyword-free-back");
    back["texts"][0]["source_hash"] = json!(text_hash("Synthetic keyword-free reverse"));
    back["texts"][1]["source_hash"] = json!(text_hash(""));
    input["source_payload"]["faces"]
        .as_array_mut()
        .unwrap()
        .push(back);
    let mut records = snapshot()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    let record = records
        .iter_mut()
        .find(|record| record["number"] == number)
        .unwrap();
    let mut reverse = record["faces"][0].clone();
    reverse["name"] = json!("Synthetic keyword-free reverse");
    reverse["text"] = json!("");
    record["faces"].as_array_mut().unwrap().push(reverse);
    let snapshot = records
        .iter()
        .map(Value::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    (selection, snapshot)
}

fn text_hash(text: &str) -> String {
    use sha2::{Digest as _, Sha256};
    const HEX: &[u8] = b"0123456789abcdef";
    Sha256::digest(text.as_bytes())
        .iter()
        .flat_map(|byte| {
            [
                char::from(HEX[usize::from(byte >> 4_u8)]),
                char::from(HEX[usize::from(byte & 15)]),
            ]
        })
        .collect()
}

#[test]
fn effective_face_and_field_zone_control_both_keyword_rules() {
    // CR 2.15 / 10.3.5: only the effective face's definitions apply on Field.
    let (selection, snapshot) = synthetic_reverse();
    for seat in ["P1", "P2"] {
        let other = opponent(seat);
        for face in [0_u8, 1_u8] {
            let mut initial = setup(seat);
            initial["players"][seat]["zones"]["field"] =
                json!([{"id":"producer","card":"producer"}]);
            initial["players"][other]["zones"]["field"] = json!([{"id":"subject","card":NUMBER,"owner":seat,"state":{"face":face,"acted":true}}]);
            let destroy = Action::Destroy {
                objects: Selector::Bound {
                    reference: Binding::new(10, ScopeId(1)),
                },
            };
            let mut game = selected_game(
                &initial,
                selection.clone(),
                &snapshot,
                &producer(vec![destroy], Some(PlayerRelation::Opponents)),
            );
            let before = views(&game);
            let attempted = game.activate_internal_card("producer", AbilityId(1), actor(seat));
            if face == 0 {
                attempted.unwrap_err();
                assert_eq!(views(&game), before, "the printed front has Aura");
            } else {
                let q = question(attempted.unwrap());
                let InputDomain::Targets(domain) = &q.domain else {
                    panic!("target")
                };
                assert_eq!(
                    domain.members.len(),
                    1,
                    "front Aura must not leak onto the reverse"
                );
            }
            let mut attack_game = selected_game(
                &initial,
                selection.clone(),
                &snapshot,
                &producer(vec![], None),
            );
            let attacked = attack_game.internal_rule_action(&json!({"do":"attack","attacker":"producer","target":format!("{other}.leader")})).unwrap();
            assert_eq!(
                attacked.outcome == "cannot-attack",
                face == 0,
                "front Ward must not leak onto the reverse"
            );
        }
        for zone in ["hand", "ex", "cemetery"] {
            let mut initial = setup(seat);
            initial["players"][seat]["zones"]["field"] =
                json!([{"id":"producer","card":"producer"}]);
            initial["players"][other]["zones"][zone] = json!([{"id":"subject","card":NUMBER}]);
            let mut game = selected_game(
                &initial,
                selection.clone(),
                &snapshot,
                &producer(vec![], None),
            );
            let attacked = game.internal_rule_action(&json!({"do":"attack","attacker":"producer","target":format!("{other}.leader")})).unwrap();
            assert_eq!(
                attacked.outcome, "resolved",
                "off-field keywords impose no attack restriction"
            );
            if zone != "hand" {
                assert_eq!(state(&game, "subject")["keywords"], json!([]));
            }
        }
    }
}

fn views(game: &Game) -> [Value; 3] {
    [View::P1, View::P2, View::Referee].map(|view| game.projection(view).unwrap())
}

fn state(game: &Game, id: &str) -> Value {
    game.projection(View::Referee).unwrap()["objects"][id].clone()
}

fn question(play: CardPlay) -> DecisionView {
    if let CardPlay::Awaiting(view) = play {
        view
    } else {
        panic!("expected question")
    }
}

fn resolved(play: CardPlay) -> Activation {
    if let CardPlay::Resolved(result) = play {
        result
    } else {
        panic!("expected resolution")
    }
}

#[test]
fn ward_restricts_attacks_by_controller_and_allows_either_acted_ward() {
    // CR 8.4.3 and 12.8.2; FAQ Q29. A standing or silenced Ward imposes no restriction.
    for seat in ["P1", "P2"] {
        let other = opponent(seat);
        for (acted, silenced) in [(true, false), (false, false), (true, true)] {
            for target in ["leader", "plain", "ward1", "ward2"] {
                let mut initial = setup(seat);
                initial["players"][seat]["zones"]["field"] =
                    json!([{"id":"attacker","card":"plain"}]);
                initial["players"][other]["zones"]["field"] = json!([
                    {"id":"ward1","card":NUMBER,"owner":seat,"state":{"acted":acted,"silenced":silenced}},
                    {"id":"ward2","card":NUMBER,"owner":seat,"state":{"acted":acted,"silenced":silenced}},
                    {"id":"plain","card":"plain","state":{"acted":true}}
                ]);
                let mut game = game(&initial, NUMBER, &producer(vec![], None));
                let target = if target == "leader" {
                    format!("{other}.leader")
                } else {
                    target.into()
                };
                let before = views(&game);
                let step = game
                    .internal_rule_action(
                        &json!({"do":"attack","attacker":"attacker","target":target}),
                    )
                    .unwrap();
                let allowed = if target.starts_with("ward") {
                    acted
                } else {
                    !acted || silenced
                };
                assert_eq!(
                    step.outcome,
                    if allowed { "resolved" } else { "cannot-attack" },
                    "attack target must obey the current Ward set"
                );
                if !allowed {
                    assert_eq!(views(&game), before, "rejected attacks are atomic");
                }
            }
        }
    }
}

#[test]
fn ward_end_phase_choice_is_optional_and_precedes_quick() {
    // CR 7.4.3 / 12.8.2; FAQ Q257 and Q260 fix the trigger -> Ward -> Quick order.
    for seat in ["P1", "P2"] {
        for selected in [json!([]), json!(["ward"])] {
            let mut initial = setup(seat);
            initial["players"][seat]["zones"]["field"] =
                json!([{"id":"ward","card":NUMBER,"owner":opponent(seat)}]);
            let mut game = game(&initial, NUMBER, &producer(vec![], None));
            game.internal_rule_action(&json!({"do":"end-phase"}))
                .unwrap();
            assert!(
                game.internal_quick_choice().unwrap().is_none(),
                "Ward precedes Quick"
            );
            let before = views(&game);
            assert_eq!(
                game.internal_rule_action(&json!({"do":"guard-act","select":["ward","ward"]}))
                    .unwrap()
                    .outcome,
                "cannot-play"
            );
            assert_eq!(views(&game), before);
            let step = game
                .internal_rule_action(&json!({"do":"guard-act","select":selected}))
                .unwrap();
            assert_eq!(step.outcome, "resolved");
            assert_eq!(
                state(&game, "ward")["acted"],
                !selected.as_array().unwrap().is_empty()
            );
            let q = game.internal_quick_choice().unwrap().unwrap();
            assert_eq!(q.actor, actor(opponent(seat)));
            assert!(matches!(
                q.domain,
                InputDomain::Quick {
                    window: QuickWindow::EndPhase,
                    ..
                }
            ));
        }
    }
}

#[test]
fn ordinary_ward_play_offers_stand_then_optional_act_with_bound_answers() {
    // CR 12.8.2: enter standing, optionally act. This is not an unconditional entry posture.
    for seat in ["P1", "P2"] {
        for zone in ["hand", "ex"] {
            for acted in [false, true] {
                let mut initial = setup(seat);
                initial["players"][seat]["zones"][zone] = json!([{"id":"incoming","card":NUMBER}]);
                let mut game = game(&initial, NUMBER, &producer(vec![], None));
                let q = question(game.play_internal_card("incoming", actor(seat)).unwrap());
                assert!(matches!(&q.domain,InputDomain::WardEntry {card,..} if card == NUMBER));
                let before = views(&game);
                game.answer_internal_card(actor(opponent(seat)), q.token, Answer::WardEntry(acted))
                    .unwrap_err();
                game.answer_internal_card(q.actor, q.token, Answer::OptionalCost(acted))
                    .unwrap_err();
                assert_eq!(
                    views(&game),
                    before,
                    "a foreign actor or another decision kind cannot answer Ward"
                );
                let played = game
                    .answer_internal_card(q.actor, q.token, Answer::WardEntry(acted))
                    .unwrap();
                assert!(matches!(played, CardPlay::Entered(_)));
                assert_eq!(state(&game, "incoming")["acted"], acted);
                assert_eq!(state(&game, "incoming")["generation"], 2_u8);
                assert_eq!(
                    state(&game, "incoming")["keywords"],
                    json!(["オーラ", "守護"])
                );
                game.answer_internal_card(q.actor, q.token, Answer::WardEntry(!acted))
                    .unwrap_err();
                game.resume_internal_card().unwrap();
            }
        }
    }
}

#[test]
fn aura_filters_only_opponents_play_targets_and_keeps_own_targets() {
    // CR 12.15.1-12.15.2.1 and FAQ Q36: use the ability controller, never its owner.
    for seat in ["P1", "P2"] {
        let other = opponent(seat);
        let mut initial = setup(seat);
        initial["players"][seat]["zones"]["field"] = json!([
            {"id":"producer","card":"producer","owner":other},
            {"id":"own-aura","card":NUMBER,"owner":other}
        ]);
        initial["players"][other]["zones"]["field"] = json!([
            {"id":"opposing-aura","card":NUMBER,"owner":seat},
            {"id":"plain","card":"plain","owner":seat}
        ]);
        let body = Action::Destroy {
            objects: Selector::Bound {
                reference: Binding::new(10, ScopeId(1)),
            },
        };
        let program = producer(vec![body.clone()], Some(PlayerRelation::All));
        let mut game = game(&initial, NUMBER, &program);
        let q = question(
            game.activate_internal_card("producer", AbilityId(1), actor(seat))
                .unwrap(),
        );
        let InputDomain::Targets(domain) = &q.domain else {
            panic!("target question")
        };
        assert_eq!(domain.members.len(), 3);
        assert_eq!(
            domain
                .members
                .iter()
                .filter(|m| m.card.as_deref() == Some(NUMBER))
                .count(),
            1
        );
        let own = domain
            .members
            .iter()
            .find(|m| m.card.as_deref() == Some(NUMBER))
            .unwrap();
        assert_eq!(own.player, actor(seat));
        resolved(
            game.answer_internal_card(q.actor, q.token, Answer::Targets(vec![own.handle]))
                .unwrap(),
        );
        assert_eq!(state(&game, "own-aura")["generation"], 1_u8);
        assert_eq!(state(&game, "opposing-aura")["generation"], 0_u8);

        initial["players"][other]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .pop();
        let mut blocked_game = self::game(
            &initial,
            NUMBER,
            &producer(vec![body], Some(PlayerRelation::Opponents)),
        );
        let before = views(&blocked_game);
        blocked_game
            .activate_internal_card("producer", AbilityId(1), actor(seat))
            .unwrap_err();
        assert_eq!(
            views(&blocked_game),
            before,
            "a mandatory target cannot be paid or played when only opposing Aura exists"
        );
    }
}

#[test]
fn real_advance_entry_ward_and_fanfare_destroy_aura_without_targeting() {
    // CR 9.2.2 / 12.4 / 12.8.2 / 12.15; FAQ Q1363 and Q1263 distinguish entry and non-targeted effects.
    for number in ["BP19-P32", "BP19-114"] {
        for seat in ["P1", "P2"] {
            for acted in [false, true] {
                let other = opponent(seat);
                let mut initial = setup(seat);
                initial["players"][seat]["zones"]["field"] =
                    json!([{"id":"producer","card":"producer"}]);
                initial["players"][seat]["zones"]["evolve_deck"] =
                    json!([{"id":"incoming","card":number}]);
                initial["players"][other]["zones"]["field"] = json!([
                    {"id":"opposing-aura","card":NUMBER,"owner":seat},
                    {"id":"plain","card":"plain"},{"id":"amulet","card":"amulet"}
                ]);
                let mut game = game(
                    &initial,
                    number,
                    &producer(vec![entry(ZoneKind::Evolve)], None),
                );
                let q = question(
                    game.activate_internal_card("producer", AbilityId(1), actor(seat))
                        .unwrap(),
                );
                assert!(matches!(&q.domain,InputDomain::WardEntry {card,..} if card == number));
                let entered = resolved(
                    game.answer_internal_card(q.actor, q.token, Answer::WardEntry(acted))
                        .unwrap(),
                );
                assert_eq!(state(&game, "incoming")["acted"], acted);
                assert!(
                    entered
                        .progress
                        .facts
                        .iter()
                        .any(|fact| matches!(fact, Fact::Movement(_)))
                );
                game.resume_internal_card().unwrap();
                let fanfare_question = game.internal_pending_choice().unwrap().unwrap();
                let InputDomain::Pending(options) = &fanfare_question.domain else {
                    panic!("fanfare")
                };
                assert_eq!(options.len(), 1);
                let fanfare = resolved(
                    game.answer_internal_card(
                        fanfare_question.actor,
                        fanfare_question.token,
                        Answer::Pending(options[0].handle),
                    )
                    .unwrap(),
                );
                assert_eq!(fanfare.source.holder.generation, Generation(1));
                assert_eq!(fanfare.source.controller, actor(seat));
                let view = game.projection(View::Referee).unwrap();
                assert_eq!(
                    view[seat]["cemetery"],
                    json!(["opposing-aura"]),
                    "a non-targeted effect destroys opposing Aura to its owner"
                );
                assert_eq!(view[other]["cemetery"], json!(["plain"]));
                assert_eq!(view[other]["field"], json!(["amulet"]));
                assert_eq!(view[seat]["field"], json!(["producer", "incoming"]));
            }
        }
    }
}

#[test]
fn ability_loss_and_old_zone_information_do_not_survive_a_new_field_generation() {
    // CR 4.1.4: zone changes create a new object; 12.8/12.15 are only active on Field.
    // Ability suppression is an initial condition; this slice does not admit a suppression action.
    for seat in ["P1", "P2"] {
        let mut initial = setup(seat);
        initial["players"][seat]["zones"]["field"] = json!([{"id":"producer","card":"producer"}]);
        initial["players"][seat]["zones"]["cemetery"] = json!([
            {"id":"incoming","card":NUMBER,"state":{"silenced":true}}
        ]);
        let mut game = game(
            &initial,
            NUMBER,
            &producer(vec![entry(ZoneKind::Cemetery)], None),
        );
        assert_eq!(state(&game, "incoming")["keywords"], json!([]));
        let q = question(
            game.activate_internal_card("producer", AbilityId(1), actor(seat))
                .unwrap(),
        );
        let entered = resolved(
            game.answer_internal_card(q.actor, q.token, Answer::WardEntry(true))
                .unwrap(),
        );
        assert_eq!(state(&game, "incoming")["generation"], 1_u8);
        assert_eq!(
            state(&game, "incoming")["keywords"],
            json!(["オーラ", "守護"])
        );
        assert!(
            entered.progress.facts.iter().any(
                |f| matches!(f,Fact::Posture(event) if event.object.generation==Generation(1))
            )
        );
    }
}

fn entry_observer() -> WellFormedCardProgram {
    let mut card = producer(vec![entry(ZoneKind::Evolve)], None).card().clone();
    let mut observer = card.abilities[0].clone();
    observer.source.line = 2;
    observer.source.scope = "entry-observer".into();
    observer.node.id = AbilityId(2);
    observer.node.scope = ScopeId(2);
    observer.node.source.origin = AbilityOrigin::Printed {
        face: FaceId("0".into()),
        line: Nat(2),
    };
    observer.node.kind = AbilityKind::Triggered;
    observer.node.play = None;
    let event = Binding::new(70, ScopeId(2));
    observer.node.trigger = Some(TriggerSpec {
        active_zones: vec![ZoneKind::Field],
        view: EventSnapshotPolicy::AfterEntry,
        limit: None,
        aggregation: TriggerAggregation::PerFact {},
        on: TriggerCondition::Event {
            pattern: EventPattern::EnterField {
                binding: event,
                predicate: Expr::Compute(Box::new(Predicate::And {
                    predicates: vec![
                        Expr::Compute(Box::new(Predicate::PlayerEquals {
                            left: Expr::Frozen(Box::new(PlayerFrozen::EnterFieldController {
                                event,
                            })),
                            right: Expr::Literal(PlayerRef::Controller {}),
                        })),
                        Expr::Compute(Box::new(Predicate::KindEquals {
                            left: Expr::Frozen(Box::new(KindFrozen::Characters {
                                characters: Expr::Frozen(Box::new(
                                    CharacteristicsFields::EnterFieldCharacters { event },
                                )),
                            })),
                            right: Expr::Literal(CardKind::Follower),
                        })),
                    ],
                })),
            },
        },
    });
    let mut own_field = field(PlayerRelation::SelfPlayer);
    if let Selector::Zone { binding, .. } = &mut own_field {
        *binding = Binding::new(50, observer.node.scope);
    }
    observer.node.body = Some(Program {
        scope: ScopeId(2),
        steps: vec![Located {
            source: observer.source.clone(),
            node: Node::Instruction {
                instruction: Box::new(Instruction {
                    id: InstructionId(2),
                    receipt: ReceiptId(2),
                    actor: PlayerRef::Controller {},
                    source: observer.node.source.clone(),
                    context: InstructionContext::Effect,
                    precondition: None,
                    action: Action::Destroy { objects: own_field },
                }),
            },
        }],
    });
    card.abilities.push(observer);
    WellFormedCardProgram::check(card).unwrap()
}

#[test]
fn queued_fanfare_keeps_its_event_source_after_the_keyword_holder_leaves() {
    // CR 10.7.7 / 10.6.2.8.2.1: a queued ability is played and resolves after its source leaves.
    for seat in ["P1", "P2"] {
        let other = opponent(seat);
        let mut initial = setup(seat);
        initial["players"][seat]["zones"]["field"] = json!([{"id":"producer","card":"producer"}]);
        initial["players"][seat]["zones"]["evolve_deck"] =
            json!([{"id":"incoming","card":"BP19-P32"}]);
        initial["players"][other]["zones"]["field"] = json!([{"id":"opposing-aura","card":NUMBER}]);
        let mut game = game(&initial, "BP19-P32", &entry_observer());
        let q = question(
            game.activate_internal_card("producer", AbilityId(1), actor(seat))
                .unwrap(),
        );
        resolved(
            game.answer_internal_card(q.actor, q.token, Answer::WardEntry(true))
                .unwrap(),
        );
        game.resume_internal_card().unwrap();
        let observer_question = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(entry_options) = &observer_question.domain else {
            panic!("pending entry abilities")
        };
        let observer = entry_options
            .iter()
            .find(|option| option.card == "producer")
            .unwrap();
        resolved(
            game.answer_internal_card(
                observer_question.actor,
                observer_question.token,
                Answer::Pending(observer.handle),
            )
            .unwrap(),
        );
        assert_eq!(state(&game, "incoming")["generation"], 3_u8);
        assert_eq!(
            state(&game, "incoming")["keywords"],
            json!([]),
            "the returned Advance has no active Field keywords"
        );
        game.resume_internal_card().unwrap();
        let fanfare_question = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(fanfare_options) = &fanfare_question.domain else {
            panic!("queued fanfare")
        };
        assert_eq!(fanfare_options.len(), 1);
        let result = resolved(
            game.answer_internal_card(
                fanfare_question.actor,
                fanfare_question.token,
                Answer::Pending(fanfare_options[0].handle),
            )
            .unwrap(),
        );
        assert_eq!(
            result.source.holder.generation,
            Generation(1),
            "the event source keeps its old generation"
        );
        assert_eq!(result.source.controller, actor(seat));
        assert_eq!(
            game.projection(View::Referee).unwrap()[other]["cemetery"],
            json!(["opposing-aura"])
        );
    }
}

#[test]
fn aura_advance_is_an_own_target_and_never_an_opposing_one() {
    // CR 12.15 / FAQ Q36: Advance followers are followers; Aura only hides them from opponents.
    for number in ["BP19-P32", "BP19-114"] {
        for seat in ["P1", "P2"] {
            let other = opponent(seat);
            let mut initial = setup(seat);
            initial["players"][seat]["zones"]["field"] = json!([
                {"id":"producer","card":"producer","owner":other},
                {"id":"own-advance","card":number,"owner":other}
            ]);
            initial["players"][other]["zones"]["field"] = json!([
                {"id":"opposing-advance","card":number,"owner":seat},
                {"id":"plain","card":"plain","owner":seat}
            ]);
            let body = Action::Destroy {
                objects: Selector::Bound {
                    reference: Binding::new(10, ScopeId(1)),
                },
            };
            let mut game = game(
                &initial,
                number,
                &producer(vec![body], Some(PlayerRelation::All)),
            );
            let q = question(
                game.activate_internal_card("producer", AbilityId(1), actor(seat))
                    .unwrap(),
            );
            let InputDomain::Targets(domain) = &q.domain else {
                panic!("target question")
            };
            assert_eq!(domain.members.len(), 3, "own Advance, plain, own producer");
            let own = domain
                .members
                .iter()
                .find(|m| m.card.as_deref() == Some(number))
                .unwrap();
            assert_eq!(own.player, actor(seat));
            resolved(
                game.answer_internal_card(q.actor, q.token, Answer::Targets(vec![own.handle]))
                    .unwrap(),
            );
            assert_eq!(state(&game, "own-advance")["generation"], 2_u8);
            assert_eq!(state(&game, "opposing-advance")["generation"], 0_u8);
        }
    }
}

#[test]
fn simultaneous_identical_wards_have_distinct_entry_choices_before_the_batch_moves() {
    // CR 12.8.2 / 4.1.5: each arriving Ward has its own optional act in the same entry batch.
    for seat in ["P1", "P2"] {
        let mut initial = setup(seat);
        initial["players"][seat]["zones"]["field"] = json!([{"id":"producer","card":"producer"}]);
        initial["players"][seat]["zones"]["ex"] = json!([
            {"id":"first","card":NUMBER},{"id":"second","card":NUMBER}
        ]);
        let mut game = game(&initial, NUMBER, &producer(vec![entry(ZoneKind::Ex)], None));
        let first = question(
            game.activate_internal_card("producer", AbilityId(1), actor(seat))
                .unwrap(),
        );
        assert!(matches!(&first.domain, InputDomain::WardEntry {zone,position:1,..} if zone=="ex"));
        let second = question(
            game.answer_internal_card(first.actor, first.token, Answer::WardEntry(true))
                .unwrap(),
        );
        assert!(
            matches!(&second.domain, InputDomain::WardEntry {zone,position:2,..} if zone=="ex")
        );
        assert_eq!(
            game.projection(View::Referee).unwrap()[seat]["field"],
            json!(["producer"]),
            "the whole batch waits for every posture choice"
        );
        let before = views(&game);
        game.answer_internal_card(first.actor, first.token, Answer::WardEntry(false))
            .unwrap_err();
        assert_eq!(
            views(&game),
            before,
            "the earlier entry token cannot answer the next copy"
        );
        resolved(
            game.answer_internal_card(second.actor, second.token, Answer::WardEntry(false))
                .unwrap(),
        );
        assert_eq!(state(&game, "first")["acted"], true);
        assert_eq!(state(&game, "second")["acted"], false);
        assert_eq!(state(&game, "first")["generation"], 1_u8);
        assert_eq!(state(&game, "second")["generation"], 1_u8);
    }
}
