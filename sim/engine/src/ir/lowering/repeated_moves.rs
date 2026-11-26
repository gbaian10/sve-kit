//! Live repeat termination and explicit follow permissions retain each successful movement.
use super::{
    AbilitySource, Binding, BodyContext, LoweringError, Node, Selector, SourceSpan, ast,
    unsupported,
};
use crate::ir::repeated_moves as ir_repeated_moves;
use crate::ir::{
    Control, Expr, FollowPermission, GenerationDeduplication, IterationCheck, MovementReceipt,
    MovementReceiptList, ObjectReceipt, Predicate, ReceiptExport, ReceiptLocation,
    ReceiptReference, RepeatBody, Termination,
};

impl BodyContext<'_> {
    pub(super) fn repeated_move(
        &mut self,
        value: &ast::Repeat,
        path: &str,
        source: &SourceSpan,
        origin: &AbilitySource,
    ) -> Result<Node, LoweringError> {
        let (predicate, while_true) = match (value.times, &value.until, &value.r#while) {
            (None, Some(predicate), None) => (predicate, false),
            (None, None, Some(predicate)) => (predicate, true),
            _ => return Err(unsupported(path)),
        };
        let after = matches!(value.check, Some(ast::RepeatCheck::After));
        let mill = while_true
            && after
            && matches!(predicate, ast::Condition::Cmp(comparison)
                if matches!(comparison.left, ast::Number::ReceiptCount { .. }));
        let before = if mill {
            None
        } else {
            Some(self.condition(predicate, path)?)
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
        let predicate = before.map_or_else(|| self.condition(predicate, path), Ok);
        self.scope = parent;
        self.restore_bindings(saved);
        self.dice_results = dice;
        self.receipt_reads = reads;
        let body = body?;
        let Some(step) = body.steps.last() else {
            return Err(unsupported(path));
        };
        let Node::Instruction { instruction } = &step.node else {
            return Err(unsupported(path));
        };
        let receipt = instruction.receipt;
        let mut predicate = predicate?;
        if mill {
            let Expr::Compute(operation) = &mut predicate else {
                return Err(unsupported(path));
            };
            let Predicate::ReceiptAny {
                receipt:
                    ObjectReceipt::Movement {
                        receipt: MovementReceipt::Move { receipt: read },
                    },
                ..
            } = operation.as_mut()
            else {
                return Err(unsupported(path));
            };
            if !matches!(read.location(), ReceiptLocation::Local { id } if *id == receipt) {
                return Err(unsupported(path));
            }
            *read = ReceiptReference::iteration(iteration);
        }
        let termination = if while_true {
            Termination::While { predicate }
        } else {
            Termination::Until { predicate }
        };
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
