//! The meal entry expands resource payment and race facts into separate instructions.
use super::{Allocator, CardCoordinates, LoweringError, ast, source_span, unsupported};
use crate::ir::{
    Ability, AbilityId, AbilityKind, AbilityQualification, Action, CostSpec, CostStep, Expr,
    Instruction, InstructionContext, InstructionId, Int, KeywordId, Located, MealRaceOperation,
    Nat, Node, ObjectOperand, PlaySpec, PlayerRef, Program, ReceiptId, Resource, ResourceOperation,
    ScopeId, ZoneKind,
};

pub(super) const fn action(count: u64, operation: MealRaceOperation) -> Action {
    Action::MealRace {
        subject: ObjectOperand::SourceHolder {},
        operation,
        amount: Expr::Literal(Nat(count)),
        resource_links: vec![],
    }
}

pub(super) fn lower(
    allocator: &mut Allocator,
    key: &CardCoordinates<'_>,
    path: &str,
    entry: &ast::Ability,
    scope: ScopeId,
) -> Result<Located<Ability>, LoweringError> {
    let head = entry.feed.as_ref().ok_or_else(|| unsupported(path))?;
    let pp = i64::try_from(head.cost.pp).map_err(|_private| unsupported(path))?;
    let count = head.times.unwrap_or(1);
    let source = source_span(key, path, entry)?;
    let origin = super::ability_source(&source, Some(KeywordId::Meal));
    let mut make_instruction = |action, context| {
        let id = allocator.id(path)?;
        Ok::<_, LoweringError>(Instruction {
            id: InstructionId(id),
            receipt: ReceiptId(id),
            actor: PlayerRef::Controller {},
            source: origin.clone(),
            precondition: None,
            context,
            action,
        })
    };
    let meal = make_instruction(
        action(count, MealRaceOperation::Meal),
        InstructionContext::Cost,
    )?;
    let payment = make_instruction(
        Action::Resource {
            player: PlayerRef::Controller {},
            resource: Resource::PpCurrent,
            operation: ResourceOperation::Pay,
            amount: Expr::Literal(Int(pp)),
        },
        InstructionContext::Cost,
    )?;
    let race = make_instruction(
        action(count, MealRaceOperation::Race),
        InstructionContext::Effect,
    )?;
    Ok(Located {
        source: source.clone(),
        node: Ability {
            id: AbilityId(scope.0),
            scope,
            source: origin,
            kind: AbilityKind::Activated,
            qualification: AbilityQualification::Ordinary,
            active_zones: vec![ZoneKind::Field],
            event_captures: vec![],
            declarations: vec![],
            trigger: None,
            play: Some(PlaySpec {
                limit: None,
                captures: vec![],
                parameters: vec![],
                costs: CostSpec::Ordered {
                    steps: vec![meal, payment]
                        .into_iter()
                        .map(|instruction| CostStep {
                            instruction: Box::new(instruction),
                            materials: None,
                        })
                        .collect(),
                },
            }),
            body: Some(Program {
                scope,
                steps: vec![Located {
                    source,
                    node: Node::Instruction {
                        instruction: Box::new(race),
                    },
                }],
            }),
        },
    })
}
