//! Live repeat termination and explicit follow permissions retain each successful movement.
use super::{
    AbilitySource, Binding, BodyContext, LoweringError, Node, Selector, SourceSpan, ast,
    unsupported,
};
use crate::ir::repeated_moves as ir_repeated_moves;
use crate::ir::{
    Control, FollowPermission, GenerationDeduplication, IterationCheck, MovementReceiptList,
    ReceiptExport, ReceiptReference, RepeatBody, Termination,
};

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Repeat lowering shares the enclosing typed environment."
)]
impl BodyContext<'_> {
    pub(super) fn repeated_move(
        &mut self,
        value: &ast::Repeat,
        path: &str,
        source: &SourceSpan,
        origin: &AbilitySource,
    ) -> Result<Node, LoweringError> {
        let termination = match (value.times, &value.until, &value.r#while) {
            (None, Some(predicate), None) => Termination::Until {
                predicate: self.condition(predicate, path)?,
            },
            (None, None, Some(predicate)) => Termination::While {
                predicate: self.condition(predicate, path)?,
            },
            _ => return Err(unsupported(path)),
        };
        let parent = self.scope;
        let scope = self.card_allocator.scope(path)?;
        let iteration = Binding::new(self.allocator.id(path)?, scope);
        let collect = value
            .collect
            .as_ref()
            .map(|_| self.allocator.id(path).map(|id| Binding::new(id, parent)))
            .transpose()?;
        let saved = self.local_bindings();
        let dice = self.dice_results.clone();
        let reads = self.receipt_reads.clone();
        self.scope = self.card_allocator.scope(path)?;
        let body = self.steps(&value.steps, &format!("{path}/repeat/do"), source, origin);
        self.scope = parent;
        self.restore_bindings(saved);
        self.dice_results = dice;
        self.receipt_reads = reads;
        let body = body?;
        let [step] = body.steps.as_slice() else {
            return Err(unsupported(path));
        };
        let Node::Instruction { instruction } = &step.node else {
            return Err(unsupported(path));
        };
        let receipt = instruction.receipt;
        let control = Control::Repeat {
            body: RepeatBody::Program { body },
            termination,
            check: match value.check {
                Some(ast::RepeatCheck::After) => IterationCheck::AfterIteration,
                None | Some(ast::RepeatCheck::Before) => IterationCheck::BeforeIteration,
            },
            scope,
            receipts: vec![ReceiptExport::Move {
                source: ReceiptReference::new(receipt),
                iteration,
                collect,
            }],
        };
        if ir_repeated_moves::projection(&control).is_none() {
            return Err(unsupported(path));
        }
        if let (Some(name), Some(receipts)) = (&value.collect, collect)
            && self
                .ex_outputs
                .insert(
                    name.receipts.clone(),
                    Selector::MoveAfterAll {
                        receipts: MovementReceiptList::Move { receipts },
                        permission: FollowPermission::SameEffectExplicitReference {},
                        deduplicate: GenerationDeduplication::ByObjectAndGeneration,
                    },
                )
                .is_some()
        {
            return Err(unsupported(path));
        }
        Ok(Node::Control { control })
    }
}
