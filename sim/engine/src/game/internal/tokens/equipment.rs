use super::super::{Execution, Fact, Game, InstructionOutcome, Result, invalid, unsupported};
use crate::ir::{
    Action, EquipEvent, Instruction, Nat, ObjectOperand, ObjectRef, RuntimeReceiptId, SnapshotId,
    ZoneKind,
};

pub(in crate::game::internal) fn check_action(action: &Action) -> Result<()> {
    if let Action::Equip {
        holder,
        template,
        count,
    } = action
        && (*holder != (ObjectOperand::SourceHolder {})
            || *count != Nat(1)
            || template.0.is_empty())
    {
        return Err(unsupported("one named equipment on the source holder"));
    }
    Ok(())
}

pub(in crate::game::internal) fn check_catalog(game: &Game, action: &Action) -> Result<()> {
    check_action(action)?;
    if let Action::Equip { template, .. } = action {
        super::check_template(game, &template.0, ZoneKind::Equipment)?;
        let card = game.catalog.token_print(&template.0)?;
        if game.catalog.face(&card, 0)?["card_type"] != "イクイップメント・トークン" {
            return Err(unsupported("equipment requires an equipment token"));
        }
    }
    Ok(())
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Equipment reuses token allocation, physical movement and generation-bound associations."
)]
impl Game {
    pub(in crate::game::internal) fn internal_equipment_host(
        &self,
        execution: &Execution,
    ) -> Result<Vec<ObjectRef>> {
        let source = self.object(&execution.context.source)?;
        if source.zone != "equipment" || source.generation != execution.context.generation.0 {
            return Ok(vec![]);
        }
        let holder = source.state["equipped_to"]
            .as_str()
            .and_then(|id| self.state.objects.get(id))
            .filter(|holder| holder.zone == "field");
        let Some(holder) = holder else {
            return Ok(vec![]);
        };
        if !self.object_type(&holder.id)?.contains("フォロワー") {
            return Ok(vec![]);
        }
        Ok(vec![execution.reference(self, &holder.id)?])
    }

    pub(in crate::game::internal) fn internal_equipment_links(&self, holder: &str) -> Result<bool> {
        let mut attached = false;
        for equipment in self
            .state
            .objects
            .values()
            .filter(|object| object.state["equipped_to"] == holder)
        {
            if equipment.zone != "equipment"
                || self.object(holder)?.zone != "field"
                || self.object_type(&equipment.id)? != "イクイップメント・トークン"
                || !self.card_runtime.has_definition(&equipment.card)
            {
                return Err(unsupported("equipment departure association"));
            }
            attached = true;
        }
        Ok(attached)
    }

    pub(in crate::game::internal) fn internal_equip(
        &mut self,
        execution: &mut Execution,
        instruction: &Instruction,
    ) -> Result<()> {
        let Action::Equip {
            holder, template, ..
        } = &instruction.action
        else {
            return Err(unsupported("equipment instruction"));
        };
        check_catalog(self, &instruction.action)?;
        let before = SnapshotId(self.state.ir_epoch);
        let source = execution
            .ability
            .ok_or_else(|| unsupported("equipment whole-card authority"))?;
        let holder = execution.object(self, holder)?;
        let id = execution.object_id(holder)?.to_owned();
        let object = self.object(&id)?;
        if object.generation != holder.generation.0
            || object.zone != "field"
            || !self.object_type(&id)?.contains("フォロワー")
        {
            execution
                .receipts
                .insert(instruction.receipt, InstructionOutcome::Equipped(vec![]));
            return Ok(());
        }
        let controller = object.controller.clone();
        let print = self.catalog.token_print(&template.0)?;
        if !self.card_runtime.is_loaded(&print) {
            return Err(unsupported(
                "created equipment requires an admitted whole card",
            ));
        }
        let ids = self.internal_create_prints(&[print], &controller)?;
        let created = self.commit_token_objects(execution, ZoneKind::Equipment, &ids)?;
        let [equipment] = created.as_slice() else {
            return Err(invalid("equipment creation cardinality"));
        };
        let equipment_id = execution.object_id(*equipment)?.to_owned();
        self.attach_equipment(&id, &equipment_id, &execution.frame())?;
        let event = EquipEvent {
            holder,
            equipment: *equipment,
            association: self.internal_object_association(holder, *equipment)?,
            source,
            created_receipt: RuntimeReceiptId {
                execution: execution.id,
                slot: instruction.receipt,
            },
        };
        execution.facts.push(Fact::Equipped(event));
        execution.receipts.insert(
            instruction.receipt,
            InstructionOutcome::Equipped(vec![event]),
        );
        self.internal_equipped_event(event, execution, before)
    }
}
