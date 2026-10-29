#![expect(
    clippy::indexing_slicing,
    reason = "Synthetic fixture coordinates must fail the test when their shape changes."
)]

use super::*;
use sve_engine::ir::lowering::ExpandedLowerer;
use sve_engine::ir::{
    AbilityOrigin, AbilitySource, Action, ActionPattern, Binding, Bool, CardProgram, Cardinality,
    CausePattern, Control, CostSpec, Duration, DurationEnd, Effect, EffectSource, Expr, FaceId,
    Generation, InstallationSubjects, InstallationTime, InstructionId, MemberProjection, Nat,
    NatOperation, Node, PermissionSubject, PlaySpec, PlayerOrder, PlayerRead, PlayerRef,
    PlayerRelation, PlayerSet, Polarity, Predicate, Program, ReceiptId, Role, RuleId, ScopeId,
    SelectionView, Selector, SourceSpan, TextQuantifier, TimeAnchor, TokenDestination,
    WellFormedCardProgram, WellFormedProgram, WellFormedReason, ZoneKind,
};

fn steps() -> Value {
    json!([
        {"destroy":{"ref":{"who":"all","zone":"field","kind":"follower"},"result":"broken"}},
        {"each_player":{"as":"p","stages":[[
            {"create":{"token":"token","to":"field","by":"p","n":{"count":{
                "in":{"objects_of":"broken","view":"before"},"as":"victim",
                "where":{"cmp":{"left":{"player":{"master_of":"victim"}},"op":"eq","right":{"player":"p"}}}
            }}}}
        ]]}}
    ])
}

fn selected(steps: Value, ward: bool) -> (String, String, Value) {
    let (snapshot, _, mut input) = selection(1);
    input["authors"][0]["input"]["tree"]["cards"]["creator"]["faces"]["f:11111111111111111111111111111111"]
        ["abilities"][0]["spell"]["do"] = steps;
    let (token, author) = author(
        "token",
        "22222222222222222222222222222222",
        "フォロワー・トークン",
        if ward {
            vec![json!({"keywords":["ward"]})]
        } else {
            vec![]
        },
    );
    let snapshot = format!("{}\n{token}\n", snapshot.lines().next().unwrap());
    input["authors"][1] = author;
    (
        snapshot,
        include_str!("../../../../authored/rules/keywords.yaml").into(),
        input,
    )
}

fn position() -> Value {
    let mut position = initial(0);
    position["players"]["P1"]["zones"]["field"] =
        json!([{"id":"mine","card":"token","owner":"P2"}]);
    position["players"]["P2"]["zones"]["field"] = json!([
        {"id":"theirs-1","card":"token","owner":"P1"},
        {"id":"theirs-2","card":"token","owner":"P1"}
    ]);
    position
}

fn game(steps: Value, ward: bool, position: &Value) -> Game {
    let (snapshot, keywords, input) = selected(steps, ward);
    let selected = Catalog::from_current_selection(&snapshot, &keywords, &input).unwrap();
    Game::from_selected_effects(
        selected,
        position,
        &Value::Null,
        &Value::Null,
        "dynamic-creation",
    )
    .unwrap()
}

#[test]
fn destroyed_counts_use_departure_controller_and_include_erased_tokens() {
    let mut game = game(steps(), false, &position());
    let CardPlay::Resolved(result) = game
        .play_internal_card("creator", PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("no-choice destruction and creation");
    };
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["field"].as_array().unwrap().len(), 1);
    assert_eq!(view["P2"]["field"].as_array().unwrap().len(), 2);
    for id in ["mine", "theirs-1", "theirs-2"] {
        assert!(
            view["objects"].get(id).is_none(),
            "the destroyed token was erased"
        );
    }
    assert!(result.progress.complete);
    assert_eq!(view["P1"]["cemetery"], json!(["creator"]));
    assert_eq!(view["P2"]["cemetery"], json!([]));
}

