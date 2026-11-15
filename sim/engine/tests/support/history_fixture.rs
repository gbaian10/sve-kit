//! Lib unit and integration cases share these exact synthetic IR constructors via `#[path]`.
//! Only APIs visible in both contexts are used, so relocating wire assertions cannot change their producer.

use super::board_fixtures;
use core::fmt::Display;
use serde_json::{Value, json};
use sve_engine::game::Game;
use sve_engine::game::internal::Context;
use sve_engine::ir::{
    Ability, AbilityId, AbilityKind, AbilityOrigin, AbilityQualification, AbilitySource, Action,
    ActionPattern, Binding, Bool, CardProgram, Cardinality, Control, CostSpec, CostStep,
    DiceOperation, Duration, DurationEnd, Effect, Expr, FaceId, Generation, Instruction,
    InstructionContext, InstructionId, Int, IterationCheck, Located, Nat, Node, ObjectOperand,
    PermissionSubject, PlaySpec, PlayerId, PlayerRef, PlayerRelation, PlayerSet, Polarity, Program,
    ReceiptId, RepeatBody, Resource, ResourceOperation, RuleId, ScopeId, SourceSpan, Termination,
    TextQuantifier, TimeAnchor, WellFormedCardProgram, ZoneKind,
};
use sve_engine::{EngineFailure, Result};
pub(crate) fn source(card: &str) -> SourceSpan {
    SourceSpan {
        card_id: card.into(),
        card_no: card.into(),
        scope: "fragment".into(),
        owner: "card".into(),
        face: "0".into(),
        field: "text".into(),
        section: None,
        line: 1,
        fragment: Some("D1".into()),
        author_path: "/body".into(),
    }
}

pub(crate) fn node(node: Node) -> Located<Node> {
    Located {
        node,
        source: source("synthetic"),
    }
}

pub(crate) const fn body(scope: u64, steps: Vec<Located<Node>>) -> Program {
    Program {
        scope: ScopeId(scope),
        steps,
    }
}

pub(crate) fn context(seat: u32) -> Context {
    Context {
        source: "s".into(),
        generation: Generation(0),
        controller: PlayerId { seat },
    }
}

pub(crate) fn instruction(id: u64, action: Action) -> Located<Node> {
    node(Node::Instruction {
        instruction: Box::new(Instruction {
            id: InstructionId(id),
            actor: PlayerRef::Controller {},
            source: AbilitySource {
                holder: ObjectOperand::SourceHolder {},
                origin: AbilityOrigin::Printed {
                    face: FaceId("0".into()),
                    line: Nat(1),
                },
                controller: PlayerRef::Controller {},
            },
            precondition: None,
            context: InstructionContext::Effect,
            receipt: ReceiptId(id),
            action,
        }),
    })
}

pub(crate) fn locate_steps(steps: &mut [Located<Node>], span: &SourceSpan, origin: &AbilitySource) {
    for step in steps {
        step.source = span.clone();
        match &mut step.node {
            Node::Instruction { instruction } => instruction.source = origin.clone(),
            Node::Control {
                control:
                    Control::Sequence { body }
                    | Control::EachObject { body, .. }
                    | Control::Repeat {
                        body: RepeatBody::Program { body },
                        ..
                    },
            } => locate_steps(&mut body.steps, span, origin),
            Node::Control {
                control:
                    Control::If {
                        then_body,
                        else_body,
                        ..
                    },
            } => {
                locate_steps(&mut then_body.steps, span, origin);
                if let Some(body) = else_body {
                    locate_steps(&mut body.steps, span, origin);
                }
            }
            Node::Control {
                control: Control::Mode { modes, .. },
            } => {
                for mode in modes {
                    locate_steps(&mut mode.body.steps, span, origin);
                }
            }
            Node::Control { .. } | Node::Capture { .. } | Node::Decision { .. } => {}
        }
    }
}

pub(crate) fn ability(id: u64, mut steps: Vec<Located<Node>>) -> Result<Located<Ability>> {
    let mut span = source("unit-follower");
    span.fragment = None;
    span.line = u32::try_from(id).map_err(error)?;
    let origin = AbilitySource {
        holder: ObjectOperand::SourceHolder {},
        origin: AbilityOrigin::Printed {
            face: FaceId("0".into()),
            line: Nat(id),
        },
        controller: PlayerRef::Controller {},
    };
    locate_steps(&mut steps, &span, &origin);
    Ok(Located {
        source: span,
        node: Ability {
            id: AbilityId(id),
            scope: ScopeId(id),
            source: origin,
            kind: AbilityKind::Activated,
            qualification: AbilityQualification::Ordinary,
            active_zones: vec![ZoneKind::Field],
            event_captures: vec![],
            play: Some(PlaySpec {
                limit: None,
                captures: vec![],
                parameters: vec![],
                costs: CostSpec::Ordered { steps: vec![] },
            }),
            trigger: None,
            body: Some(body(id, steps)),
            declarations: vec![],
        },
    })
}

pub(crate) fn card(abilities: Vec<Located<Ability>>) -> Result<WellFormedCardProgram> {
    WellFormedCardProgram::check(CardProgram {
        abilities,
        card_play_costs: vec![],
    })
    .map_err(error)
}

