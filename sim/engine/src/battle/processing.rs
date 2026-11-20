//! Native disclosure owns authorization; the wire boundary only resolves its frozen data.

use super::{BattleReferences, ProjectionError, objects::ObjectScope, sources};
use crate::catalog::Catalog;
use crate::game::player::{Viewer, observation as native};
use crate::ir::QuantityUnit;
use core::result;
use sve_battle_contract::{Int, Nat, Seat, Subject, observation as wire};

type Result<T> = result::Result<T, ProjectionError>;

pub(super) fn project(
    catalog: &Catalog,
    references: &BattleReferences,
    scope: &ObjectScope<'_>,
    context: &native::ProcessingContext,
) -> Result<wire::ProcessingContext> {
    Ok(wire::ProcessingContext {
        stage: context.stage.map(stage),
        actor: context.actor.map(seat),
        controller: Some(seat(context.controller)),
        source: context
            .source
            .as_ref()
            .map(|source| sources::source(catalog, references, scope, source))
            .transpose()?,
        subject: context
            .subject
            .as_ref()
            .map(|value| subject(catalog, references, scope, value))
            .transpose()?,
        rule: None,
        procedure: context
            .procedure
            .map(|procedure| -> Result<_> {
                Ok(wire::Procedure {
                    kind: match procedure.kind {
                        native::ProcedureKind::EachPlayer => wire::ProcedureKind::EachPlayer,
                        native::ProcedureKind::Repeat => wire::ProcedureKind::Repeat,
                    },
                    actor: seat(procedure.actor),
                    round: Some(Nat::try_from(procedure.round)?),
                })
            })
            .transpose()?,
        declared: context
            .declared
            .iter()
            .map(|value| declaration(catalog, references, scope, value))
            .collect::<Result<_>>()?,
        payments: context
            .payments
            .iter()
            .map(payment)
            .collect::<Result<_>>()?,
    })
}

fn subject(
    catalog: &Catalog,
    references: &BattleReferences,
    scope: &ObjectScope<'_>,
    value: &native::ProcessingSubject,
) -> Result<Subject> {
    Ok(match value {
        native::ProcessingSubject::Leader(player) => Subject::Leader {
            player: seat(*player),
        },
        native::ProcessingSubject::Card { card, object } => Subject::Card {
            card: references.resolve(catalog, card)?,
            object: scope.optional(object.as_ref())?,
        },
    })
}

fn declaration(
    catalog: &Catalog,
    references: &BattleReferences,
    scope: &ObjectScope<'_>,
    value: &native::ProcessingDeclaration,
) -> Result<wire::ProcessingDeclaration> {
    Ok(match value {
        native::ProcessingDeclaration::Modes { actor, modes } => {
            wire::ProcessingDeclaration::Modes {
                actor: seat(*actor),
                modes: modes
                    .iter()
                    .map(|mode| -> Result<_> {
                        Ok(wire::DeclaredMode {
                            mode: Nat::try_from(*mode)?,
                            text: None,
                        })
                    })
                    .collect::<Result<_>>()?,
            }
        }
        native::ProcessingDeclaration::X { actor, value } => wire::ProcessingDeclaration::Number {
            actor: seat(*actor),
            role: wire::DeclaredNumber::X,
            value: Int::try_from(*value)?,
        },
        native::ProcessingDeclaration::Number { actor, value } => {
            wire::ProcessingDeclaration::Number {
                actor: seat(*actor),
                role: wire::DeclaredNumber::Free,
                value: Int::try_from(*value)?,
            }
        }
        native::ProcessingDeclaration::Name { actor, rules_name } => {
            wire::ProcessingDeclaration::Name {
                actor: seat(*actor),
                rules_name: rules_name.clone(),
            }
        }
        native::ProcessingDeclaration::Targets { actor, targets } => {
            wire::ProcessingDeclaration::Targets {
                actor: seat(*actor),
                targets: targets
                    .iter()
                    .map(|target| subject(catalog, references, scope, target))
                    .collect::<Result<_>>()?,
            }
        }
        native::ProcessingDeclaration::OptionalCost {
            actor,
            required,
            accepted,
        } => wire::ProcessingDeclaration::OptionalCost {
            actor: seat(*actor),
            required: *required,
            accepted: *accepted,
        },
        native::ProcessingDeclaration::Distribution { actor, allocations } => {
            wire::ProcessingDeclaration::Distribution {
                actor: seat(*actor),
                allocations: allocations
                    .iter()
                    .map(|(target, amount)| -> Result<_> {
                        Ok(wire::DeclaredAllocation {
                            target: subject(catalog, references, scope, target)?,
                            amount: Nat::try_from(*amount)?,
                        })
                    })
                    .collect::<Result<_>>()?,
            }
        }
    })
}

