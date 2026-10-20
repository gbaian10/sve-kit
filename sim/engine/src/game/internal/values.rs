use super::{
    Arithmetic, Binding, Bool, Capture, CaptureRef, Comparison, DamageRecipient, Execution, Expr,
    Game, InstructionOutcome, Int, IntFrozen, IntOperation, IntRead, MeasuredReceipt, Nat,
    NatFrozen, NatOperation, NatRead, ObjectFrozen, ObjectOperand, ObjectRef, PlayerId, PlayerRef,
    PlayerRelation, PlayerResource, PlayerSet, Predicate, ReceiptLocation, RecipientSet,
    ResolvedRecipients, Result, RuntimeDamageRecipient, Selector, Slot, ZoneKind, contains, int,
    invalid, player_id, seat_name, unsupported,
};
use alloc::collections::BTreeSet;
use core::convert::identity;

impl Execution {
    pub(super) fn player(&self, game: &Game, player: &PlayerRef) -> Result<PlayerId> {
        match player {
            PlayerRef::Controller {} => Ok(self.context.controller),
            PlayerRef::Owner {} => player_id(&game.object(&self.context.source)?.owner),
            PlayerRef::Active {} => player_id(game.active()),
            PlayerRef::RequireOne { players } => {
                let values = self.players(game, players)?;
                if values.len() == 1 {
                    values
                        .first()
                        .copied()
                        .ok_or_else(|| invalid("missing player"))
                } else {
                    Err(invalid("player set is not a singleton"))
                }
            }
            _ => Err(unsupported("player expression")),
        }
    }

    fn players(&self, game: &Game, players: &PlayerSet) -> Result<Vec<PlayerId>> {
        let relative = self.player(game, &players.relative_to)?;
        let values = match players.relation {
            PlayerRelation::SelfPlayer => vec![relative],
            PlayerRelation::Opponents => vec![PlayerId {
                seat: if relative.seat == 1 { 2 } else { 1 },
            }],
            PlayerRelation::All => vec![PlayerId { seat: 1 }, PlayerId { seat: 2 }],
            PlayerRelation::Bound => return Err(unsupported("bound player sets")),
        };
        if !contains(
            players.cardinality,
            Nat(u64::try_from(values.len()).map_err(invalid)?),
        ) {
            return Err(invalid("player set cardinality does not match this game"));
        }
        Ok(values)
    }

    pub(super) fn object(&self, game: &Game, object: &ObjectOperand) -> Result<ObjectRef> {
        match object {
            ObjectOperand::SourceHolder {} => {
                let mut reference = self.reference(game, &self.context.source)?;
                reference.generation = self.context.generation;
                Ok(reference)
            }
            ObjectOperand::FromExpression { value } => match value.as_ref() {
                Expr::Frozen(frozen)
                    if matches!(frozen.as_ref(), ObjectFrozen::SourceHolder {}) =>
                {
                    self.object(game, &ObjectOperand::SourceHolder {})
                }
                _ => Err(unsupported("object expression")),
            },
            _ => Err(unsupported("object operand")),
        }
    }

    pub(super) fn objects(&self, game: &Game, selector: &Selector) -> Result<Vec<ObjectRef>> {
        let objects = match selector {
            Selector::Objects { objects } => objects
                .iter()
                .map(|object| self.object(game, object))
                .collect::<Result<Vec<_>>>()?,
            Selector::Zone {
                players,
                zone,
                predicate,
                ..
            } => {
                let mut objects = Vec::new();
                // Admitted predicates cannot read the candidate binding, so one evaluation filters all.
                if self.boolean(game, predicate)?.0 {
                    for player in self.players(game, players)? {
                        for id in game.zone_ids(seat_name(player)?, zone_name(*zone)?) {
                            objects.push(self.reference(game, &id)?);
                        }
                    }
                }
                objects
            }
            Selector::Union { sets } => sets
                .iter()
                .map(|set| self.objects(game, set))
                .collect::<Result<Vec<_>>>()?
                .into_iter()
                .flatten()
                .collect(),
            Selector::Captured { reference } => self.object_slot(reference.binding)?,
            _ => return Err(unsupported("selector")),
        };
        let mut seen = BTreeSet::new();
        Ok(objects
            .into_iter()
            .filter(|object| seen.insert((object.id, object.generation)))
            .collect())
    }

    fn object_slot(&self, binding: Binding<Selector>) -> Result<Vec<ObjectRef>> {
        if let Some(Slot::Objects(objects)) = self.slots.get(&binding.key()) {
            Ok(objects.clone())
        } else {
            Err(invalid("missing typed object binding"))
        }
    }