#[test]
fn empty_destroy_result_and_per_player_ex_capacity_are_preserved() {
    for occupied in [0_usize, 4, 5] {
        let mut steps = steps();
        steps[1]["each_player"]["stages"][0][0]["create"]["to"] = json!("ex");
        let mut position = position();
        position["players"]["P2"]["zones"]["field"] = json!([]);
        position["players"]["P1"]["zones"]["ex"] =
            initial(occupied)["players"]["P1"]["zones"]["ex"].clone();
        let mut game = game(steps, false, &position);
        assert!(matches!(
            game.play_internal_card("creator", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Resolved(_)
        ));
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(
            view["P1"]["ex"].as_array().unwrap().len(),
            (occupied + 1).min(5)
        );
        assert_eq!(view["P2"]["ex"], json!([]));
    }
}

#[test]
fn each_recipient_answers_its_own_ward_entry_and_resume_does_not_recreate() {
    let mut game = game(steps(), true, &position());
    let mut play = game
        .play_internal_card("creator", PlayerId { seat: 1 })
        .unwrap();
    for seat in [1, 2, 2] {
        let CardPlay::Awaiting(question) = play else {
            panic!("Ward entry for each arriving token");
        };
        assert_eq!(question.actor, PlayerId { seat });
        assert!(matches!(question.domain, InputDomain::WardEntry { .. }));
        let pending = game.projection(View::Referee).unwrap();
        assert_eq!(
            pending["P1"]["field"],
            json!([]),
            "all arrivals wait for both players' entry choices"
        );
        assert_eq!(pending["P2"]["field"], json!([]));
        let before = game.projection(View::Referee).unwrap();
        game.answer_internal_card(
            PlayerId { seat: 3 - seat },
            question.token,
            Answer::WardEntry(true),
        )
        .unwrap_err();
        assert_eq!(game.projection(View::Referee).unwrap(), before);
        play = game
            .answer_internal_card(question.actor, question.token, Answer::WardEntry(seat == 2))
            .unwrap();
    }
    assert!(matches!(play, CardPlay::Resolved(_)));
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["field"].as_array().unwrap().len(), 1);
    let opponents = view["P2"]["field"].as_array().unwrap();
    assert_eq!(opponents.len(), 2);
    assert!(
        opponents
            .iter()
            .all(|id| view["objects"][id.as_str().unwrap()]["acted"] == true)
    );
}

#[test]
fn dynamic_creation_rejects_unproved_receipts_views_and_player_scope() {
    let base = steps();
    let count_path = "/1/each_player/stages/0/0/create/n/count";
    let mut variants = Vec::new();
    let mut after = base.clone();
    after
        .pointer_mut(&format!("{count_path}/in/view"))
        .unwrap()
        .clone_from(&json!("after"));
    variants.push(after);
    let mut forward = base.clone();
    forward
        .pointer_mut(&format!("{count_path}/in/objects_of"))
        .unwrap()
        .clone_from(&json!("future"));
    variants.push(forward);
    let mut owner = base.clone();
    owner
        .pointer_mut(&format!("{count_path}/where/cmp/left/player"))
        .unwrap()
        .clone_from(&json!({"owner_of":"victim"}));
    variants.push(owner);
    let mut wrong = base.clone();
    wrong[1]["each_player"]["stages"][0][0]["create"]["by"] = json!("foreign");
    variants.push(wrong);
    let mut extra = base.clone();
    extra[1]["each_player"]["stages"][0]
        .as_array_mut()
        .unwrap()
        .push(json!({"draw":{"n":1_u8}}));
    variants.push(extra);
    let mut conditional = base.clone();
    conditional[0] = json!({"if":{"cond":{"awakened":true},"then":[base[0].clone()]}});
    variants.push(conditional);
    for steps in variants {
        let (snapshot, keywords, input) = selected(steps, false);
        Catalog::from_current_selection(&snapshot, &keywords, &input).unwrap_err();
    }
}

fn raw_body() -> Program {
    let (_, _, input) = selected(steps(), false);
    let lowered = ExpandedLowerer::new()
        .unwrap()
        .lower_current(&input["authors"][0]["input"])
        .unwrap();
    lowered.program.card().abilities[0]
        .node
        .body
        .as_ref()
        .unwrap()
        .clone()
}

fn raw_game() -> Game {
    let (snapshot, keywords, mut input) = selected(steps(), false);
    input["authors"].as_array_mut().unwrap().remove(0);
    let selected = Catalog::from_current_selection(&snapshot, &keywords, &input).unwrap();
    Game::from_selected_effects(
        selected,
        &position(),
        &Value::Null,
        &Value::Null,
        "raw-token-projection",
    )
    .unwrap()
}

