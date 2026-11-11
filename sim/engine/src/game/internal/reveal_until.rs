//! Public prefix observations reuse the deck, printed candidate evaluator and movement receipts.
use core::slice;

use super::{Execution, Game, InstructionOutcome, Result, invalid, seat_name, unsupported};
use crate::ir::{
    Action, Bool, Expr, Instruction, Nat, ObjectReceipt, ObjectRef, ReceiptLocation,
    ReceiptReference, RevealUntilMembers, RevealUntilOutcome, RuntimeObjectView, Selector,
    SnapshotId, reveal_until,
};

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Prefix revelation uses the same authoritative deck and public knowledge."
)]
impl Game {
    pub(super) fn internal_reveal_until(
        &mut self,
        execution: &mut Execution,
        instruction: &Instruction,
    ) -> Result<()> {
        if !reveal_until::action(&instruction.action) {
            return Err(unsupported("public original-cost follower prefix"));
        }
        let Action::RevealUntil {
            deck,
            predicate,
            required: Expr::Literal(Nat(required)),
            ..
        } = &instruction.action
        else {
            return Err(unsupported("reveal-until action"));
        };
        let player = execution.player(self, deck)?;
        let cards = self.internal_known_deck(seat_name(player)?)?;
        let required = usize::try_from(*required).map_err(invalid)?;
        let mut revealed = Vec::new();
        let mut matched = Vec::new();
        for id in &cards {
            if matched.len() == required {
                break;
            }
            let object = execution.reference(self, id)?;
            let qualifies = super::search::matches_candidate(self, id, predicate)?;
            self.internal_record_reveal(execution, object, id)?;
            self.reveal(slice::from_ref(id), "all", &execution.frame(), true)?;
            let member = RuntimeObjectView {
                object,
                snapshot: SnapshotId(self.state.ir_epoch),
            };
            revealed.push(member);
            if qualifies {
                matched.push(member);
            }
        }
        let exhausted = Bool(revealed.len() == cards.len());
        execution.receipts.insert(
            instruction.receipt,
            InstructionOutcome::RevealUntil(RevealUntilOutcome {
                revealed,
                matched,
                exhausted,
            }),
        );
        Ok(())
    }
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Public observations use the same authority-owned receipt map as ordinary values."
)]
impl Execution {
    pub(super) fn revealed_objects(
        &self,
        receipt: &ReceiptReference<RevealUntilOutcome>,
        members: RevealUntilMembers,
    ) -> Result<Vec<ObjectRef>> {
        let ReceiptLocation::Local { id } = receipt.location() else {
            return Err(unsupported("iteration reveal-until receipt"));
        };
        let Some(InstructionOutcome::RevealUntil(outcome)) = self.receipts.get(id) else {
            return Err(invalid("missing reveal-until receipt"));
        };
        let views = match members {
            RevealUntilMembers::Revealed => &outcome.revealed,
            RevealUntilMembers::Matched => &outcome.matched,
        };
        Ok(views.iter().map(|member| member.object).collect())
    }

    pub(super) fn is_revealed_match(&self, selector: &Selector, member: ObjectRef) -> bool {
        if !reveal_until::matched(selector) {
            return false;
        }
        let Selector::ReceiptMembers {
            receipt: ObjectReceipt::RevealUntil { receipt, members },
            ..
        } = selector
        else {
            return false;
        };
        self.revealed_objects(receipt, *members)
            .is_ok_and(|matched| matched.contains(&member))
    }
}