    pub(super) fn recipients(
        &self,
        game: &Game,
        recipients: &RecipientSet,
    ) -> Result<ResolvedRecipients> {
        let values = match recipients {
            RecipientSet::Objects { objects } => self
                .objects(game, objects)?
                .into_iter()
                .map(|object| RuntimeDamageRecipient::Object { object })
                .collect(),
            RecipientSet::Leaders { players } => self
                .players(game, players)?
                .into_iter()
                .map(|player| RuntimeDamageRecipient::Leader { player })
                .collect(),
            RecipientSet::Union { sets } => sets
                .iter()
                .map(|set| self.recipients(game, set))
                .collect::<Result<Vec<_>>>()?
                .into_iter()
                .flat_map(|set| set.members().to_vec())
                .collect(),
            RecipientSet::Explicit { recipients } => recipients
                .iter()
                .map(|recipient| match recipient {
                    DamageRecipient::Object { object } => Ok(RuntimeDamageRecipient::Object {
                        object: self.object(game, object)?,
                    }),
                    DamageRecipient::Leader { player } => Ok(RuntimeDamageRecipient::Leader {
                        player: self.player(game, player)?,
                    }),
                    _ => Err(unsupported("damage recipient")),
                })
                .collect::<Result<Vec<_>>>()?,
            RecipientSet::Bound { .. } => return Err(unsupported("recipient binding")),
        };
        Ok(ResolvedRecipients::new(values))
    }

    pub(super) fn capture(&mut self, game: &Game, capture: &Capture) -> Result<()> {
        let (key, value) = match capture {
            Capture::Integer { value, into, .. } => (
                into.binding.key(),
                Slot::Integer(self.integer(game, value)?),
            ),
            Capture::Natural { value, into, .. } => (
                into.binding.key(),
                Slot::Natural(self.natural(game, value)?),
            ),
            Capture::Boolean { value, into, .. } => (
                into.binding.key(),
                Slot::Boolean(self.boolean(game, value)?),
            ),
            Capture::Objects { value, into, .. } => (
                into.binding.key(),
                Slot::Objects(self.objects(game, value)?),
            ),
            _ => return Err(unsupported("capture domain")),
        };
        self.slots.insert(key, value);
        Ok(())
    }

    pub(super) fn integer(&self, game: &Game, expr: &Expr<Int>) -> Result<Int> {
        match expr {
            Expr::Literal(value) => Ok(*value),
            Expr::Bound(binding) | Expr::Captured(CaptureRef { binding }) => {
                if let Some(Slot::Integer(value)) = self.slots.get(&binding.key()) {
                    Ok(*value)
                } else {
                    Err(invalid("missing signed binding"))
                }
            }
            Expr::Live(read) => match read.as_ref() {
                IntRead::PlayerResource { player, property } => {
                    let player = game.player(seat_name(self.player(game, player)?)?)?;
                    Ok(Int(match property {
                        PlayerResource::Life => int(&player.leader["life"]),
                        PlayerResource::PpCurrent => int(&player.pp["current"]),
                        PlayerResource::PpMaximum => int(&player.pp["max"]),
                        PlayerResource::Ep => player.ep,
                        PlayerResource::Sep => player.sep,
                    }))
                }
                IntRead::HistorySum { .. } => Err(unsupported("signed read")),
            },
            Expr::Frozen(frozen) => match frozen.as_ref() {
                IntFrozen::ReceiptRequested { receipt } => {
                    self.quantity(receipt).map(|(requested, _)| requested)
                }
                _ => Err(unsupported("signed projection")),
            },
            Expr::Compute(operation) => match operation.as_ref() {
                IntOperation::FromNat { value } => Ok(Int(i64::try_from(
                    self.natural(game, value)?.0,
                )
                .map_err(invalid)?)),
                IntOperation::Conditional {
                    condition,
                    then_value,
                    else_value,
                } => self.integer(
                    game,
                    if self.boolean(game, condition)?.0 {
                        then_value
                    } else {
                        else_value
                    },
                ),
                IntOperation::Arithmetic {
                    operation,
                    arguments,
                } => {
                    let values = arguments
                        .iter()
                        .map(|argument| self.integer(game, argument).map(|value| value.0))
                        .collect::<Result<Vec<_>>>()?;
                    arithmetic(*operation, &values).map(Int)
                }
                _ => Err(unsupported("signed operation")),
            },
        }
    }