fn protected_game(body: Program) -> Game {
    fn locate(program: &mut Program, span: &SourceSpan, origin: &AbilitySource) {
        for step in &mut program.steps {
            step.source = span.clone();
            match &mut step.node {
                Node::Instruction { instruction } => instruction.source = origin.clone(),
                Node::Control {
                    control: Control::EachPlayer { stages, .. },
                } => {
                    for stage in stages {
                        locate(&mut stage.body, span, origin);
                    }
                }
                Node::Control { .. } | Node::Capture { .. } | Node::Decision { .. } => {}
            }
        }
    }
    let (snapshot, keywords, input) = selected(steps(), false);
    let document = json!({"format":1_u8,"kind":"effect_set","cards":{
        "creator":{"status":"complete","abilities":[]},
        "token":{"status":"complete","token_template":true,"abilities":[]}
    }})
    .to_string();
    let catalog = Arc::new(
        Catalog::from_documents(&snapshot, &keywords, &[("synthetic.yaml".into(), document)])
            .unwrap(),
    );
    let mut game = Game::new(
        catalog,
        &position(),
        &Value::Null,
        &Value::Null,
        "protected-token",
    )
    .unwrap();
    game.load_internal_card(
        "token",
        &WellFormedCardProgram::check(CardProgram {
            abilities: vec![],
            card_play_costs: vec![],
        })
        .unwrap(),
    )
    .unwrap();
    let lowered = ExpandedLowerer::new()
        .unwrap()
        .lower_current(&input["authors"][0]["input"])
        .unwrap();
    let mut card = lowered.program.card().clone();
    let ability = &mut card.abilities[0];
    ability.source.card_no = "creator".into();
    ability.source.card_id = "creator".into();
    ability.source.face = "0".into();
    ability.source.owner = "card".into();
    ability.source.field = "text".into();
    ability.node.source.origin = AbilityOrigin::Printed {
        face: FaceId("0".into()),
        line: Nat(1),
    };
    if let Some(PlaySpec {
        costs: CostSpec::Ordered { steps },
        ..
    }) = &mut ability.node.play
    {
        for step in steps {
            step.instruction.source = ability.node.source.clone();
        }
    }
    ability.node.body = Some(body);
    locate(
        ability.node.body.as_mut().unwrap(),
        &ability.source,
        &ability.node.source,
    );
    let checked = WellFormedCardProgram::check(card).unwrap();
    game.load_internal_card("creator", &checked).unwrap();
    game
}

