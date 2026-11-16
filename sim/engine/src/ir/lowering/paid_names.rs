use super::{BodyContext, LoweringError, ast, unsupported};
use crate::ir::{Binding, Bool, Expr, ObjectReceipt, ObjectRef, paid_names};

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Named payment objects share the body's checked receipt environment."
)]
impl BodyContext<'_> {
    pub(super) fn record_paid_name(
        &mut self,
        name: &str,
        receipt: ObjectReceipt,
        path: &str,
    ) -> Result<(), LoweringError> {
        if name.is_empty()
            || self.receipt_reads.paid_names.contains_key(name)
            || self.receipt_reads.paid_materials.contains_key(name)
            || self.receipt_reads.paid_material_acts.contains_key(name)
            || self.receipt_reads.paid_counters.contains_key(name)
            || self.observations.contains_key(name)
            || self.targets.contains_key(name)
            || self.recipient_targets.contains_key(name)
            || self.definitions.contains_key(name)
            || self.searches.contains(name)
            || self.ex_outputs.contains_key(name)
        {
            return Err(unsupported(path));
        }
        self.receipt_reads
            .paid_names
            .insert(name.to_owned(), receipt);
        Ok(())
    }

    pub(super) fn paid_name_filter(
        &mut self,
        read: &ast::BeforeObjects,
        candidate: Binding<ObjectRef>,
        path: &str,
    ) -> Result<Expr<Bool>, LoweringError> {
        let receipt = self
            .receipt_reads
            .paid_names
            .get(&read.objects_of)
            .ok_or_else(|| unsupported(path))?
            .clone();
        let member = Binding::new(1, self.card_allocator.scope(path)?);
        Ok(paid_names::predicate(receipt, candidate, member))
    }
}