pub(crate) fn paid_ability(
    actions: Vec<Action>,
    body: Vec<Located<Node>>,
) -> Result<Located<Ability>> {
    let mut a = ability(1, body)?;
    a.node.play.as_mut().ok_or_else(missing)?.costs = CostSpec::Ordered {
        steps: actions
            .into_iter()
            .enumerate()
            .map(|(index, action)| {
                let id = u64::try_from(index)
                    .map_err(error)?
                    .checked_add(100)
                    .ok_or_else(missing)?;
                Ok(CostStep {
                    keyword: None,
                    instruction: Box::new(Instruction {
                        id: InstructionId(id),
                        actor: PlayerRef::Controller {},
                        source: a.node.source.clone(),
                        precondition: None,
                        context: InstructionContext::Cost,
                        receipt: ReceiptId(id),
                        action,
                    }),
                    materials: None,
                })
            })
            .collect::<Result<Vec<_>>>()?,
    };
    Ok(a)
}

pub(crate) fn spell(steps: Vec<Located<Node>>) -> Result<Located<Ability>> {
    let mut ability = paid_ability(
        vec![Action::Resource {
            player: PlayerRef::Controller {},
            resource: Resource::PpCurrent,
            operation: ResourceOperation::Pay,
            amount: Expr::Literal(Int(1)),
        }],
        steps,
    )?;
    ability.source = source("unit-spell");
    ability.source.fragment = None;
    ability.node.kind = AbilityKind::Spell;
    ability.node.active_zones = vec![ZoneKind::Resolution];
    locate_steps(
        &mut ability.node.body.as_mut().ok_or_else(missing)?.steps,
        &ability.source,
        &ability.node.source,
    );
    Ok(ability)
}

pub(crate) fn position_with_state(
    seat: u32,
    program: &WellFormedCardProgram,
    state: &Value,
) -> Result<Game> {
    let actor = if seat == 1 { "P1" } else { "P2" };
    let mut initial = board_fixtures::setup(seat, json!([{"id":"d","card":"unit-spell"}]));
    *initial.pointer_mut("/turn/active").ok_or_else(missing)? = json!(actor);
    *initial
        .pointer_mut(&format!("/players/{actor}/zones/hand"))
        .ok_or_else(missing)? = json!([]);
    initial
        .pointer_mut(&format!("/players/{actor}/zones"))
        .and_then(Value::as_object_mut)
        .ok_or_else(missing)?
        .insert(
            "field".into(),
            json!([{"id":"s","card":"unit-follower","state":state}]),
        );
    let mut game = Game::new(
        board_fixtures::catalog()?,
        &initial,
        &Value::Null,
        &Value::Null,
        "card-seed",
    )?;
    game.load_internal_card("unit-follower", program)?;
    game.load_internal_card("unit-spell", &card(vec![])?)?;
    Ok(game)
}

pub(crate) fn reroll() -> Result<Located<Ability>> {
    let mut entry = ability(2, vec![])?;
    entry.node.kind = AbilityKind::Static;
    entry.node.play = None;
    entry.node.body = None;
    entry.node.declarations = vec![Effect::Permission {
        subject: PermissionSubject::Players {
            players: PlayerSet {
                relative_to: PlayerRef::Controller {},
                relation: PlayerRelation::SelfPlayer,
                cardinality: Cardinality {
                    minimum: Nat(1),
                    maximum: Some(Nat(1)),
                },
                text_quantifier: TextQuantifier::Implicit,
            },
        },
        action: ActionPattern::Reroll {
            player: PlayerRef::Controller {},
            times: Nat(1),
        },
        polarity: Polarity::Allow,
        condition: Expr::Literal(Bool(true)),
        duration: Duration {
            end: DurationEnd::NoTemporalExpiry {},
            anchor: TimeAnchor::Installation {},
            usage: None,
            intervals: vec![],
        },
        rule: RuleId(1),
    }];
    Ok(entry)
}

pub(crate) fn roll(scope: u64) -> Located<Node> {
    instruction(
        10,
        Action::Dice {
            operation: DiceOperation::Roll {},
            result: Binding::new(20, ScopeId(scope)),
        },
    )
}

pub(crate) fn repeated() -> Result<Located<Ability>> {
    ability(
        1,
        vec![node(Node::Control {
            control: Control::Repeat {
                body: RepeatBody::Program {
                    body: body(10, vec![roll(10)]),
                },
                termination: Termination::Count {
                    count: Expr::Literal(Nat(2)),
                },
                check: IterationCheck::BeforeIteration,
                scope: ScopeId(9),
                receipts: vec![],
            },
        })],
    )
}

fn missing() -> EngineFailure {
    EngineFailure::Invalid("incomplete shared history fixture".into())
}
fn error(value: impl Display) -> EngineFailure {
    EngineFailure::Invalid(format!("shared history fixture: {value}"))
}

pub(crate) fn public_history_board(hidden: &str) -> Result<Game> {
    let mut initial = board_fixtures::setup(1, json!([]));
    for (player, zone, value) in [
        ("P1", "hand", json!([])),
        ("P1", "field", json!([{"id":"s","card":"unit-follower"}])),
        ("P2", "field", json!([])),
        ("P2", "hand", json!([{"id":"secret","card":hidden}])),
    ] {
        initial
            .pointer_mut(&format!("/players/{player}/zones"))
            .and_then(Value::as_object_mut)
            .ok_or_else(missing)?
            .insert(zone.into(), value);
    }
    let mut game = Game::new(
        board_fixtures::catalog()?,
        &initial,
        &Value::Null,
        &Value::Null,
        "rule-observation",
    )?;
    let empty = card(vec![])?;
    for number in ["unit-follower", "unit-spell", "unit-amulet"] {
        game.load_internal_card(number, &empty)?;
    }
    Ok(game)
}
