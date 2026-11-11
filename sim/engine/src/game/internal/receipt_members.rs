//! Receipt adapters expose only information retained by their checked producer.

use super::{Execution, Fact, Game, InstructionOutcome, Result, invalid, unsupported};
use crate::ir::{
    CardKind, CardName, Characteristics, ClassId, DestroyOutcome, DiscardOutcome, Info, Int,
    MaterialActOutcome, MemberProjection, MoveMethod, MoveOutcome, MoveResult, MovementReceipt,
    Nat, NumericProperty, ObjectReceipt, ObjectRef, PlayerRef, Quantity, QuantityUnit, ReceiptId,
    ReceiptLocation, ReceiptReference, Selector, TraitId, ZoneKind,
};
use alloc::collections::BTreeSet;

/// The member information a receipt condition may read.
pub(super) struct MemberCard {
    pub(super) names: Vec<CardName>,
    pub(super) kind: CardKind,
    pub(super) traits: Vec<TraitId>,
    pub(super) class: Option<ClassId>,
}

impl From<&Characteristics> for MemberCard {
    fn from(characters: &Characteristics) -> Self {
        Self {
            names: characters.names.clone(),
            kind: characters.kind,
            traits: characters.traits.clone(),
            class: characters.class.clone(),
        }
    }
}

pub(super) fn local_receipt(receipt: &ObjectReceipt) -> Result<ReceiptId> {
    let id = match receipt {
        ObjectReceipt::Movement {
            receipt: MovementReceipt::Discard { receipt },
        } => match receipt.location() {
            ReceiptLocation::Local { id } => Some(id),
            ReceiptLocation::Iteration { .. } => None,
        },
        ObjectReceipt::Movement {
            receipt: MovementReceipt::Move { receipt },
        } => match receipt.location() {
            ReceiptLocation::Local { id } => Some(id),
            ReceiptLocation::Iteration { .. } => None,
        },
        ObjectReceipt::Look { receipt } => match receipt.location() {
            ReceiptLocation::Local { id } => Some(id),
            ReceiptLocation::Iteration { .. } => None,
        },
        ObjectReceipt::Fusion { receipt } => match receipt.location() {
            ReceiptLocation::Local { id } => Some(id),
            ReceiptLocation::Iteration { .. } => None,
        },
        _ => return Err(unsupported("receipt member condition")),
    };
    id.copied()
        .ok_or_else(|| unsupported("iterated receipt member condition"))
}

pub(super) struct MemberView {
    pub(super) before: ObjectRef,
    card: MemberCard,
}

impl MemberView {
    pub(super) const fn card(&self) -> &MemberCard {
        &self.card
    }

    pub(super) fn original_cost(&self, game: &Game, execution: &Execution) -> Result<i64> {
        // Base printing is immutable across generations and effective-cost changes.
        let card = &game.object(execution.object_id(self.before)?)?.card;
        super::cost::printed_cost(&game.catalog.face(card, 0)?["cost"])
    }
}

struct PaidMembers<'execution> {
    quantity: &'execution Quantity,
    subjects: &'execution [ObjectRef],
    characters: &'execution [Characteristics],
}

impl<'execution> PaidMembers<'execution> {
    fn load(
        execution: &'execution Execution,
        id: ReceiptId,
        missing: &'static str,
    ) -> Result<Self> {
        let Some(InstructionOutcome::Payment {
            quantity,
            subjects,
            characters,
        }) = execution.receipts.get(&id)
        else {
            return Err(invalid(missing));
        };
        Ok(Self {
            quantity,
            subjects,
            characters,
        })
    }

    fn count(&self) -> Result<Int> {
        if self.quantity.unit != QuantityUnit::Cards
            || self.quantity.amount.0 < 0
            || usize::try_from(self.quantity.amount.0).map_err(invalid)? != self.subjects.len()
        {
            return Err(invalid("material Act quantity"));
        }
        Ok(self.quantity.amount)
    }

