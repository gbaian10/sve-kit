use super::{BodyContext, LoweringError, ast, unsupported};
use crate::ir::{Expr, Int, IntFrozen, ReceiptId, ReceiptReference};

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Material values share the checked body receipt environment."
)]
impl BodyContext<'_> {
    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "These producers share paid-material names; new named steps must join this guard."
    )]
    pub(super) fn check_paid_material_name(
        &self,
        step: &ast::Step,
        path: &str,
    ) -> Result<(), LoweringError> {
        let name = match step {
            ast::Step::Move(value) => value.result.as_deref(),
            ast::Step::Search(value) => value.result.as_deref(),
            ast::Step::Create(value) => value.result.as_deref().or(value.bind.as_deref()),
            ast::Step::Look(value) => Some(value.bind.as_str()),
            ast::Step::Choose(value) => Some(value.bind.as_str()),
            ast::Step::Either(value) => value.bind.as_deref(),
            ast::Step::Destroy(ast::ObjectAction::Long { result, .. }) => result.as_deref(),
            _ => None,
        };
        if name.is_some_and(|name| {
            self.receipt_reads.paid_materials.contains_key(name)
                || self.receipt_reads.paid_material_acts.contains_key(name)
        }) {
            return Err(unsupported(path));
        }
        Ok(())
    }

    pub(super) fn record_paid_material(
        &mut self,
        material: &ast::Material,
        receipt: ReceiptId,
        path: &str,
    ) -> Result<(), LoweringError> {
        let Some(name) = &material.result else {
            return Ok(());
        };
        if material.n != Some(1)
            || material.any
            || !matches!(material.sel.zone, ast::PublicZone::Field)
            || !matches!(material.sel.who, ast::Side::Controller)
            || !matches!(material.sel.kind, Some(ast::SearchKind::Follower))
        {
            return Ok(());
        }
        if name.is_empty()
            || self.receipt_reads.paid_material_acts.contains_key(name)
            || self.receipt_reads.paid_materials.contains_key(name)
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
            .paid_materials
            .insert(name.clone(), receipt);
        Ok(())
    }

    pub(super) fn record_paid_material_acts(
        &mut self,
        material: &ast::Material,
        receipt: ReceiptId,
        path: &str,
    ) -> Result<(), LoweringError> {
        let Some(name) = &material.result else {
            return Ok(());
        };
        if !matches!(material.sel.who, ast::Side::Controller)
            || !matches!(material.sel.zone, ast::PublicZone::Field)
        {
            return Err(unsupported(path));
        }
        if name.is_empty()
            || self.receipt_reads.paid_material_acts.contains_key(name)
            || self.receipt_reads.paid_materials.contains_key(name)
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
            .paid_material_acts
            .insert(name.clone(), receipt);
        Ok(())
    }

    pub(super) fn paid_material_attack(
        &self,
        name: &str,
        path: &str,
    ) -> Result<Expr<Int>, LoweringError> {
        let receipt = *self
            .receipt_reads
            .paid_materials
            .get(name)
            .ok_or_else(|| unsupported(path))?;
        Ok(Expr::Frozen(Box::new(IntFrozen::PaidMaterialAttack {
            receipt: ReceiptReference::new(receipt),
        })))
    }
}