fn payment(value: &native::ProcessingPayment) -> Result<wire::ProcessingPayment> {
    Ok(wire::ProcessingPayment {
        actor: seat(value.actor),
        kind: match &value.kind {
            native::PaymentKind::Pp => wire::PaymentKind::Pp,
            native::PaymentKind::Ep => wire::PaymentKind::Ep,
            native::PaymentKind::EntryResources => wire::PaymentKind::EntryResources,
            native::PaymentKind::Fusion => wire::PaymentKind::Fusion,
            native::PaymentKind::SkipTurn => wire::PaymentKind::SkipTurn,
            native::PaymentKind::Posture => wire::PaymentKind::Posture,
            native::PaymentKind::Life => wire::PaymentKind::Life,
            native::PaymentKind::Move => wire::PaymentKind::Move,
            native::PaymentKind::Discard => wire::PaymentKind::Discard,
            native::PaymentKind::Banish => wire::PaymentKind::Banish,
            native::PaymentKind::Reveal => wire::PaymentKind::Reveal,
            native::PaymentKind::FlipDown => wire::PaymentKind::FlipDown,
            native::PaymentKind::FlipUp => wire::PaymentKind::FlipUp,
            native::PaymentKind::Counter(name) => wire::PaymentKind::Counter { name: name.clone() },
        },
        purpose: value.purpose.map(|purpose| match purpose {
            native::PaymentPurpose::Original => wire::PaymentPurpose::Original,
            native::PaymentPurpose::Modes => wire::PaymentPurpose::Modes,
            native::PaymentPurpose::Additional => wire::PaymentPurpose::Additional,
            native::PaymentPurpose::Optional { required } => {
                wire::PaymentPurpose::Optional { required }
            }
            native::PaymentPurpose::Resolution => wire::PaymentPurpose::Resolution,
            native::PaymentPurpose::ResourceEntry => wire::PaymentPurpose::ResourceEntry,
            native::PaymentPurpose::Exchange => wire::PaymentPurpose::Exchange,
        }),
        amount: Int::try_from(value.amount)?,
        unit: match value.unit {
            QuantityUnit::Cards => wire::PaymentUnit::Cards,
            QuantityUnit::Damage => wire::PaymentUnit::Damage,
            QuantityUnit::Counters => wire::PaymentUnit::Counters,
            QuantityUnit::Life => wire::PaymentUnit::Life,
            QuantityUnit::Pp => wire::PaymentUnit::Pp,
            QuantityUnit::Ep => wire::PaymentUnit::Ep,
            QuantityUnit::Sep => wire::PaymentUnit::Sep,
            QuantityUnit::Times => wire::PaymentUnit::Times,
        },
    })
}

pub(super) const fn stage(value: native::ProcessingStage) -> wire::ProcessingStage {
    match value {
        native::ProcessingStage::Declare => wire::ProcessingStage::Declare,
        native::ProcessingStage::Target => wire::ProcessingStage::Target,
        native::ProcessingStage::Allocate => wire::ProcessingStage::Allocate,
        native::ProcessingStage::Pay => wire::ProcessingStage::Pay,
        native::ProcessingStage::Resolve => wire::ProcessingStage::Resolve,
    }
}

const fn seat(value: Viewer) -> Seat {
    match value {
        Viewer::P1 => Seat::P1,
        Viewer::P2 => Seat::P2,
    }
}