    fn number(&self, property: NumericProperty) -> Result<Int> {
        let ([_], [characters]) = (self.subjects, self.characters) else {
            return Err(invalid("material number requires one frozen follower"));
        };
        if self.quantity.unit != QuantityUnit::Cards || self.quantity.amount != Int(1) {
            return Err(invalid("material number quantity"));
        }
        let information = match property {
            NumericProperty::Attack => &characters.attack,
            NumericProperty::Cost => &characters.cost,
            _ => return Err(unsupported("paid material numeric property")),
        };
        match information {
            Info::Present(amount) => Ok(*amount),
            Info::AbsentByRules => Err(invalid("missing frozen material number")),
        }
    }

    fn fusion(&self) -> Result<Vec<MemberView>> {
        if self.subjects.len() != self.characters.len() {
            return Err(invalid("missing Fusion material characteristics"));
        }
        Ok(self
            .characters
            .iter()
            .zip(self.subjects)
            .map(|(characters, before)| MemberView {
                before: *before,
                card: MemberCard::from(characters),
            })
            .collect())
    }
}

fn discarded_characters<'execution>(
    execution: &'execution Execution,
    before: ObjectRef,
    missing: &'static str,
) -> Result<&'execution Characteristics> {
    execution
        .facts
        .iter()
        .find_map(|fact| match fact {
            Fact::Discard(event) if event.card_before == before => Some(&event.characters),
            _ => None,
        })
        .ok_or_else(|| invalid(missing))
}

pub(super) fn members(
    game: &Game,
    execution: &Execution,
    receipt: &ObjectReceipt,
    destination: Option<ZoneKind>,
) -> Result<Vec<MemberView>> {
    let id = local_receipt(receipt)?;
    if matches!(receipt, ObjectReceipt::Fusion { .. }) {
        return PaidMembers::load(execution, id, "missing Fusion payment outcome")?.fusion();
    }
    if matches!(receipt, ObjectReceipt::Look { .. }) {
        return execution
            .public_look_objects(
                game,
                &Selector::ReceiptMembers {
                    receipt: receipt.clone(),
                    projection: MemberProjection::Before,
                },
            )?
            .into_iter()
            .map(|before| {
                Ok(MemberView {
                    before,
                    card: printed_card(game, execution, before)?,
                })
            })
            .collect();
    }
    match execution.receipts.get(&id) {
        Some(InstructionOutcome::Discard(outcome)) => outcome
            .successful
            .iter()
            .map(|result| {
                let before = result
                    .before
                    .ok_or_else(|| invalid("missing discard before member"))?;
                Ok(MemberView {
                    before: before.object,
                    card: MemberCard::from(discarded_characters(
                        execution,
                        before.object,
                        "missing discarded characteristics",
                    )?),
                })
            })
            .collect(),
        Some(InstructionOutcome::Move(_)) => moved_deck_members(execution, receipt, destination)?
            .into_iter()
            .map(|before| {
                Ok(MemberView {
                    before,
                    card: printed_card(game, execution, before)?,
                })
            })
            .collect(),
        _ => Err(invalid("missing receipt member producer")),
    }
}

fn moved_deck_members(
    execution: &Execution,
    receipt: &ObjectReceipt,
    destination: Option<ZoneKind>,
) -> Result<Vec<ObjectRef>> {
    let Some(InstructionOutcome::Move(outcome)) = execution.receipts.get(&local_receipt(receipt)?)
    else {
        return Err(invalid("missing deck movement outcome"));
    };
    outcome
        .moved
        .iter()
        .filter(|movement| {
            movement.after.is_some()
                && destination
                    .is_none_or(|zone| movement.actual_to.is_some_and(|to| to.kind == zone))
        })
        .map(|movement| {
            if !execution.facts.iter().any(|fact| {
                matches!(fact, Fact::Movement(moved)
            if moved.movement.before == movement.before && moved.from.kind == ZoneKind::Deck)
            }) {
                return Err(unsupported("receipt member that did not leave the deck"));
            }
            Ok(movement.before)
        })
        .collect()
}

pub(super) fn distinct_cost_receipt(
    receipt: &ObjectReceipt,
    projection: MemberProjection,
) -> Result<ReceiptId> {
    if projection != MemberProjection::Before
        || !matches!(
            receipt,
            ObjectReceipt::Movement {
                receipt: MovementReceipt::Move { .. }
            }
        )
    {
        return Err(unsupported(
            "distinct original costs require successful deck movement before members",
        ));
    }
    local_receipt(receipt)
}