    pub(super) fn natural(&self, game: &Game, expr: &Expr<Nat>) -> Result<Nat> {
        match expr {
            Expr::Literal(value) => Ok(*value),
            Expr::Bound(binding) | Expr::Captured(CaptureRef { binding }) => {
                if let Some(Slot::Natural(value)) = self.slots.get(&binding.key()) {
                    Ok(*value)
                } else {
                    Err(invalid("missing natural binding"))
                }
            }
            Expr::Frozen(frozen) => match frozen.as_ref() {
                NatFrozen::ReceiptActual { receipt } => {
                    self.quantity(receipt).map(|(_, actual)| actual)
                }
                _ => Err(unsupported("natural projection")),
            },
            Expr::Live(read) => match read.as_ref() {
                NatRead::ZoneCount { selector } => Ok(Nat(u64::try_from(
                    self.objects(game, selector)?.len(),
                )
                .map_err(invalid)?)),
                _ => Err(unsupported("natural read")),
            },
            Expr::Compute(operation) => match operation.as_ref() {
                NatOperation::Count { objects } => Ok(Nat(u64::try_from(
                    self.objects(game, objects)?.len(),
                )
                .map_err(invalid)?)),
                NatOperation::CheckedInt { value } => Ok(Nat(u64::try_from(
                    self.integer(game, value)?.0,
                )
                .map_err(invalid)?)),
                NatOperation::Conditional {
                    condition,
                    then_value,
                    else_value,
                } => self.natural(
                    game,
                    if self.boolean(game, condition)?.0 {
                        then_value
                    } else {
                        else_value
                    },
                ),
                _ => Err(unsupported("natural operation")),
            },
        }
    }

    pub(super) fn boolean(&self, game: &Game, expr: &Expr<Bool>) -> Result<Bool> {
        match expr {
            Expr::Literal(value) => Ok(*value),
            Expr::Bound(binding) | Expr::Captured(CaptureRef { binding }) => {
                if let Some(Slot::Boolean(value)) = self.slots.get(&binding.key()) {
                    Ok(*value)
                } else {
                    Err(invalid("missing boolean binding"))
                }
            }
            Expr::Compute(operation) => Ok(Bool(match operation.as_ref() {
                Predicate::And { predicates } => predicates
                    .iter()
                    .map(|predicate| self.boolean(game, predicate).map(|value| value.0))
                    .collect::<Result<Vec<_>>>()?
                    .into_iter()
                    .all(identity),
                Predicate::Or { predicates } => predicates
                    .iter()
                    .map(|predicate| self.boolean(game, predicate).map(|value| value.0))
                    .collect::<Result<Vec<_>>>()?
                    .into_iter()
                    .any(identity),
                Predicate::Not { predicate } => !self.boolean(game, predicate)?.0,
                Predicate::CompareInt {
                    left,
                    comparison,
                    right,
                } => compare(
                    self.integer(game, left)?.0,
                    *comparison,
                    self.integer(game, right)?.0,
                ),
                Predicate::CompareNat {
                    left,
                    comparison,
                    right,
                } => compare(
                    self.natural(game, left)?.0,
                    *comparison,
                    self.natural(game, right)?.0,
                ),
                Predicate::Exists { objects } => !self.objects(game, objects)?.is_empty(),
                _ => return Err(unsupported("predicate")),
            })),
            _ => Err(unsupported("boolean expression")),
        }
    }

    fn quantity(&self, receipt: &MeasuredReceipt) -> Result<(Int, Nat)> {
        // Damage requested/actual aggregation awaits Astra; programs may not branch on it yet.
        let MeasuredReceipt::Cards { receipt } = receipt else {
            return Err(unsupported("non-card quantity receipt"));
        };
        let ReceiptLocation::Local { id } = receipt.location() else {
            return Err(unsupported("iteration receipt"));
        };
        match self.receipts.get(id) {
            Some(InstructionOutcome::Cards(outcome)) => Ok((outcome.requested, outcome.actual)),
            _ => Err(invalid("missing typed card receipt")),
        }
    }
}

fn arithmetic(operation: Arithmetic, values: &[i64]) -> Result<i64> {
    let (&first, rest) = values
        .split_first()
        .ok_or_else(|| invalid("empty arithmetic"))?;
    rest.iter().try_fold(first, |left, &right| {
        match operation {
            Arithmetic::Add => left.checked_add(right),
            Arithmetic::Subtract => left.checked_sub(right),
            Arithmetic::Multiply => left.checked_mul(right),
            Arithmetic::Minimum => Some(left.min(right)),
            Arithmetic::Maximum => Some(left.max(right)),
            _ => None,
        }
        .ok_or_else(|| invalid("signed arithmetic overflow"))
    })
}
fn compare<T>(left: T, comparison: Comparison, right: T) -> bool
where
    T: Ord + Copy,
{
    match comparison {
        Comparison::Equal => left == right,
        Comparison::NotEqual => left != right,
        Comparison::Less => left < right,
        Comparison::LessEqual => left <= right,
        Comparison::Greater => left > right,
        Comparison::GreaterEqual => left >= right,
    }
}
fn zone_name(zone: ZoneKind) -> Result<&'static str> {
    match zone {
        ZoneKind::Field => Ok("field"),
        ZoneKind::Ex => Ok("ex"),
        ZoneKind::Cemetery => Ok("cemetery"),
        ZoneKind::Banish => Ok("banish"),
        _ => Err(unsupported("hidden or unsupported selector zone")),
    }
}
