use super::super::{
    CostSpec, CostStep, MoveDestination, MoveReason, MoveSpec, NumericChange, OwnerZone, PlaySpec,
    Posture, Resource, ResourceOperation,
};
use super::{
    AbilitySource, Action, BodyContext, Expr, Instruction, InstructionContext, InstructionId, Int,
    LoweringError, PlayerRef, ReceiptId, ast, numeric, unsupported,
};
use core::slice::from_ref;

pub(super) fn lower(
    context: &mut BodyContext<'_>,
    cost: &ast::Cost,
    source: &AbilitySource,
    path: &str,
) -> Result<PlaySpec, LoweringError> {
    let operations = match cost {
        ast::Cost::Short(operation) => from_ref(operation.as_ref()),
        ast::Cost::Ordered { steps } => steps.as_slice(),
    };
    lower_operations(context, operations, source, path, false)
}

pub(super) fn lower_one(
    context: &mut BodyContext<'_>,
    operation: &ast::CostOperation,
    source: &AbilitySource,
    path: &str,
) -> Result<PlaySpec, LoweringError> {
    lower_operations(context, from_ref(operation), source, path, false)
}

pub(super) fn lower_additional(
    context: &mut BodyContext<'_>,
    cost: &ast::Cost,
    source: &AbilitySource,
    path: &str,
) -> Result<PlaySpec, LoweringError> {
    let operations = match cost {
        ast::Cost::Short(operation) => from_ref(operation.as_ref()),
        ast::Cost::Ordered { steps } => steps.as_slice(),
    };
    lower_operations(context, operations, source, path, true)
}