pub(super) fn distinct_original_costs(
    game: &Game,
    execution: &Execution,
    receipt: &ObjectReceipt,
    projection: MemberProjection,
) -> Result<Nat> {
    distinct_cost_receipt(receipt, projection)?;
    let mut costs = BTreeSet::new();
    for before in moved_deck_members(execution, receipt, Some(ZoneKind::Cemetery))? {
        // Base printing is immutable across generations and effective-cost changes.
        let card = &game.object(execution.object_id(before)?)?.card;
        costs.insert(super::cost::printed_play_cost(
            &game.catalog.face(card, 0)?["cost"],
        )?);
    }
    Ok(Nat(u64::try_from(costs.len()).map_err(invalid)?))
}

/// This trait-only deck read does not require unrelated ability-derived names.
fn printed_card(game: &Game, execution: &Execution, before: ObjectRef) -> Result<MemberCard> {
    let card = &game.object(execution.object_id(before)?)?.card;
    let printed = game.catalog.face(card, 0)?;
    let kind = match printed["card_type"].as_str().unwrap_or_default() {
        kind if kind.contains("フォロワー") => CardKind::Follower,
        kind if kind.starts_with("スペル") => CardKind::Spell,
        kind if kind.starts_with("アミュレット") => CardKind::Amulet,
        _ => return Err(unsupported("receipt member card kind")),
    };
    let text = |value: &serde_json::Value| {
        value
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| invalid("unknown printed receipt member text"))
    };
    Ok(MemberCard {
        names: vec![CardName(text(&printed["name"])?)],
        kind,
        traits: printed["traits"]
            .as_array()
            .ok_or_else(|| invalid("unknown printed traits"))?
            .iter()
            .map(|value| text(value).map(TraitId))
            .collect::<Result<_>>()?,
        class: Some(ClassId(text(&printed["card_class"])?)),
    })
}

pub(super) fn paid_material_acts(
    execution: &Execution,
    receipt: &ReceiptReference<MaterialActOutcome>,
) -> Result<Int> {
    let ReceiptLocation::Local { id } = receipt.location() else {
        return Err(unsupported("iterated paid material Acts"));
    };
    PaidMembers::load(execution, *id, "missing material Act payment")?.count()
}

pub(super) fn paid_material_number(
    execution: &Execution,
    receipt: &ReceiptReference<MoveOutcome>,
    property: NumericProperty,
) -> Result<Int> {
    let ReceiptLocation::Local { id } = receipt.location() else {
        return Err(unsupported("iterated paid material number"));
    };
    PaidMembers::load(execution, *id, "missing material payment")?.number(property)
}

pub(super) fn discarded_cost(
    execution: &Execution,
    receipt: &ReceiptReference<DiscardOutcome>,
) -> Result<Int> {
    let ReceiptLocation::Local { id } = receipt.location() else {
        return Err(unsupported("iteration discarded cost"));
    };
    let Some(InstructionOutcome::Discard(outcome)) = execution.receipts.get(id) else {
        return Err(invalid("missing discarded cost producer"));
    };
    let [member] = outcome.successful.as_slice() else {
        return Err(invalid("discarded cost requires exactly one card"));
    };
    let before = member
        .before
        .ok_or_else(|| invalid("missing discarded cost before member"))?;
    let characters = discarded_characters(
        execution,
        before.object,
        "missing discarded cost characteristics",
    )?;
    match characters.cost {
        Info::Present(amount) if amount.0 >= 0 => Ok(amount),
        _ => Err(invalid("missing discarded numeric cost")),
    }
}