#[test]
fn failed_destruction_does_not_create_a_replacement_for_the_protected_controller() {
    let mut body = raw_body();
    let affected = Binding::new(1, ScopeId(900));
    let objects = Selector::Bound {
        reference: affected,
    };
    let duration = Duration {
        end: DurationEnd::EndResolution {},
        anchor: TimeAnchor::Installation {},
        usage: None,
        intervals: vec![],
    };
    let mut protection = body.steps[0].clone();
    let Node::Instruction { instruction } = &mut protection.node else {
        panic!("destroy instruction");
    };
    instruction.id = InstructionId(900);
    instruction.receipt = ReceiptId(900);
    instruction.action = Action::InstallEffect {
        subjects: InstallationSubjects::Objects {
            objects: Selector::Zone {
                players: PlayerSet {
                    relation: PlayerRelation::SelfPlayer,
                    relative_to: PlayerRef::Controller {},
                    cardinality: Cardinality {
                        minimum: Nat(0),
                        maximum: None,
                    },
                    text_quantifier: TextQuantifier::Implicit,
                },
                zone: ZoneKind::Field,
                view: SelectionView::Current {},
                binding: Binding::new(1, ScopeId(901)),
                predicate: Box::new(Expr::Literal(Bool(true))),
            },
            affected,
        },
        capture: InstallationTime::InstructionStart,
        effect: Box::new(Effect::Permission {
            subject: PermissionSubject::Objects {
                objects: objects.clone(),
            },
            action: ActionPattern::Destroy {
                objects,
                cause: CausePattern::Effect {
                    source: EffectSource::Any {},
                },
            },
            polarity: Polarity::Forbid,
            condition: Expr::Literal(Bool(true)),
            duration: duration.clone(),
            rule: RuleId(1),
        }),
        duration,
    };
    body.steps.insert(0, protection);
    let mut game = protected_game(body);
    let CardPlay::Resolved(result) = game
        .play_internal_card("creator", PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("protected destruction and creation finish");
    };
    let successful = result
        .progress
        .receipts
        .values()
        .find_map(|outcome| {
            if let InstructionOutcome::Destroy(outcome) = outcome {
                Some(outcome.successful.len())
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!(successful, 2);
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(
        view["P1"]["field"],
        json!(["mine"]),
        "the surviving follower is not counted as destroyed"
    );
    assert_eq!(view["P2"]["field"].as_array().unwrap().len(), 2);
}

#[test]
fn raw_player_stages_reject_unimplemented_order_and_mismatched_destination_atomically() {
    use sve_engine::game::internal::Context;
    for wrong_destination in [false, true] {
        let mut body = raw_body();
        let Node::Control {
            control:
                Control::EachPlayer {
                    stages,
                    choice_order,
                    ..
                },
        } = &mut body.steps[1].node
        else {
            panic!("player stage");
        };
        if wrong_destination {
            let Node::Instruction { instruction } = &mut stages[0].body.steps[0].node else {
                panic!("token instruction");
            };
            let Action::Token {
                destination:
                    TokenDestination::Single {
                        zone: Expr::Literal(zone),
                    },
                ..
            } = &mut instruction.action
            else {
                panic!("single player destination");
            };
            zone.player = PlayerRef::Controller {};
        } else {
            *choice_order = PlayerOrder::FirstPlayerFirst;
        }
        let source = body.steps[0].source.clone();
        let checked = WellFormedProgram::check(body).unwrap();
        let mut game = raw_game();
        let before = format!("{game:?}");
        game.execute_internal(
            &checked,
            &source,
            Context {
                source: "creator".into(),
                generation: Generation(0),
                controller: PlayerId { seat: 1 },
            },
        )
        .unwrap_err();
        assert_eq!(format!("{game:?}"), before);
    }
}

#[test]
fn historical_controller_reads_require_a_successful_destroy_before_candidate() {
    for variant in 0_u8..3 {
        let mut body = raw_body();
        let Node::Control {
            control: Control::EachPlayer { stages, .. },
        } = &mut body.steps[1].node
        else {
            panic!("player stage");
        };
        let Node::Instruction { instruction } = &mut stages[0].body.steps[0].node else {
            panic!("token instruction");
        };
        let Action::Token { tokens, .. } = &mut instruction.action else {
            panic!("token action");
        };
        let Expr::Compute(operation) = &mut tokens[0].count else {
            panic!("count projection");
        };
        let NatOperation::Count {
            objects: Selector::Filter {
                input, predicate, ..
            },
        } = operation.as_mut()
        else {
            panic!("count filter");
        };
        match variant {
            0 => **input = Selector::Objects { objects: vec![] },
            1 => {
                let Selector::ReceiptMembers { projection, .. } = input.as_mut() else {
                    panic!("receipt members");
                };
                *projection = MemberProjection::After;
            }
            _ => {
                let Expr::Compute(predicate) = predicate.as_mut() else {
                    panic!("player comparison");
                };
                let Predicate::PlayerEquals {
                    left: Expr::Live(read),
                    ..
                } = predicate.as_mut()
                else {
                    panic!("player role");
                };
                let PlayerRead::Role { role, .. } = read.as_mut() else {
                    panic!("role read");
                };
                *role = Role::Owner;
            }
        }
        let error = WellFormedProgram::check(body).unwrap_err();
        assert_eq!(error.reason, WellFormedReason::UnsupportedShape);
    }
}

#[test]
fn both_players_new_observers_see_the_same_simultaneous_entry_batch() {
    let (snapshot, keywords, mut input) = selected(steps(), false);
    let (_, token) = author(
        "token",
        "22222222222222222222222222222222",
        "フォロワー・トークン",
        vec![json!({
            "auto":{"on":{"enter_field":{"who":"opp","sel":{"kind":"follower"}}},"do":[{"life":{"who":"self","delta":1_u8}}]}
        })],
    );
    input["authors"][1] = token;
    let selected = Catalog::from_current_selection(&snapshot, &keywords, &input).unwrap();
    let mut position = position();
    position["players"]["P1"]["leader"]["life"] = json!(10_u8);
    position["players"]["P2"]["leader"]["life"] = json!(10_u8);
    let mut game = Game::from_selected_effects(
        selected,
        &position,
        &Value::Null,
        &Value::Null,
        "simultaneous-token-observers",
    )
    .unwrap();
    assert!(matches!(
        game.play_internal_card("creator", PlayerId { seat: 1 })
            .unwrap(),
        CardPlay::Resolved(_)
    ));
    game.resume_internal_card().unwrap();
    let mut triggers = [0_u8; 2];
    while let Some(question) = game.internal_pending_choice().unwrap() {
        let InputDomain::Pending(options) = &question.domain else {
            panic!("new token's entry observer");
        };
        assert!(options.iter().all(|option| option.card == "token"));
        let slot = usize::try_from(question.actor.seat - 1).unwrap();
        triggers[slot] += 1;
        assert!(matches!(
            game.answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(options[0].handle)
            )
            .unwrap(),
            CardPlay::Resolved(_)
        ));
        game.resume_internal_card().unwrap();
    }
    assert_eq!(
        triggers,
        [2_u8, 2_u8],
        "both new sides are active when the complete batch enters"
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["leader"]["life"], 12_u8);
    assert_eq!(state["P2"]["leader"]["life"], 12_u8);
}