#[expect(
    clippy::too_many_lines,
    reason = "Ordered payment and material declarations share one cost boundary."
)]
fn lower_operations(
    context: &mut BodyContext<'_>,
    operations: &[ast::CostOperation],
    source: &AbilitySource,
    path: &str,
    multiple_materials: bool,
) -> Result<PlaySpec, LoweringError> {
    if operations
        .iter()
        .filter(|op| {
            matches!(
                op,
                ast::CostOperation::Discard(ast::DiscardCost::Material(_))
            )
        })
        .count()
        > 1
    {
        return Err(unsupported(path));
    }
    let mut steps = Vec::new();
    let mut parameters = Vec::new();
    for operation in operations {
        let keyword = matches!(operation, ast::CostOperation::EarthRite(_))
            .then_some(super::KeywordId::EarthRite);
        let action = match operation {
            ast::CostOperation::EarthRite(amount) => {
                let (action, decision) = super::earth::cost(context, *amount, path)?;
                parameters.extend(decision);
                action
            }
            ast::CostOperation::Counter(value) => {
                let (action, decision) = super::counters::cost(context, value, path)?;
                parameters.extend(decision);
                action
            }
            ast::CostOperation::Lesson(amount) => {
                let material = ast::Material {
                    n: Some(*amount),
                    any: false,
                    result: None,
                    sel: ast::PublicQuery {
                        other: false,
                        who: ast::Side::Controller,
                        zone: ast::PublicZone::Ex,
                        kind: None,
                        keyword: None,
                        r#trait: None,
                        name: Some("魔法のアイテム".to_owned()),
                        names: Vec::new(),
                        name_contains: None,
                        cost_eq: None,
                        cost_le: None,
                        cost_ge: None,
                        orig_cost_eq: None,
                        orig_cost_le: None,
                        orig_cost_ge: None,
                        hp_le: None,
                        class: None,
                        not_class: None,
                        token: None,
                        racing: None,
                        r#where: None,
                    },
                };
                let (action, decision) = super::material::lower(
                    context.card_allocator,
                    &mut context.allocator,
                    context.scope,
                    &material,
                    super::material::Operation::Banish,
                    path,
                )?;
                parameters.push(decision);
                action
            }
            ast::CostOperation::ToCemetery(value)
            | ast::CostOperation::ReturnToHand(value)
            | ast::CostOperation::Banish(value)
            | ast::CostOperation::Act(value)
            | ast::CostOperation::ToEx(value) => {
                let operation = material_operation(operation, path)?;
                let (action, decision) = super::material::lower(
                    context.card_allocator,
                    &mut context.allocator,
                    context.scope,
                    value,
                    operation,
                    path,
                )?;
                parameters.push(decision);
                action
            }
            ast::CostOperation::Discard(ast::DiscardCost::Reference { r#ref: name })
                if name == "this" =>
            {
                Action::Discard {
                    player: PlayerRef::Controller {},
                    mode: super::super::DiscardMode::All {
                        eligible: super::hand_payment::source(context.card_allocator, path)?,
                    },
                }
            }
            ast::CostOperation::BanishSelf(true) => Action::Banish {
                objects: numeric::holder(),
            },
            ast::CostOperation::Discard(ast::DiscardCost::Material(material)) => {
                super::hand_payment::lower(context.card_allocator, material, path)?
            }
            ast::CostOperation::Pp(amount) => Action::Resource {
                player: PlayerRef::Controller {},
                resource: Resource::PpCurrent,
                operation: ResourceOperation::Pay,
                amount: Expr::Literal(Int(
                    i64::try_from(*amount).map_err(|_private| unsupported(path))?
                )),
            },
            ast::CostOperation::LeaderLife(amount) => Action::Life {
                player: PlayerRef::Controller {},
                change: NumericChange::Add {
                    amount: Expr::Literal(Int(i64::try_from(*amount)
                        .map_err(|_private| unsupported(path))?
                        .checked_neg()
                        .ok_or_else(|| unsupported(path))?)),
                },
            },
            ast::CostOperation::ActSelf(true) => Action::Posture {
                objects: numeric::holder(),
                posture: Posture::Acted,
            },
            ast::CostOperation::SelfToCemetery(true) => Action::Move {
                objects: numeric::holder(),
                movement: MoveSpec {
                    to: MoveDestination::Owners {
                        zone: OwnerZone::Cemetery,
                    },
                    position: None,
                    order: None,
                    initial_posture: None,
                    entry_effects: vec![],
                },
                reason: MoveReason::Put,
            },
            ast::CostOperation::Discard(ast::DiscardCost::Reference { .. })
            | ast::CostOperation::BanishSelf(false)
            | ast::CostOperation::ActSelf(false)
            | ast::CostOperation::SelfToCemetery(false) => {
                return Err(unsupported(path));
            }
        };
        let id = context.card_allocator.id(path)?;
        if let ast::CostOperation::Discard(ast::DiscardCost::Material(material)) = operation
            && let Some(name) = &material.result
        {
            record_discard(
                context,
                name,
                ReceiptId(id),
                super::hand_payment::cardinality(material, path)?
                    .maximum
                    .is_some_and(|maximum| maximum.0 <= 1),
                path,
            )?;
        }
        if let ast::CostOperation::ToCemetery(material) = operation {
            context.record_paid_material(material, ReceiptId(id), path)?;
        }
        steps.push(CostStep {
            keyword,
            materials: None,
            instruction: Box::new(Instruction {
                id: InstructionId(id),
                receipt: ReceiptId(id),
                actor: PlayerRef::Controller {},
                source: source.clone(),
                precondition: None,
                context: InstructionContext::Cost,
                action,
            }),
        });
    }
    if !multiple_materials && parameters.len() > 1 {
        return Err(unsupported(path));
    }
    Ok(PlaySpec {
        limit: None,
        captures: vec![],
        parameters,
        costs: CostSpec::Ordered { steps },
    })
}

fn material_operation(
    operation: &ast::CostOperation,
    path: &str,
) -> Result<super::material::Operation, LoweringError> {
    match operation {
        ast::CostOperation::ToCemetery(_) => Ok(super::material::Operation::Put),
        ast::CostOperation::ReturnToHand(_) => Ok(super::material::Operation::Hand),
        ast::CostOperation::Banish(_) => Ok(super::material::Operation::Banish),
        ast::CostOperation::Act(_) => Ok(super::material::Operation::Act),
        ast::CostOperation::ToEx(_) => Ok(super::material::Operation::Ex),
        ast::CostOperation::Counter(_)
        | ast::CostOperation::EarthRite(_)
        | ast::CostOperation::Pp(_)
        | ast::CostOperation::Lesson(_)
        | ast::CostOperation::LeaderLife(_)
        | ast::CostOperation::BanishSelf(_)
        | ast::CostOperation::Discard(_)
        | ast::CostOperation::ActSelf(_)
        | ast::CostOperation::SelfToCemetery(_) => Err(unsupported(path)),
    }
}

pub(super) fn record_discard(
    context: &mut BodyContext<'_>,
    name: &str,
    receipt: ReceiptId,
    single: bool,
    path: &str,
) -> Result<(), LoweringError> {
    if name.is_empty()
        || context.observations.contains_key(name)
        || context.targets.contains_key(name)
        || context.recipient_targets.contains_key(name)
        || context.definitions.contains_key(name)
        || context.searches.contains(name)
        || context.ex_outputs.contains_key(name)
        || context.receipt_reads.paid_materials.contains_key(name)
    {
        return Err(unsupported(path));
    }
    if single {
        context.receipt_reads.single_discards.insert(receipt);
    }
    context.observations.insert(
        name.to_owned(),
        super::super::ObjectReceipt::Movement {
            receipt: super::super::MovementReceipt::Discard {
                receipt: super::super::ReceiptReference::new(receipt),
            },
        },
    );
    Ok(())
}