impl Execution {
    pub(super) fn successful_moves(
        &self,
        receipt: &MovementReceipt,
    ) -> Result<impl Iterator<Item = &MoveResult>> {
        let moves = match receipt {
            MovementReceipt::Move { receipt } => {
                match self.receipts.get(local_id(receipt.location())?) {
                    Some(InstructionOutcome::Move(outcome)) => &outcome.moved,
                    _ => return Err(invalid("missing move receipt")),
                }
            }
            MovementReceipt::Search { receipt } => {
                match self.receipts.get(local_id(receipt.location())?) {
                    Some(InstructionOutcome::Search(outcome)) => &outcome.moved,
                    _ => return Err(invalid("missing search receipt")),
                }
            }
            MovementReceipt::Discard { receipt } => {
                match self.receipts.get(local_id(receipt.location())?) {
                    Some(InstructionOutcome::Discard(outcome)) => &outcome.moves,
                    _ => return Err(invalid("missing discard receipt")),
                }
            }
            MovementReceipt::Destroy { receipt } => {
                match self.receipts.get(local_id(receipt.location())?) {
                    Some(InstructionOutcome::Destroy(outcome)) => &outcome.moves,
                    _ => return Err(invalid("missing destroy receipt")),
                }
            }
            MovementReceipt::Banish { receipt } => {
                match self.receipts.get(local_id(receipt.location())?) {
                    Some(InstructionOutcome::Banish(outcome)) => &outcome.moves,
                    _ => return Err(invalid("missing banish receipt")),
                }
            }
            _ => return Err(unsupported("movement receipt projection")),
        };
        Ok(moves.iter().filter(|movement| {
            movement.after.is_some_and(|after| {
                after.id == movement.before.id && after.generation != movement.before.generation
            }) && movement.actual_to.is_some()
        }))
    }

    /// Successful before members; a same-deck reorder keeps generations, so all ordered cards count.
    pub(super) fn movement_before(&self, receipt: &MovementReceipt) -> Result<Vec<ObjectRef>> {
        let MovementReceipt::Order { receipt } = receipt else {
            return Ok(self
                .successful_moves(receipt)?
                .map(|movement| movement.before)
                .collect());
        };
        match self.receipts.get(local_id(receipt.location())?) {
            Some(InstructionOutcome::Order(outcome)) => Ok(outcome
                .successful
                .iter()
                .filter_map(|member| member.before.map(|view| view.object))
                .collect()),
            _ => Err(invalid("missing order receipt")),
        }
    }
}

fn local_id<T>(location: &ReceiptLocation<T>) -> Result<&ReceiptId> {
    match location {
        ReceiptLocation::Local { id } => Ok(id),
        ReceiptLocation::Iteration { .. } => Err(unsupported("iteration movement receipt")),
    }
}

pub(super) fn public_look_distinct_names(
    game: &Game,
    execution: &Execution,
    objects: &Selector,
) -> Result<Nat> {
    let names = execution
        .public_look_objects(game, objects)?
        .into_iter()
        .map(|member| game.card_name(execution.object_id(member)?))
        .collect::<Result<BTreeSet<_>>>()?;
    Ok(Nat(u64::try_from(names.len()).map_err(invalid)?))
}

pub(super) fn destroyed_controller(
    execution: &Execution,
    game: &Game,
    receipt: &ReceiptReference<DestroyOutcome>,
    player: &PlayerRef,
) -> Result<u64> {
    let ReceiptLocation::Local { id } = receipt.location() else {
        return Err(unsupported("local destroy result"));
    };
    let Some(InstructionOutcome::Destroy(outcome)) = execution.receipts.get(id) else {
        return Err(invalid("missing destroy result"));
    };
    let player = execution.player(game, player)?;
    outcome.successful.iter().try_fold(0_u64, |count, member| {
        let before = member
            .before
            .ok_or_else(|| invalid("destroy result missing Before"))?;
        let movement = execution
            .facts
            .iter()
            .find_map(|fact| match fact {
                Fact::Movement(fact)
                    if fact.method == MoveMethod::Destroy
                        && fact.movement.before == before.object =>
                {
                    Some(fact)
                }
                _ => None,
            })
            .ok_or_else(|| invalid("destroy result missing departure fact"))?;
        if movement.from.kind != ZoneKind::Field {
            return Err(invalid("destroy result did not leave field"));
        }
        count
            .checked_add(u64::from(movement.controller_before == player))
            .ok_or_else(|| invalid("token count overflow"))
    })
}
